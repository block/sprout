use reqwest::Method;
use tauri::State;

use crate::{
    app_state::AppState,
    events,
    relay::{api_path, build_authed_request, send_json_request, submit_event},
};

#[tauri::command]
pub async fn get_canvas(
    channel_id: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let path = api_path(&["channels", &channel_id, "canvas"]);
    let request = build_authed_request(&state.http_client, Method::GET, &path, &state)?;
    send_json_request(request).await
}

#[tauri::command]
pub async fn set_canvas(
    channel_id: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let uuid = uuid::Uuid::parse_str(&channel_id)
        .map_err(|_| format!("invalid channel UUID: {channel_id}"))?;
    let builder = events::build_set_canvas(uuid, &content)?;
    let result = submit_event(builder, &state).await?;

    Ok(serde_json::json!({
        "ok": true,
        "event_id": result.event_id,
    }))
}
