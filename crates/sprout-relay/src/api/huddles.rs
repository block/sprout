//! LiveKit huddle token endpoint.
//!
//! ## Routes
//! - `POST /api/huddles/{channel_id}/token` — generate a LiveKit access token
//!   for the authenticated user to join the channel's huddle room.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use uuid::Uuid;

use super::{
    check_channel_membership, check_token_channel_access, extract_auth_context, internal_error,
    scope_error,
};
use crate::state::AppState;

/// `POST /api/huddles/{channel_id}/token` — generate a LiveKit access token.
///
/// Returns `{ "token": "<jwt>", "url": "<livekit_url>", "room": "sprout-<channel_id>" }`.
/// Returns 501 if LiveKit is not configured on this relay.
/// Returns 403 if the caller is not a member of the channel.
pub async fn huddle_token(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::MessagesRead)
        .map_err(scope_error)?;
    check_token_channel_access(&ctx, &channel_id)?;

    // Require LiveKit to be configured.
    let huddle_service = state.huddle_service.as_ref().ok_or_else(|| {
        (
            StatusCode::NOT_IMPLEMENTED,
            Json(serde_json::json!({
                "error": "huddles_not_configured",
                "message": "LiveKit is not configured on this relay"
            })),
        )
    })?;

    // Verify the caller is a member of the channel (or it's an open channel).
    check_channel_membership(&state, channel_id, &ctx.pubkey_bytes).await?;

    // Generate the LiveKit token.
    let room = sprout_huddle::HuddleService::create_room_name(channel_id);
    let identity = ctx.pubkey.to_hex();
    let lk_token = huddle_service
        .generate_token(&room, &identity, &identity)
        .map_err(|e| internal_error(&format!("token generation failed: {e}")))?;

    let livekit_url = state.livekit_url.as_deref().unwrap_or_default();

    if livekit_url.is_empty() {
        return Err(internal_error("livekit_url is not configured"));
    }

    Ok(Json(serde_json::json!({
        "token": lk_token.token,
        "url": livekit_url,
        "room": room,
    })))
}
