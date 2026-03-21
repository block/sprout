//! Channel metadata REST API handlers.
//!
//! Endpoints:
//!   GET  /api/channels/{channel_id}           — Get channel details
//!   PUT  /api/channels/{channel_id}           — Update channel name/description
//!   PUT  /api/channels/{channel_id}/topic     — Set channel topic
//!   PUT  /api/channels/{channel_id}/purpose   — Set channel purpose
//!   POST /api/channels/{channel_id}/archive   — Archive a channel
//!   POST /api/channels/{channel_id}/unarchive — Unarchive a channel
//!
//! NOTE: These handlers call `state.db.*` methods that are wired through
//! `sprout-db/src/lib.rs` by the orchestrator:
//!   - `state.db.get_channel_detail(channel_id)` → channel::get_channel
//!   - `state.db.update_channel(channel_id, updates)` → channel::update_channel
//!   - `state.db.set_topic(channel_id, topic, set_by)` → channel::set_topic
//!   - `state.db.set_purpose(channel_id, purpose, set_by)` → channel::set_purpose
//!   - `state.db.archive_channel(channel_id)` → channel::archive_channel
//!   - `state.db.unarchive_channel(channel_id)` → channel::unarchive_channel
//!   - `state.db.get_member_count(channel_id)` → channel::get_member_count
//!   - `state.db.get_member_role(channel_id, pubkey)` → channel::get_member_role

use std::sync::Arc;

use axum::{
    extract::{Json as ExtractJson, Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use nostr::util::hex as nostr_hex;
use serde::Deserialize;
use sprout_db::channel::{ChannelRecord, ChannelUpdate};

use crate::handlers::side_effects::emit_system_message;
use crate::state::AppState;

use super::{
    api_error, check_channel_access, check_token_channel_access, extract_auth_context, forbidden,
    internal_error, not_found, scope_error,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Parse a channel_id path parameter as a UUID.
fn parse_channel_id(raw: &str) -> Result<uuid::Uuid, (StatusCode, Json<serde_json::Value>)> {
    uuid::Uuid::parse_str(raw).map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid channel_id"))
}

/// Serialize a `ChannelRecord` to JSON, including topic, purpose, and member_count.
fn channel_detail_to_json(record: &ChannelRecord, member_count: i64) -> serde_json::Value {
    serde_json::json!({
        "id": record.id.to_string(),
        "name": record.name,
        "channel_type": record.channel_type,
        "visibility": record.visibility,
        "description": record.description,
        "topic": record.topic,
        "topic_set_by": record.topic_set_by.as_deref().map(nostr_hex::encode),
        "topic_set_at": record.topic_set_at.map(|t| t.to_rfc3339()),
        "purpose": record.purpose,
        "purpose_set_by": record.purpose_set_by.as_deref().map(nostr_hex::encode),
        "purpose_set_at": record.purpose_set_at.map(|t| t.to_rfc3339()),
        "created_by": nostr_hex::encode(&record.created_by),
        "created_at": record.created_at.to_rfc3339(),
        "updated_at": record.updated_at.to_rfc3339(),
        "archived_at": record.archived_at.map(|t| t.to_rfc3339()),
        "member_count": member_count,
        "topic_required": record.topic_required,
        "max_members": record.max_members,
        "nip29_group_id": record.nip29_group_id,
    })
}

/// Check that the actor is an owner or admin of the channel.
///
/// Returns `Err(403)` if the actor is not a member or lacks an elevated role.
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
        Some(_) => Err(forbidden("owner or admin role required")),
        None => Err(forbidden("not a member of this channel")),
    }
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// GET /api/channels/{channel_id} — Get full channel details.
///
/// Requires the caller to be a member or the channel to be open.
pub async fn get_channel_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel_id_str): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::ChannelsRead)
        .map_err(scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();
    let channel_id = parse_channel_id(&channel_id_str)?;
    check_token_channel_access(&ctx, &channel_id)?;

    check_channel_access(&state, channel_id, &pubkey_bytes).await?;

    let record = state
        .db
        .get_channel(channel_id)
        .await
        .map_err(|e| match e {
            sprout_db::error::DbError::ChannelNotFound(_) => not_found("channel not found"),
            other => internal_error(&format!("db error: {other}")),
        })?;

    let member_count = state
        .db
        .get_member_count(channel_id)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    Ok(Json(channel_detail_to_json(&record, member_count)))
}

/// Request body for updating channel name/description.
#[derive(Debug, Deserialize)]
pub struct UpdateChannelBody {
    /// New channel name (optional).
    pub name: Option<String>,
    /// New channel description (optional).
    pub description: Option<String>,
}

/// Update channel properties (name, description).
///
/// Requires owner or admin role. Topic and purpose are settable by any member
/// via separate endpoints — see `channels_metadata.rs`. This asymmetry is
/// intentional: name and description are structural metadata, while topic and
/// purpose are collaborative content metadata (NIP-29 parity).
pub async fn update_channel_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel_id_str): Path<String>,
    ExtractJson(body): ExtractJson<UpdateChannelBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::ChannelsWrite)
        .map_err(scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();
    let channel_id = parse_channel_id(&channel_id_str)?;
    check_token_channel_access(&ctx, &channel_id)?;

    require_owner_or_admin(&state, channel_id, &pubkey_bytes).await?;

    // Reject writes to archived channels.
    let channel = state
        .db
        .get_channel(channel_id)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;
    if channel.archived_at.is_some() {
        return Err(api_error(StatusCode::FORBIDDEN, "channel is archived"));
    }

    if body.name.is_none() && body.description.is_none() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "at least one of name or description must be provided",
        ));
    }

    let name = body
        .name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty());
    let description = body
        .description
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty());

    // Re-check after trimming: whitespace-only values collapse to None.
    if name.is_none() && description.is_none() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "at least one field must be provided (non-empty)",
        ));
    }

    let name_changed = name.is_some();
    let new_name = name.clone();

    let record = state
        .db
        .update_channel(channel_id, ChannelUpdate { name, description })
        .await
        .map_err(|e| match e {
            sprout_db::error::DbError::ChannelNotFound(_) => not_found("channel not found"),
            sprout_db::error::DbError::InvalidData(msg) => api_error(StatusCode::BAD_REQUEST, &msg),
            other => internal_error(&format!("db error: {other}")),
        })?;

    if name_changed {
        let actor_hex = nostr_hex::encode(&pubkey_bytes);
        if let Err(e) = emit_system_message(
            &state,
            channel_id,
            serde_json::json!({
                "type": "channel_renamed",
                "actor": actor_hex,
                "name": new_name,
            }),
        )
        .await
        {
            tracing::warn!("Failed to emit system message: {e}");
        }
    }

    let member_count = state
        .db
        .get_member_count(channel_id)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    Ok(Json(channel_detail_to_json(&record, member_count)))
}

/// Request body for setting the channel topic.
#[derive(Debug, Deserialize)]
pub struct SetTopicBody {
    /// The new topic text.
    pub topic: String,
}

/// PUT /api/channels/{channel_id}/topic — Set the channel topic.
///
/// Any active member may set the topic.
pub async fn set_topic_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel_id_str): Path<String>,
    ExtractJson(body): ExtractJson<SetTopicBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::ChannelsWrite)
        .map_err(scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();
    let channel_id = parse_channel_id(&channel_id_str)?;
    check_token_channel_access(&ctx, &channel_id)?;

    check_channel_access(&state, channel_id, &pubkey_bytes).await?;

    // Reject writes to archived channels.
    let channel = state
        .db
        .get_channel(channel_id)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;
    if channel.archived_at.is_some() {
        return Err(api_error(StatusCode::FORBIDDEN, "channel is archived"));
    }

    let topic = body.topic.trim().to_string();
    if topic.is_empty() {
        return Err(api_error(StatusCode::BAD_REQUEST, "topic cannot be empty"));
    }

    state
        .db
        .set_topic(channel_id, &topic, &pubkey_bytes)
        .await
        .map_err(|e| match e {
            sprout_db::error::DbError::ChannelNotFound(_) => not_found("channel not found"),
            other => internal_error(&format!("db error: {other}")),
        })?;

    let actor_hex = nostr_hex::encode(&pubkey_bytes);
    if let Err(e) = emit_system_message(
        &state,
        channel_id,
        serde_json::json!({
            "type": "topic_changed",
            "actor": actor_hex,
            "topic": topic,
        }),
    )
    .await
    {
        tracing::warn!("Failed to emit system message: {e}");
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Request body for setting the channel purpose.
#[derive(Debug, Deserialize)]
pub struct SetPurposeBody {
    /// The new purpose text.
    pub purpose: String,
}

/// PUT /api/channels/{channel_id}/purpose — Set the channel purpose.
///
/// Any active member may set the purpose.
pub async fn set_purpose_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel_id_str): Path<String>,
    ExtractJson(body): ExtractJson<SetPurposeBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::ChannelsWrite)
        .map_err(scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();
    let channel_id = parse_channel_id(&channel_id_str)?;
    check_token_channel_access(&ctx, &channel_id)?;

    check_channel_access(&state, channel_id, &pubkey_bytes).await?;

    // Reject writes to archived channels.
    let channel = state
        .db
        .get_channel(channel_id)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;
    if channel.archived_at.is_some() {
        return Err(api_error(StatusCode::FORBIDDEN, "channel is archived"));
    }

    let purpose = body.purpose.trim().to_string();
    if purpose.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "purpose cannot be empty",
        ));
    }

    state
        .db
        .set_purpose(channel_id, &purpose, &pubkey_bytes)
        .await
        .map_err(|e| match e {
            sprout_db::error::DbError::ChannelNotFound(_) => not_found("channel not found"),
            other => internal_error(&format!("db error: {other}")),
        })?;

    let actor_hex = nostr_hex::encode(&pubkey_bytes);
    if let Err(e) = emit_system_message(
        &state,
        channel_id,
        serde_json::json!({
            "type": "purpose_changed",
            "actor": actor_hex,
            "purpose": purpose,
        }),
    )
    .await
    {
        tracing::warn!("Failed to emit system message: {e}");
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// POST /api/channels/{channel_id}/archive — Archive a channel.
///
/// Requires owner or admin role.
/// Returns 409 Conflict if the channel is already archived.
pub async fn archive_channel_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel_id_str): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::AdminChannels)
        .map_err(scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();
    let channel_id = parse_channel_id(&channel_id_str)?;
    check_token_channel_access(&ctx, &channel_id)?;

    require_owner_or_admin(&state, channel_id, &pubkey_bytes).await?;

    state
        .db
        .archive_channel(channel_id)
        .await
        .map_err(|e| match e {
            sprout_db::error::DbError::ChannelNotFound(_) => not_found("channel not found"),
            sprout_db::error::DbError::AccessDenied(msg) => api_error(StatusCode::CONFLICT, &msg),
            other => internal_error(&format!("db error: {other}")),
        })?;

    let actor_hex = nostr_hex::encode(&pubkey_bytes);
    if let Err(e) = emit_system_message(
        &state,
        channel_id,
        serde_json::json!({
            "type": "channel_archived",
            "actor": actor_hex,
        }),
    )
    .await
    {
        tracing::warn!("Failed to emit system message: {e}");
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// POST /api/channels/{channel_id}/unarchive — Unarchive a channel.
///
/// Requires owner or admin role.
/// Returns 409 Conflict if the channel is not currently archived.
pub async fn unarchive_channel_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel_id_str): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::AdminChannels)
        .map_err(scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();
    let channel_id = parse_channel_id(&channel_id_str)?;
    check_token_channel_access(&ctx, &channel_id)?;

    require_owner_or_admin(&state, channel_id, &pubkey_bytes).await?;

    state
        .db
        .unarchive_channel(channel_id)
        .await
        .map_err(|e| match e {
            sprout_db::error::DbError::ChannelNotFound(_) => not_found("channel not found"),
            sprout_db::error::DbError::AccessDenied(msg) => api_error(StatusCode::CONFLICT, &msg),
            other => internal_error(&format!("db error: {other}")),
        })?;

    let actor_hex = nostr_hex::encode(&pubkey_bytes);
    if let Err(e) = emit_system_message(
        &state,
        channel_id,
        serde_json::json!({
            "type": "channel_unarchived",
            "actor": actor_hex,
        }),
    )
    .await
    {
        tracing::warn!("Failed to emit system message: {e}");
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// DELETE /api/channels/{channel_id} — Soft-delete a channel.
///
/// Requires owner role. Sets `deleted_at` on the channel record; the channel
/// becomes invisible to all queries that filter on `deleted_at IS NULL`.
pub async fn delete_channel_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel_id_str): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::AdminChannels)
        .map_err(scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();
    let channel_id = parse_channel_id(&channel_id_str)?;
    check_token_channel_access(&ctx, &channel_id)?;

    // Only channel owners may delete.
    let role = state
        .db
        .get_member_role(channel_id, &pubkey_bytes)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;
    match role.as_deref() {
        Some("owner") => {}
        Some(_) => return Err(forbidden("only channel owner can delete")),
        None => return Err(forbidden("not a member of this channel")),
    }

    let (deleted, removed_members) = state
        .db
        .delete_channel_and_members(channel_id)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    if !deleted {
        return Err(api_error(StatusCode::CONFLICT, "channel already deleted"));
    }

    tracing::info!(%channel_id, removed_members, "channel deleted with member cleanup");

    let actor_hex = nostr_hex::encode(&pubkey_bytes);
    if let Err(e) = emit_system_message(
        &state,
        channel_id,
        serde_json::json!({
            "type": "channel_deleted",
            "actor": actor_hex,
        }),
    )
    .await
    {
        tracing::warn!("Failed to emit system message: {e}");
    }

    Ok(Json(serde_json::json!({ "ok": true, "deleted": true })))
}
