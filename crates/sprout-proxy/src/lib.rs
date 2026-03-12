#![deny(unsafe_code)]
#![warn(missing_docs)]
//! `sprout-proxy` — Guest relay proxy for Nostr client compatibility.
//!
//! Translates standard Nostr kinds ↔ Sprout custom kinds, derives deterministic
//! shadow keypairs for external users, and authenticates guests via invite tokens.

/// Bidirectional UUID ↔ NIP-28 kind:40 event ID mapping.
pub mod channel_map;
/// Error types for the proxy layer.
pub mod error;
/// Invite token management for guest authentication.
pub mod invite;
/// Kind translation between standard Nostr and Sprout-internal kinds.
pub mod kind_translator;
/// Deterministic shadow keypair derivation and caching.
pub mod shadow_keys;
/// Thread-safe invite token registry.
pub mod invite_store;

pub use error::ProxyError;
pub use invite::InviteToken;
pub use kind_translator::KindTranslator;
pub use shadow_keys::ShadowKeyManager;
pub use invite_store::InviteStore;

/// Configuration for the guest relay proxy.
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    /// URL of the upstream Sprout relay to forward events to.
    pub upstream_relay_url: String,
    /// Address the proxy WebSocket listener binds to (e.g. `0.0.0.0:4869`).
    pub listen_addr: String,
}

impl ProxyConfig {
    /// Create a new [`ProxyConfig`].
    pub fn new(upstream_relay_url: impl Into<String>, listen_addr: impl Into<String>) -> Self {
        Self {
            upstream_relay_url: upstream_relay_url.into(),
            listen_addr: listen_addr.into(),
        }
    }
}

/// The top-level proxy service, combining config, kind translation, and shadow key management.
pub struct ProxyService {
    /// Proxy configuration.
    pub config: ProxyConfig,
    /// Translates between standard Nostr kinds and Sprout-internal kinds.
    pub kind_translator: KindTranslator,
    /// Manages deterministic shadow keypairs for external users.
    pub shadow_keys: ShadowKeyManager,
}

impl ProxyService {
    /// Create a new [`ProxyService`] with the given config and shadow key salt.
    pub fn new(config: ProxyConfig, shadow_key_salt: &[u8]) -> Result<Self, ProxyError> {
        Ok(Self {
            config,
            kind_translator: KindTranslator::new(),
            shadow_keys: ShadowKeyManager::new(shadow_key_salt)?,
        })
    }
}
