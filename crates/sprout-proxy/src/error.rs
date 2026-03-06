use thiserror::Error;

/// Errors returned by the proxy layer.
#[derive(Debug, Error)]
pub enum ProxyError {
    /// The invite token was not found in the store.
    #[error("invite token not found")]
    InviteNotFound,

    /// The invite token has passed its expiry time.
    #[error("invite token expired")]
    InviteExpired,

    /// The invite token has reached its maximum use count.
    #[error("invite token exhausted")]
    InviteExhausted,

    /// The supplied external public key is not a valid 32-byte hex string.
    #[error("invalid external pubkey: {0}")]
    InvalidPubkey(String),

    /// Shadow key derivation failed.
    #[error("shadow key derivation failed: {0}")]
    KeyDerivation(String),
}
