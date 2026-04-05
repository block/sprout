//! Authentication orchestration for Sprout mobile.
//! Coordinates key management, NIP-42 WebSocket auth, NIP-98 HTTP auth, and API tokens.

/// Keypair management utilities.
pub mod keys;
/// NIP-42 WebSocket auth helpers.
pub mod nip42;
/// NIP-98 HTTP auth helpers.
pub mod nip98;
/// API token minting and caching.
pub mod token;
