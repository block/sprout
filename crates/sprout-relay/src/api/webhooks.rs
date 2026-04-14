//! LiveKit webhook handler — server-side huddle presence tracking.
//!
//! Receives webhook events from LiveKit and emits corresponding Nostr
//! huddle lifecycle events (kinds 48100–48103). This provides authoritative
//! presence tracking that survives client crashes — LiveKit fires
//! `participant_left` when the WebRTC connection drops, regardless of
//! whether the client performed a graceful shutdown.
//!
//! ## Route
//! `POST /internal/livekit/webhook` — internal only, not exposed through
//! the public API gateway.

use std::sync::Arc;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
};
use sprout_huddle::WebhookEvent;

use crate::state::AppState;

/// Handle a LiveKit webhook event.
///
/// Verifies the webhook signature, parses the event, and emits the
/// corresponding Nostr huddle lifecycle event to the parent channel.
///
/// Returns 200 on success (even if the event type is unrecognized —
/// LiveKit expects 2xx for all webhook deliveries).
/// Returns 401 if signature verification fails.
/// Returns 501 if huddles are not configured.
pub async fn handle_livekit_webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: String,
) -> StatusCode {
    let huddle_service = match state.huddle_service.as_ref() {
        Some(svc) => svc,
        None => return StatusCode::NOT_IMPLEMENTED,
    };

    // Verify signature and parse the event.
    let auth_header = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let event = match huddle_service.parse_webhook(body.as_bytes(), auth_header) {
        Ok(e) => e,
        Err(sprout_huddle::HuddleError::InvalidWebhookSignature) => {
            tracing::warn!("LiveKit webhook: signature verification failed");
            return StatusCode::UNAUTHORIZED;
        }
        Err(e) => {
            // Signed but malformed/unknown — log and return 200 so LiveKit
            // doesn't retry. Parse failures are not auth failures.
            tracing::warn!("LiveKit webhook: parse error (signed OK): {e}");
            return StatusCode::OK;
        }
    };

    // Dispatch on the parsed enum variant.
    // Room names follow the format "sprout-{channel_uuid}".
    match event {
        WebhookEvent::RoomStarted { room } => {
            tracing::info!("LiveKit: room started {room}");
            // TODO: Emit kind:48100 signed by relay keypair
        }
        WebhookEvent::RoomFinished { room } => {
            tracing::info!("LiveKit: room finished {room}");
            // TODO: Emit kind:48103 + archive ephemeral channel
        }
        WebhookEvent::ParticipantJoined { room, identity } => {
            let Some(channel_id) = parse_channel_id(&room) else {
                tracing::debug!("LiveKit webhook for non-sprout room or invalid UUID: {room}");
                return StatusCode::OK;
            };
            tracing::info!(
                "LiveKit: participant joined room {room} (channel {channel_id}), identity={identity}"
            );
            // TODO: Emit kind:48101 signed by relay keypair.
            // The client also emits this event; the server-side copy provides
            // crash-recovery redundancy.
        }
        WebhookEvent::ParticipantLeft { room, identity } => {
            let Some(channel_id) = parse_channel_id(&room) else {
                tracing::debug!("LiveKit webhook for non-sprout room or invalid UUID: {room}");
                return StatusCode::OK;
            };
            tracing::info!(
                "LiveKit: participant left room {room} (channel {channel_id}), identity={identity}"
            );
            // TODO: Emit kind:48102 signed by relay keypair.
            // This is the key crash-recovery path — fires even if the client crashed.
        }
        WebhookEvent::TrackPublished {
            room,
            identity,
            kind,
        } => {
            tracing::debug!("LiveKit: track published in {room} by {identity} (kind={kind})");
            // TODO: Emit kind:48104 (track published) if/when that event kind is defined.
        }
    }

    StatusCode::OK
}

/// Extract the channel UUID from a LiveKit room name.
///
/// Room names follow the format `sprout-{uuid}`.  Returns `None` if the name
/// does not match the expected prefix or contains an invalid UUID.
fn parse_channel_id(room_name: &str) -> Option<uuid::Uuid> {
    let channel_id = room_name.strip_prefix("sprout-")?;
    uuid::Uuid::parse_str(channel_id)
        .map_err(|_| {
            tracing::warn!("LiveKit webhook: invalid channel UUID in room name: {room_name}");
        })
        .ok()
}
