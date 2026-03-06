//! API token creation, hashing, and validation.
//!
//! Only the SHA-256 hash is stored — the raw token is shown once at creation.
//! Format: `sprout_<32-random-bytes-as-hex>` (71 characters).

use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

const TOKEN_PREFIX: &str = "sprout_";

/// Generate a new random API token (CSPRNG, 32 bytes, hex-encoded with prefix).
pub fn generate_token() -> String {
    use rand::Rng;
    let bytes: [u8; 32] = rand::thread_rng().gen();
    format!("{}{}", TOKEN_PREFIX, hex::encode(bytes))
}

/// SHA-256 hash of a raw token (the value stored in `api_tokens.token_hash`).
pub fn hash_token(token: &str) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hasher.finalize().to_vec()
}

/// Constant-time verification that `raw_token` matches `expected_hash`.
pub fn verify_token_hash(raw_token: &str, expected_hash: &[u8]) -> bool {
    let computed = hash_token(raw_token);
    computed.ct_eq(expected_hash).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_format_and_length() {
        let token = generate_token();
        assert!(token.starts_with("sprout_"));
        assert_eq!(token.len(), 7 + 64);
    }

    #[test]
    fn tokens_are_unique() {
        assert_ne!(generate_token(), generate_token());
    }

    #[test]
    fn hash_verify_round_trip() {
        let token = generate_token();
        let hash = hash_token(&token);
        assert_eq!(hash.len(), 32);
        assert!(verify_token_hash(&token, &hash));
    }

    #[test]
    fn wrong_token_rejected() {
        let hash = hash_token(&generate_token());
        assert!(!verify_token_hash(&generate_token(), &hash));
    }

    #[test]
    fn hash_is_deterministic() {
        let token = "sprout_test_abc123";
        assert_eq!(hash_token(token), hash_token(token));
    }
}
