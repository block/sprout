//! Direct Message REST API.
//!
//! Endpoints:
//!   POST /api/dms                      — Open or create a DM (idempotent)
//!   POST /api/dms/{channel_id}/members — Add member to group DM (creates new DM)
//!   GET  /api/dms                      — List user's DM conversations

use std::sync::Arc;

use axum::{
    extract::{Json as ExtractJson, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use nostr::util::hex as nostr_hex;
use serde::Deserialize;
use uuid::Uuid;

use crate::handlers::side_effects::emit_system_message;
use crate::state::AppState;

use super::{api_error, extract_auth_context, internal_error};

// ── Request / query types ─────────────────────────────────────────────────────

/// Request body for opening a DM.
#[derive(Debug, Deserialize)]
pub struct OpenDmBody {
    /// Hex-encoded pubkeys of the OTHER participants (self is added automatically).
    /// Must contain 1–8 entries (self brings the total to 2–9).
    pub pubkeys: Vec<String>,
}

/// Request body for adding a member to a group DM.
#[derive(Debug, Deserialize)]
pub struct AddDmMemberBody {
    /// Hex-encoded pubkeys of the new participants to add.
    pub pubkeys: Vec<String>,
}

/// Query parameters for listing DMs.
#[derive(Debug, Deserialize)]
pub struct ListDmsQuery {
    /// Pagination cursor (channel_id of the last item from the previous page).
    pub cursor: Option<String>,
    /// Maximum number of results to return (default 50, max 200).
    pub limit: Option<u32>,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `POST /api/dms` — Open or create a DM conversation.
///
/// The caller is automatically added as a participant. The operation is
/// idempotent: the same participant set always returns the same channel.
pub async fn open_dm_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    ExtractJson(body): ExtractJson<OpenDmBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::MessagesWrite)
        .map_err(super::scope_error)?;
    let self_bytes = ctx.pubkey_bytes.clone();

    if body.pubkeys.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "pubkeys must contain at least 1 other participant",
        ));
    }
    if body.pubkeys.len() > 8 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "pubkeys may contain at most 8 other participants (9 total including self)",
        ));
    }

    // Decode all provided pubkeys.
    let mut other_bytes: Vec<Vec<u8>> = Vec::with_capacity(body.pubkeys.len());
    for hex in &body.pubkeys {
        let bytes = hex::decode(hex).map_err(|_| {
            api_error(
                StatusCode::BAD_REQUEST,
                &format!("invalid pubkey hex: {hex}"),
            )
        })?;
        if bytes.len() != 32 {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                &format!("pubkey must be 32 bytes (64 hex chars): {hex}"),
            ));
        }
        other_bytes.push(bytes);
    }

    // Build the full participant slice (self + others).
    let mut all_bytes: Vec<Vec<u8>> = vec![self_bytes.clone()];
    for ob in &other_bytes {
        if !all_bytes.iter().any(|b| b == ob) {
            all_bytes.push(ob.clone());
        }
    }

    let all_refs: Vec<&[u8]> = all_bytes.iter().map(|b| b.as_slice()).collect();

    let (channel, was_created) = state
        .db
        .open_dm(&all_refs, &self_bytes)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    if was_created {
        let actor_hex = nostr_hex::encode(&self_bytes);
        let participant_hexes: Vec<String> = all_bytes.iter().map(nostr_hex::encode).collect();
        if let Err(e) = emit_system_message(
            &state,
            channel.id,
            serde_json::json!({
                "type": "dm_created",
                "actor": actor_hex,
                "participants": participant_hexes,
            }),
        )
        .await
        {
            tracing::warn!("Failed to emit system message: {e}");
        }
    }

    // Resolve participant display names.
    let participants = resolve_participants(&state, channel.id).await;

    let status = if was_created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };

    Ok((
        status,
        Json(serde_json::json!({
            "channel_id": channel.id.to_string(),
            "created": was_created,
            "participants": participants,
        })),
    ))
}

/// `POST /api/dms/{channel_id}/members` — Add a member to a group DM.
///
/// Because DM participant sets are immutable, this creates a NEW DM with the
/// expanded participant set. The original DM is not modified.
pub async fn add_dm_member_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel_id_str): Path<String>,
    ExtractJson(body): ExtractJson<AddDmMemberBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::MessagesWrite)
        .map_err(super::scope_error)?;
    let self_bytes = ctx.pubkey_bytes.clone();

    let channel_id = Uuid::parse_str(&channel_id_str)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid channel_id format"))?;

    if body.pubkeys.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "pubkeys must contain at least 1 new participant",
        ));
    }

    // Verify caller is a member of the existing DM.
    let is_member = state
        .db
        .is_member(channel_id, &self_bytes)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;
    if !is_member {
        return Err(super::forbidden("not a member of this DM"));
    }

    // Verify the channel is actually a DM.
    let existing_channel = state
        .db
        .get_channel(channel_id)
        .await
        .map_err(|_| super::not_found("DM not found"))?;
    if existing_channel.channel_type != "dm" {
        return Err(api_error(StatusCode::BAD_REQUEST, "channel is not a DM"));
    }

    // Get existing participants.
    let existing_members = state
        .db
        .get_members(channel_id)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    let mut all_bytes: Vec<Vec<u8>> = existing_members.into_iter().map(|m| m.pubkey).collect();

    // Decode and merge new pubkeys.
    for hex in &body.pubkeys {
        let bytes = hex::decode(hex).map_err(|_| {
            api_error(
                StatusCode::BAD_REQUEST,
                &format!("invalid pubkey hex: {hex}"),
            )
        })?;
        if bytes.len() != 32 {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                &format!("pubkey must be 32 bytes (64 hex chars): {hex}"),
            ));
        }
        if !all_bytes.iter().any(|b| b == &bytes) {
            all_bytes.push(bytes);
        }
    }

    // Enforce max 9 participants.
    if all_bytes.len() > 9 {
        return Err(api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "DM supports at most 9 participants",
        ));
    }

    let all_refs: Vec<&[u8]> = all_bytes.iter().map(|b| b.as_slice()).collect();

    let (new_channel, was_created) = state
        .db
        .open_dm(&all_refs, &self_bytes)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    let participants = resolve_participants(&state, new_channel.id).await;

    let status = if was_created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };

    Ok((
        status,
        Json(serde_json::json!({
            "channel_id": new_channel.id.to_string(),
            "created": was_created,
            "participants": participants,
            "note": "A new DM was created with the expanded participant set. The original DM is unchanged.",
        })),
    ))
}

/// `GET /api/dms` — List the authenticated user's DM conversations.
///
/// Returns DMs ordered by most recent activity (updated_at DESC).
/// Supports cursor-based pagination.
pub async fn list_dms_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<ListDmsQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::MessagesRead)
        .map_err(super::scope_error)?;
    let self_bytes = ctx.pubkey_bytes.clone();

    let limit = params.limit.unwrap_or(50).min(200);

    let cursor = params
        .cursor
        .as_deref()
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid cursor format"))?;

    let dms = state
        .db
        .list_dms_for_user(&self_bytes, limit, cursor)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    let next_cursor = dms.last().map(|d| d.channel_id.to_string());

    let dm_json: Vec<serde_json::Value> = dms
        .iter()
        .map(|dm| {
            let participants: Vec<serde_json::Value> = dm
                .participants
                .iter()
                .map(|p| {
                    serde_json::json!({
                        "pubkey": nostr_hex::encode(&p.pubkey),
                        "display_name": p.display_name,
                        "role": p.role,
                    })
                })
                .collect();

            serde_json::json!({
                "channel_id": dm.channel_id.to_string(),
                "participants": participants,
                "last_message_at": dm.last_message_at.map(|t| t.to_rfc3339()),
                "created_at": dm.created_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "dms": dm_json,
        "next_cursor": next_cursor,
    })))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Fetch and format participant info for a DM channel.
async fn resolve_participants(state: &AppState, channel_id: Uuid) -> Vec<serde_json::Value> {
    let members = state.db.get_members(channel_id).await.unwrap_or_else(|e| {
        tracing::error!("dms: failed to load members for channel {channel_id}: {e}");
        vec![]
    });

    let member_pubkeys: Vec<Vec<u8>> = members.iter().map(|m| m.pubkey.clone()).collect();

    let user_records = state
        .db
        .get_users_bulk(&member_pubkeys)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("dms: failed to load user records for DM participants: {e}");
            vec![]
        });

    let user_map: std::collections::HashMap<String, Option<String>> = user_records
        .into_iter()
        .map(|u| (nostr_hex::encode(&u.pubkey), u.display_name))
        .collect();

    members
        .iter()
        .map(|m| {
            let hex = nostr_hex::encode(&m.pubkey);
            let display_name = user_map.get(&hex).and_then(|n| n.clone());
            serde_json::json!({
                "pubkey": hex,
                "display_name": display_name,
                "role": m.role,
            })
        })
        .collect()
}
