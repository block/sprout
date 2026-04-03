//! Shadow keypair management — deterministic internal keys derived from external pubkeys.
//!
//! HMAC-SHA256(key=server_salt, msg=external_pubkey_bytes) → secp256k1 secret key. Cached in moka.
//! A server-side salt is required to prevent offline derivation by anyone who knows only
//! the external public key.
//!
//! **Key derivation note**: This uses HMAC-SHA256 (not raw SHA-256) for proper domain
//! separation and resistance to length-extension attacks. If the derivation scheme is
//! ever changed, all existing shadow keys will differ — acceptable for MVP (no persistent
//! state), but must be coordinated with a migration for production deployments.
//!
//! # Cache eviction
//!
//! The in-memory cache is bounded to `MAX_CACHE_SIZE` entries using `moka::sync::Cache`,
//! which provides proper LRU eviction. Individual entries are evicted as capacity is
//! reached, avoiding the thundering-herd problem of the previous "flush on full" strategy.
//! Because shadow keys are deterministically re-derivable from the salt and the public key,
//! eviction is always safe — the next lookup simply re-derives and re-caches the key.

use moka::sync::Cache;

use hmac::{Hmac, Mac};
use nostr::util::hex;
use nostr::{Keys, SecretKey};
use sha2::Sha256;

use crate::error::ProxyError;

type HmacSha256 = Hmac<Sha256>;

/// Maximum number of shadow keys held in the in-memory cache at one time.
/// Entries are evicted via LRU when this limit is reached.
/// Worst-case memory: `MAX_CACHE_SIZE × ~200 bytes` ≈ 2 MB at the default.
pub const MAX_CACHE_SIZE: usize = 10_000;

/// Manages deterministic shadow keypairs derived from external Nostr public keys.
pub struct ShadowKeyManager {
    salt: Vec<u8>,
    cache: Cache<String, Keys>,
}

impl ShadowKeyManager {
    /// Create a new [`ShadowKeyManager`] with the given server-side salt.
    ///
    /// Returns an error if `salt` is empty.
    pub fn new(salt: &[u8]) -> Result<Self, ProxyError> {
        if salt.is_empty() {
            return Err(ProxyError::KeyDerivation(
                "shadow key salt must not be empty".into(),
            ));
        }
        Ok(Self {
            salt: salt.to_vec(),
            cache: Cache::builder().max_capacity(MAX_CACHE_SIZE as u64).build(),
        })
    }

    /// Return the shadow [`Keys`] for `external_pubkey`, deriving and caching them if needed.
    pub fn get_or_create(&self, external_pubkey: &str) -> Result<Keys, ProxyError> {
        if let Some(keys) = self.cache.get(external_pubkey) {
            return Ok(keys);
        }
        let keys = self.derive(external_pubkey)?;
        self.cache.insert(external_pubkey.to_string(), keys.clone());
        Ok(keys)
    }

    /// Return cached shadow keys for `external_pubkey` without deriving new ones.
    pub fn lookup(&self, external_pubkey: &str) -> Option<Keys> {
        self.cache.get(external_pubkey)
    }

    fn derive(&self, external_pubkey: &str) -> Result<Keys, ProxyError> {
        let pubkey_bytes = hex::decode(external_pubkey)
            .map_err(|e| ProxyError::InvalidPubkey(format!("hex decode failed: {e}")))?;

        if pubkey_bytes.len() != 32 {
            return Err(ProxyError::InvalidPubkey(format!(
                "expected 32 bytes, got {}",
                pubkey_bytes.len()
            )));
        }

        // HMAC-SHA256(key=salt, msg=pubkey_bytes) — provides proper domain separation
        // and resistance to length-extension attacks vs. raw SHA-256(salt || pubkey).
        let mut mac = HmacSha256::new_from_slice(&self.salt)
            .map_err(|e| ProxyError::KeyDerivation(format!("HMAC init: {e}")))?;
        mac.update(&pubkey_bytes);
        let secret_bytes: [u8; 32] = mac.finalize().into_bytes().into();
        let secret_key = SecretKey::from_slice(&secret_bytes)
            .map_err(|e| ProxyError::KeyDerivation(e.to_string()))?;

        Ok(Keys::new(secret_key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PUBKEY_A: &str = "0101010101010101010101010101010101010101010101010101010101010101";
    const PUBKEY_B: &str = "0202020202020202020202020202020202020202020202020202020202020202";
    const TEST_SALT: &[u8] = b"test-server-salt-do-not-use-in-production";

    fn mgr() -> ShadowKeyManager {
        ShadowKeyManager::new(TEST_SALT).unwrap()
    }

    #[test]
    fn empty_salt_returns_error() {
        assert!(matches!(
            ShadowKeyManager::new(b""),
            Err(ProxyError::KeyDerivation(_))
        ));
    }

    #[test]
    fn deterministic_same_pubkey() {
        let m = mgr();
        let k1 = m.get_or_create(PUBKEY_A).unwrap();
        let k2 = m.get_or_create(PUBKEY_A).unwrap();
        assert_eq!(k1.public_key().to_hex(), k2.public_key().to_hex());
    }

    #[test]
    fn different_pubkeys_produce_different_shadows() {
        let m = mgr();
        let ka = m.get_or_create(PUBKEY_A).unwrap();
        let kb = m.get_or_create(PUBKEY_B).unwrap();
        assert_ne!(ka.public_key().to_hex(), kb.public_key().to_hex());
    }

    #[test]
    fn invalid_pubkey_hex_rejected() {
        let m = mgr();
        assert!(matches!(
            m.get_or_create("not-hex!"),
            Err(ProxyError::InvalidPubkey(_))
        ));
    }

    #[test]
    fn wrong_length_pubkey_rejected() {
        let m = mgr();
        assert!(matches!(
            m.get_or_create("01020304050607080910111213141516"),
            Err(ProxyError::InvalidPubkey(_))
        ));
    }

    #[test]
    fn stable_across_manager_instances() {
        let k1 = ShadowKeyManager::new(TEST_SALT)
            .unwrap()
            .get_or_create(PUBKEY_A)
            .unwrap();
        let k2 = ShadowKeyManager::new(TEST_SALT)
            .unwrap()
            .get_or_create(PUBKEY_A)
            .unwrap();
        assert_eq!(k1.public_key().to_hex(), k2.public_key().to_hex());
    }

    #[test]
    fn different_salts_produce_different_keys() {
        let k1 = ShadowKeyManager::new(b"salt-1")
            .unwrap()
            .get_or_create(PUBKEY_A)
            .unwrap();
        let k2 = ShadowKeyManager::new(b"salt-2")
            .unwrap()
            .get_or_create(PUBKEY_A)
            .unwrap();
        assert_ne!(k1.public_key().to_hex(), k2.public_key().to_hex());
    }

    #[test]
    fn cache_hit_returns_same_key() {
        let m = mgr();
        let k_before = m.get_or_create(PUBKEY_A).unwrap();
        // Second call should hit the cache and return the same key.
        let k_after = m.get_or_create(PUBKEY_A).unwrap();
        assert_eq!(
            k_before.public_key().to_hex(),
            k_after.public_key().to_hex()
        );
    }
}
