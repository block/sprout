//! Write-side Tauri commands: save / set-default / delete-one /
//! delete-all. Split from `commands.rs` to keep each file under the
//! repo's 500-line lint cap.
//!
//! All writes hold `AppState::agent_provider_settings_lock` across the
//! read-modify-write of the envelope file (lock order: settings → keys).

use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, State};
use zeroize::Zeroizing;

/// Tauri event broadcast to every window after a successful write to the
/// encrypted agent-provider envelope. Mirrors
/// `AGENT_PROVIDER_SETTINGS_CHANGED_EVENT` on the frontend; lets a second
/// open Sprout window invalidate its cached list/profile queries instead
/// of holding pre-mutation data forever (queries are staleTime=Infinity).
const SETTINGS_CHANGED_EVENT: &str = "agent-provider-settings:changed";

/// Best-effort broadcast. We never fail a write because of a stale-event
/// emit (the caller already got the durable disk write); just log.
fn emit_settings_changed(app: &AppHandle) {
    if let Err(e) = app.emit(SETTINGS_CHANGED_EVENT, ()) {
        // Match the existing logging style in this crate (no tracing/log
        // dep) — write to stderr; the disk write already succeeded so
        // we're never failing a user-visible action over this.
        eprintln!("[agent_provider_settings] failed to emit settings-changed event: {e}",);
    }
}

use super::storage::{
    decrypt_settings, normalize_origin, read_envelope, settings_path, validate_input,
};
use super::storage_profiles::{
    new_profile_id, parse_or_migrate_plaintext, validate_label, write_profiles_envelope,
    ParsedPlaintext,
};
use super::{
    AgentProviderSettingsInput, NamedProfile, ProfilesPlaintext, SaveProfileResponse,
    StoredSettings, CURRENT_PLAINTEXT_SCHEMA,
};
use crate::app_state::AppState;

#[tauri::command]
pub fn save_agent_provider_profile(
    mut input: AgentProviderSettingsInput,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SaveProfileResponse, String> {
    // ── Trim & validate non-secret fields (mirrors today's flow) ──────────
    input.label = validate_label(&input.label)?;
    input.model = input.model.trim().to_owned();
    input.base_url = input.base_url.trim().to_owned();
    input.detected_provider_id = input.detected_provider_id.trim().to_owned();
    if let Some(v) = input.anthropic_api_version.as_mut() {
        *v = v.trim().to_owned();
    }
    // SECURITY: extract the api_key into Zeroizing before trim so the un-
    // trimmed buffer is wiped at end-of-scope.
    if let Some(raw) = input.api_key.take() {
        let zeroized_raw: Zeroizing<String> = Zeroizing::new(raw);
        let trimmed = zeroized_raw.trim();
        if trimmed.is_empty() {
            return Err("API key cannot be empty".into());
        }
        input.api_key = Some(trimmed.to_owned());
    }

    validate_input(&input)?;

    let _settings_guard = state
        .agent_provider_settings_lock
        .lock()
        .map_err(|e| e.to_string())?;

    let path = settings_path(&app)?;
    let keys = state.keys.lock().map_err(|e| e.to_string())?;
    let current_pubkey = keys.public_key().to_hex();

    // Read existing wrapper (if any). On corrupt envelope, we tolerate
    // only when the caller is creating a brand-new profile with their own
    // api_key — that's a fresh start. Updates and key-reuse paths require
    // a readable existing state.
    let mut wrapper = match read_envelope(&path) {
        Ok(Some(envelope)) => {
            if envelope.pubkey != current_pubkey {
                return Err(
                    "Saved settings were encrypted for a different identity — open the panel \
                     and clear them or sign back in"
                        .into(),
                );
            }
            let plain = decrypt_settings(&keys, &envelope.ciphertext)?;
            let parsed = parse_or_migrate_plaintext(&plain, &envelope.pubkey)
                .map_err(|e| format!("parse existing settings: {e}"))?;
            match parsed {
                ParsedPlaintext::Current(w) => w,
                ParsedPlaintext::Migrated(w) => w,
            }
        }
        Ok(None) => ProfilesPlaintext {
            schema_version: CURRENT_PLAINTEXT_SCHEMA,
            owner_pubkey: current_pubkey.clone(),
            default_profile_id: None,
            profiles: Vec::new(),
        },
        Err(e) => {
            if input.profile_id.is_none() && input.api_key.is_some() {
                // Creating a fresh profile with a fresh key — start over.
                ProfilesPlaintext {
                    schema_version: CURRENT_PLAINTEXT_SCHEMA,
                    owner_pubkey: current_pubkey.clone(),
                    default_profile_id: None,
                    profiles: Vec::new(),
                }
            } else {
                return Err(format!(
                    "Existing settings unreadable ({e}); add a new profile from scratch to \
                     overwrite."
                ));
            }
        }
    };

    // Compute the new/updated profile's StoredSettings.
    let now = now_unix();
    let (profile_id, set_as_default) = match input.profile_id.clone() {
        // ── UPDATE path ──────────────────────────────────────────────────
        Some(id) => {
            let pos = wrapper
                .profiles
                .iter()
                .position(|p| p.id == id)
                .ok_or_else(|| format!("unknown profile id {id}"))?;

            let resolved_api_key: Zeroizing<String> = match input.api_key.take() {
                Some(k) if k.is_empty() => return Err("API key cannot be empty".into()),
                Some(k) => Zeroizing::new(k),
                None => {
                    // Key reuse: same provider, detected_provider_id, base-URL
                    // origin as the existing slot. Identical to today's save
                    // contract — see commands.rs pre-multi-profile.
                    let prev = &wrapper.profiles[pos].settings;
                    if !prev.owner_pubkey.is_empty() && prev.owner_pubkey != current_pubkey {
                        return Err("Saved profile owner mismatch — re-enter the API key".into());
                    }
                    if prev.provider != input.provider {
                        return Err("Provider changed — re-enter API key".into());
                    }
                    if prev.detected_provider_id != input.detected_provider_id {
                        return Err("Issuer changed — re-enter API key".into());
                    }
                    let prev_origin = normalize_origin(&prev.base_url)?;
                    let new_origin = normalize_origin(&input.base_url)?;
                    if prev_origin != new_origin {
                        return Err("Base URL origin changed — re-enter API key".into());
                    }
                    if prev.api_key.is_empty() {
                        return Err("No previously stored API key to reuse".into());
                    }
                    Zeroizing::new(prev.api_key.clone())
                }
            };

            let updated = build_stored_from_input(&mut input, &resolved_api_key, &current_pubkey);
            wrapper.profiles[pos].label = input.label.clone();
            wrapper.profiles[pos].updated_at = now;
            wrapper.profiles[pos].settings = updated;

            let set_as_default = ensure_default(&mut wrapper, &id);
            (id, set_as_default)
        }
        // ── CREATE path ──────────────────────────────────────────────────
        None => {
            let api_key = match input.api_key.take() {
                Some(k) if k.is_empty() => return Err("API key cannot be empty".into()),
                Some(k) => Zeroizing::new(k),
                None => {
                    return Err(
                        "API key required when creating a new profile (no previously saved \
                         settings to reuse)"
                            .into(),
                    );
                }
            };
            let new_id = new_profile_id();
            let stored = build_stored_from_input(&mut input, &api_key, &current_pubkey);
            wrapper.profiles.push(NamedProfile {
                id: new_id.clone(),
                label: input.label.clone(),
                created_at: now,
                updated_at: now,
                settings: stored,
            });
            let set_as_default = ensure_default(&mut wrapper, &new_id);
            (new_id, set_as_default)
        }
    };

    // Sanity: owner_pubkey on the wrapper must match the current identity.
    // We may have just read a wrapper migrated from a v2 plaintext under an
    // earlier-but-same identity; we re-stamp it as a defense.
    wrapper.owner_pubkey = current_pubkey.clone();
    wrapper.schema_version = CURRENT_PLAINTEXT_SCHEMA;

    write_profiles_envelope(&path, &keys, &wrapper)?;
    drop(keys);
    emit_settings_changed(&app);

    Ok(SaveProfileResponse {
        profile_id,
        set_as_default,
    })
}

/// Build a `StoredSettings` from a validated input + resolved api_key.
/// Caller owns the lifetime of `input` (we move String fields out via
/// `std::mem::take`).
fn build_stored_from_input(
    input: &mut AgentProviderSettingsInput,
    resolved_api_key: &Zeroizing<String>,
    owner_pubkey: &str,
) -> StoredSettings {
    StoredSettings {
        // v2 plaintext schema for per-profile settings — the wrapper bumps
        // to v3 outside this function. v2 carries owner_pubkey for
        // defense-in-depth and continues to drive the v2 cross-check in
        // load_for_spawn.
        schema_version: 2,
        owner_pubkey: owner_pubkey.to_owned(),
        provider: std::mem::take(&mut input.provider),
        api_key: resolved_api_key.to_string(),
        model: std::mem::take(&mut input.model),
        base_url: std::mem::take(&mut input.base_url),
        anthropic_api_version: input.anthropic_api_version.take(),
        system_prompt: input.system_prompt.take(),
        max_rounds: input.max_rounds,
        max_output_tokens: input.max_output_tokens,
        llm_timeout_secs: input.llm_timeout_secs,
        tool_timeout_secs: input.tool_timeout_secs,
        max_history_bytes: input.max_history_bytes,
        detected_provider_id: std::mem::take(&mut input.detected_provider_id),
        detection_overridden: input.detection_overridden,
    }
}

/// First-profile / no-default auto-default policy. If the wrapper has no
/// `default_profile_id` after we just touched a profile, set it to the
/// touched id. Returns true if we changed the default.
fn ensure_default(wrapper: &mut ProfilesPlaintext, touched: &str) -> bool {
    match wrapper.default_profile_id.as_deref() {
        Some(existing) if wrapper.profiles.iter().any(|p| p.id == existing) => false,
        _ => {
            wrapper.default_profile_id = Some(touched.to_owned());
            true
        }
    }
}

/// Set or clear the default profile.
///
/// `Some(id)` MUST resolve to an existing profile; unknown id is rejected
/// (no persistent dangling default).
#[tauri::command]
pub fn set_default_agent_provider_profile(
    profile_id: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let _settings_guard = state
        .agent_provider_settings_lock
        .lock()
        .map_err(|e| e.to_string())?;

    let path = settings_path(&app)?;
    let keys = state.keys.lock().map_err(|e| e.to_string())?;
    let envelope = read_envelope(&path)?
        .ok_or_else(|| "No saved settings — add a profile first".to_owned())?;
    if envelope.pubkey != keys.public_key().to_hex() {
        return Err(
            "Saved settings were encrypted for a different identity — open the panel and \
             clear them or sign back in"
                .into(),
        );
    }
    let plain = decrypt_settings(&keys, &envelope.ciphertext)?;
    let parsed = parse_or_migrate_plaintext(&plain, &envelope.pubkey)
        .map_err(|e| format!("parse existing settings: {e}"))?;
    let mut wrapper = match parsed {
        ParsedPlaintext::Current(w) => w,
        ParsedPlaintext::Migrated(w) => w,
    };

    match profile_id.as_deref() {
        Some(id) => {
            if !wrapper.profiles.iter().any(|p| p.id == id) {
                return Err(format!("unknown profile id {id}"));
            }
            wrapper.default_profile_id = Some(id.to_owned());
        }
        None => {
            wrapper.default_profile_id = None;
        }
    }

    write_profiles_envelope(&path, &keys, &wrapper)?;
    emit_settings_changed(&app);
    Ok(())
}

/// Delete a single profile. If the deleted profile was the default,
/// clears `default_profile_id` (UI banner state, plan §5).
#[tauri::command]
pub fn delete_agent_provider_profile(
    profile_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let _settings_guard = state
        .agent_provider_settings_lock
        .lock()
        .map_err(|e| e.to_string())?;

    let path = settings_path(&app)?;
    let keys = state.keys.lock().map_err(|e| e.to_string())?;
    let envelope = read_envelope(&path)?.ok_or_else(|| "No saved settings".to_owned())?;
    if envelope.pubkey != keys.public_key().to_hex() {
        return Err(
            "Saved settings were encrypted for a different identity — open the panel and \
             clear them or sign back in"
                .into(),
        );
    }
    let plain = decrypt_settings(&keys, &envelope.ciphertext)?;
    let parsed = parse_or_migrate_plaintext(&plain, &envelope.pubkey)
        .map_err(|e| format!("parse existing settings: {e}"))?;
    let mut wrapper = match parsed {
        ParsedPlaintext::Current(w) => w,
        ParsedPlaintext::Migrated(w) => w,
    };

    let before = wrapper.profiles.len();
    wrapper.profiles.retain(|p| p.id != profile_id);
    if wrapper.profiles.len() == before {
        return Err(format!("unknown profile id {profile_id}"));
    }
    if wrapper.default_profile_id.as_deref() == Some(profile_id.as_str()) {
        wrapper.default_profile_id = None;
    }

    write_profiles_envelope(&path, &keys, &wrapper)?;
    emit_settings_changed(&app);
    Ok(())
}

/// Remove the entire encrypted envelope file. After this command the panel
/// returns `none`. The user can re-add profiles from scratch.
#[tauri::command]
pub fn delete_agent_provider_settings(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let _settings_guard = state
        .agent_provider_settings_lock
        .lock()
        .map_err(|e| e.to_string())?;
    let path = settings_path(&app)?;
    match std::fs::remove_file(&path) {
        Ok(_) => {
            emit_settings_changed(&app);
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Nothing on disk to clear, but a stale in-memory state on
            // another window might still be wrong — broadcast anyway.
            emit_settings_changed(&app);
            Ok(())
        }
        Err(e) => Err(format!("delete settings file: {e}")),
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
