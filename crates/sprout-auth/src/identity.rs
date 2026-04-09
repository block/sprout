//! Corporate identity mode for the Sprout relay.
//!
//! Supports proxy-based identity where an upstream reverse proxy (e.g. cf-doorman)
//! injects identity JWTs. The relay derives deterministic Nostr keypairs from
//! the corporate UID claim, so users don't need to manage Nostr keys directly.

use std::fmt;
use std::str::FromStr;

use hmac::Mac;
use serde::{Deserialize, Serialize};

use crate::error::AuthError;

/// How corporate identity is resolved for incoming connections.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum IdentityMode {
    /// Identity mode is disabled — standard Nostr key-based authentication.
    #[default]
    Disabled,
    /// A reverse proxy (e.g. cf-doorman) injects identity JWTs into requests.
    /// All connections **must** present a valid identity JWT — no fallback.
    Proxy,
    /// Transitional mode: proxy identity is preferred for human users, but
    /// connections without an identity JWT fall through to standard auth
    /// (API tokens, Okta JWTs, NIP-42). Use this while agents lack JWTs.
    Hybrid,
}

impl IdentityMode {
    /// Returns `true` if proxy identity JWT validation is active
    /// (either strict `Proxy` or transitional `Hybrid` mode).
    pub fn is_proxy(&self) -> bool {
        matches!(self, Self::Proxy | Self::Hybrid)
    }
}

impl fmt::Display for IdentityMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => write!(f, "disabled"),
            Self::Proxy => write!(f, "proxy"),
            Self::Hybrid => write!(f, "hybrid"),
        }
    }
}

impl FromStr for IdentityMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "disabled" => Ok(Self::Disabled),
            "proxy" => Ok(Self::Proxy),
            "hybrid" => Ok(Self::Hybrid),
            other => Err(format!("unknown identity mode: {other}")),
        }
    }
}

/// Configuration for corporate identity resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityConfig {
    /// The identity mode to use.
    #[serde(default = "default_mode")]
    pub mode: IdentityMode,
    /// JWT claim name containing the corporate user ID.
    #[serde(default = "default_uid_claim")]
    pub uid_claim: String,
    /// JWT claim name containing the human-readable username.
    #[serde(default = "default_user_claim")]
    pub user_claim: String,
    /// High-entropy secret used as the HMAC key for deterministic keypair derivation.
    ///
    /// **Required** when `mode = Proxy`. Without this, anyone who knows a UID could
    /// derive that user's private key. The secret ensures that UID alone is insufficient.
    #[serde(default)]
    pub secret: String,
    /// Domain-separation context string for keypair derivation (versioned for rotation).
    #[serde(default = "default_context")]
    pub context: String,
}

impl Default for IdentityConfig {
    fn default() -> Self {
        Self {
            mode: default_mode(),
            uid_claim: default_uid_claim(),
            user_claim: default_user_claim(),
            secret: String::new(),
            context: default_context(),
        }
    }
}

fn default_mode() -> IdentityMode {
    IdentityMode::Disabled
}

fn default_uid_claim() -> String {
    "uid".to_string()
}

fn default_user_claim() -> String {
    "user".to_string()
}

fn default_context() -> String {
    "sprout/nostr-id/v1".to_string()
}

// Custom serde for IdentityMode as a lowercase string.
impl Serialize for IdentityMode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for IdentityMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

/// Derive a deterministic Nostr keypair from a corporate UID using a secret-backed HMAC.
///
/// Uses HMAC-SHA256 with `secret` as the key and `context:uid` as the message.
/// The `secret` must be high-entropy material (≥32 bytes recommended) — without it,
/// anyone who knows a UID could derive that user's private key.
///
/// The `context` string provides domain separation and version namespacing
/// (e.g. `"sprout/nostr-id/v1"`), enabling key rotation by changing the context.
///
/// The 32-byte HMAC output is used directly as a secp256k1 secret key.
///
/// # Errors
///
/// Returns [`AuthError::Internal`] if `uid` or `secret` is empty, or key derivation fails.
pub fn derive_keypair_from_uid(
    secret: &str,
    context: &str,
    uid: &str,
) -> Result<nostr::Keys, AuthError> {
    if uid.is_empty() {
        return Err(AuthError::Internal("uid must not be empty".into()));
    }
    if secret.is_empty() {
        return Err(AuthError::Internal(
            "identity secret must not be empty — set SPROUT_IDENTITY_SECRET".into(),
        ));
    }

    let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(secret.as_bytes())
        .map_err(|e| AuthError::Internal(format!("HMAC init failed: {e}")))?;
    mac.update(context.as_bytes());
    mac.update(b":");
    mac.update(uid.as_bytes());
    let result = mac.finalize().into_bytes();

    let secret_key = nostr::SecretKey::from_slice(&result)
        .map_err(|e| AuthError::Internal(format!("key derivation failed: {e}")))?;
    Ok(nostr::Keys::new(secret_key))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SECRET: &str = "test-secret-at-least-32-bytes-long!!";

    #[test]
    fn derive_keypair_deterministic() {
        let a = derive_keypair_from_uid(TEST_SECRET, "ctx", "alice").unwrap();
        let b = derive_keypair_from_uid(TEST_SECRET, "ctx", "alice").unwrap();
        assert_eq!(a.public_key(), b.public_key());
    }

    #[test]
    fn derive_keypair_different_uid() {
        let a = derive_keypair_from_uid(TEST_SECRET, "ctx", "alice").unwrap();
        let b = derive_keypair_from_uid(TEST_SECRET, "ctx", "bob").unwrap();
        assert_ne!(a.public_key(), b.public_key());
    }

    #[test]
    fn derive_keypair_different_context() {
        let a = derive_keypair_from_uid(TEST_SECRET, "ctx-a", "alice").unwrap();
        let b = derive_keypair_from_uid(TEST_SECRET, "ctx-b", "alice").unwrap();
        assert_ne!(a.public_key(), b.public_key());
    }

    #[test]
    fn derive_keypair_different_secret() {
        let a =
            derive_keypair_from_uid("secret-one-xxxxxxxxxxxxxxxxxxxxxxx", "ctx", "alice").unwrap();
        let b =
            derive_keypair_from_uid("secret-two-xxxxxxxxxxxxxxxxxxxxxxx", "ctx", "alice").unwrap();
        assert_ne!(a.public_key(), b.public_key());
    }

    #[test]
    fn derive_keypair_empty_uid_fails() {
        let result = derive_keypair_from_uid(TEST_SECRET, "ctx", "");
        assert!(result.is_err());
    }

    #[test]
    fn derive_keypair_empty_secret_fails() {
        let result = derive_keypair_from_uid("", "ctx", "alice");
        assert!(result.is_err());
    }

    #[test]
    fn identity_mode_from_str() {
        assert_eq!(
            "disabled".parse::<IdentityMode>().unwrap(),
            IdentityMode::Disabled
        );
        assert_eq!(
            "proxy".parse::<IdentityMode>().unwrap(),
            IdentityMode::Proxy
        );
        assert_eq!(
            "Proxy".parse::<IdentityMode>().unwrap(),
            IdentityMode::Proxy
        );
        assert_eq!(
            "hybrid".parse::<IdentityMode>().unwrap(),
            IdentityMode::Hybrid
        );
        assert_eq!(
            "Hybrid".parse::<IdentityMode>().unwrap(),
            IdentityMode::Hybrid
        );
        assert!("unknown".parse::<IdentityMode>().is_err());
    }

    #[test]
    fn identity_mode_is_proxy() {
        assert!(!IdentityMode::Disabled.is_proxy());
        assert!(IdentityMode::Proxy.is_proxy());
        assert!(IdentityMode::Hybrid.is_proxy());
    }

    /// Golden vector: pin the derivation output so relay and desktop bootstrap stay in sync.
    /// If this test breaks, all existing proxy-mode identities will rotate.
    #[test]
    fn derive_keypair_golden_vector() {
        let keys = derive_keypair_from_uid(
            "golden-test-secret-do-not-change!",
            "sprout/nostr-id/v1",
            "12345",
        )
        .unwrap();
        let hex = keys.public_key().to_hex();
        // Pin the value — changing this means all existing proxy-mode identities rotate.
        // Computed with: HMAC-SHA256(key="golden-test-secret-do-not-change!", msg="sprout/nostr-id/v1:12345")
        assert_eq!(
            hex,
            "92458a8e3e8e3203b3c8d0c8772bf948f124d5ab973dc666d2a9e62a94c6d29d"
        );
    }

    #[test]
    fn identity_config_defaults() {
        let config = IdentityConfig::default();
        assert_eq!(config.mode, IdentityMode::Disabled);
        assert_eq!(config.uid_claim, "uid");
        assert_eq!(config.user_claim, "user");
        assert!(config.secret.is_empty());
        assert_eq!(config.context, "sprout/nostr-id/v1");
    }
}
