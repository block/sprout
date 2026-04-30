//! Git hosting — Smart HTTP transport, permission hooks, and policy engine.
//!
//! # Module structure
//!
//! - `transport` — Smart HTTP protocol (info/refs, upload-pack, receive-pack)
//! - `hook` — Pre-receive hook script and injection
//! - `policy` — Internal policy endpoint (HMAC-authenticated callback from hook)

pub mod hook;
pub mod policy;
pub mod transport;

pub use transport::git_router;
