//! Mesh-LLM client: discover, dial, and publish kind:31990 offers.
//!
//! This module is the desktop-side counterpart to `sprout-relay`'s embedded
//! iroh-relay (see `crates/sprout-relay/src/iroh_relay.rs`). The relay gates
//! admission with NIP-98 + relay membership; this module signs that bearer
//! token, dials offers advertised under kind:31990, and publishes our own
//! offer when the user enables compute-sharing.
//!
//! ## Submodules
//!
//! - [`endpoint`]: long-lived iroh endpoint keypair persisted at
//!   `{app_data_dir}/mesh_iroh.key`.
//! - [`nip98`]: build the NIP-98 bearer event signed with the user's Nostr
//!   key for a given canonical relay URL.
//! - [`offer`]: load/save the user's mesh-LLM offer preferences
//!   (VRAM/RAM/concurrency caps, models).

pub mod endpoint;
pub mod nip11;
pub mod nip98;
pub mod offer;
pub mod runtime;

// Wildcard re-exports are deliberately avoided so that adding an
// unused-by-design helper to a submodule (e.g. a publisher that hasn't been
// wired yet) doesn't trip dead-code lints at the top level. Callers reach
// into the submodules directly until a public API surface stabilises.

pub use endpoint::load_or_create_endpoint_key;
pub use nip11::fetch_iroh_relay_url;
pub use offer::ComputeSharingPrefs;
