use reqwest::Method;
use tauri::State;

use crate::{
    app_state::AppState,
    models::SetCanvasBody,
    relay::{build_authed_request, send_json_request},
};

#[tauri::command]
pub async fn get_canvas(
    channel_id: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let path = format!("/api/channels/{channel_id}/canvas");
    let request = build_authed_request(&state.http_client, Method::GET, &path, &state)?;
    send_json_request(request).await
}

#[tauri::command]
pub async fn set_canvas(
    channel_id: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let path = format!("/api/channels/{channel_id}/canvas");
    let request = build_authed_request(&state.http_client, Method::PUT, &path, &state)?
        .json(&SetCanvasBody { content: &content });
    send_json_request(request).await
}
