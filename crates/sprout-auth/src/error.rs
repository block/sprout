//! Error types for sprout-auth.

/// All errors that can occur during authentication and authorization.
///
/// Variants are designed to be safe to return to callers without leaking
/// internal implementation details. Do **not** include raw token values,
/// database contents, or stack traces in error messages.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// The NIP-42 event signature is invalid or the event is structurally malformed.
    #[error("invalid signature or malformed auth event")]
    InvalidSignature,

    /// The `challenge` tag in the AUTH event does not match the relay's issued challenge.
    #[error("challenge mismatch")]
    ChallengeMismatch,

    /// The `relay` tag in the AUTH event does not match this relay's URL.
    #[error("relay url mismatch")]
    RelayUrlMismatch,

    /// The AUTH event's `created_at` timestamp is more than ±60 seconds from now.
    #[error("auth event timestamp outside ±60s window")]
    EventExpired,

    /// JWT validation failed (bad signature, expired, wrong issuer/audience, missing claim, etc.).
    ///
    /// The inner string provides diagnostics for server logs. Do **not** forward
    /// this detail to unauthenticated WebSocket clients.
    #[error("invalid JWT: {0}")]
    InvalidJwt(String),

    /// The API token hash does not match, or the token has expired.
    #[error("api token invalid or expired")]
    TokenInvalid,

    /// The API token was found in the database but has been revoked.
    ///
    /// Distinct from [`TokenInvalid`] so the relay can return `401 token_revoked`
    /// rather than the generic `invalid_token` error code.
    #[error("api token has been revoked")]
    TokenRevoked,

    /// The API token was found in the database but has passed its expiry timestamp.
    ///
    /// Distinct from [`TokenInvalid`] so the relay can return `401 token_expired`
    /// rather than the generic `invalid_token` error code.
    #[error("api token has expired")]
    TokenExpired,

    /// NIP-98 HTTP Auth event (kind:27235) failed verification.
    ///
    /// The inner string describes the specific failure (signature, timestamp, URL, etc.)
    /// and is safe to include in server logs. Do **not** forward raw event content to clients.
    #[error("NIP-98 HTTP Auth verification failed: {0}")]
    Nip98Invalid(String),

    /// The pubkey in the NIP-42 event does not match the identity in the JWT or API token.
    #[error("pubkey mismatch: event pubkey does not match authenticated identity")]
    PubkeyMismatch,

    /// The authenticated context does not have the required scope for this operation.
    #[error("insufficient scope: required {required}, have {have:?}")]
    InsufficientScope {
        /// The scope that was required.
        required: String,
        /// The scopes the caller actually holds.
        have: Vec<String>,
    },

    /// The authenticated user is not a member of the requested channel.
    #[error("channel access denied")]
    ChannelAccessDenied,

    /// The JWKS endpoint returned an error or an unparseable response.
    ///
    /// The inner string provides diagnostics for server logs.
    #[error("jwks fetch error: {0}")]
    JwksFetchError(String),

    /// An unexpected internal error occurred (e.g. a `spawn_blocking` panic).
    #[error("internal auth error: {0}")]
    Internal(String),
}
