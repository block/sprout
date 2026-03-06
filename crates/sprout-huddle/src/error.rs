use thiserror::Error;

/// Errors returned by the huddle layer.
#[derive(Debug, Error)]
pub enum HuddleError {
    /// JWT encoding failed.
    #[error("JWT encoding failed: {0}")]
    JwtEncoding(#[from] jsonwebtoken::errors::Error),

    /// The webhook `Authorization` header did not match the expected HMAC-SHA256 signature.
    #[error("webhook signature invalid")]
    InvalidWebhookSignature,

    /// The webhook request body could not be deserialized.
    #[error("webhook body invalid: {0}")]
    InvalidWebhookBody(#[from] serde_json::Error),

    /// The webhook payload contained an event type not handled by this implementation.
    #[error("unknown webhook event type: {0}")]
    UnknownEventType(String),

    /// A required field was absent in the webhook payload.
    #[error("missing required field: {0}")]
    MissingField(&'static str),

    /// The track type string in the webhook payload was not a recognised kind.
    #[error("invalid track kind: {0}")]
    InvalidTrackKind(String),
}
