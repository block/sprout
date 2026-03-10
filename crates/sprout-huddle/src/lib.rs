#![deny(unsafe_code)]
#![warn(missing_docs)]
//! LiveKit integration for real-time audio/video huddles.
//!
//! Sessions are tracked in-memory only — they are lost on process restart.
//! Persistent session state (recordings, participant history) must be stored
//! externally if needed.

/// Error types for the huddle layer.
pub mod error;
/// In-memory huddle session and participant tracking.
pub mod session;
/// LiveKit access token generation.
pub mod token;
/// LiveKit webhook signature verification and event parsing.
pub mod webhook;

pub use error::HuddleError;
pub use session::{HuddleParticipant, HuddleSession, TrackInfo, TrackKind};
pub use token::LiveKitToken;
pub use webhook::WebhookEvent;

use uuid::Uuid;

pub use sprout_core::kind::{
    KIND_HUDDLE_ENDED, KIND_HUDDLE_PARTICIPANT_JOINED, KIND_HUDDLE_PARTICIPANT_LEFT,
    KIND_HUDDLE_RECORDING_AVAILABLE, KIND_HUDDLE_STARTED, KIND_HUDDLE_TRACK_PUBLISHED,
};

/// Configuration for the LiveKit huddle service.
#[derive(Debug, Clone)]
pub struct HuddleConfig {
    /// LiveKit server URL (e.g. `wss://livekit.example.com`).
    pub livekit_url: String,
    /// LiveKit API key used to sign access tokens and verify webhooks.
    pub livekit_api_key: String,
    /// LiveKit API secret used to sign access tokens and verify webhooks.
    pub livekit_api_secret: String,
}

/// High-level service for LiveKit huddle operations.
///
/// Wraps token generation and webhook parsing behind a single struct.
pub struct HuddleService {
    config: HuddleConfig,
}

impl HuddleService {
    /// Create a new [`HuddleService`] with the given LiveKit credentials.
    pub fn new(config: HuddleConfig) -> Self {
        Self { config }
    }

    /// Generate a LiveKit access token for `identity` to join `room` as `name`.
    pub fn generate_token(
        &self,
        room: &str,
        identity: &str,
        name: &str,
    ) -> Result<LiveKitToken, HuddleError> {
        token::generate_token(
            &self.config.livekit_api_key,
            &self.config.livekit_api_secret,
            room,
            identity,
            name,
            None,
        )
    }

    /// Derive the LiveKit room name for a given Sprout channel.
    pub fn create_room_name(channel_id: Uuid) -> String {
        format!("sprout-{}", channel_id)
    }

    /// Verify the webhook signature and parse the LiveKit event payload.
    pub fn parse_webhook(
        &self,
        body: &[u8],
        auth_header: &str,
    ) -> Result<WebhookEvent, HuddleError> {
        webhook::parse_webhook(body, auth_header, &self.config.livekit_api_secret)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
    use serde::{Deserialize, Serialize};

    fn make_service() -> HuddleService {
        HuddleService::new(HuddleConfig {
            livekit_url: "wss://livekit.example.com".to_string(),
            livekit_api_key: "APIkey123".to_string(),
            livekit_api_secret: "supersecretvalue".to_string(),
        })
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct MinClaims {
        iss: String,
        sub: String,
        name: String,
    }

    #[test]
    fn token_is_valid_jwt_with_correct_claims() {
        let svc = make_service();
        let lk = svc
            .generate_token("sprout-test-room", "abc123pubkey", "Alice")
            .unwrap();

        assert_eq!(lk.room_name, "sprout-test-room");
        assert_eq!(lk.participant_identity, "abc123pubkey");
        assert!(!lk.token.is_empty());
        assert!(lk.expires_at > chrono::Utc::now());

        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_issuer(&["APIkey123"]);
        validation.set_required_spec_claims(&["iss", "sub", "exp"]);
        let claims = decode::<MinClaims>(
            &lk.token,
            &DecodingKey::from_secret(b"supersecretvalue"),
            &validation,
        )
        .unwrap()
        .claims;

        assert_eq!(claims.iss, "APIkey123");
        assert_eq!(claims.sub, "abc123pubkey");
        assert_eq!(claims.name, "Alice");
    }

    #[test]
    fn room_name_format_is_stable() {
        let id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        assert_eq!(
            HuddleService::create_room_name(id),
            "sprout-550e8400-e29b-41d4-a716-446655440000"
        );
        assert_eq!(
            HuddleService::create_room_name(id),
            HuddleService::create_room_name(id)
        );
    }

    #[test]
    fn session_lifecycle() {
        let channel_id = Uuid::new_v4();
        let room_name = HuddleService::create_room_name(channel_id);
        let mut session = HuddleSession::new(channel_id, &room_name);

        assert!(session.is_active());
        assert_eq!(session.active_participants().count(), 0);

        session.join(HuddleParticipant::new("alice_pubkey", "Alice"));
        session.join(HuddleParticipant::new("bob_pubkey", "Bob"));
        assert_eq!(session.active_participants().count(), 2);

        assert!(session.leave("alice_pubkey"));
        assert_eq!(session.active_participants().count(), 1);
        assert!(!session.leave("nobody"));

        session.end();
        assert!(!session.is_active());
        assert!(session.ended_at.is_some());
    }
}
