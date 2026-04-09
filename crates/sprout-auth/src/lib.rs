#![deny(unsafe_code)]
#![warn(missing_docs)]
//! `sprout-auth` — Authentication and authorization for the Sprout relay.
//!
//! ## Auth paths
//!
//! | Path | Transport | Description |
//! |------|-----------|-------------|
//! | NIP-42 | WebSocket | Challenge/response; client signs kind:22242 event |
//! | Okta JWT | NIP-42 `auth_token` tag | SSO via Okta JWKS validation |
//! | API token | NIP-42 `auth_token` tag | Hash stored in DB; see below |
//!
//! ## Security invariants
//!
//! - **AUTH events (kind:22242) are NEVER stored or logged.**
//! - All paths produce an [`AuthContext`] bound to the WebSocket connection.

/// Channel access checking trait and helpers.
pub mod access;
/// Authentication error types.
pub mod error;
/// Corporate identity mode (proxy-injected JWTs, identity claims extraction).
pub mod identity;
/// NIP-42 challenge–response authentication.
pub mod nip42;
/// NIP-98 HTTP Auth verification (kind:27235).
pub mod nip98;
/// Okta OIDC integration and JWKS validation.
pub mod okta;
/// Per-connection rate limiting.
pub mod rate_limit;
/// OAuth scope parsing and enforcement.
pub mod scope;
/// API token hashing and verification.
pub mod token;

pub use access::{check_read_access, check_write_access, require_scope, ChannelAccessChecker};
pub use error::AuthError;
pub use identity::{IdentityConfig, IdentityMode, ProxyIdentityClaims};
pub use nip42::{generate_challenge, verify_nip42_event};
pub use nip98::verify_nip98_event;
pub use okta::{CachedJwks, Jwks, JwksCache, OktaConfig};
pub use rate_limit::{
    ip_rate_limit_key, rate_limit_key, LimitType, RateLimitConfig, RateLimitResult, RateLimiter,
};
pub use scope::{is_self_mintable, parse_scopes, Scope, SELF_MINTABLE_SCOPES};
pub use token::{generate_token, hash_token, verify_token_hash};

#[cfg(any(test, feature = "test-utils"))]
pub use access::MockAccessChecker;
#[cfg(any(test, feature = "test-utils"))]
pub use rate_limit::AlwaysAllowRateLimiter;

/// How the connection was authenticated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthMethod {
    /// NIP-42 challenge/response only — no JWT or API token present.
    ///
    /// Only possible when `require_token = false` (dev/open-relay mode).
    Nip42PubkeyOnly,
    /// NIP-42 with an Okta JWT bearer token in the `auth_token` tag.
    Nip42Okta,
    /// NIP-42 with a `sprout_` API token in the `auth_token` tag.
    Nip42ApiToken,
    /// Proxy identity — pubkey derived from a proxy-injected identity JWT.
    ProxyIdentity,
}

/// The result of a successful authentication, bound to a WebSocket connection.
#[derive(Debug, Clone)]
pub struct AuthContext {
    /// The authenticated Nostr public key.
    pub pubkey: nostr::PublicKey,
    /// Permission scopes granted to this connection.
    pub scopes: Vec<Scope>,
    /// Token-level channel restriction, if authentication used a scoped API token.
    ///
    /// `None` means unrestricted or not token-authenticated.
    pub channel_ids: Option<Vec<uuid::Uuid>>,
    /// How the connection was authenticated.
    pub auth_method: AuthMethod,
}

impl AuthContext {
    /// Returns `true` if this context includes the given [`Scope`].
    pub fn has_scope(&self, scope: &Scope) -> bool {
        self.scopes.contains(scope)
    }
}

/// Top-level authentication configuration, typically loaded from the relay's TOML config file.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AuthConfig {
    /// Okta OIDC settings (issuer, audience, JWKS URI, etc.).
    #[serde(default)]
    pub okta: OktaConfig,
    /// Per-user and per-IP rate limit thresholds.
    #[serde(default)]
    pub rate_limits: RateLimitConfig,
    /// Corporate identity mode (proxy JWT, identity claims extraction).
    #[serde(default)]
    pub identity: IdentityConfig,
}

/// Primary authentication service.
///
/// Holds shared state (JWKS cache, HTTP client, config). Clone-cheap (Arc internals).
///
/// **API token auth** is not handled here — `AuthService` has no database access.
/// The relay layer must intercept API tokens from the `auth_token` tag and call
/// [`AuthService::verify_api_token_against_hash`] after fetching the token record.
#[derive(Debug, Clone)]
pub struct AuthService {
    config: AuthConfig,
    jwks_cache: std::sync::Arc<JwksCache>,
    http_client: reqwest::Client,
}

impl AuthService {
    /// Create a new `AuthService` with the given configuration.
    ///
    /// Initialises a fresh JWKS cache and a shared `reqwest::Client`.
    /// Intended to be constructed once at startup and shared via `Arc<AuthService>`.
    pub fn new(config: AuthConfig) -> Self {
        Self {
            config,
            jwks_cache: JwksCache::new(),
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .connect_timeout(std::time::Duration::from_secs(5))
                .build()
                .expect("SAFETY: default builder with timeout config cannot fail"),
        }
    }

    /// Verify a NIP-42 AUTH event and return an [`AuthContext`].
    ///
    /// Validates event structure, signature, challenge, relay URL, timestamp,
    /// then dispatches to Okta JWT validation if a bearer token is present.
    /// The `auth_event` is **not** retained after this call.
    pub async fn verify_auth_event(
        &self,
        auth_event: nostr::Event,
        expected_challenge: &str,
        relay_url: &str,
    ) -> Result<AuthContext, AuthError> {
        let event_clone = auth_event.clone();
        let challenge_owned = expected_challenge.to_string();
        let relay_owned = relay_url.to_string();
        tokio::task::spawn_blocking(move || {
            verify_nip42_event(&event_clone, &challenge_owned, &relay_owned)
        })
        .await
        .map_err(|_| AuthError::Internal("spawn_blocking panicked".into()))??;

        // ⚠️ SECURITY: Do NOT log auth_token — it contains a bearer token.
        let auth_token = auth_event
            .tags
            .iter()
            .find(|t| t.kind().to_string() == "auth_token")
            .and_then(|t| t.content())
            .map(|s| s.to_string());

        let (verified_pubkey, scopes, auth_method) = match auth_token.as_deref() {
            Some(token) if token.starts_with("eyJ") => {
                let (pk, sc) = self.verify_okta_jwt(token, &auth_event.pubkey).await?;
                (pk, sc, AuthMethod::Nip42Okta)
            }
            Some(_) => {
                // API tokens require a DB lookup the relay must perform before
                // calling verify_auth_event. Reaching here means the relay
                // hasn't intercepted the token.
                return Err(AuthError::TokenInvalid);
            }
            None => {
                if self.config.okta.require_token {
                    return Err(AuthError::InvalidJwt(
                        "auth_token tag required in production mode".into(),
                    ));
                }
                // Default-open: no token present and require_token=false.
                // Grant all scopes so the connection is fully usable in dev mode.
                // The ingest pipeline enforces per-kind scope checks, so NIP-42
                // pubkey-only connections need the full set — including admin
                // scopes for kind:9000 (add member), kind:9001 (remove member),
                // kind:9008 (delete group), etc.
                (
                    auth_event.pubkey,
                    Scope::all_known(),
                    AuthMethod::Nip42PubkeyOnly,
                )
            }
        };

        if verified_pubkey != auth_event.pubkey {
            return Err(AuthError::PubkeyMismatch);
        }

        Ok(AuthContext {
            pubkey: verified_pubkey,
            scopes,
            channel_ids: None,
            auth_method,
        })
    }

    async fn verify_okta_jwt(
        &self,
        jwt: &str,
        claimed_pubkey: &nostr::PublicKey,
    ) -> Result<(nostr::PublicKey, Vec<Scope>), AuthError> {
        let cached = self
            .jwks_cache
            .get_or_refresh(
                &self.config.okta.jwks_uri,
                self.config.okta.jwks_refresh_secs,
                &self.http_client,
            )
            .await?;

        let claims = cached.validate(jwt, &self.config.okta.issuer, &self.config.okta.audience)?;

        let pubkey_hex = claims
            .get(&self.config.okta.pubkey_claim)
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AuthError::InvalidJwt(format!(
                    "missing '{}' claim in JWT",
                    self.config.okta.pubkey_claim
                ))
            })?;

        let pubkey = nostr::PublicKey::from_hex(pubkey_hex)
            .map_err(|_| AuthError::InvalidJwt("invalid pubkey hex in JWT claim".into()))?;

        if &pubkey != claimed_pubkey {
            return Err(AuthError::PubkeyMismatch);
        }

        let scopes = extract_scopes_from_claims(&claims);
        Ok((pubkey, scopes))
    }

    /// Validate a raw JWT Bearer token (no Nostr event wrapper).
    ///
    /// Returns the authenticated pubkey and scopes. Used by HTTP REST API endpoints
    /// where there is no NIP-42 Nostr event to compare against — only a raw JWT.
    ///
    /// Reuses the existing JWKS validation logic but skips the pubkey cross-check
    /// (there is no claimed_pubkey from a Nostr event in the HTTP path).
    pub async fn validate_bearer_jwt(
        &self,
        jwt: &str,
    ) -> Result<(nostr::PublicKey, Vec<Scope>), AuthError> {
        let cached = self
            .jwks_cache
            .get_or_refresh(
                &self.config.okta.jwks_uri,
                self.config.okta.jwks_refresh_secs,
                &self.http_client,
            )
            .await?;

        let claims = cached.validate(jwt, &self.config.okta.issuer, &self.config.okta.audience)?;

        let pubkey_hex = claims
            .get(&self.config.okta.pubkey_claim)
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AuthError::InvalidJwt(format!(
                    "missing '{}' claim in JWT",
                    self.config.okta.pubkey_claim
                ))
            })?;

        let pubkey = nostr::PublicKey::from_hex(pubkey_hex)
            .map_err(|_| AuthError::InvalidJwt("invalid pubkey hex in JWT claim".into()))?;

        // Extract scopes from the JWT claims. `extract_scopes_from_claims` returns
        // `[MessagesRead]` when no scope claim is present (read-only safe default).
        // HTTP REST callers additionally need `ChannelsRead` to list channels, so we
        // always ensure that scope is present regardless of what the token says.
        let scopes = {
            let mut extracted = extract_scopes_from_claims(&claims);
            if !extracted.contains(&Scope::ChannelsRead) {
                extracted.push(Scope::ChannelsRead);
            }
            extracted
        };

        Ok((pubkey, scopes))
    }

    /// Verify a raw API token against a pre-fetched hash from the database.
    ///
    /// The relay layer is responsible for fetching the token record (hash, owner pubkey,
    /// expiry, scopes) from the database before calling this method. This keeps
    /// `sprout-auth` free of database dependencies.
    ///
    /// Returns `(owner_pubkey, scopes)` on success.
    ///
    /// # Errors
    ///
    /// - [`AuthError::TokenInvalid`] — hash mismatch or token expired.
    /// - [`AuthError::PubkeyMismatch`] — `claimed_pubkey` does not match the token owner.
    pub fn verify_api_token_against_hash(
        &self,
        raw_token: &str,
        stored_hash: &[u8],
        owner_pubkey: &nostr::PublicKey,
        claimed_pubkey: &nostr::PublicKey,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
        scopes_raw: &[String],
    ) -> Result<(nostr::PublicKey, Vec<Scope>), AuthError> {
        if !verify_token_hash(raw_token, stored_hash) {
            return Err(AuthError::TokenInvalid);
        }

        if let Some(exp) = expires_at {
            if exp < chrono::Utc::now() {
                return Err(AuthError::TokenInvalid);
            }
        }

        if owner_pubkey != claimed_pubkey {
            return Err(AuthError::PubkeyMismatch);
        }

        let scopes = parse_scopes(scopes_raw);
        Ok((*owner_pubkey, scopes))
    }

    /// Returns a reference to the identity configuration.
    pub fn identity_config(&self) -> &IdentityConfig {
        &self.config.identity
    }

    /// Validate a proxy-injected identity JWT and extract the corporate identity claims.
    ///
    /// Used in proxy identity mode where cf-doorman injects `x-forwarded-identity-token`.
    /// Validates the JWT via JWKS (same infrastructure as Okta), extracts the `uid` and
    /// `user` claims.
    ///
    /// Returns `(claims, all_known_scopes)` on success.
    pub async fn validate_identity_jwt(
        &self,
        jwt: &str,
    ) -> Result<(identity::ProxyIdentityClaims, Vec<Scope>), AuthError> {
        let cached = self
            .jwks_cache
            .get_or_refresh(
                &self.config.okta.jwks_uri,
                self.config.okta.jwks_refresh_secs,
                &self.http_client,
            )
            .await?;

        let claims = cached.validate(jwt, &self.config.okta.issuer, &self.config.okta.audience)?;

        let uid = claims
            .get(&self.config.identity.uid_claim)
            .and_then(|v| {
                v.as_str()
                    .map(String::from)
                    .or_else(|| v.as_u64().map(|n| n.to_string()))
            })
            .ok_or_else(|| {
                AuthError::InvalidJwt(format!(
                    "missing '{}' claim in identity JWT",
                    self.config.identity.uid_claim
                ))
            })?;

        let username = claims
            .get(&self.config.identity.user_claim)
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let scopes = Scope::all_known();

        Ok((identity::ProxyIdentityClaims { uid, username }, scopes))
    }
}

/// Derive a deterministic Nostr pubkey from a username string.
///
/// Uses `SHA-256("sprout-test-key:{username}")` as the secret key material.
/// This matches the derivation used by the desktop's `set_test_identity` function,
/// allowing the relay to resolve Keycloak usernames to Nostr pubkeys in dev mode.
///
/// # ⚠️ SECURITY — Dev/test only
///
/// This function is gated behind `#[cfg(any(test, feature = "dev"))]`
/// and **must never be compiled into a production release build**.
///
/// - The derived keys are deterministic and predictable from the username alone.
/// - Any attacker who knows a username can compute the corresponding private key.
/// - In production, JWTs must contain a real `nostr_pubkey` claim issued by Okta.
///
/// ## When it is compiled in
///
/// | Build command | Included? | Reason |
/// |---|---|---|
/// | `cargo test` | ✅ Yes | `test` cfg |
/// | `cargo build` (debug) | ❌ No | Not included without `dev` feature |
/// | `cargo build --release` | ❌ No | Neither `test` nor `dev` feature |
/// | `cargo build --release --features dev` | ✅ Yes | `dev` feature — use only for integration harnesses |
///
/// ## The `dev` feature
///
/// The `dev` feature exists solely to enable this function (and other dev-mode
/// helpers) in release-mode integration test harnesses. It must **not** be
/// enabled in production relay deployments. Check `sprout-relay/Cargo.toml` to
/// ensure `sprout-auth` is not listed with `features = ["dev"]` in production.
#[cfg(any(test, feature = "dev"))]
pub fn derive_pubkey_from_username(username: &str) -> Result<nostr::PublicKey, AuthError> {
    use sha2::{Digest, Sha256};
    let seed = format!("sprout-test-key:{username}");
    let hash: [u8; 32] = Sha256::digest(seed.as_bytes()).into();
    let secret_key = nostr::SecretKey::from_slice(&hash)
        .map_err(|e| AuthError::Internal(format!("key derivation failed: {e}")))?;
    Ok(nostr::Keys::new(secret_key).public_key())
}

/// Extract scopes from JWT claims (`scp` array or `scope` space-delimited string).
///
/// Checks `scp` (Okta array format) first, then `scope` (RFC 8693 space-delimited string).
///
/// # Missing scope claim
///
/// When a **valid, signature-verified JWT** contains no `scp` or `scope` claim at all,
/// this function returns **read-only** (`[MessagesRead]`). This is a deliberate
/// security default: a token that omits scopes entirely should not silently gain
/// write access. Production Okta configurations must include explicit scope claims.
///
/// Note: the `None` (no token) path in [`AuthService::verify_auth_event`] grants
/// `[MessagesRead, MessagesWrite]` when `require_token = false` — that is a
/// separate, intentional dev-mode behaviour documented there.
fn extract_scopes_from_claims(
    claims: &std::collections::HashMap<String, serde_json::Value>,
) -> Vec<Scope> {
    if let Some(scp) = claims.get("scp").and_then(|v| v.as_array()) {
        let raw: Vec<String> = scp
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        return parse_scopes(&raw);
    }
    if let Some(scope_str) = claims.get("scope").and_then(|v| v.as_str()) {
        let raw: Vec<String> = scope_str.split_whitespace().map(str::to_string).collect();
        return parse_scopes(&raw);
    }
    // JWT is valid (signature verified) but contains no scope claim.
    // Default to read-only — never silently grant write access from a scopeless token.
    // Production Okta configs must include explicit `scp` or `scope` claims.
    vec![Scope::MessagesRead]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token;
    use nostr::{EventBuilder, Keys, Kind, Url};

    fn make_auth_event(keys: &Keys, challenge: &str, relay_url: &str) -> nostr::Event {
        let url: Url = relay_url.parse().expect("valid url");
        EventBuilder::auth(challenge, url)
            .sign_with_keys(keys)
            .expect("signing failed")
    }

    fn open_mode_service() -> AuthService {
        let mut config = AuthConfig::default();
        config.okta.require_token = false;
        AuthService::new(config)
    }

    #[test]
    fn auth_context_scope_check() {
        let keys = Keys::generate();
        let ctx = AuthContext {
            pubkey: keys.public_key(),
            scopes: vec![Scope::MessagesRead, Scope::ChannelsRead],
            channel_ids: None,
            auth_method: AuthMethod::Nip42PubkeyOnly,
        };
        assert!(ctx.has_scope(&Scope::MessagesRead));
        assert!(!ctx.has_scope(&Scope::MessagesWrite));
    }

    #[tokio::test]
    async fn open_mode_auth_succeeds() {
        let keys = Keys::generate();
        let challenge = generate_challenge();
        let relay = "wss://relay.example.com";
        let event = make_auth_event(&keys, &challenge, relay);

        let ctx = open_mode_service()
            .verify_auth_event(event, &challenge, relay)
            .await
            .expect("open-mode auth should succeed");

        assert_eq!(ctx.pubkey, keys.public_key());
        assert_eq!(ctx.auth_method, AuthMethod::Nip42PubkeyOnly);
        assert!(ctx.has_scope(&Scope::MessagesRead));
        assert!(ctx.has_scope(&Scope::MessagesWrite));
    }

    #[tokio::test]
    async fn wrong_challenge_rejected() {
        let keys = Keys::generate();
        let challenge = generate_challenge();
        let relay = "wss://relay.example.com";
        let event = make_auth_event(&keys, &challenge, relay);

        let result = open_mode_service()
            .verify_auth_event(event, "wrong-challenge", relay)
            .await;
        assert!(matches!(result, Err(AuthError::ChallengeMismatch)));
    }

    #[tokio::test]
    async fn wrong_kind_rejected() {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::TextNote, "not auth", [])
            .sign_with_keys(&keys)
            .expect("sign");

        let result = open_mode_service()
            .verify_auth_event(event, &generate_challenge(), "wss://relay.example.com")
            .await;
        assert!(matches!(result, Err(AuthError::InvalidSignature)));
    }

    #[tokio::test]
    async fn require_token_enforced() {
        let keys = Keys::generate();
        let challenge = generate_challenge();
        let relay = "wss://relay.example.com";
        let event = make_auth_event(&keys, &challenge, relay);

        let result = AuthService::new(AuthConfig::default())
            .verify_auth_event(event, &challenge, relay)
            .await;
        assert!(matches!(result, Err(AuthError::InvalidJwt(_))));
    }

    #[test]
    fn extract_scopes_from_scp_array() {
        let mut claims = std::collections::HashMap::new();
        claims.insert(
            "scp".to_string(),
            serde_json::json!(["messages:read", "channels:write"]),
        );
        let scopes = extract_scopes_from_claims(&claims);
        assert!(scopes.contains(&Scope::MessagesRead));
        assert!(scopes.contains(&Scope::ChannelsWrite));
    }

    #[test]
    fn extract_scopes_from_scope_string() {
        let mut claims = std::collections::HashMap::new();
        claims.insert(
            "scope".to_string(),
            serde_json::json!("messages:read messages:write"),
        );
        let scopes = extract_scopes_from_claims(&claims);
        assert!(scopes.contains(&Scope::MessagesRead));
        assert!(scopes.contains(&Scope::MessagesWrite));
    }

    #[test]
    fn extract_scopes_defaults_when_absent() {
        // A JWT with no scope claim should default to read-only, NOT read+write.
        // Silently granting write access from a scopeless token would be a privilege escalation.
        let scopes = extract_scopes_from_claims(&std::collections::HashMap::new());
        assert!(scopes.contains(&Scope::MessagesRead));
        assert!(
            !scopes.contains(&Scope::MessagesWrite),
            "scopeless JWT must NOT grant write access"
        );
        assert_eq!(scopes.len(), 1, "default is exactly [MessagesRead]");
    }

    #[test]
    fn verify_api_token_valid() {
        let service = open_mode_service();
        let keys = Keys::generate();
        let pubkey = keys.public_key();

        let raw = token::generate_token();
        let hash = token::hash_token(&raw);
        let scopes_raw = vec!["messages:read".to_string(), "messages:write".to_string()];

        let result =
            service.verify_api_token_against_hash(&raw, &hash, &pubkey, &pubkey, None, &scopes_raw);
        assert!(result.is_ok());
        let (pk, scopes) = result.unwrap();
        assert_eq!(pk, pubkey);
        assert!(scopes.contains(&Scope::MessagesRead));
        assert!(scopes.contains(&Scope::MessagesWrite));
    }

    #[test]
    fn verify_api_token_wrong_hash_rejected() {
        let service = open_mode_service();
        let keys = Keys::generate();
        let pubkey = keys.public_key();

        let raw = token::generate_token();
        let wrong_hash = token::hash_token("not-the-right-token");

        let result =
            service.verify_api_token_against_hash(&raw, &wrong_hash, &pubkey, &pubkey, None, &[]);
        assert!(matches!(result, Err(AuthError::TokenInvalid)));
    }

    #[test]
    fn verify_api_token_expired_rejected() {
        let service = open_mode_service();
        let keys = Keys::generate();
        let pubkey = keys.public_key();

        let raw = token::generate_token();
        let hash = token::hash_token(&raw);
        let expired = chrono::Utc::now() - chrono::Duration::seconds(1);

        let result = service.verify_api_token_against_hash(
            &raw,
            &hash,
            &pubkey,
            &pubkey,
            Some(expired),
            &[],
        );
        assert!(matches!(result, Err(AuthError::TokenInvalid)));
    }

    #[test]
    fn verify_api_token_pubkey_mismatch_rejected() {
        let service = open_mode_service();
        let owner_keys = Keys::generate();
        let claimed_keys = Keys::generate();

        let raw = token::generate_token();
        let hash = token::hash_token(&raw);

        let result = service.verify_api_token_against_hash(
            &raw,
            &hash,
            &owner_keys.public_key(),
            &claimed_keys.public_key(),
            None,
            &[],
        );
        assert!(matches!(result, Err(AuthError::PubkeyMismatch)));
    }
}
