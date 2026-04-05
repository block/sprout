// ABOUTME: Nostr keypair management — generate, import, and export.
// ABOUTME: Keys are held in memory; native shells handle secure persistent storage.

use nostr::{Keys, ToBech32};

use crate::error::SproutError;
use crate::types::UserIdentity;

/// Manages Nostr keypair lifecycle for the mobile client.
///
/// Keys are held in memory only. The native shell is responsible for persisting
/// the nsec in platform secure storage (iOS Keychain / Android Keystore) and
/// passing it back on app launch via [`ClientConfig::nsec_or_hex`].
#[derive(Debug)]
pub(crate) struct KeyManager {
    keys: Keys,
}

impl KeyManager {
    /// Generate a fresh random keypair.
    #[allow(dead_code)]
    pub fn generate() -> Self {
        Self {
            keys: Keys::generate(),
        }
    }

    /// Import from an nsec (bech32) or 64-char hex secret key string.
    pub fn from_nsec_or_hex(input: &str) -> Result<Self, SproutError> {
        let keys = Keys::parse(input).map_err(|e| SproutError::InvalidKey {
            message: e.to_string(),
        })?;
        Ok(Self { keys })
    }

    /// Export the secret key as bech32-encoded nsec.
    pub fn export_nsec(&self) -> Result<String, SproutError> {
        self.keys
            .secret_key()
            .to_bech32()
            .map_err(|e| SproutError::InvalidKey {
                message: e.to_string(),
            })
    }

    /// Export the public key as bech32-encoded npub.
    pub fn npub(&self) -> String {
        self.keys
            .public_key()
            .to_bech32()
            .expect("public key bech32 encoding should never fail")
    }

    /// Export the public key as 64-character hex.
    pub fn pubkey_hex(&self) -> String {
        self.keys.public_key().to_hex()
    }

    /// Build a [`UserIdentity`] from the current keypair.
    pub fn identity(&self) -> UserIdentity {
        UserIdentity {
            pubkey: self.pubkey_hex(),
            npub: self.npub(),
            display_name: None,
        }
    }

    /// Borrow the underlying `nostr::Keys` for event signing.
    pub fn keys(&self) -> &Keys {
        &self.keys
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_produces_valid_keypair() {
        let km = KeyManager::generate();
        assert_eq!(km.pubkey_hex().len(), 64);
        assert!(km.npub().starts_with("npub1"));
    }

    #[test]
    fn roundtrip_nsec_export_import() {
        let km = KeyManager::generate();
        let nsec = km.export_nsec().unwrap();
        assert!(nsec.starts_with("nsec1"));

        let km2 = KeyManager::from_nsec_or_hex(&nsec).unwrap();
        assert_eq!(km.pubkey_hex(), km2.pubkey_hex());
    }

    #[test]
    fn import_hex_secret_key() {
        let km = KeyManager::generate();
        let hex = km.keys().secret_key().to_secret_hex();
        let km2 = KeyManager::from_nsec_or_hex(&hex).unwrap();
        assert_eq!(km.pubkey_hex(), km2.pubkey_hex());
    }

    #[test]
    fn import_invalid_key_returns_error() {
        let result = KeyManager::from_nsec_or_hex("not-a-valid-key");
        assert!(result.is_err());
        match result.unwrap_err() {
            SproutError::InvalidKey { .. } => {}
            e => panic!("expected InvalidKey, got {e:?}"),
        }
    }

    #[test]
    fn identity_has_correct_fields() {
        let km = KeyManager::generate();
        let id = km.identity();
        assert_eq!(id.pubkey, km.pubkey_hex());
        assert_eq!(id.npub, km.npub());
        assert!(id.display_name.is_none());
    }
}
