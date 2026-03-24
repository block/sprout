/// NIP-42 authentication handler.
pub mod auth;
/// Subscription close (CLOSE) handler.
pub mod close;
pub mod event;
/// Transport-neutral event ingestion pipeline.
pub mod ingest;
pub mod req;
/// NIP-29 and NIP-25 side-effect handlers.
pub mod side_effects;
