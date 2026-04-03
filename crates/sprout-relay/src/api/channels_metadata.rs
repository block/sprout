//! Channel metadata REST API handlers.
//!
//! Endpoints:
//!   GET  /api/channels/{channel_id}           — Get channel details

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::Json,
};
use nostr::util::hex as nostr_hex;
use sprout_db::channel::ChannelRecord;

use crate::state::AppState;

use super::{
    check_channel_access, check_token_channel_access, extract_auth_context, internal_error,
    not_found, scope_error, ApiError,
};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Parse a channel_id path parameter as a UUID.
fn parse_channel_id(raw: &str) -> Result<uuid::Uuid, ApiError> {
    uuid::Uuid::parse_str(raw).map_err(|_| ApiError::BadRequest("invalid channel_id".into()))
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

// ── Handlers ──────────────────────────────────────────────────────────────────

/// GET /api/channels/{channel_id} — Get full channel details.
///
/// Requires the caller to be a member or the channel to be open.
pub async fn get_channel_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel_id_str): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
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
