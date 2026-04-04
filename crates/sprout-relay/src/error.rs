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
    #[error("unauthorized: {0}")]
    Unauthorized(String),
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
    /// Custom error with explicit status, code, and message.
    /// Use for domain-specific error codes (e.g. "nip98_not_supported", "scope_escalation").
    /// The optional fourth field is merged into the JSON response body (must be an object).
    #[error("{2}")]
    Custom(StatusCode, &'static str, String, Option<serde_json::Value>),
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
        // Custom variant uses a two-field envelope: {"error": domain_code, "message": text}
        // All other variants use the original single-field format: {"error": text}
        // Standard variants restore the original single-field envelope: {"error": "text"}.
        // Custom variants use the two-field envelope: {"error": "code", "message": "text"}.
        // This matches the pre-refactor API contract where most endpoints returned
        // single-field errors and token endpoints returned two-field errors.
        match self {
            ApiError::Custom(status, code, message, extra) => {
                let mut body = serde_json::json!({ "error": code, "message": message });
                if let Some(extra_obj) = extra {
                    if let (Some(base), Some(ext)) = (body.as_object_mut(), extra_obj.as_object()) {
                        base.extend(ext.iter().map(|(k, v)| (k.clone(), v.clone())));
                    }
                }
                (status, Json(body)).into_response()
            }
            ApiError::Internal(ref e) => {
                tracing::error!("internal API error: {e}");
                let body = serde_json::json!({ "error": "internal server error" });
                (StatusCode::INTERNAL_SERVER_ERROR, Json(body)).into_response()
            }
            ApiError::Database(ref e) => {
                tracing::error!("database API error: {e}");
                let body = serde_json::json!({ "error": "internal server error" });
                (StatusCode::INTERNAL_SERVER_ERROR, Json(body)).into_response()
            }
            other => {
                let status = match &other {
                    ApiError::NotFound(_) => StatusCode::NOT_FOUND,
                    ApiError::Forbidden(_) => StatusCode::FORBIDDEN,
                    ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
                    ApiError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
                    ApiError::Conflict(_) => StatusCode::CONFLICT,
                    ApiError::Gone(_) => StatusCode::GONE,
                    ApiError::TooManyRequests(_) => StatusCode::TOO_MANY_REQUESTS,
                    ApiError::UnprocessableEntity(_) => StatusCode::UNPROCESSABLE_ENTITY,
                    _ => unreachable!(),
                };
                // Extract the human-readable message from the variant.
                let message = match other {
                    ApiError::NotFound(m)
                    | ApiError::Forbidden(m)
                    | ApiError::BadRequest(m)
                    | ApiError::Unauthorized(m)
                    | ApiError::Conflict(m)
                    | ApiError::Gone(m)
                    | ApiError::TooManyRequests(m)
                    | ApiError::UnprocessableEntity(m) => m,
                    _ => unreachable!(),
                };
                let body = serde_json::json!({ "error": message });
                (status, Json(body)).into_response()
            }
        }
    }
}
