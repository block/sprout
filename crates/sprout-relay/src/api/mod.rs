//! HTTP API — media, git, NIP-05, and the Nostr HTTP bridge.

pub mod bridge;
pub mod events;
pub mod git;
pub mod media;
pub mod nip05;

// Re-export imeta helpers used by ingest pipeline.
pub use crate::handlers::imeta::{validate_imeta_tags, verify_imeta_blobs};

// ── Shared helpers (used by media.rs, bridge.rs) ──────────────────────────────

use axum::{http::StatusCode, response::Json};

/// Standard error envelope.
pub(crate) fn api_error(status: StatusCode, msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({ "error": msg })))
}

pub(crate) fn internal_error(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    tracing::error!("Internal error: {msg}");
    api_error(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
}

#[allow(dead_code)]
pub(crate) fn not_found(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    api_error(StatusCode::NOT_FOUND, msg)
}

/// Relay membership enforcement — single gate for all authenticated entry points.
///
/// Moved here from the deleted `relay_members` module. Called by `media.rs` and `bridge.rs`.
pub mod relay_members {
    use axum::{http::StatusCode, response::Json};

    use crate::state::AppState;

    /// Enforce relay membership for a pubkey.
    ///
    /// - If `config.require_relay_membership` is false → always Ok (no-op).
    /// - If enabled → checks `relay_members` table. Returns 403 if not a member.
    pub async fn enforce_relay_membership(
        state: &AppState,
        pubkey_bytes: &[u8],
    ) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
        if !state.config.require_relay_membership {
            return Ok(());
        }

        let pubkey_hex = hex::encode(pubkey_bytes);
        let is_member = state.db.is_relay_member(&pubkey_hex).await.map_err(|e| {
            tracing::error!("relay membership check failed: {e}");
            super::api_error(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
        })?;

        if is_member {
            Ok(())
        } else {
            Err((
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({
                    "error": "relay_membership_required",
                    "message": "You must be a relay member to access this relay"
                })),
            ))
        }
    }
}
