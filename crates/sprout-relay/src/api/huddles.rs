//! LiveKit huddle token endpoint.
//!
//! ## Routes
//! - `POST /api/huddles/{channel_id}/token` — generate a LiveKit access token
//!   for the authenticated user to join the channel's huddle room.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use uuid::Uuid;

use super::{
    check_channel_membership, check_token_channel_access, extract_auth_context, internal_error,
    scope_error,
};
use crate::state::AppState;

/// Query parameters for [`huddle_token`].
#[derive(serde::Deserialize)]
pub struct HuddleTokenQuery {
    /// The parent (non-ephemeral) channel this huddle belongs to.
    ///
    /// When provided and the caller is a member of the parent channel, the relay
    /// will auto-add them to the private ephemeral huddle channel so they can
    /// obtain a token without requiring an explicit invite.
    pub parent_channel_id: Option<Uuid>,
}

/// `POST /api/huddles/{channel_id}/token` — generate a LiveKit access token.
///
/// Returns `{ "token": "<jwt>", "url": "<livekit_url>", "room": "sprout-<channel_id>" }`.
/// Returns 501 if LiveKit is not configured on this relay.
/// Returns 403 if the caller is not a member of the channel.
pub async fn huddle_token(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel_id): Path<Uuid>,
    Query(query): Query<HuddleTokenQuery>,
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
    // If they're not a member, attempt relay-side auto-add when:
    //   1. parent_channel_id was provided
    //   2. The target channel is private + ephemeral (ttl_seconds IS NOT NULL)
    //   3. The caller IS a member of the parent channel
    let membership_result = check_channel_membership(&state, channel_id, &ctx.pubkey_bytes).await;

    if let Err(ref membership_err) = membership_result {
        if let Some(parent_id) = query.parent_channel_id {
            // Gate 1: caller must be a member of the parent channel.
            check_channel_membership(&state, parent_id, &ctx.pubkey_bytes).await?;

            // Gate 2: target channel must be private + ephemeral.
            let channel = state
                .db
                .get_channel(channel_id)
                .await
                .map_err(|e| internal_error(&format!("db error: {e}")))?;

            if channel.visibility == "private" && channel.ttl_seconds.is_some() {
                // Auto-add: use the channel creator as invited_by (truthful attribution).
                // The creator is always an active owner, satisfying add_member's invite check.
                state
                    .db
                    .add_member(
                        channel_id,
                        &ctx.pubkey_bytes,
                        sprout_db::channel::MemberRole::Member,
                        Some(&channel.created_by),
                    )
                    .await
                    .map_err(|e| internal_error(&format!("auto-add failed: {e}")))?;

                tracing::info!(
                    "Huddle auto-add: added {} to ephemeral channel {} (parent: {})",
                    ctx.pubkey.to_hex(),
                    channel_id,
                    parent_id
                );
                // Fall through to token generation.
            } else {
                // Not a private ephemeral channel — return the original 403.
                return Err(membership_err.clone());
            }
        } else {
            // No parent_channel_id provided — return the original 403.
            membership_result?;
        }
    }

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
