//! Media error types.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// Errors from media operations.
#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("unknown content type")]
    UnknownContentType,
    #[error("disallowed content type: {0}")]
    DisallowedContentType(String),
    #[error("file too large: {size} bytes (max {max})")]
    FileTooLarge { size: u64, max: u64 },
    #[error("image dimensions too large")]
    ImageTooLarge,
    #[error("invalid image data")]
    InvalidImage,
    #[error("invalid signature")]
    InvalidSignature,
    #[error("invalid auth event kind")]
    InvalidAuthKind,
    #[error("invalid auth verb")]
    InvalidAuthVerb,
    #[error("missing required tag: {0}")]
    MissingTag(&'static str),
    #[error("hash mismatch")]
    HashMismatch,
    #[error("server mismatch")]
    ServerMismatch,
    #[error("token expired")]
    TokenExpired,
    #[error("timestamp out of window")]
    TimestampOutOfWindow,
    #[error("storage error: {0}")]
    StorageError(String),
    #[error("internal error")]
    Internal,
    #[error("not found")]
    NotFound,
    #[error("missing authorization header")]
    MissingAuth,
    #[error("invalid authorization scheme")]
    InvalidAuthScheme,
    #[error("invalid base64 encoding")]
    InvalidBase64,
    #[error("invalid auth event")]
    InvalidAuthEvent,
    #[error("unauthorized")]
    Unauthorized,
    #[error("insufficient scope")]
    InsufficientScope,
    #[error("token revoked")]
    TokenRevoked,
    #[error("pubkey mismatch")]
    PubkeyMismatch,
}

impl From<image::ImageError> for MediaError {
    fn from(_: image::ImageError) -> Self {
        Self::InvalidImage
    }
}

impl From<s3::error::S3Error> for MediaError {
    fn from(e: s3::error::S3Error) -> Self {
        Self::StorageError(e.to_string())
    }
}

impl From<serde_json::Error> for MediaError {
    fn from(e: serde_json::Error) -> Self {
        Self::StorageError(e.to_string())
    }
}

impl IntoResponse for MediaError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            Self::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            Self::DisallowedContentType(_) => {
                (StatusCode::UNSUPPORTED_MEDIA_TYPE, self.to_string())
            }
            Self::FileTooLarge { .. } | Self::ImageTooLarge => {
                (StatusCode::PAYLOAD_TOO_LARGE, self.to_string())
            }
            // All authentication failures return the same generic 401 to prevent oracle enumeration.
            // InsufficientScope is intentionally 403 — it's an authorization (not authentication)
            // failure and is safe to distinguish since it requires a valid identity first.
            Self::MissingAuth
            | Self::InvalidAuthScheme
            | Self::InvalidBase64
            | Self::InvalidAuthEvent
            | Self::InvalidSignature
            | Self::InvalidAuthKind
            | Self::InvalidAuthVerb
            | Self::TokenExpired
            | Self::TimestampOutOfWindow
            | Self::Unauthorized
            | Self::TokenRevoked
            | Self::PubkeyMismatch
            | Self::HashMismatch
            | Self::ServerMismatch
            | Self::MissingTag(_) => {
                tracing::warn!(error = %self, "authentication failed");
                (StatusCode::UNAUTHORIZED, "authentication failed".to_string())
            }
            Self::InsufficientScope => (StatusCode::FORBIDDEN, self.to_string()),
            Self::UnknownContentType | Self::InvalidImage => {
                (StatusCode::BAD_REQUEST, self.to_string())
            }
            Self::StorageError(_) | Self::Internal => {
                tracing::error!(error = %self, "media storage error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into())
            }
        };
        (status, axum::Json(serde_json::json!({"error": msg}))).into_response()
    }
}
