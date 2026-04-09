use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::{error::HuddleError, session::TrackKind};

/// Raw JSON payload received from a LiveKit webhook.
#[derive(Debug, Deserialize, Serialize)]
pub struct LiveKitWebhookPayload {
    /// The event type string (e.g. `"room_started"`).
    pub event: String,
    /// Room information, present for room and participant events.
    pub room: Option<WebhookRoom>,
    /// Participant information, present for participant and track events.
    pub participant: Option<WebhookParticipant>,
    /// Track information, present for track events.
    pub track: Option<WebhookTrack>,
}

/// Room metadata from a LiveKit webhook payload.
#[derive(Debug, Deserialize, Serialize)]
pub struct WebhookRoom {
    /// The LiveKit room name.
    pub name: String,
}

/// Participant metadata from a LiveKit webhook payload.
#[derive(Debug, Deserialize, Serialize)]
pub struct WebhookParticipant {
    /// The participant's identity string.
    pub identity: String,
}

/// Track metadata from a LiveKit webhook payload.
#[derive(Debug, Deserialize, Serialize)]
pub struct WebhookTrack {
    /// The track type string (e.g. `"audio"`, `"video"`, `"screen_share"`).
    #[serde(rename = "type")]
    pub kind: String,
}

/// A parsed LiveKit webhook event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebhookEvent {
    /// A room was created and is now active.
    RoomStarted {
        /// The LiveKit room name.
        room: String,
    },
    /// A room has ended.
    RoomFinished {
        /// The LiveKit room name.
        room: String,
    },
    /// A participant joined the room.
    ParticipantJoined {
        /// The LiveKit room name.
        room: String,
        /// The participant's identity string.
        identity: String,
    },
    /// A participant left the room.
    ParticipantLeft {
        /// The LiveKit room name.
        room: String,
        /// The participant's identity string.
        identity: String,
    },
    /// A participant published a media track.
    TrackPublished {
        /// The LiveKit room name.
        room: String,
        /// The participant's identity string.
        identity: String,
        /// The kind of track that was published.
        kind: TrackKind,
    },
}

/// Verify the `Authorization` header via constant-time HMAC comparison.
fn verify_signature(body: &[u8], auth_header: &str, api_secret: &str) -> Result<(), HuddleError> {
    let mut mac = Hmac::<Sha256>::new_from_slice(api_secret.as_bytes())
        .map_err(|_| HuddleError::InvalidWebhookSignature)?;
    mac.update(body);
    let sig_bytes =
        hex::decode(auth_header.trim()).map_err(|_| HuddleError::InvalidWebhookSignature)?;
    mac.verify_slice(&sig_bytes)
        .map_err(|_| HuddleError::InvalidWebhookSignature)
}

/// Verify the HMAC-SHA256 signature and parse a LiveKit webhook payload.
///
/// `auth_header` is the hex-encoded HMAC-SHA256 of `body` using `api_secret`.
/// Returns [`HuddleError::InvalidWebhookSignature`] if the signature does not match.
pub fn parse_webhook(
    body: &[u8],
    auth_header: &str,
    api_secret: &str,
) -> Result<WebhookEvent, HuddleError> {
    verify_signature(body, auth_header, api_secret)?;

    let payload: LiveKitWebhookPayload = serde_json::from_slice(body)?;

    let room_name = || -> Result<String, HuddleError> {
        payload
            .room
            .as_ref()
            .map(|r| r.name.clone())
            .ok_or(HuddleError::MissingField("room.name"))
    };

    let identity = || -> Result<String, HuddleError> {
        payload
            .participant
            .as_ref()
            .map(|p| p.identity.clone())
            .ok_or(HuddleError::MissingField("participant.identity"))
    };

    let event = match payload.event.as_str() {
        "room_started" => WebhookEvent::RoomStarted { room: room_name()? },
        "room_finished" => WebhookEvent::RoomFinished { room: room_name()? },
        "participant_joined" => WebhookEvent::ParticipantJoined {
            room: room_name()?,
            identity: identity()?,
        },
        "participant_left" => WebhookEvent::ParticipantLeft {
            room: room_name()?,
            identity: identity()?,
        },
        "track_published" => {
            let track_kind = payload
                .track
                .as_ref()
                .map(|t| t.kind.as_str())
                .unwrap_or("audio");
            let kind = match track_kind {
                "audio" => TrackKind::Audio,
                "video" => TrackKind::Video,
                "screen_share" => TrackKind::ScreenShare,
                other => return Err(HuddleError::InvalidTrackKind(other.to_string())),
            };
            WebhookEvent::TrackPublished {
                room: room_name()?,
                identity: identity()?,
                kind,
            }
        }
        other => return Err(HuddleError::UnknownEventType(other.to_string())),
    };

    Ok(event)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hmac::{Hmac, KeyInit, Mac};
    use sha2::Sha256;

    fn make_sig(body: &[u8], secret: &str) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        hex::encode(mac.finalize().into_bytes())
    }

    const SECRET: &str = "test-secret";

    fn signed_parse(json: &str) -> Result<WebhookEvent, HuddleError> {
        let body = json.as_bytes();
        let sig = make_sig(body, SECRET);
        parse_webhook(body, &sig, SECRET)
    }

    #[test]
    fn test_webhook_parsing() {
        let json = r#"{"event":"room_started","room":{"name":"channel-abc"}}"#;
        let event = signed_parse(json).expect("should parse room_started");
        assert_eq!(
            event,
            WebhookEvent::RoomStarted {
                room: "channel-abc".to_string()
            }
        );
    }

    #[test]
    fn test_webhook_event_variants() {
        let ev = signed_parse(r#"{"event":"room_started","room":{"name":"r1"}}"#).unwrap();
        assert_eq!(ev, WebhookEvent::RoomStarted { room: "r1".into() });

        let ev = signed_parse(r#"{"event":"room_finished","room":{"name":"r1"}}"#).unwrap();
        assert_eq!(ev, WebhookEvent::RoomFinished { room: "r1".into() });

        let ev =
            signed_parse(r#"{"event":"participant_joined","room":{"name":"r1"},"participant":{"identity":"alice"}}"#)
                .unwrap();
        assert_eq!(
            ev,
            WebhookEvent::ParticipantJoined {
                room: "r1".into(),
                identity: "alice".into()
            }
        );

        let ev =
            signed_parse(r#"{"event":"participant_left","room":{"name":"r1"},"participant":{"identity":"alice"}}"#)
                .unwrap();
        assert_eq!(
            ev,
            WebhookEvent::ParticipantLeft {
                room: "r1".into(),
                identity: "alice".into()
            }
        );

        let ev = signed_parse(
            r#"{"event":"track_published","room":{"name":"r1"},"participant":{"identity":"alice"},"track":{"type":"audio"}}"#,
        )
        .unwrap();
        assert_eq!(
            ev,
            WebhookEvent::TrackPublished {
                room: "r1".into(),
                identity: "alice".into(),
                kind: TrackKind::Audio,
            }
        );

        let ev = signed_parse(
            r#"{"event":"track_published","room":{"name":"r1"},"participant":{"identity":"alice"},"track":{"type":"video"}}"#,
        )
        .unwrap();
        assert_eq!(
            ev,
            WebhookEvent::TrackPublished {
                room: "r1".into(),
                identity: "alice".into(),
                kind: TrackKind::Video,
            }
        );

        let ev = signed_parse(
            r#"{"event":"track_published","room":{"name":"r1"},"participant":{"identity":"alice"},"track":{"type":"screen_share"}}"#,
        )
        .unwrap();
        assert_eq!(
            ev,
            WebhookEvent::TrackPublished {
                room: "r1".into(),
                identity: "alice".into(),
                kind: TrackKind::ScreenShare,
            }
        );
    }

    #[test]
    fn test_invalid_signature_rejected() {
        let json = r#"{"event":"room_started","room":{"name":"r1"}}"#;
        let result = parse_webhook(json.as_bytes(), "badsig", SECRET);
        assert!(matches!(result, Err(HuddleError::InvalidWebhookSignature)));
    }

    #[test]
    fn test_unknown_event_type() {
        let json = r#"{"event":"unknown_event","room":{"name":"r1"}}"#;
        let result = signed_parse(json);
        assert!(matches!(result, Err(HuddleError::UnknownEventType(_))));
    }
}
