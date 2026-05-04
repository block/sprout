use reqwest::Method;
use tauri::State;

use crate::{
    app_state::AppState,
    events,
    relay::{api_path, build_authed_request, send_json_request, submit_event},
};

#[tauri::command]
pub async fn list_relay_members(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let request = build_authed_request(
        &state.http_client,
        Method::GET,
        &api_path(&["relay", "members"]),
        &state,
    )?;
    send_json_request(request).await
}

#[tauri::command]
pub async fn get_my_relay_membership(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let request = build_authed_request(
        &state.http_client,
        Method::GET,
        &api_path(&["relay", "members", "me"]),
        &state,
    )?;
    send_json_request(request).await
}

#[tauri::command]
pub async fn add_relay_member(
    target_pubkey: String,
    role: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let builder = events::build_relay_admin_add(&target_pubkey, &role)?;
    let result = submit_event(builder, &state).await?;
    Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
}

#[tauri::command]
pub async fn remove_relay_member(
    target_pubkey: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let builder = events::build_relay_admin_remove(&target_pubkey)?;
    let result = submit_event(builder, &state).await?;
    Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
}

#[tauri::command]
pub async fn change_relay_member_role(
    target_pubkey: String,
    new_role: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let builder = events::build_relay_admin_change_role(&target_pubkey, &new_role)?;
    let result = submit_event(builder, &state).await?;
    Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
}
