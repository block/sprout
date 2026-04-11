use reqwest::Method;
use tauri::State;

use crate::{
    app_state::AppState,
    models::{ChannelInfo, OpenDmBody, OpenDmResponse},
    relay::{api_path, build_authed_request, send_empty_request, send_json_request},
};

#[tauri::command]
pub async fn open_dm(
    pubkeys: Vec<String>,
    state: State<'_, AppState>,
) -> Result<ChannelInfo, String> {
    let request = build_authed_request(&state.http_client, Method::POST, "/api/dms", &state)?
        .json(&OpenDmBody { pubkeys: &pubkeys });
    let response: OpenDmResponse = send_json_request(request).await?;

    let path = api_path(&["channels", &response.channel_id]);
    let request = build_authed_request(&state.http_client, Method::GET, &path, &state)?;
    send_json_request(request).await
}

#[tauri::command]
pub async fn hide_dm(channel_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let path = api_path(&["dms", &channel_id, "hide"]);
    let request = build_authed_request(&state.http_client, Method::POST, &path, &state)?;
    send_empty_request(request).await
}
