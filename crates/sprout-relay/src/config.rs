//! Relay configuration from environment variables.

use std::net::SocketAddr;

use thiserror::Error;
use tracing::warn;

/// Errors that can occur while loading relay configuration.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// The `SPROUT_BIND_ADDR` environment variable could not be parsed as a socket address.
    #[error("invalid SPROUT_BIND_ADDR: {0}")]
    InvalidBindAddr(String),
}

/// Relay runtime configuration, loaded from environment variables.
#[derive(Debug, Clone)]
pub struct Config {
    /// Address the relay HTTP/WebSocket server binds to.
    pub bind_addr: SocketAddr,
    /// MySQL database connection URL.
    pub database_url: String,
    /// Redis connection URL used by the pub/sub manager.
    pub redis_url: String,
    /// Typesense search server URL.
    pub typesense_url: String,
    /// Typesense API key.
    pub typesense_key: String,
    /// Public WebSocket URL of this relay, advertised in NIP-11.
    pub relay_url: String,
    /// Maximum number of concurrent WebSocket connections.
    pub max_connections: usize,
    /// Maximum number of concurrently executing message handlers.
    pub max_concurrent_handlers: usize,
    /// Per-connection outbound message buffer size (number of messages).
    pub send_buffer_size: usize,
    /// Authentication provider configuration.
    pub auth: sprout_auth::AuthConfig,
    /// Whether clients must authenticate via NIP-42 before sending events.
    pub require_auth_token: bool,
    /// Comma-separated list of allowed CORS origins.
    /// If empty, permissive CORS is used (dev mode).
    /// Example: "tauri://localhost,http://localhost:3000"
    pub cors_origins: Vec<String>,
    /// Optional hex-encoded private key for the relay's signing keypair.
    /// If absent, a fresh keypair is generated at startup.
    pub relay_private_key: Option<String>,
    /// Optional Unix Domain Socket path. When set, the relay also listens on this
    /// UDS for traffic (e.g. service mesh sidecar). Health probes still use TCP.
    pub uds_path: Option<String>,
    /// When true, only pubkeys in the `pubkey_allowlist` table may authenticate
    /// via NIP-42 without a JWT or API token. Has no effect when
    /// `require_auth_token` is false (open-relay mode already allows all pubkeys).
    pub pubkey_allowlist_enabled: bool,
}

impl Config {
    /// Loads configuration from environment variables, falling back to development defaults.
    pub fn from_env() -> Result<Self, ConfigError> {
        let bind_addr = std::env::var("SPROUT_BIND_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:3000".to_string())
            .parse::<SocketAddr>()
            .map_err(|e| ConfigError::InvalidBindAddr(e.to_string()))?;

        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "mysql://sprout:sprout_dev@localhost:3306/sprout".to_string());

        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());

        let typesense_url =
            std::env::var("TYPESENSE_URL").unwrap_or_else(|_| "http://localhost:8108".to_string());

        let typesense_key =
            std::env::var("TYPESENSE_API_KEY").unwrap_or_else(|_| "sprout_dev_key".to_string());

        let relay_url =
            std::env::var("RELAY_URL").unwrap_or_else(|_| "ws://localhost:3000".to_string());

        let max_connections = std::env::var("SPROUT_MAX_CONNECTIONS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10_000);

        let max_concurrent_handlers = std::env::var("SPROUT_MAX_CONCURRENT_HANDLERS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(64);

        let send_buffer_size = std::env::var("SPROUT_SEND_BUFFER")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1_000);

        let require_auth_token = std::env::var("SPROUT_REQUIRE_AUTH_TOKEN")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        let pubkey_allowlist_enabled = std::env::var("SPROUT_PUBKEY_ALLOWLIST")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        let mut auth = sprout_auth::AuthConfig::default();
        auth.okta.require_token = require_auth_token;

        if let Ok(issuer) = std::env::var("OKTA_ISSUER") {
            auth.okta.issuer = issuer;
        }
        if let Ok(audience) = std::env::var("OKTA_AUDIENCE") {
            auth.okta.audience = audience;
        }
        if let Ok(jwks_uri) = std::env::var("OKTA_JWKS_URI") {
            auth.okta.jwks_uri = jwks_uri;
        }

        if !require_auth_token {
            warn!(
                "SPROUT_REQUIRE_AUTH_TOKEN is false — relay accepts unauthenticated connections. \
                 Set to true for production."
            );
        }

        let cors_origins = std::env::var("SPROUT_CORS_ORIGINS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let relay_private_key = std::env::var("SPROUT_RELAY_PRIVATE_KEY").ok();

        let uds_path = std::env::var("SPROUT_UDS_PATH").ok();

        Ok(Self {
            bind_addr,
            database_url,
            redis_url,
            typesense_url,
            typesense_key,
            relay_url,
            max_connections,
            max_concurrent_handlers,
            send_buffer_size,
            auth,
            require_auth_token,
            cors_origins,
            relay_private_key,
            uds_path,
            pubkey_allowlist_enabled,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mutex to serialize tests that mutate environment variables.
    // Parallel env-var mutation causes `defaults_are_valid` to see the invalid
    // value set by `invalid_bind_addr_returns_error`, causing a flaky failure.
    static ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn defaults_are_valid() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let config = Config::from_env().expect("default config");
        assert!(config.bind_addr.port() > 0);
        assert!(!config.database_url.is_empty());
        assert!(!config.redis_url.is_empty());
        assert!(config.max_connections > 0);
        assert!(config.send_buffer_size > 0);
    }

    #[test]
    fn invalid_bind_addr_returns_error() {
        let _guard = ENV_MUTEX.lock().unwrap();
        std::env::set_var("SPROUT_BIND_ADDR", "not-an-addr");
        let result = Config::from_env();
        std::env::remove_var("SPROUT_BIND_ADDR");
        assert!(matches!(result, Err(ConfigError::InvalidBindAddr(_))));
    }
}
