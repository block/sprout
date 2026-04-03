//! Error types for the relay crate.

use thiserror::Error;

/// Top-level error type for relay operations.
#[derive(Debug, Error)]
pub enum RelayError {
    /// A WebSocket transport error occurred.
    #[error("WebSocket error: {0}")]
    WebSocket(String),

    /// A JSON serialization or deserialization error.
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    /// A database operation failed.
    #[error("Database error: {0}")]
    Database(#[from] sprout_db::DbError),

    /// An authentication error from the auth service.
    #[error("Auth error: {0}")]
    Auth(#[from] sprout_auth::AuthError),

    /// A pub/sub error from the pubsub service.
    #[error("PubSub error: {0}")]
    PubSub(#[from] sprout_pubsub::PubSubError),

    /// The relay has reached its maximum number of concurrent connections.
    #[error("Connection limit reached")]
    ConnectionLimitReached,

    /// The client has exceeded the allowed request rate.
    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    /// The client attempted an operation that requires authentication.
    #[error("Not authenticated")]
    NotAuthenticated,

    /// The client sent a message that could not be parsed.
    #[error("Invalid message format: {0}")]
    InvalidMessage(String),

    /// An unexpected internal error occurred.
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Convenience alias for relay operation results.
pub type Result<T> = std::result::Result<T, RelayError>;

// ── REST API error type ───────────────────────────────────────────────────────

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

/// Typed error for REST API handlers — replaces raw `(StatusCode, Json<Value>)` tuples.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// 404 Not Found.
    #[error("not found: {0}")]
    NotFound(String),
    /// 403 Forbidden.
    #[error("forbidden: {0}")]
    Forbidden(String),
    /// 400 Bad Request.
    #[error("bad request: {0}")]
    BadRequest(String),
    /// 401 Unauthorized.
    #[error("unauthorized")]
    Unauthorized,
    /// 409 Conflict.
    #[error("conflict: {0}")]
    Conflict(String),
    /// 410 Gone.
    #[error("gone: {0}")]
    Gone(String),
    /// 429 Too Many Requests.
    #[error("too many requests: {0}")]
    TooManyRequests(String),
    /// 422 Unprocessable Entity.
    #[error("unprocessable entity: {0}")]
    UnprocessableEntity(String),
    /// 500 Internal Server Error (anyhow-wrapped).
    #[error("internal error: {0}")]
    Internal(#[from] anyhow::Error),
    /// 500 Internal Server Error (database error).
    #[error("database error: {0}")]
    Database(#[from] sprout_db::DbError),
}

// ── Side-effect error type ────────────────────────────────────────────────────

/// Typed error for post-ingest side effects.
#[derive(Debug, thiserror::Error)]
pub enum SideEffectError {
    /// A database operation failed.
    #[error("database error: {0}")]
    Database(#[from] sprout_db::DbError),
    /// A JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    /// A pub/sub operation failed.
    #[error("pubsub error: {0}")]
    PubSub(#[from] sprout_pubsub::PubSubError),
    /// A nostr tag/event build error.
    #[error("nostr error: {0}")]
    Nostr(String),
    /// A generic internal error (for cases that don't fit above).
    #[error("{0}")]
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            ApiError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            ApiError::Forbidden(_) => (StatusCode::FORBIDDEN, "forbidden"),
            ApiError::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            ApiError::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            ApiError::Gone(_) => (StatusCode::GONE, "gone"),
            ApiError::TooManyRequests(_) => (StatusCode::TOO_MANY_REQUESTS, "too_many_requests"),
            ApiError::UnprocessableEntity(_) => {
                (StatusCode::UNPROCESSABLE_ENTITY, "unprocessable_entity")
            }
            ApiError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
            ApiError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "database_error"),
        };
        let body = serde_json::json!({ "error": self.to_string(), "code": code });
        (status, Json(body)).into_response()
    }
}
