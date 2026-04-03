use reqwest::Method;
use serde::Serialize;
use tauri::State;

use crate::{
    app_state::AppState,
    relay::{build_authed_request, send_empty_request, send_json_request},
};

// ── Reads ───────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_channel_workflows(
    channel_id: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let path = format!("/api/channels/{channel_id}/workflows");
    let request = build_authed_request(&state.http_client, Method::GET, &path, &state)?;
    send_json_request(request).await
}

#[tauri::command]
pub async fn get_workflow(
    workflow_id: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let path = format!("/api/workflows/{workflow_id}");
    let request = build_authed_request(&state.http_client, Method::GET, &path, &state)?;
    send_json_request(request).await
}

#[tauri::command]
pub async fn get_workflow_runs(
    workflow_id: String,
    limit: Option<u32>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let mut path = format!("/api/workflows/{workflow_id}/runs");
    if let Some(limit) = limit {
        path.push_str(&format!("?limit={limit}"));
    }
    let request = build_authed_request(&state.http_client, Method::GET, &path, &state)?;
    send_json_request(request).await
}

// ── Writes ──────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct CreateWorkflowBody {
    yaml_definition: String,
}

#[tauri::command]
pub async fn create_workflow(
    channel_id: String,
    yaml_definition: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let path = format!("/api/channels/{channel_id}/workflows");
    let request = build_authed_request(&state.http_client, Method::POST, &path, &state)?
        .json(&CreateWorkflowBody { yaml_definition });
    send_json_request(request).await
}

#[derive(Serialize)]
struct UpdateWorkflowBody {
    yaml_definition: String,
}

#[tauri::command]
pub async fn update_workflow(
    workflow_id: String,
    yaml_definition: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let path = format!("/api/workflows/{workflow_id}");
    let request = build_authed_request(&state.http_client, Method::PUT, &path, &state)?
        .json(&UpdateWorkflowBody { yaml_definition });
    send_json_request(request).await
}

#[tauri::command]
pub async fn delete_workflow(
    workflow_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let path = format!("/api/workflows/{workflow_id}");
    let request = build_authed_request(&state.http_client, Method::DELETE, &path, &state)?;
    send_empty_request(request).await
}

#[tauri::command]
pub async fn trigger_workflow(
    workflow_id: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let path = format!("/api/workflows/{workflow_id}/trigger");
    let request = build_authed_request(&state.http_client, Method::POST, &path, &state)?;
    send_json_request(request).await
}

// ── Approvals ───────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_run_approvals(
    workflow_id: String,
    run_id: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let path = format!("/api/workflows/{workflow_id}/runs/{run_id}/approvals");
    let request = build_authed_request(&state.http_client, Method::GET, &path, &state)?;
    send_json_request(request).await
}

#[derive(Serialize)]
struct ApprovalBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
}

#[tauri::command]
pub async fn grant_approval(
    token: String,
    note: Option<String>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let path = format!("/api/approvals/{token}/grant");
    let request = build_authed_request(&state.http_client, Method::POST, &path, &state)?
        .json(&ApprovalBody { note });
    send_json_request(request).await
}

#[tauri::command]
pub async fn deny_approval(
    token: String,
    note: Option<String>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let path = format!("/api/approvals/{token}/deny");
    let request = build_authed_request(&state.http_client, Method::POST, &path, &state)?
        .json(&ApprovalBody { note });
    send_json_request(request).await
}
