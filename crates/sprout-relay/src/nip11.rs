//! NIP-11 relay information document.

use serde::{Deserialize, Serialize};

use crate::connection::MAX_FRAME_BYTES;

/// Relay information document served at `GET /` with `Accept: application/nostr+json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayInfo {
    /// Human-readable relay name.
    pub name: String,
    /// Human-readable relay description.
    pub description: String,
    /// Relay operator's public key (hex), if published.
    pub pubkey: Option<String>,
    /// Contact address for the relay operator.
    pub contact: Option<String>,
    /// NIPs supported by this relay.
    pub supported_nips: Vec<u32>,
    /// URL of the relay software repository.
    pub software: String,
    /// Relay software version string.
    pub version: String,
    /// Protocol and resource limits advertised to clients.
    pub limitation: Option<RelayLimitation>,
}

/// Protocol and resource limits advertised in the NIP-11 document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayLimitation {
    /// Maximum WebSocket frame size in bytes.
    pub max_message_length: Option<u64>,
    /// Maximum number of concurrent subscriptions per connection.
    pub max_subscriptions: Option<u32>,
    /// Maximum number of filters per subscription.
    pub max_filters: Option<u32>,
    /// Maximum value of the `limit` field in a filter.
    pub max_limit: Option<u32>,
    /// Maximum length of a subscription ID string.
    pub max_subid_length: Option<u32>,
    /// Minimum proof-of-work difficulty required for events.
    pub min_pow_difficulty: Option<u32>,
    /// Whether NIP-42 authentication is required before sending events.
    pub auth_required: bool,
    /// Whether payment is required to use the relay.
    pub payment_required: bool,
    /// Whether writes are restricted to authorized pubkeys.
    pub restricted_writes: bool,
}

impl RelayInfo {
    /// Builds a `RelayInfo` document from the relay's runtime config.
    pub fn from_config(config: &crate::config::Config) -> Self {
        Self {
            name: "Sprout Relay".to_string(),
            description: "Sprout — private team communication relay".to_string(),
            pubkey: None,
            contact: None,
            supported_nips: vec![1, 11, 42],
            software: "https://github.com/sprout-rs/sprout".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            limitation: Some(RelayLimitation {
                max_message_length: Some(MAX_FRAME_BYTES as u64),
                max_subscriptions: Some(100),
                max_filters: Some(10),
                max_limit: Some(500),
                max_subid_length: Some(256),
                min_pow_difficulty: None,
                auth_required: config.require_auth_token,
                payment_required: false,
                restricted_writes: true,
            }),
        }
    }
}

/// Axum handler that returns the NIP-11 relay information document as JSON.
pub async fn relay_info_handler(
    axum::extract::State(state): axum::extract::State<std::sync::Arc<crate::state::AppState>>,
) -> axum::response::Json<RelayInfo> {
    axum::response::Json(RelayInfo::from_config(&state.config))
}
