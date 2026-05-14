//! Encrypted agent-provider settings (configuration for `sprout-agent`).
//!
//! The user enters provider, API key, model, base URL, and optional behavior
//! knobs in the desktop Settings UI. The blob is encrypted at rest with
//! NIP-44 v2 self-encryption (the user's own nostr key) and lives in
//! `app_data_dir()/agent-provider-settings.json`.
//!
//! Threat model: same bar as `identity.key` — anyone with read access to the
//! app data dir already wins. NIP-44-self is honest obfuscation that keeps
//! the API key out of casual disk reads, accidental backups, log shipping,
//! screenshots, and one-shot `cat ~/Library/...` inspection.
//!
//! Plaintext lifetime: the API key crosses the IPC boundary exactly once,
//! on `save_agent_provider_settings`. We wrap it in `zeroize::Zeroizing`
//! between deserialize and `nip44::encrypt`, then drop. After save, the
//! plaintext only re-materializes inside this module during a spawn (via
//! `load_for_spawn`) where it goes straight into a child process's env.
//!
//! Module layout:
//! - `storage` — on-disk envelope read/write/encrypt/decrypt, origin
//!   normalization, save-time validation. Private (submodule items used
//!   internally by `commands` and `spawn`).
//! - `commands` — Tauri command entrypoints (`get_*`, `save_*`, `delete_*`,
//!   `get_*_env_presence`). Re-exported from this module.
//! - `spawn` — `load_for_spawn` + env-pair mapping. Used by
//!   `managed_agents::runtime` at agent-spawn time.
//! - `tests` — unit tests over the private items.

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

// ─── Module-wide constants ──────────────────────────────────────────────────

/// Max plaintext size before encrypt. NIP-44 v2 caps at 65535; we set this
/// well below so a user can't accidentally produce an envelope that another
/// NIP-44 reader will reject.
pub(crate) const MAX_PLAINTEXT_BYTES: usize = 48 * 1024;

/// Max envelope size on disk. Defensive — protects against malformed files
/// stuffing memory if someone hand-edits the file.
pub(crate) const MAX_ENVELOPE_BYTES: u64 = 128 * 1024;

/// System-prompt size cap, matches sprout-agent's `HANDOFF_PROMPT_MAX_BYTES`.
/// Strictly less than `MAX_PLAINTEXT_BYTES` so a max-sized prompt + the
/// other fields + JSON overhead still fits in a single encrypted plaintext
/// (the previous arrangement put both at 32 KB, so a 32-KB prompt would pass
/// `validate_input` then fail `encrypt_settings` after JSON serialization).
pub(crate) const MAX_SYSTEM_PROMPT_BYTES: usize = 32 * 1024;

/// Required minimum for sprout-agent's `SPROUT_AGENT_MAX_HISTORY_BYTES`.
/// Matches `MAX_PROMPT_BYTES` in `crates/sprout-agent/src/config.rs`.
pub(crate) const MIN_HISTORY_BYTES: usize = 1024 * 1024;

pub(crate) const SETTINGS_FILENAME: &str = "agent-provider-settings.json";
pub(crate) const ENVELOPE_VERSION: u32 = 1;
pub(crate) const ENVELOPE_ALG: &str = "nip44-v2-self";

pub const PROVIDER_ANTHROPIC: &str = "anthropic";
pub const PROVIDER_OPENAI: &str = "openai";

/// Env vars the panel owns for sprout-agent. Spawn path removes these from
/// inherited parent env before injecting the GUI's values, so a stale shell
/// export can never shadow saved settings.
pub const OWNED_AGENT_ENV_VARS: &[&str] = &[
    "SPROUT_AGENT_PROVIDER",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_API_VERSION",
    "OPENAI_COMPAT_API_KEY",
    "OPENAI_COMPAT_MODEL",
    "OPENAI_COMPAT_BASE_URL",
    "SPROUT_AGENT_SYSTEM_PROMPT",
    "SPROUT_AGENT_MAX_ROUNDS",
    "SPROUT_AGENT_MAX_OUTPUT_TOKENS",
    "SPROUT_AGENT_LLM_TIMEOUT_SECS",
    "SPROUT_AGENT_TOOL_TIMEOUT_SECS",
    "SPROUT_AGENT_MAX_HISTORY_BYTES",
];

// ─── On-disk envelope ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SettingsEnvelope {
    pub(crate) version: u32,
    pub(crate) alg: String,
    /// Hex pubkey (64 chars) whose nsec encrypted the ciphertext. Lets us
    /// detect identity rotation without attempting decrypt.
    pub(crate) pubkey: String,
    pub(crate) ciphertext: String,
    pub(crate) updated_at: u64,
}

// ─── Plaintext (lives only inside this module + during a single spawn) ──────

/// Plaintext that lives inside the NIP-44 envelope. Owner pubkey is
/// included here (not just in the unauthenticated envelope) so an attacker
/// who can swap envelope files cannot trick the loader into accepting a
/// blob encrypted for a different identity — NIP-44 self-decrypt would
/// succeed on a same-identity ciphertext from any older state, so this is
/// the integrity check for "this plaintext belongs to this user".
///
/// `Debug` is intentionally not derived: a panic backtrace or log line
/// rendering this struct would otherwise leak the API key.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct StoredSettings {
    #[serde(default = "default_schema_version")]
    pub(crate) schema_version: u32,
    /// Hex pubkey (64 chars) of the identity this plaintext was encrypted
    /// for. Loader rejects on mismatch. Defaults to empty string for older
    /// envelopes that pre-date schema_version 2 — those still verify against
    /// the envelope's pubkey field as the only check.
    #[serde(default)]
    pub(crate) owner_pubkey: String,
    pub(crate) provider: String,
    pub(crate) api_key: String,
    pub(crate) model: String,
    pub(crate) base_url: String,
    #[serde(default)]
    pub(crate) anthropic_api_version: Option<String>,
    #[serde(default)]
    pub(crate) system_prompt: Option<String>,
    #[serde(default)]
    pub(crate) max_rounds: Option<u32>,
    #[serde(default)]
    pub(crate) max_output_tokens: Option<u32>,
    #[serde(default)]
    pub(crate) llm_timeout_secs: Option<u64>,
    #[serde(default)]
    pub(crate) tool_timeout_secs: Option<u64>,
    #[serde(default)]
    pub(crate) max_history_bytes: Option<usize>,
    #[serde(default = "default_detected_provider_id")]
    pub(crate) detected_provider_id: String,
    #[serde(default)]
    pub(crate) detection_overridden: bool,
}

fn default_schema_version() -> u32 {
    1
}

/// Zeroize the API key on drop. The `api_key` field carries plaintext that
/// transits this struct on save (input → encrypt) and load (decrypt → IPC
/// view, which copies only metadata + last-4 preview). Manual `impl Drop`
/// rather than `#[derive(ZeroizeOnDrop)]` so we keep `Clone` derive working
/// (Drop and Clone don't compose with the derive macro). Only the secret
/// field is zeroized; other fields are non-sensitive metadata.
impl Drop for StoredSettings {
    fn drop(&mut self) {
        self.api_key.zeroize();
    }
}

fn default_detected_provider_id() -> String {
    "custom".into()
}

// ─── Profiles wrapper (plaintext schema_version >= 3) ───────────────────────

/// Schema dispatch probe. Decryption produces `Zeroizing<String>`; we
/// `serde_json::from_str` into this small struct (which carries no
/// secret-bearing fields) to decide whether to parse the plaintext as a
/// legacy single `StoredSettings` (v1/v2) or the multi-profile wrapper (v3+).
///
/// Using a probe instead of `serde_json::Value` keeps API key bytes out of
/// a generic JSON tree where they'd be hard to zeroize.
#[derive(Deserialize)]
pub(crate) struct SchemaProbe {
    #[serde(default)]
    pub(crate) schema_version: u32,
}

/// v3 plaintext: many named profiles, one optionally marked as the default.
///
/// `owner_pubkey` is the wrapper-level integrity check (matches
/// envelope.pubkey). Each `NamedProfile.settings` still carries its own
/// `owner_pubkey` (v2 defense-in-depth); the spawn path cross-checks both.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ProfilesPlaintext {
    pub(crate) schema_version: u32, // == 3
    pub(crate) owner_pubkey: String,
    #[serde(default)]
    pub(crate) default_profile_id: Option<String>,
    #[serde(default)]
    pub(crate) profiles: Vec<NamedProfile>,
}

/// One slot in `ProfilesPlaintext.profiles`. `settings` is exactly today's
/// per-profile plaintext shape; it carries the secret-bearing `api_key`,
/// which is zeroized on drop via `StoredSettings::Drop`.
///
/// `Debug` is intentionally not derived (same reason as `StoredSettings`).
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct NamedProfile {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) created_at: u64,
    pub(crate) updated_at: u64,
    pub(crate) settings: StoredSettings,
}

/// Plaintext-schema version we write to disk. v1/v2 are legacy and only
/// produced by older builds; we read them through `migrate_to_profiles`.
pub(crate) const CURRENT_PLAINTEXT_SCHEMA: u32 = 3;

/// Highest plaintext schema we know how to parse. Reading a higher value
/// is an error (the user downgraded sprout-desktop after running a newer
/// version) — we'd rather fail visibly than silently strip newer fields.
pub(crate) const MAX_KNOWN_PLAINTEXT_SCHEMA: u32 = 3;

// ─── IPC types ──────────────────────────────────────────────────────────────

/// Result of `get_agent_provider_settings_state`. Multi-profile aware:
/// `Ok` carries the full list + which one is default.
///
/// Four variants:
/// - `None` — settings file is absent.
/// - `Ok` — settings file present, decrypted, parsed. `profiles` may be
///   empty (last-profile-deleted state).
/// - `IdentityMismatch` — envelope was encrypted under a different nsec;
///   no plaintext returned, UI must prompt user to clear or re-key.
/// - `Error` — file present but unreadable (corrupt, decrypt fail,
///   migration write failure, etc.). Preserves today's load-error banner.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum SettingsStateResponse {
    None,
    #[serde(rename_all = "camelCase")]
    Ok {
        default_profile_id: Option<String>,
        profiles: Vec<ProfileSummary>,
    },
    #[serde(rename_all = "camelCase")]
    IdentityMismatch {
        stored_pubkey: String,
    },
    Error {
        message: String,
    },
}

/// One row in the profile list. Identifies a profile and shows its
/// non-secret metadata + key preview for picker UI. **Never** carries
/// the full API key.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSummary {
    pub id: String,
    pub label: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub detected_provider_id: String,
    pub api_key_present: bool,
    pub api_key_preview: Option<String>,
}

/// Status returned by `get_agent_provider_profile(id)` for a single
/// profile's full view (used by the edit dialog to prefill).
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum ProfileLoadStatus {
    None,
    Ok {
        view: AgentProviderSettingsView,
    },
    #[serde(rename_all = "camelCase")]
    IdentityMismatch {
        stored_pubkey: String,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProviderSettingsView {
    /// Human-readable profile label. Included here so the edit dialog
    /// hydrates label + form atomically from a single response (no
    /// initialLabel prop, no two-effect hydration race).
    pub label: String,
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub anthropic_api_version: Option<String>,
    pub system_prompt: Option<String>,
    pub max_rounds: Option<u32>,
    pub max_output_tokens: Option<u32>,
    pub llm_timeout_secs: Option<u64>,
    pub tool_timeout_secs: Option<u64>,
    pub max_history_bytes: Option<usize>,
    pub detected_provider_id: String,
    pub detection_overridden: bool,
    pub api_key_present: bool,
    /// Last 4 chars of the saved API key for UI preview ("••••sk-7").
    /// Never the full key.
    pub api_key_preview: Option<String>,
}

/// `Debug` is intentionally not derived: the `api_key` field carries
/// plaintext on the way in from the IPC boundary. Logging or panicking on
/// this struct would leak the key. Use scoped, redacted formatting at error
/// sites if needed.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProviderSettingsInput {
    /// `None` = create a fresh profile (server generates the id). `Some(id)`
    /// = update an existing profile in place. An unknown id is rejected.
    #[serde(default)]
    pub profile_id: Option<String>,
    /// Human-readable label. Required for create; for update, an empty/
    /// whitespace-only value is rejected. Trimmed, control-chars rejected,
    /// max 64 chars (see `validate_label`).
    pub label: String,
    pub provider: String,
    /// `None` = preserve the previously stored key (only valid when an existing
    /// record has the same provider, detected_provider_id, and base-URL origin).
    /// `Some(s)` = use this key. `Some("")` is rejected.
    /// **Create with `None` is rejected** — there's no previous key to reuse.
    #[serde(default)]
    pub api_key: Option<String>,
    pub model: String,
    pub base_url: String,
    #[serde(default)]
    pub anthropic_api_version: Option<String>,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub max_rounds: Option<u32>,
    #[serde(default)]
    pub max_output_tokens: Option<u32>,
    #[serde(default)]
    pub llm_timeout_secs: Option<u64>,
    #[serde(default)]
    pub tool_timeout_secs: Option<u64>,
    #[serde(default)]
    pub max_history_bytes: Option<usize>,
    pub detected_provider_id: String,
    #[serde(default)]
    pub detection_overridden: bool,
}

/// Zeroize the optional API key on drop so a `validate_input` failure (or any
/// other early-return path before the key is moved into a `Zeroizing` wrapper)
/// doesn't leave a plaintext `String` on the heap awaiting a normal drop.
impl Drop for AgentProviderSettingsInput {
    fn drop(&mut self) {
        if let Some(k) = self.api_key.as_mut() {
            k.zeroize();
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProviderEnvPresence {
    pub sprout_agent_provider: bool,
    pub anthropic_api_key: bool,
    pub openai_compat_api_key: bool,
}

/// Return value of `save_agent_provider_profile` — gives the UI the id of
/// the (possibly newly created) profile so it can re-select it after
/// invalidation.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProfileResponse {
    pub profile_id: String,
    /// True when this save also set the default (first-profile auto-default
    /// per §5 of the plan).
    pub set_as_default: bool,
}

// ─── Submodules ─────────────────────────────────────────────────────────────

mod commands;
mod commands_write;
mod spawn;
mod storage;
mod storage_profiles;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_profiles;
#[cfg(test)]
mod tests_spawn;

pub use commands::{
    get_agent_provider_env_presence, get_agent_provider_profile, get_agent_provider_settings_state,
};
// crate-local: agents.rs / agent_models.rs use this to defensively
// validate a pinned profile id before persisting an agent record.
pub(crate) use commands::{check_provider_profile_id, ProfileIdCheck};
pub use commands_write::{
    delete_agent_provider_profile, delete_agent_provider_settings, save_agent_provider_profile,
    set_default_agent_provider_profile,
};
pub use spawn::{apply_to_command, load_for_spawn};
// `LoadForSpawn` and `EnvPairs` are re-exported test-only so the
// integration-style tests in `managed_agents::runtime::tests` can drive the
// apply path through every variant (None / Ok(pairs) / IdentityMismatch /
// Error) without filesystem or Tauri State setup. Production callers in
// `runtime.rs` only need `load_for_spawn` + `apply_to_command` and never
// name these types, so we don't re-export them unconditionally.
#[cfg(test)]
pub use spawn::{EnvPairs, LoadForSpawn};
