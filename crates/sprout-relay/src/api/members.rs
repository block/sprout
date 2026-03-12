//! Channel membership REST API.
//!
//! Endpoints:
//!   POST   /api/channels/{channel_id}/members          — Add member(s)
//!   DELETE /api/channels/{channel_id}/members/{pubkey} — Remove member
//!   GET    /api/channels/{channel_id}/members          — List members
//!   POST   /api/channels/{channel_id}/join             — Self-join (open channels)
//!   POST   /api/channels/{channel_id}/leave            — Self-leave
//!   GET    /api/channels/{channel_id}                  — Get channel details

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Json as ExtractJson, Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use nostr::util::hex as nostr_hex;
use serde::Deserialize;
use sprout_db::channel::MemberRole;
use uuid::Uuid;

use crate::handlers::side_effects::emit_system_message;
use crate::state::AppState;

use super::{
    api_error, check_channel_access, check_token_channel_access, extract_auth_context, forbidden,
    internal_error, scope_error,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Verify the actor is an owner or admin of the channel. Returns 403 if not.
async fn require_owner_or_admin(
    state: &AppState,
    channel_id: uuid::Uuid,
    pubkey_bytes: &[u8],
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    let role = state
        .db
        .get_member_role(channel_id, pubkey_bytes)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;
    match role.as_deref() {
        Some("owner") | Some("admin") => Ok(()),
        _ => Err(forbidden("requires owner or admin role")),
    }
}

// ── Request bodies ────────────────────────────────────────────────────────────

/// Request body for adding member(s) to a channel.
#[derive(Debug, Deserialize)]
pub struct AddMembersBody {
    /// Hex-encoded public keys to add.
    pub pubkeys: Vec<String>,
    /// Role to assign (`"member"`, `"admin"`, `"guest"`, `"bot"`).
    #[serde(default = "default_role")]
    pub role: String,
}

fn default_role() -> String {
    "member".to_string()
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `POST /api/channels/{channel_id}/members` — Add member(s) to a channel.
///
/// Actor must be an owner or admin. Returns lists of added pubkeys and any errors.
pub async fn add_members(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel_id): Path<String>,
    ExtractJson(body): ExtractJson<AddMembersBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::AdminChannels)
        .map_err(scope_error)?;
    let actor_bytes = ctx.pubkey_bytes.clone();

    let channel_id = Uuid::parse_str(&channel_id)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid channel_id"))?;
    check_token_channel_access(&ctx, &channel_id)?;

    // Private channels require owner/admin to add members.
    let channel = state
        .db
        .get_channel(channel_id)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;
    if channel.visibility == "private" {
        require_owner_or_admin(&state, channel_id, &actor_bytes).await?;
    }

    // Reject writes to archived channels.
    if channel.archived_at.is_some() {
        return Err(api_error(StatusCode::FORBIDDEN, "channel is archived"));
    }

    let role: MemberRole = body
        .role
        .parse()
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid role"))?;

    let actor_hex = nostr_hex::encode(&actor_bytes);
    let mut added = Vec::new();
    let mut errors = Vec::new();

    for hex_pk in &body.pubkeys {
        let pubkey_bytes = match hex::decode(hex_pk) {
            Ok(b) if b.len() == 32 => b,
            _ => {
                errors.push(serde_json::json!({
                    "pubkey": hex_pk,
                    "error": "invalid pubkey hex"
                }));
                continue;
            }
        };

        // --- Agent Channel Protection: self-add bypass + policy check ---
        // Self-add: always allowed, skip policy check.
        if pubkey_bytes == actor_bytes {
            match state
                .db
                .add_member(channel_id, &pubkey_bytes, role.clone(), Some(&actor_bytes))
                .await
            {
                Ok(_) => {
                    let target_hex = nostr_hex::encode(&pubkey_bytes);
                    if let Err(e) = emit_system_message(
                        &state,
                        channel_id,
                        serde_json::json!({
                            "type": "member_joined",
                            "actor": &actor_hex,
                            "target": target_hex,
                        }),
                    )
                    .await
                    {
                        tracing::warn!("Failed to emit system message: {e}");
                    }
                    added.push(hex_pk.clone());
                }
                Err(e) => {
                    errors.push(serde_json::json!({
                        "pubkey": hex_pk,
                        "error": e.to_string()
                    }));
                }
            }
            continue;
        }

        // Third-party add: check channel_add_policy.
        // FAIL CLOSED: DB errors block the add (never bypass protection).
        match state.db.get_agent_channel_policy(&pubkey_bytes).await {
            Err(e) => {
                errors.push(serde_json::json!({
                    "pubkey": hex_pk,
                    "error": format!("policy lookup failed: {e}"),
                }));
                continue;
            }
            Ok(Some((ref policy, ref owner))) => {
                let blocked = match policy.as_str() {
                    "owner_only" => match owner {
                        Some(owner_bytes) => actor_bytes.as_slice() != owner_bytes.as_slice(),
                        None => true,
                    },
                    "nobody" => true,
                    // "anyone" or any unknown value → allow.
                    // NOTE: DB ENUM constraint prevents unknown values from being stored.
                    // If a new policy value is added to the ENUM, update this match.
                    _ => false,
                };

                if blocked {
                    let reason = match policy.as_str() {
                        "owner_only" if owner.is_none() => {
                            "policy:owner_only — agent has no owner set"
                        }
                        "owner_only" => {
                            "policy:owner_only — only the agent owner can add this agent"
                        }
                        "nobody" => {
                            "policy:nobody — this agent has disabled external channel additions"
                        }
                        _ => "policy:blocked",
                    };
                    errors.push(serde_json::json!({
                        "pubkey": hex_pk,
                        "error": reason,
                    }));
                    continue;
                }
            }
            Ok(None) => {
                // Pubkey not in users table — no policy row, treat as "anyone" (default).
            }
        }
        // --- End Agent Channel Protection ---

        match state
            .db
            .add_member(channel_id, &pubkey_bytes, role.clone(), Some(&actor_bytes))
            .await
        {
            Ok(_) => {
                let target_hex = nostr_hex::encode(&pubkey_bytes);
                if let Err(e) = emit_system_message(
                    &state,
                    channel_id,
                    serde_json::json!({
                        "type": "member_joined",
                        "actor": actor_hex,
                        "target": target_hex,
                    }),
                )
                .await
                {
                    tracing::warn!("Failed to emit system message: {e}");
                }
                added.push(hex_pk.clone());
            }
            Err(e) => {
                errors.push(serde_json::json!({
                    "pubkey": hex_pk,
                    "error": e.to_string()
                }));
            }
        }
    }

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "added": added,
            "errors": errors,
        })),
    ))
}

/// `DELETE /api/channels/{channel_id}/members/{pubkey}` — Remove a member.
///
/// Actor must be an owner/admin, or removing themselves.
pub async fn remove_member(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((channel_id, pubkey)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::AdminChannels)
        .map_err(scope_error)?;
    let actor_bytes = ctx.pubkey_bytes.clone();

    let channel_id = Uuid::parse_str(&channel_id)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid channel_id"))?;
    check_token_channel_access(&ctx, &channel_id)?;

    let target_bytes = hex::decode(&pubkey)
        .ok()
        .filter(|b| b.len() == 32)
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "invalid pubkey"))?;

    let is_self_remove = target_bytes == actor_bytes;
    if !is_self_remove {
        require_owner_or_admin(&state, channel_id, &actor_bytes).await?;
    }

    // Reject membership changes on archived channels.
    // NOTE: This intentionally blocks self-removal too. If a user is stuck in
    // an archived channel, an admin must unarchive first. The `leave_channel`
    // endpoint has the same restriction for consistency.
    let channel = state
        .db
        .get_channel(channel_id)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;
    if channel.archived_at.is_some() {
        return Err(api_error(StatusCode::FORBIDDEN, "channel is archived"));
    }

    // Prevent last-owner orphaning on self-removal.
    if is_self_remove {
        let members = state
            .db
            .get_members(channel_id)
            .await
            .map_err(|e| internal_error(&format!("db error: {e}")))?;
        let owner_count = members.iter().filter(|m| m.role == "owner").count();
        let actor_is_owner = members
            .iter()
            .any(|m| m.pubkey == actor_bytes && m.role == "owner");
        if actor_is_owner && owner_count <= 1 {
            return Err(api_error(
                StatusCode::CONFLICT,
                "cannot remove the last owner — transfer ownership first",
            ));
        }
    }

    state
        .db
        .remove_member(channel_id, &target_bytes, &actor_bytes)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    let actor_hex = nostr_hex::encode(&actor_bytes);
    let target_hex = nostr_hex::encode(&target_bytes);
    let msg_type = if target_bytes == actor_bytes {
        "member_left"
    } else {
        "member_removed"
    };
    if let Err(e) = emit_system_message(
        &state,
        channel_id,
        serde_json::json!({
            "type": msg_type,
            "actor": actor_hex,
            "target": target_hex,
        }),
    )
    .await
    {
        tracing::warn!("Failed to emit system message: {e}");
    }

    Ok(Json(serde_json::json!({ "removed": true })))
}

/// `GET /api/channels/{channel_id}/members` — List members of a channel.
///
/// Requires channel membership or open visibility.
pub async fn list_members(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::ChannelsRead)
        .map_err(scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();

    let channel_id = Uuid::parse_str(&channel_id)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid channel_id"))?;
    check_token_channel_access(&ctx, &channel_id)?;

    check_channel_access(&state, channel_id, &pubkey_bytes).await?;

    let members = state
        .db
        .get_members(channel_id)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    // Resolve display names in bulk.
    let member_pubkeys: Vec<Vec<u8>> = members.iter().map(|m| m.pubkey.clone()).collect();
    let user_records = state
        .db
        .get_users_bulk(&member_pubkeys)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("list_members: failed to load user records: {e}");
            vec![]
        });

    let display_name_map: HashMap<String, String> = user_records
        .into_iter()
        .filter_map(|u| {
            let hex = nostr_hex::encode(&u.pubkey);
            u.display_name.map(|name| (hex, name))
        })
        .collect();

    let result: Vec<serde_json::Value> = members
        .iter()
        .map(|m| {
            let hex = nostr_hex::encode(&m.pubkey);
            let display_name = display_name_map.get(&hex).cloned();
            serde_json::json!({
                "pubkey": hex,
                "role": m.role,
                "joined_at": m.joined_at.to_rfc3339(),
                "display_name": display_name,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "members": result,
        "next_cursor": serde_json::Value::Null,
    })))
}

/// `POST /api/channels/{channel_id}/join` — Self-join an open channel.
///
/// Only works for channels with `visibility = "open"`.
pub async fn join_channel(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::ChannelsRead)
        .map_err(scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();

    let channel_id = Uuid::parse_str(&channel_id)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid channel_id"))?;

    // Only open channels allow self-join.
    let channel = state
        .db
        .get_channel(channel_id)
        .await
        .map_err(|_| api_error(StatusCode::NOT_FOUND, "channel not found"))?;

    if channel.visibility != "open" {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            "channel is private — request an invitation",
        ));
    }

    // Reject writes to archived channels.
    if channel.archived_at.is_some() {
        return Err(api_error(StatusCode::FORBIDDEN, "channel is archived"));
    }

    state
        .db
        .add_member(channel_id, &pubkey_bytes, MemberRole::Member, None)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    let actor_hex = nostr_hex::encode(&pubkey_bytes);
    if let Err(e) = emit_system_message(
        &state,
        channel_id,
        serde_json::json!({
            "type": "member_joined",
            "actor": actor_hex,
            "target": actor_hex,
        }),
    )
    .await
    {
        tracing::warn!("Failed to emit system message: {e}");
    }

    Ok(Json(serde_json::json!({
        "joined": true,
        "role": "member",
    })))
}

/// `POST /api/channels/{channel_id}/leave` — Self-leave a channel.
///
/// Returns 409 if the actor is the last owner (must transfer ownership first).
pub async fn leave_channel(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::ChannelsRead)
        .map_err(scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();

    let channel_id = Uuid::parse_str(&channel_id)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid channel_id"))?;

    let channel = state
        .db
        .get_channel(channel_id)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;
    if channel.archived_at.is_some() {
        return Err(api_error(StatusCode::FORBIDDEN, "channel is archived"));
    }

    // Guard: if actor is the last owner, block the leave.
    let members = state
        .db
        .get_members(channel_id)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    let owner_count = members.iter().filter(|m| m.role == "owner").count();
    let actor_is_owner = members
        .iter()
        .any(|m| m.pubkey == pubkey_bytes && m.role == "owner");

    if actor_is_owner && owner_count <= 1 {
        return Err(api_error(
            StatusCode::CONFLICT,
            "owner must transfer ownership before leaving",
        ));
    }

    state
        .db
        .remove_member(channel_id, &pubkey_bytes, &pubkey_bytes)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    let actor_hex = nostr_hex::encode(&pubkey_bytes);
    if let Err(e) = emit_system_message(
        &state,
        channel_id,
        serde_json::json!({
            "type": "member_left",
            "actor": actor_hex,
        }),
    )
    .await
    {
        tracing::warn!("Failed to emit system message: {e}");
    }

    Ok(Json(serde_json::json!({ "left": true })))
}
