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
    /// The `SPROUT_IDENTITY_MODE` environment variable contains an unrecognised value.
    #[error("invalid SPROUT_IDENTITY_MODE: {0}")]
    InvalidIdentityMode(String),
}

/// Relay runtime configuration, loaded from environment variables.
#[derive(Debug, Clone)]
pub struct Config {
    /// Address the relay HTTP/WebSocket server binds to.
    pub bind_addr: SocketAddr,
    /// Postgres database connection URL.
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
    /// TCP port for the health-only router (`/_liveness`, `/_readiness`, `/_status`).
    /// Separate from the app router so K8s probes bypass Istio and auth middleware.
    pub health_port: u16,
    /// TCP port for the Prometheus metrics exporter (`GET /metrics`).
    pub metrics_port: u16,

    /// When true, NIP-42 pubkey-only authentication (no JWT or API token) is
    /// restricted to pubkeys in the `pubkey_allowlist` table. Users with valid
    /// API tokens or Okta JWTs bypass the allowlist entirely.
    /// Applies to all NIP-42 pubkey-only connections, regardless of `require_auth_token`.
    pub pubkey_allowlist_enabled: bool,
    /// Media storage configuration (S3/MinIO).
    pub media: sprout_media::MediaConfig,

    /// Optional override for ephemeral channel TTL (in seconds).
    /// When set, any channel created with a TTL tag will use this value instead
    /// of the client-provided one. Useful for testing ephemeral expiry quickly.
    /// Example: `SPROUT_EPHEMERAL_TTL_OVERRIDE=60` → all ephemeral channels expire
    /// 60 seconds after the last message.
    pub ephemeral_ttl_override: Option<i32>,
}

impl Config {
    /// Loads configuration from environment variables, falling back to development defaults.
    pub fn from_env() -> Result<Self, ConfigError> {
        let bind_addr = std::env::var("SPROUT_BIND_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:3000".to_string())
            .parse::<SocketAddr>()
            .map_err(|e| ConfigError::InvalidBindAddr(e.to_string()))?;

        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://sprout:sprout_dev@localhost:5432/sprout".to_string());

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
            .unwrap_or(1024);

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

        // ── Identity mode ──────────────────────────────────────────────────────
        let identity_mode = std::env::var("SPROUT_IDENTITY_MODE")
            .unwrap_or_else(|_| "disabled".to_string())
            .parse::<sprout_auth::IdentityMode>()
            .map_err(ConfigError::InvalidIdentityMode)?;

        auth.identity.mode = identity_mode.clone();

        if let Ok(uid_claim) = std::env::var("SPROUT_IDENTITY_UID_CLAIM") {
            auth.identity.uid_claim = uid_claim;
        }
        if let Ok(user_claim) = std::env::var("SPROUT_IDENTITY_USER_CLAIM") {
            auth.identity.user_claim = user_claim;
        }
        if let Ok(jwks_uri) = std::env::var("SPROUT_IDENTITY_JWKS_URI") {
            auth.identity.jwks_uri = jwks_uri;
        }
        if let Ok(issuer) = std::env::var("SPROUT_IDENTITY_ISSUER") {
            auth.identity.issuer = issuer;
        }
        if let Ok(audience) = std::env::var("SPROUT_IDENTITY_AUDIENCE") {
            auth.identity.audience = audience;
        }

        // When identity mode is active the relay sits behind a trusted proxy
        // (cf-doorman) — force require_auth_token so the NIP-42 fallback path
        // cannot be used with bare keypair-only auth.
        let require_auth_token = if identity_mode.is_proxy() {
            if !require_auth_token {
                tracing::info!(
                    "Identity mode: {identity_mode} — overriding SPROUT_REQUIRE_AUTH_TOKEN to true"
                );
            }
            tracing::warn!(
                "Identity mode: {identity_mode} — relay trusts x-forwarded-identity-token headers. \
                 Ensure the relay is reachable ONLY via the trusted reverse proxy (cf-doorman). \
                 Direct access to the relay port would allow header injection."
            );
            auth.okta.require_token = true;
            true
        } else {
            require_auth_token
        };

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

        let uds_path = std::env::var("SPROUT_UDS_PATH")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let health_port = std::env::var("SPROUT_HEALTH_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8080);

        let metrics_port = std::env::var("SPROUT_METRICS_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(9102);

        let media = sprout_media::MediaConfig {
            s3_endpoint: std::env::var("SPROUT_S3_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:9000".to_string()),
            s3_access_key: std::env::var("SPROUT_S3_ACCESS_KEY")
                .unwrap_or_else(|_| "sprout_dev".to_string()),
            s3_secret_key: std::env::var("SPROUT_S3_SECRET_KEY")
                .unwrap_or_else(|_| "sprout_dev_secret".to_string()),
            s3_bucket: std::env::var("SPROUT_S3_BUCKET")
                .unwrap_or_else(|_| "sprout-media".to_string()),
            max_image_bytes: std::env::var("SPROUT_MAX_IMAGE_BYTES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(50 * 1024 * 1024),
            max_gif_bytes: std::env::var("SPROUT_MAX_GIF_BYTES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10 * 1024 * 1024),
            max_video_bytes: std::env::var("SPROUT_MAX_VIDEO_BYTES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(500 * 1024 * 1024),
            public_base_url: std::env::var("SPROUT_MEDIA_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:3000/media".to_string()),
            server_domain: std::env::var("SPROUT_MEDIA_SERVER_DOMAIN")
                .ok()
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    // Auto-derive from RELAY_URL so desktop uploads work out-of-the-box
                    // without requiring an extra env var in dev mode.
                    url::Url::parse(
                        &relay_url
                            .replace("ws://", "http://")
                            .replace("wss://", "https://"),
                    )
                    .ok()
                    .and_then(|u| {
                        let host = u.host_str()?.to_string();
                        match u.port() {
                            Some(p) => Some(format!("{host}:{p}")),
                            None => Some(host),
                        }
                    })
                }),
        };

        let ephemeral_ttl_override = std::env::var("SPROUT_EPHEMERAL_TTL_OVERRIDE")
            .ok()
            .and_then(|v| v.parse::<i32>().ok())
            .filter(|&v| v > 0);

        if let Some(ttl) = ephemeral_ttl_override {
            warn!(
                "SPROUT_EPHEMERAL_TTL_OVERRIDE={ttl}s — all ephemeral channels will use \
                 this TTL instead of the client-provided value."
            );
        }

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
            health_port,
            metrics_port,
            pubkey_allowlist_enabled,
            media,
            ephemeral_ttl_override,
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
        assert!(
            !config.pubkey_allowlist_enabled,
            "pubkey_allowlist_enabled should default to false"
        );
    }

    #[test]
    fn invalid_bind_addr_returns_error() {
        let _guard = ENV_MUTEX.lock().unwrap();
        std::env::set_var("SPROUT_BIND_ADDR", "not-an-addr");
        let result = Config::from_env();
        std::env::remove_var("SPROUT_BIND_ADDR");
        assert!(matches!(result, Err(ConfigError::InvalidBindAddr(_))));
    }

    #[test]
    fn server_domain_auto_derived_from_relay_url() {
        let _guard = ENV_MUTEX.lock().unwrap();
        // Clear explicit override so auto-derive kicks in
        std::env::remove_var("SPROUT_MEDIA_SERVER_DOMAIN");
        std::env::set_var("RELAY_URL", "ws://localhost:3000");
        let config = Config::from_env().expect("config");
        std::env::remove_var("RELAY_URL");
        assert_eq!(
            config.media.server_domain.as_deref(),
            Some("localhost:3000")
        );
    }

    #[test]
    fn server_domain_auto_derived_default_port() {
        let _guard = ENV_MUTEX.lock().unwrap();
        std::env::remove_var("SPROUT_MEDIA_SERVER_DOMAIN");
        std::env::set_var("RELAY_URL", "wss://relay.example.com");
        let config = Config::from_env().expect("config");
        std::env::remove_var("RELAY_URL");
        assert_eq!(
            config.media.server_domain.as_deref(),
            Some("relay.example.com")
        );
    }

    #[test]
    fn server_domain_explicit_override_wins() {
        let _guard = ENV_MUTEX.lock().unwrap();
        std::env::set_var("SPROUT_MEDIA_SERVER_DOMAIN", "custom.example.com");
        std::env::set_var("RELAY_URL", "ws://localhost:3000");
        let config = Config::from_env().expect("config");
        std::env::remove_var("SPROUT_MEDIA_SERVER_DOMAIN");
        std::env::remove_var("RELAY_URL");
        assert_eq!(
            config.media.server_domain.as_deref(),
            Some("custom.example.com")
        );
    }
}
