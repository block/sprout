// ABOUTME: API token management — mint, cache, refresh, and revoke.
// ABOUTME: Tokens are cached in SQLite with expiry tracking.

use crate::error::SproutError;
use crate::store::Store;

/// KV key for the cached API token.
const KV_API_TOKEN: &str = "api_token";
/// KV key for the token expiry timestamp.
const KV_TOKEN_EXPIRES_AT: &str = "api_token_expires_at";

/// Token cache operations backed by SQLite key-value store.
pub(crate) struct TokenCache;

impl TokenCache {
    /// Store a token in the cache with optional expiry.
    pub fn save(store: &Store, token: &str, expires_at: Option<i64>) -> Result<(), SproutError> {
        store.kv_set(KV_API_TOKEN, token)?;
        if let Some(ts) = expires_at {
            store.kv_set(KV_TOKEN_EXPIRES_AT, &ts.to_string())?;
        } else {
            store.kv_delete(KV_TOKEN_EXPIRES_AT)?;
        }
        Ok(())
    }

    /// Load the cached token, returning None if missing or expired.
    pub fn load(store: &Store) -> Result<Option<String>, SproutError> {
        let token = match store.kv_get(KV_API_TOKEN)? {
            Some(t) => t,
            None => return Ok(None),
        };

        // Check expiry.
        if let Some(expires_str) = store.kv_get(KV_TOKEN_EXPIRES_AT)? {
            if let Ok(expires_at) = expires_str.parse::<i64>() {
                let now = chrono::Utc::now().timestamp();
                if now >= expires_at {
                    // Token is expired; clear it.
                    Self::clear(store)?;
                    return Ok(None);
                }
            }
        }

        Ok(Some(token))
    }

    /// Clear the cached token.
    pub fn clear(store: &Store) -> Result<(), SproutError> {
        store.kv_delete(KV_API_TOKEN)?;
        store.kv_delete(KV_TOKEN_EXPIRES_AT)?;
        Ok(())
    }
}

/// Build the JSON body for a token mint request.
pub(crate) fn mint_token_body(name: &str) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "scopes": [
            "messages:read", "messages:write",
            "channels:read", "channels:write",
            "users:read", "users:write",
            "files:read", "files:write"
        ],
        "expires_in_days": 7
    })
}

/// Extract token and expiry from a mint response.
pub(crate) fn parse_mint_response(
    json: &serde_json::Value,
) -> Result<(String, Option<i64>), SproutError> {
    let token = json
        .get("token")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| SproutError::AuthFailed {
            message: "mint response missing 'token' field".to_string(),
        })?;

    let expires_at = json
        .get("expires_at")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.timestamp());

    Ok((token, expires_at))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_cache_roundtrip() {
        let store = Store::open(":memory:").unwrap();
        TokenCache::save(&store, "sprout_abc123", Some(9999999999)).unwrap();

        let loaded = TokenCache::load(&store).unwrap();
        assert_eq!(loaded, Some("sprout_abc123".to_string()));
    }

    #[test]
    fn expired_token_returns_none() {
        let store = Store::open(":memory:").unwrap();
        TokenCache::save(&store, "sprout_old", Some(1)).unwrap(); // expired in 1970

        let loaded = TokenCache::load(&store).unwrap();
        assert!(loaded.is_none());
    }

    #[test]
    fn clear_removes_token() {
        let store = Store::open(":memory:").unwrap();
        TokenCache::save(&store, "sprout_abc", None).unwrap();
        TokenCache::clear(&store).unwrap();
        assert!(TokenCache::load(&store).unwrap().is_none());
    }

    #[test]
    fn mint_body_has_expected_fields() {
        let body = mint_token_body("mobile-app");
        assert_eq!(body["name"], "mobile-app");
        assert!(body["scopes"].as_array().unwrap().len() >= 8);
        assert_eq!(body["expires_in_days"], 7);
    }

    #[test]
    fn parse_mint_response_extracts_token() {
        let json = serde_json::json!({
            "token": "sprout_deadbeef",
            "expires_at": "2026-12-31T23:59:59Z"
        });
        let (token, expires_at) = parse_mint_response(&json).unwrap();
        assert_eq!(token, "sprout_deadbeef");
        assert!(expires_at.is_some());
    }
}
