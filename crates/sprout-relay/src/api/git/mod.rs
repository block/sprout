//! Git hosting — Smart HTTP transport, permission hooks, and policy engine.
//!
//! # Module structure
//!
//! - `transport` — Smart HTTP protocol (info/refs, upload-pack, receive-pack)
//! - `hook` — Pre-receive hook script and injection
//! - `policy` — Internal policy endpoint (HMAC-authenticated callback from hook)

use std::sync::Arc;

use axum::{routing::post, Router};

use crate::state::AppState;

pub mod hook;
pub mod policy;
pub mod transport;

pub use transport::git_router;

/// Build the internal git policy router.
///
/// Mounted at `/internal/git/policy` — only accessible from localhost.
/// The pre-receive hook calls this to authorize pushes.
pub fn git_policy_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/internal/git/policy", post(policy::hook_policy_check))
        .with_state(state)
}
