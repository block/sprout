use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};

use crate::error::HuddleError;

/// A signed LiveKit access token and its associated metadata.
#[derive(Debug, Clone)]
pub struct LiveKitToken {
    /// The signed JWT string to pass to the LiveKit client SDK.
    pub token: String,
    /// The LiveKit room the token grants access to.
    pub room_name: String,
    /// The participant identity encoded in the token.
    pub participant_identity: String,
    /// When the token expires.
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
struct LiveKitClaims {
    iss: String,
    sub: String,
    iat: i64,
    exp: i64,
    name: String,
    video: VideoGrant,
}

#[derive(Debug, Serialize, Deserialize)]
struct VideoGrant {
    room: String,
    #[serde(rename = "roomJoin")]
    room_join: bool,
    #[serde(rename = "canPublish")]
    can_publish: bool,
    #[serde(rename = "canSubscribe")]
    can_subscribe: bool,
}

/// Generate a signed LiveKit access token for `identity` to join `room` as `name`.
///
/// Uses `ttl` as the token lifetime, defaulting to 6 hours if `None`.
pub fn generate_token(
    api_key: &str,
    api_secret: &str,
    room: &str,
    identity: &str,
    name: &str,
    ttl: Option<Duration>,
) -> Result<LiveKitToken, HuddleError> {
    let now = Utc::now();
    let ttl = ttl.unwrap_or_else(|| Duration::hours(6));
    let expires_at = now + ttl;

    let claims = LiveKitClaims {
        iss: api_key.to_string(),
        sub: identity.to_string(),
        iat: now.timestamp(),
        exp: expires_at.timestamp(),
        name: name.to_string(),
        video: VideoGrant {
            room: room.to_string(),
            room_join: true,
            can_publish: true,
            can_subscribe: true,
        },
    };

    let header = Header::new(Algorithm::HS256);
    let key = EncodingKey::from_secret(api_secret.as_bytes());
    let token = encode(&header, &claims, &key)?;

    Ok(LiveKitToken {
        token,
        room_name: room.to_string(),
        participant_identity: identity.to_string(),
        expires_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{decode, DecodingKey, Validation};

    #[test]
    fn test_generate_token() {
        let api_key = "APItest123";
        let api_secret = "supersecretkey";
        let room = "channel-abc";
        let identity = "npub1abc";
        let name = "Alice";

        let lk_token = generate_token(api_key, api_secret, room, identity, name, None).unwrap();
        assert_eq!(lk_token.room_name, room);
        assert_eq!(lk_token.participant_identity, identity);
        assert!(!lk_token.token.is_empty());

        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_issuer(&[api_key]);
        validation.set_required_spec_claims(&["iss", "sub", "exp"]);

        let decoded = decode::<LiveKitClaims>(
            &lk_token.token,
            &DecodingKey::from_secret(api_secret.as_bytes()),
            &validation,
        );
        let claims = decoded.unwrap().claims;
        assert_eq!(claims.iss, api_key);
        assert_eq!(claims.sub, identity);
        assert_eq!(claims.name, name);
        assert_eq!(claims.video.room, room);
        assert!(claims.video.room_join);
        assert!(claims.video.can_publish);
        assert!(claims.video.can_subscribe);
    }
}
