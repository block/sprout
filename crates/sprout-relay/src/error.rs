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
        let (status, code, message) = match &self {
            ApiError::NotFound(m) => (StatusCode::NOT_FOUND, "not_found", m.clone()),
            ApiError::Forbidden(m) => (StatusCode::FORBIDDEN, "forbidden", m.clone()),
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, "bad_request", m.clone()),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", String::new()),
            ApiError::Conflict(m) => (StatusCode::CONFLICT, "conflict", m.clone()),
            ApiError::Gone(m) => (StatusCode::GONE, "gone", m.clone()),
            ApiError::TooManyRequests(m) => (
                StatusCode::TOO_MANY_REQUESTS,
                "too_many_requests",
                m.clone(),
            ),
            ApiError::UnprocessableEntity(m) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "unprocessable_entity",
                m.clone(),
            ),
            // Internal/Database errors: log the real error, return a generic message
            // to avoid leaking implementation details to API clients.
            ApiError::Internal(ref e) => {
                tracing::error!("internal API error: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "internal server error".to_string(),
                )
            }
            ApiError::Database(ref e) => {
                tracing::error!("database API error: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "database_error",
                    "internal server error".to_string(),
                )
            }
        };
        let body = serde_json::json!({ "error": message, "code": code });
        (status, Json(body)).into_response()
    }
}
