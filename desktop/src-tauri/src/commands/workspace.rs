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
/// Tauri backend with the selected workspace's relay URL, keys, and token.
#[tauri::command]
pub fn apply_workspace(
    relay_url: String,
    nsec: Option<String>,
    token: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Set relay URL override
    {
        let mut override_guard = state.relay_url_override.lock().map_err(|e| e.to_string())?;
        *override_guard = Some(relay_url);
    }

    // Set keys if nsec is provided
    if let Some(nsec_str) = nsec {
        let nsec_trimmed = nsec_str.trim();
        if !nsec_trimmed.is_empty() {
            let new_keys = Keys::parse(nsec_trimmed).map_err(|e| format!("invalid nsec: {e}"))?;
            let mut keys_guard = state.keys.lock().map_err(|e| e.to_string())?;
            *keys_guard = new_keys;
        }
    }

    // Set API token override
    {
        let mut token_guard = state.session_token.lock().map_err(|e| e.to_string())?;
        *token_guard = token;
    }

    Ok(())
}
