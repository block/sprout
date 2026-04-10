//! Corporate identity mode for the Sprout relay.
//!
//! Supports proxy-based identity where an upstream auth proxy
//! injects identity JWTs. The relay extracts corporate identity claims and binds
//! the client's self-generated pubkey to them.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// How corporate identity is resolved for incoming connections.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum IdentityMode {
    /// Identity mode is disabled — standard Nostr key-based authentication.
    #[default]
    Disabled,
    /// An auth proxy injects identity JWTs into requests.
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
    /// JWKS endpoint URL for the identity provider (e.g. the auth proxy).
    /// Falls back to the main Okta/JWKS URI if empty.
    #[serde(default)]
    pub jwks_uri: String,
    /// Expected JWT issuer claim for identity JWTs.
    /// Falls back to the main Okta issuer if empty.
    #[serde(default)]
    pub issuer: String,
    /// Expected JWT audience claim for identity JWTs.
    /// Falls back to the main Okta audience if empty.
    #[serde(default)]
    pub audience: String,
    /// HTTP header containing the identity JWT injected by the auth proxy.
    #[serde(default = "default_identity_jwt_header")]
    pub identity_jwt_header: String,
    /// HTTP header containing the device common name from the client certificate.
    #[serde(default = "default_device_cn_header")]
    pub device_cn_header: String,
}

impl Default for IdentityConfig {
    fn default() -> Self {
        Self {
            mode: default_mode(),
            uid_claim: default_uid_claim(),
            user_claim: default_user_claim(),
            jwks_uri: String::new(),
            issuer: String::new(),
            audience: String::new(),
            identity_jwt_header: default_identity_jwt_header(),
            device_cn_header: default_device_cn_header(),
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

fn default_identity_jwt_header() -> String {
    "x-forwarded-identity-token".to_string()
}

fn default_device_cn_header() -> String {
    "x-block-client-cert-subject-cn".to_string()
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

/// Claims extracted from a validated proxy identity JWT.
///
/// Used by the relay to identify the corporate user without deriving keys.
/// The relay binds the client's self-generated pubkey to these claims.
#[derive(Debug, Clone)]
pub struct ProxyIdentityClaims {
    /// Corporate user identifier (stable, immutable).
    pub uid: String,
    /// Human-readable username for display purposes.
    pub username: String,
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn identity_config_defaults() {
        let config = IdentityConfig::default();
        assert_eq!(config.mode, IdentityMode::Disabled);
        assert_eq!(config.uid_claim, "uid");
        assert_eq!(config.user_claim, "user");
        assert_eq!(config.identity_jwt_header, "x-forwarded-identity-token");
        assert_eq!(config.device_cn_header, "x-block-client-cert-subject-cn");
    }
}
