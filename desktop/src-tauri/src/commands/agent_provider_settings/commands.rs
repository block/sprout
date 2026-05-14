//! Tauri command entrypoints — the public IPC surface for the
//! Settings → Agent Provider panel.
//!
//! All commands that touch the envelope file hold
//! `AppState::agent_provider_settings_lock` across the read-modify-write to
//! serialize against concurrent saves and identity rotation. Lock order is
//! invariant: **settings lock → keys lock** (see `app_state.rs`).
//!
//! ## Multi-profile semantics
//!
//! The file is one NIP-44-self-encrypted envelope containing a
//! `ProfilesPlaintext` wrapper with N named profiles. The "current default"
//! is `wrapper.default_profile_id` (Option). Agents may pin a specific
//! profile via `ManagedAgentRecord.provider_profile_id`; if `None`, the
//! agent uses the default at spawn.

use tauri::{AppHandle, State};

use super::storage::{decrypt_settings, read_envelope, settings_path};
use super::storage_profiles::{
    parse_or_migrate_plaintext, write_profiles_envelope, ParseError, ParsedPlaintext,
};
use super::{
    AgentProviderEnvPresence, AgentProviderSettingsView, NamedProfile, ProfileLoadStatus,
    ProfileSummary, ProfilesPlaintext, SettingsStateResponse,
};
use crate::app_state::AppState;

// ─── Helpers ───────────────────────────────────────────────────────────────

/// Read + decrypt + parse-or-migrate the envelope under the settings lock.
/// On a Migrated outcome we persist to disk and **fail loudly** on write
/// error — read-path commands must not return a generated-but-unpersisted
/// profile id (plan §4 R3 MED).
///
/// Returns:
/// - `Ok(Some(wrapper))` when the envelope is present and readable.
/// - `Ok(None)` when the envelope file is absent.
fn read_wrapper_locked(app: &AppHandle, state: &AppState) -> Result<ReadResult, String> {
    let path = settings_path(app)?;
    let envelope = match read_envelope(&path)? {
        Some(e) => e,
        None => return Ok(ReadResult::None),
    };
    let keys = state.keys.lock().map_err(|e| e.to_string())?;
    let current_pubkey = keys.public_key().to_hex();
    if envelope.pubkey != current_pubkey {
        return Ok(ReadResult::IdentityMismatch {
            stored_pubkey: envelope.pubkey,
        });
    }
    let plain = decrypt_settings(&keys, &envelope.ciphertext)?;
    let parsed = match parse_or_migrate_plaintext(&plain, &envelope.pubkey) {
        Ok(p) => p,
        // Embedded owner_pubkey mismatch ⇒ same threat as envelope.pubkey
        // mismatch; surface to UI as IdentityMismatch.
        Err(ParseError::IdentityMismatch) => {
            return Ok(ReadResult::IdentityMismatch {
                stored_pubkey: envelope.pubkey,
            });
        }
        Err(ParseError::Other(msg)) => {
            return Err(format!("parse profiles plaintext: {msg}"));
        }
    };
    let wrapper = match parsed {
        ParsedPlaintext::Current(w) => w,
        ParsedPlaintext::Migrated(w) => {
            // Read-path: write or fail loudly. Otherwise the UI would
            // edit/delete a profile id that does not survive the next call.
            write_profiles_envelope(&path, &keys, &w)
                .map_err(|e| format!("settings migration write failed: {e}"))?;
            w
        }
    };
    drop(keys);
    Ok(ReadResult::Ok { wrapper })
}

enum ReadResult {
    None,
    Ok { wrapper: ProfilesPlaintext },
    IdentityMismatch { stored_pubkey: String },
}

fn profile_to_summary(profile: &NamedProfile) -> ProfileSummary {
    ProfileSummary {
        id: profile.id.clone(),
        label: profile.label.clone(),
        created_at: profile.created_at,
        updated_at: profile.updated_at,
        provider: profile.settings.provider.clone(),
        model: profile.settings.model.clone(),
        base_url: profile.settings.base_url.clone(),
        detected_provider_id: profile.settings.detected_provider_id.clone(),
        api_key_present: !profile.settings.api_key.is_empty(),
        api_key_preview: compute_preview(&profile.settings.api_key),
    }
}

fn profile_to_view(profile: &NamedProfile) -> AgentProviderSettingsView {
    let s = &profile.settings;
    AgentProviderSettingsView {
        label: profile.label.clone(),
        provider: s.provider.clone(),
        model: s.model.clone(),
        base_url: s.base_url.clone(),
        anthropic_api_version: s.anthropic_api_version.clone(),
        system_prompt: s.system_prompt.clone(),
        max_rounds: s.max_rounds,
        max_output_tokens: s.max_output_tokens,
        llm_timeout_secs: s.llm_timeout_secs,
        tool_timeout_secs: s.tool_timeout_secs,
        max_history_bytes: s.max_history_bytes,
        detected_provider_id: s.detected_provider_id.clone(),
        detection_overridden: s.detection_overridden,
        api_key_present: !s.api_key.is_empty(),
        api_key_preview: compute_preview(&s.api_key),
    }
}

// ─── IPC commands ───────────────────────────────────────────────────────────

/// Return the full settings state: list of profiles + which is default,
/// or a discriminated error (none / identity-mismatch / error).
#[tauri::command]
pub fn get_agent_provider_settings_state(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SettingsStateResponse, String> {
    let _settings_guard = state
        .agent_provider_settings_lock
        .lock()
        .map_err(|e| e.to_string())?;

    match read_wrapper_locked(&app, &state) {
        Ok(ReadResult::None) => Ok(SettingsStateResponse::None),
        Ok(ReadResult::IdentityMismatch { stored_pubkey }) => {
            Ok(SettingsStateResponse::IdentityMismatch { stored_pubkey })
        }
        Ok(ReadResult::Ok { wrapper, .. }) => {
            let profiles = wrapper.profiles.iter().map(profile_to_summary).collect();
            Ok(SettingsStateResponse::Ok {
                default_profile_id: wrapper.default_profile_id,
                profiles,
            })
        }
        Err(message) => Ok(SettingsStateResponse::Error { message }),
    }
}

/// Return the full editable view of a single profile (no secret).
#[tauri::command]
pub fn get_agent_provider_profile(
    profile_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ProfileLoadStatus, String> {
    let _settings_guard = state
        .agent_provider_settings_lock
        .lock()
        .map_err(|e| e.to_string())?;

    match read_wrapper_locked(&app, &state) {
        Ok(ReadResult::None) => Ok(ProfileLoadStatus::None),
        Ok(ReadResult::IdentityMismatch { stored_pubkey }) => {
            Ok(ProfileLoadStatus::IdentityMismatch { stored_pubkey })
        }
        Ok(ReadResult::Ok { wrapper, .. }) => {
            match wrapper.profiles.iter().find(|p| p.id == profile_id) {
                Some(profile) => Ok(ProfileLoadStatus::Ok {
                    view: profile_to_view(profile),
                }),
                None => Ok(ProfileLoadStatus::None),
            }
        }
        Err(message) => Ok(ProfileLoadStatus::Error { message }),
    }
}
#[tauri::command]
pub fn get_agent_provider_env_presence() -> Result<AgentProviderEnvPresence, String> {
    Ok(AgentProviderEnvPresence {
        sprout_agent_provider: std::env::var_os("SPROUT_AGENT_PROVIDER").is_some(),
        anthropic_api_key: std::env::var_os("ANTHROPIC_API_KEY").is_some(),
        openai_compat_api_key: std::env::var_os("OPENAI_COMPAT_API_KEY").is_some(),
    })
}

/// Result of a defensive check that a specific profile id exists in the
/// current encrypted settings. Used by the agent create/update IPC paths
/// to harden the "stale picker value" attack surface: even if the UI
/// fails to clear a dangling pin, the backend refuses to persist it.
pub(crate) enum ProfileIdCheck {
    /// The id resolves to a profile in the wrapper.
    Ok,
    /// The wrapper is present and readable but does NOT contain this id.
    Unknown,
    /// The settings file does not exist, can't be read, or is bound to a
    /// different identity. We do not block the agent save in this state:
    /// the user can have a pinned id from an earlier readable wrapper,
    /// and a transient read failure shouldn't block unrelated edits. The
    /// spawn path still fails closed for the actual sprout-agent run.
    Indeterminate,
}

/// Validate that `id` resolves to a currently-saved profile. Takes the
/// settings lock briefly. Lock order: caller must NOT already hold the
/// agents store lock (settings → keys → agents-store).
pub(crate) fn check_provider_profile_id(
    app: &AppHandle,
    state: &AppState,
    id: &str,
) -> Result<ProfileIdCheck, String> {
    let _guard = state
        .agent_provider_settings_lock
        .lock()
        .map_err(|e| e.to_string())?;
    match read_wrapper_locked(app, state) {
        Ok(ReadResult::Ok { wrapper }) => {
            if wrapper.profiles.iter().any(|p| p.id == id) {
                Ok(ProfileIdCheck::Ok)
            } else {
                Ok(ProfileIdCheck::Unknown)
            }
        }
        Ok(ReadResult::None) | Ok(ReadResult::IdentityMismatch { .. }) | Err(_) => {
            Ok(ProfileIdCheck::Indeterminate)
        }
    }
}

/// Compute the UI preview for a saved API key. Returns the last 4 chars
/// when the key is long enough; otherwise `None`. Never the full key.
pub(super) fn compute_preview(api_key: &str) -> Option<String> {
    const PREVIEW_LEN: usize = 4;
    const MIN_KEY_LEN_FOR_PREVIEW: usize = PREVIEW_LEN * 2; // 8
    if api_key.is_empty() {
        return None;
    }
    let len = api_key.chars().count();
    if len < MIN_KEY_LEN_FOR_PREVIEW {
        return None;
    }
    Some(api_key.chars().skip(len - PREVIEW_LEN).collect())
}
