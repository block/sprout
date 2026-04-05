// ABOUTME: Unified error type for all sprout-mobile operations.
// ABOUTME: Derives uniffi::Error so it crosses the FFI boundary to Swift/Kotlin.

/// Errors returned by [`SproutClient`](crate::SproutClient) methods.
///
/// Uses `flat_error` so the FFI layer exposes only the variant discriminant;
/// field values are rendered into the `Display` message that foreign languages
/// see as the exception's `.message` / `localizedDescription`. This avoids a
/// UniFFI Kotlin codegen clash where a `message: String` field on a variant
/// collides with `kotlin.Throwable.message`.
#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum SproutError {
    /// Not connected to a relay.
    #[error("not connected to relay")]
    NotConnected,

    /// Authentication is required for this operation.
    #[error("authentication required")]
    AuthRequired,

    /// Authentication was rejected by the relay.
    #[error("authentication failed: {message}")]
    AuthFailed {
        /// Human-readable failure reason.
        message: String,
    },

    /// The API token has expired.
    #[error("token expired")]
    TokenExpired,

    /// Invalid key material (nsec, hex, or generated key).
    #[error("invalid key material: {message}")]
    InvalidKey {
        /// What was wrong with the key.
        message: String,
    },

    /// The relay returned an error response.
    #[error("relay error ({status}): {message}")]
    RelayError {
        /// HTTP status code.
        status: u16,
        /// Error message from the relay.
        message: String,
    },

    /// A network error occurred (DNS, TCP, TLS).
    #[error("network error: {message}")]
    NetworkError {
        /// Error details.
        message: String,
    },

    /// A WebSocket transport error occurred.
    #[error("websocket error: {message}")]
    WebSocketError {
        /// Error details.
        message: String,
    },

    /// The requested resource was not found.
    #[error("not found: {entity}")]
    NotFound {
        /// What was not found.
        entity: String,
    },

    /// The user lacks permission for this operation.
    #[error("permission denied: {message}")]
    PermissionDenied {
        /// What permission was missing.
        message: String,
    },

    /// Input validation failed.
    #[error("validation error: {message}")]
    ValidationError {
        /// What was invalid.
        message: String,
    },

    /// Local SQLite storage error.
    #[error("storage error: {message}")]
    StorageError {
        /// Error details.
        message: String,
    },

    /// An unexpected internal error.
    #[error("internal error: {message}")]
    InternalError {
        /// Error details.
        message: String,
    },
}
