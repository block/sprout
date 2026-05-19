//! Persisted compute-sharing preferences (the avatar-menu sliders).
//!
//! When the user turns on compute sharing and dials the VRAM/RAM/concurrency
//! sliders in the bottom-left avatar menu, those preferences live in
//! `{app_data_dir}/mesh_offer.json`. The publisher reads this file when it
//! builds a kind:31990 event; the settings UI reads + writes it via Tauri
//! commands.
//!
//! Keeping the prefs as a plain JSON file (rather than baking them into the
//! kind:31990 event directly) lets the user toggle sharing without
//! republishing on every restart and makes the file inspectable for support.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sprout_core::mesh_llm::{MeshLlmOffer, ModelOffer, ResourceCaps};
use tauri::{AppHandle, Manager};

const OFFER_FILENAME: &str = "mesh_offer.json";
const DEFAULT_D_TAG: &str = "default";

/// Errors loading or saving offer preferences.
#[derive(Debug, thiserror::Error)]
pub enum OfferPrefsError {
    /// Couldn't determine the Tauri app data dir.
    #[error("app data dir: {0}")]
    AppDataDir(String),
    /// Filesystem I/O failure.
    #[error("filesystem: {0}")]
    Io(String),
    /// JSON parse / serialize failure.
    #[error("json: {0}")]
    Json(String),
}

/// Persisted compute-sharing preferences.
///
/// `enabled = false` is the default; the publisher must skip publishing in
/// that case and **must delete any previously-published offer** (NIP-09 or
/// kind:31990 with empty content per NIP-33's replace-with-empty convention).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputeSharingPrefs {
    /// Whether the user has opted in to sharing compute. Default: `false`.
    pub enabled: bool,

    /// Caps the user wants to advertise. `None` on a field = "no cap"; the
    /// publisher passes `Some(0)` through unchanged because the schema
    /// allows it (consumers should treat 0 as "explicit zero").
    pub caps: ResourceCaps,

    /// Models the user wants to advertise. May be empty.
    pub models: Vec<ModelOffer>,

    /// Persistent `d_tag` for the user's offer. Generated once on first
    /// enable and re-used so replaces target the same address.
    pub d_tag: String,
}

impl Default for ComputeSharingPrefs {
    fn default() -> Self {
        Self {
            enabled: false,
            caps: ResourceCaps {
                max_vram_mb: None,
                max_ram_mb: None,
                max_concurrency: Some(1),
            },
            models: vec![],
            d_tag: DEFAULT_D_TAG.to_string(),
        }
    }
}

impl ComputeSharingPrefs {
    /// Builds the kind:31990 offer envelope to publish. Returns `None` if
    /// sharing is disabled; the publisher should then *delete* any prior
    /// offer rather than calling this.
    ///
    /// `expires_at` is the unix-seconds deadline after which consumers will
    /// drop this offer. Callers pass `now + OFFER_TTL_SECS` so an
    /// unannounced publisher crash naturally reaps the stale offer.
    pub fn build_offer(
        &self,
        endpoint_id: &str,
        iroh_relay_url: &str,
        expires_at: u64,
    ) -> Option<MeshLlmOffer> {
        if !self.enabled {
            return None;
        }
        Some(MeshLlmOffer {
            v: 1,
            d_tag: self.d_tag.clone(),
            endpoint_id: endpoint_id.to_string(),
            iroh_relay_url: iroh_relay_url.to_string(),
            expires_at,
            caps: self.caps.clone(),
            models: self.models.clone(),
            extra: None,
        })
    }
}

/// Default offer TTL: 15 minutes. Publishers republish on each prefs
/// change *and* should heartbeat at ~`OFFER_TTL_SECS/3` so a one-missed
/// heartbeat still leaves the offer visible.
pub const OFFER_TTL_SECS: u64 = 15 * 60;

fn prefs_path(app: &AppHandle) -> Result<PathBuf, OfferPrefsError> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| OfferPrefsError::AppDataDir(e.to_string()))?;
    std::fs::create_dir_all(&data_dir).map_err(|e| OfferPrefsError::Io(e.to_string()))?;
    Ok(data_dir.join(OFFER_FILENAME))
}

/// Load persisted prefs; returns [`ComputeSharingPrefs::default`] if the file
/// is absent. On parse errors, returns the error verbatim — callers should
/// surface it in the settings UI rather than silently resetting.
pub fn load_prefs(app: &AppHandle) -> Result<ComputeSharingPrefs, OfferPrefsError> {
    let path = prefs_path(app)?;
    if !path.exists() {
        return Ok(ComputeSharingPrefs::default());
    }
    let content = std::fs::read_to_string(&path).map_err(|e| OfferPrefsError::Io(e.to_string()))?;
    serde_json::from_str(&content).map_err(|e| OfferPrefsError::Json(e.to_string()))
}

/// Atomically replace the on-disk prefs file.
pub fn save_prefs(app: &AppHandle, prefs: &ComputeSharingPrefs) -> Result<(), OfferPrefsError> {
    let path = prefs_path(app)?;
    let dir = path
        .parent()
        .ok_or_else(|| OfferPrefsError::Io("prefs path has no parent".to_string()))?;
    let json = serde_json::to_string_pretty(prefs).map_err(|e| OfferPrefsError::Json(e.to_string()))?;
    let tmp = tempfile::NamedTempFile::new_in(dir)
        .map_err(|e| OfferPrefsError::Io(format!("temp file: {e}")))?;
    std::fs::write(tmp.path(), json.as_bytes())
        .map_err(|e| OfferPrefsError::Io(format!("write temp: {e}")))?;
    tmp.persist(&path)
        .map_err(|e| OfferPrefsError::Io(format!("rename temp: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_disabled() {
        let prefs = ComputeSharingPrefs::default();
        assert!(!prefs.enabled);
        assert_eq!(prefs.caps.max_concurrency, Some(1));
        assert_eq!(prefs.d_tag, DEFAULT_D_TAG);
    }

    #[test]
    fn build_offer_returns_none_when_disabled() {
        let prefs = ComputeSharingPrefs::default();
        assert!(
            prefs
                .build_offer("endpoint", "https://relay/iroh", 2_000_000_000)
                .is_none()
        );
    }

    #[test]
    fn build_offer_returns_envelope_when_enabled() {
        let prefs = ComputeSharingPrefs {
            enabled: true,
            ..Default::default()
        };
        let offer = prefs
            .build_offer(
                "endpoint-id-hex",
                "https://relay.example.com/iroh",
                2_000_000_000,
            )
            .expect("offer");
        assert_eq!(offer.endpoint_id, "endpoint-id-hex");
        assert_eq!(offer.iroh_relay_url, "https://relay.example.com/iroh");
        assert_eq!(offer.expires_at, 2_000_000_000);
        assert!(offer.is_publishable());
    }

    /// Round-trip prefs through serde so the on-disk format stays stable.
    #[test]
    fn round_trip_via_json() {
        let prefs = ComputeSharingPrefs {
            enabled: true,
            caps: ResourceCaps {
                max_vram_mb: Some(8192),
                max_ram_mb: Some(16_000),
                max_concurrency: Some(3),
            },
            models: vec![ModelOffer {
                id: "qwen/Qwen2.5-7B-Instruct".to_string(),
                label: Some("Qwen 2.5 7B".to_string()),
                context_tokens: Some(32_768),
            }],
            d_tag: "node-laptop".to_string(),
        };
        let s = serde_json::to_string(&prefs).unwrap();
        let back: ComputeSharingPrefs = serde_json::from_str(&s).unwrap();
        assert_eq!(prefs, back);
    }
}
