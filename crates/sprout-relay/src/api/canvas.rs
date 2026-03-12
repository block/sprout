//! Canvas REST API.
//!
//! Endpoints:
//!   GET /api/channels/:channel_id/canvas  — fetch the most recent canvas for a channel
//!   PUT /api/channels/:channel_id/canvas  — set/update the canvas for a channel
//!
//! Canvas events are Nostr events of kind KIND_CANVAS (40100) with an "h" tag
//! scoped to the channel UUID. The relay signs these events on behalf of the
//! authenticated user (same pattern as `messages.rs`).

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use nostr::util::hex as nostr_hex;
use nostr::{EventBuilder, Kind, Tag};
use serde::Deserialize;
use sprout_core::kind::KIND_CANVAS;
use sprout_db::event::EventQuery;

use crate::handlers::event::dispatch_persistent_event;
use crate::state::AppState;

use super::{
    api_error, check_channel_access, check_token_channel_access, extract_auth_context,
    internal_error,
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
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::ChannelsRead)
        .map_err(super::scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();

    let channel_id = uuid::Uuid::parse_str(&channel_id_str)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid channel UUID"))?;

    check_token_channel_access(&ctx, &channel_id)?;
    check_channel_access(&state, channel_id, &pubkey_bytes).await?;

    // Query the events table for the most recent KIND_CANVAS event scoped to
    // this channel. `channel_id` is stored as a column on the events row, so
    // we can filter directly without scanning tags.
    let q = EventQuery {
        channel_id: Some(channel_id),
        kinds: Some(vec![KIND_CANVAS as i32]),
        pubkey: None,
        since: None,
        until: None,
        limit: Some(1),
        offset: None,
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

// ── PUT /api/channels/:channel_id/canvas ─────────────────────────────────────

/// Request body for setting the canvas.
#[derive(Debug, Deserialize)]
pub struct SetCanvasBody {
    /// New canvas content (Markdown or plain text).
    pub content: String,
}

/// Set or update the canvas for a channel.
///
/// Creates a Nostr event of kind KIND_CANVAS signed by the relay keypair,
/// attributed to the authenticated user via a `p` tag. Fans out to WebSocket
/// subscribers so live clients receive the update immediately.
pub async fn set_canvas(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel_id_str): Path<String>,
    Json(body): Json<SetCanvasBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::ChannelsWrite)
        .map_err(super::scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();

    let channel_id = uuid::Uuid::parse_str(&channel_id_str)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid channel UUID"))?;

    check_token_channel_access(&ctx, &channel_id)?;
    check_channel_access(&state, channel_id, &pubkey_bytes).await?;

    // Reject writes to archived channels (consistent with messages, metadata, etc.).
    let channel = state
        .db
        .get_channel(channel_id)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;
    if channel.archived_at.is_some() {
        return Err(api_error(StatusCode::FORBIDDEN, "channel is archived"));
    }

    // Attribution: real author carried in a `p` tag; event is relay-signed.
    let user_pubkey_hex = nostr_hex::encode(&pubkey_bytes);

    let tags: Vec<Tag> = vec![
        // Real sender attribution (mirrors messages.rs pattern).
        Tag::parse(&["p", &user_pubkey_hex])
            .map_err(|e| internal_error(&format!("tag build error: {e}")))?,
        // NIP-29 channel scope tag.
        Tag::parse(&["h", &channel_id.to_string()])
            .map_err(|e| internal_error(&format!("tag build error: {e}")))?,
    ];

    let kind = Kind::from(KIND_CANVAS as u16);

    let event = EventBuilder::new(kind, &body.content, tags)
        .sign_with_keys(&state.relay_keypair)
        .map_err(|e| internal_error(&format!("event signing error: {e}")))?;

    let event_id_hex = event.id.to_hex();

    let (stored_event, was_inserted) = state
        .db
        .insert_event(&event, Some(channel_id))
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    if was_inserted {
        let _ =
            dispatch_persistent_event(&state, &stored_event, KIND_CANVAS, &user_pubkey_hex).await;
    }

    Ok(Json(serde_json::json!({
        "ok":       true,
        "event_id": event_id_hex,
    })))
}
