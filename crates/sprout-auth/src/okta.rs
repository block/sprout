//! Okta JWT validation via JWKS.
//!
//! Fetches and caches the JWKS, validates JWTs (signature, expiry, issuer, audience).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::RwLock;
use tracing::{debug, warn};

use crate::error::AuthError;

/// Default TTL for the JWKS cache in seconds (5 minutes).
///
/// After this interval the next auth attempt will trigger a background re-fetch.
/// Tune via [`OktaConfig::jwks_refresh_secs`] for environments with faster key rotation.
pub const JWKS_CACHE_TTL_SECS: u64 = 300;

/// A JSON Web Key Set as returned by the OIDC `/keys` endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct Jwks {
    /// The list of public keys in this set.
    pub keys: Vec<Jwk>,
}

/// A single JSON Web Key (RSA or EC public key).
///
/// Only the fields required for signature verification are used.
/// Unknown fields are ignored during deserialization.
#[derive(Debug, Clone, Deserialize)]
pub struct Jwk {
    /// Key type: `"RSA"` or `"EC"`.
    pub kty: String,
    /// Key ID — matched against the JWT `kid` header to select the right key.
    pub kid: Option<String>,
    /// Algorithm hint (e.g. `"RS256"`, `"ES256"`).
    pub alg: Option<String>,
    /// RSA modulus (base64url-encoded).
    pub n: Option<String>,
    /// RSA public exponent (base64url-encoded).
    pub e: Option<String>,
    /// EC curve name (e.g. `"P-256"`).
    pub crv: Option<String>,
    /// EC public key x-coordinate (base64url-encoded).
    pub x: Option<String>,
    /// EC public key y-coordinate (base64url-encoded).
    pub y: Option<String>,
}

/// A fetched JWKS together with the [`Instant`] it was retrieved.
///
/// Used by [`JwksCache`] to determine whether the cached keys are still fresh.
#[derive(Debug, Clone)]
pub struct CachedJwks {
    /// The fetched key set.
    pub jwks: Jwks,
    /// Wall-clock time at which this entry was populated.
    pub fetched_at: Instant,
}

impl CachedJwks {
    /// Validate a JWT and return decoded claims.
    pub fn validate(
        &self,
        jwt: &str,
        issuer: &str,
        audience: &str,
    ) -> Result<HashMap<String, Value>, AuthError> {
        let header = decode_header(jwt)
            .map_err(|e| AuthError::InvalidJwt(format!("bad jwt header: {e}")))?;

        let kid = header.kid.as_deref();
        let jwk = self
            .find_key(kid, &header.alg)
            .ok_or_else(|| AuthError::InvalidJwt("no matching key in JWKS".into()))?;

        let decoding_key = Self::decoding_key_from_jwk(jwk)?;

        let mut validation = Validation::new(header.alg);
        validation.set_issuer(&[issuer]);
        validation.set_audience(&[audience]);

        let token_data = decode::<HashMap<String, Value>>(jwt, &decoding_key, &validation)
            .map_err(|e| AuthError::InvalidJwt(format!("jwt validation failed: {e}")))?;

        Ok(token_data.claims)
    }

    fn find_key(&self, kid: Option<&str>, alg: &Algorithm) -> Option<&Jwk> {
        self.jwks.keys.iter().find(|k| {
            let kid_match = kid.is_none_or(|id| k.kid.as_deref() == Some(id));
            let alg_match = k.alg.as_ref().is_none_or(|a| matches_algorithm(a, alg));
            kid_match && alg_match
        })
    }

    fn decoding_key_from_jwk(jwk: &Jwk) -> Result<DecodingKey, AuthError> {
        match jwk.kty.as_str() {
            "RSA" => {
                let n = jwk
                    .n
                    .as_deref()
                    .ok_or_else(|| AuthError::InvalidJwt("RSA key missing 'n'".into()))?;
                let e = jwk
                    .e
                    .as_deref()
                    .ok_or_else(|| AuthError::InvalidJwt("RSA key missing 'e'".into()))?;
                DecodingKey::from_rsa_components(n, e)
                    .map_err(|e| AuthError::InvalidJwt(format!("invalid RSA key: {e}")))
            }
            "EC" => {
                let x = jwk
                    .x
                    .as_deref()
                    .ok_or_else(|| AuthError::InvalidJwt("EC key missing 'x'".into()))?;
                let y = jwk
                    .y
                    .as_deref()
                    .ok_or_else(|| AuthError::InvalidJwt("EC key missing 'y'".into()))?;
                DecodingKey::from_ec_components(x, y)
                    .map_err(|e| AuthError::InvalidJwt(format!("invalid EC key: {e}")))
            }
            other => Err(AuthError::InvalidJwt(format!(
                "unsupported key type: {other}"
            ))),
        }
    }

    /// Returns `true` if this entry was fetched within the last `ttl_secs` seconds.
    pub fn is_fresh(&self, ttl_secs: u64) -> bool {
        self.fetched_at.elapsed() < Duration::from_secs(ttl_secs)
    }
}

/// Returns `true` if the string `alg_str` (from a JWK's `alg` field) matches
/// the [`Algorithm`] decoded from the JWT header.
fn matches_algorithm(alg_str: &str, alg: &Algorithm) -> bool {
    match alg {
        Algorithm::RS256 => alg_str == "RS256",
        Algorithm::RS384 => alg_str == "RS384",
        Algorithm::RS512 => alg_str == "RS512",
        Algorithm::ES256 => alg_str == "ES256",
        Algorithm::ES384 => alg_str == "ES384",
        Algorithm::PS256 => alg_str == "PS256",
        Algorithm::PS384 => alg_str == "PS384",
        Algorithm::PS512 => alg_str == "PS512",
        _ => false,
    }
}

/// Thread-safe in-process JWKS cache. Wrap in `Arc` and share across tasks.
///
/// Uses double-checked locking with an **unlocked HTTP fetch** to prevent two
/// failure modes simultaneously:
///
/// 1. **Thundering herd**: N concurrent cache misses each triggering N HTTP
///    requests. The final write-lock re-check ensures only one result is stored.
///
/// 2. **Global DoS via lock-held fetch** *(the bug this design avoids)*: holding
///    the write lock across the HTTP call would block every reader (every in-flight
///    auth attempt) for the full duration of the OIDC endpoint round-trip. If the
///    endpoint is slow or unreachable, the relay becomes completely unavailable.
///
/// The trade-off: two concurrent stale-cache threads may both fetch from the OIDC
/// endpoint. This is safe — fetches are idempotent and the second writer simply
/// finds a fresh entry and discards its result.
#[derive(Debug, Default)]
pub struct JwksCache {
    inner: RwLock<Option<CachedJwks>>,
}

impl JwksCache {
    /// Create a new empty cache wrapped in an `Arc`.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: RwLock::new(None),
        })
    }

    /// Return cached JWKS if fresh, otherwise fetch and cache a new one.
    ///
    /// # Locking protocol
    ///
    /// 1. Acquire **read** lock → return if fresh (fast path, no contention).
    /// 2. Drop read lock.
    /// 3. Acquire **read** lock again → re-check freshness (another thread may
    ///    have already refreshed while we were waiting).
    /// 4. Drop read lock.
    /// 5. Fetch JWKS with **no lock held** — readers are never blocked.
    /// 6. Acquire **write** lock → re-check one final time, then store if still stale.
    ///
    /// Step 5 is the critical fix: the HTTP fetch never holds the write lock,
    /// so readers are never blocked by a slow or hung OIDC endpoint.
    /// Two concurrent threads may both fetch; that is intentional and safe
    /// (idempotent). The write-lock re-check in step 6 ensures only one result
    /// is stored.
    pub async fn get_or_refresh(
        &self,
        jwks_uri: &str,
        ttl_secs: u64,
        client: &reqwest::Client,
    ) -> Result<CachedJwks, AuthError> {
        {
            let guard = self.inner.read().await;
            if let Some(cached) = guard.as_ref() {
                if cached.is_fresh(ttl_secs) {
                    debug!("JWKS cache hit");
                    return Ok(cached.clone());
                }
            }
        }

        // Pre-fetch re-check: another thread may have refreshed between our
        // first read-lock drop and now. Use a second read lock (not write) so
        // we don't block other readers while deciding whether to fetch.
        {
            let guard = self.inner.read().await;
            if let Some(cached) = guard.as_ref() {
                if cached.is_fresh(ttl_secs) {
                    debug!("JWKS cache hit (pre-fetch re-check)");
                    return Ok(cached.clone());
                }
            }
        }

        // *** CRITICAL: fetch with NO lock held ***
        //
        // Holding the write lock across an HTTP call would block ALL readers
        // (every in-flight auth attempt) for the entire round-trip duration.
        // A slow or hung OIDC endpoint would cause a global relay DoS.
        //
        // Two concurrent threads may both reach this point and both issue a
        // fetch. That is intentional — fetches are idempotent. The write-lock
        // re-check below ensures only one result is stored.
        debug!("JWKS cache miss — fetching from {jwks_uri}");
        let jwks = fetch_jwks(jwks_uri, client).await?;
        let fetched = CachedJwks {
            jwks,
            fetched_at: Instant::now(),
        };

        // Final re-check: another thread may have stored a fresh entry while
        // we were fetching. If so, discard our result and return theirs.
        let mut guard = self.inner.write().await;
        if let Some(cached) = guard.as_ref() {
            if cached.is_fresh(ttl_secs) {
                debug!("JWKS cache hit (stored by concurrent fetcher — discarding our result)");
                return Ok(cached.clone());
            }
        }
        *guard = Some(fetched.clone());
        Ok(fetched)
    }
}

async fn fetch_jwks(uri: &str, client: &reqwest::Client) -> Result<Jwks, AuthError> {
    let response = client
        .get(uri)
        .send()
        .await
        .map_err(|e| AuthError::JwksFetchError(format!("request failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        warn!("JWKS fetch returned HTTP {status}");
        return Err(AuthError::JwksFetchError(format!(
            "HTTP {status} from JWKS endpoint"
        )));
    }

    response
        .json::<Jwks>()
        .await
        .map_err(|e| AuthError::JwksFetchError(format!("failed to parse JWKS: {e}")))
}

/// Okta OIDC configuration for JWT validation.
///
/// Loaded from relay config (TOML/env). All fields except `pubkey_claim`,
/// `jwks_refresh_secs`, and `require_token` are required in production.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OktaConfig {
    /// Expected `iss` claim in incoming JWTs (e.g. `https://example.okta.com/oauth2/default`).
    pub issuer: String,
    /// Expected `aud` claim in incoming JWTs (the Okta application client ID or custom audience).
    pub audience: String,
    /// URL of the OIDC JWKS endpoint (e.g. `https://example.okta.com/oauth2/default/v1/keys`).
    pub jwks_uri: String,
    /// JWT claim name that holds the user's Nostr public key (hex). Default: `"nostr_pubkey"`.
    pub pubkey_claim: String,
    /// How often to refresh the JWKS cache, in seconds. Default: 300 (5 minutes).
    #[serde(default = "default_jwks_refresh_secs")]
    pub jwks_refresh_secs: u64,
    /// If `true` (production default), every NIP-42 AUTH event must include an `auth_token` tag
    /// containing a valid JWT or API token. If `false` (dev/open-relay mode), connections without
    /// a token are accepted and granted baseline `[MessagesRead, MessagesWrite]` scopes.
    ///
    /// ⚠️ **Never set `require_token = false` in production.** It disables all token-based
    /// authentication and allows any Nostr keypair to connect and send messages.
    #[serde(default = "default_require_token")]
    pub require_token: bool,
}

fn default_jwks_refresh_secs() -> u64 {
    300
}
fn default_require_token() -> bool {
    true
}

impl Default for OktaConfig {
    fn default() -> Self {
        Self {
            issuer: String::new(),
            audience: String::new(),
            jwks_uri: String::new(),
            pubkey_claim: "nostr_pubkey".into(),
            jwks_refresh_secs: 300,
            require_token: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn okta_config_defaults() {
        let cfg = OktaConfig::default();
        assert_eq!(cfg.pubkey_claim, "nostr_pubkey");
        assert_eq!(cfg.jwks_refresh_secs, 300);
        assert!(cfg.require_token);
    }

    #[test]
    fn jwks_freshness() {
        let fresh = CachedJwks {
            jwks: Jwks { keys: vec![] },
            fetched_at: Instant::now(),
        };
        assert!(fresh.is_fresh(300));

        let stale = CachedJwks {
            jwks: Jwks { keys: vec![] },
            fetched_at: Instant::now() - Duration::from_secs(400),
        };
        assert!(!stale.is_fresh(300));
    }
}
