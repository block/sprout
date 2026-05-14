use nostr::Keys;
use serde::Serialize;
use tauri::State;

use crate::app_state::AppState;
use crate::relay;

#[derive(Serialize)]
pub struct ActiveWorkspaceInfo {
    relay_url: String,
    pubkey: String,
}

/// Returns the current active workspace info (relay URL + pubkey).
#[tauri::command]
pub fn get_active_workspace(state: State<'_, AppState>) -> Result<ActiveWorkspaceInfo, String> {
    let keys = state.keys.lock().map_err(|e| e.to_string())?;
    let relay_url = relay::relay_ws_url_with_override(&state);
    Ok(ActiveWorkspaceInfo {
        relay_url,
        pubkey: keys.public_key().to_hex(),
    })
}

/// Apply a workspace's configuration to the backend session.
///
/// Called by the frontend on app init (after reload) to configure the
/// Tauri backend with the selected workspace's relay URL and keys.
#[tauri::command]
pub fn apply_workspace(
    relay_url: String,
    nsec: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // ── Validate before mutating ──────────────────────────────────────────
    let parsed_keys = match nsec.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(nsec_trimmed) => {
            Some(Keys::parse(nsec_trimmed).map_err(|e| format!("invalid nsec: {e}"))?)
        }
        None => None,
    };

    // ── Apply all state changes (nothing below can fail) ──────────────────
    {
        let mut override_guard = state.relay_url_override.lock().map_err(|e| e.to_string())?;
        *override_guard = Some(relay_url);
    }

    if let Some(keys) = parsed_keys {
        // Lock order: `agent_provider_settings_lock` BEFORE `state.keys`.
        // See `AppState::agent_provider_settings_lock` — serializes against
        // in-flight settings reads/writes so they never observe a partial
        // identity rotation.
        let _settings_guard = state
            .agent_provider_settings_lock
            .lock()
            .map_err(|e| e.to_string())?;
        let mut keys_guard = state.keys.lock().map_err(|e| e.to_string())?;
        *keys_guard = keys;
    }

    Ok(())
}
