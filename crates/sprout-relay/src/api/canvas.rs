//! Canvas REST API.
//!
//! Endpoints:
//!   GET /api/channels/:channel_id/canvas  — fetch the most recent canvas for a channel

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::Json,
};
use nostr::util::hex as nostr_hex;
use sprout_core::kind::KIND_CANVAS;
use sprout_db::event::EventQuery;

use crate::state::AppState;

use super::{
    api_error, check_channel_access, check_token_channel_access, extract_auth_context,
    internal_error, ApiError,
};

// ── GET /api/channels/:channel_id/canvas ─────────────────────────────────────

/// Fetch the most recent canvas for a channel.
///
/// Returns the canvas content, author pubkey (hex), and updated_at timestamp
/// (Unix seconds) if a canvas exists, or `{"content": null}` if none has been set.
pub async fn get_canvas(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel_id_str): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::ChannelsRead)
        .map_err(super::scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();

    let channel_id = uuid::Uuid::parse_str(&channel_id_str)
        .map_err(|_| api_error(axum::http::StatusCode::BAD_REQUEST, "invalid channel UUID"))?;

    check_token_channel_access(&ctx, &channel_id)?;
    check_channel_access(&state, channel_id, &pubkey_bytes).await?;

    // Query the events table for the most recent KIND_CANVAS event scoped to
    // this channel. `channel_id` is stored as a column on the events row, so
    // we can filter directly without scanning tags.
    let q = EventQuery {
        channel_id: Some(channel_id),
        kinds: Some(vec![KIND_CANVAS as i32]),
        limit: Some(1),
        ..Default::default()
    };

    let events = state
        .db
        .query_events(&q)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    match events.into_iter().next() {
        Some(stored) => {
            // Canvas events are relay-signed; the real author is in the first `p` tag.
            let author_hex = stored
                .event
                .tags
                .find(nostr::TagKind::SingleLetter(
                    nostr::SingleLetterTag::lowercase(nostr::Alphabet::P),
                ))
                .and_then(|t| t.content().map(|s| s.to_string()))
                .unwrap_or_else(|| nostr_hex::encode(stored.event.pubkey.serialize()));
            let updated_at = stored.event.created_at.as_u64() as i64;
            Ok(Json(serde_json::json!({
                "content":    stored.event.content,
                "updated_at": updated_at,
                "author":     author_hex,
            })))
        }
        None => Ok(Json(serde_json::json!({ "content": null }))),
    }
}
