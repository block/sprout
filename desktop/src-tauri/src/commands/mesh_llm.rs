use tauri::{AppHandle, State};

use crate::{app_state::AppState, mesh_llm, relay};

pub type CmdResult<T> = Result<T, String>;

#[tauri::command]
pub async fn mesh_availability(
    state: State<'_, AppState>,
) -> CmdResult<mesh_llm::MeshAvailability> {
    match relay::query_relay(&state, &[mesh_llm::mesh_status_filter()]).await {
        Ok(events) => Ok(mesh_llm::availability_from_events(events)),
        Err(error) => Ok(mesh_llm::MeshAvailability::unavailable(error)),
    }
}

#[tauri::command]
pub async fn mesh_start_node(
    _app: AppHandle,
    state: State<'_, AppState>,
    request: mesh_llm::StartMeshNodeRequest,
) -> CmdResult<mesh_llm::MeshNodeStatus> {
    let mut runtime = state.mesh_llm_runtime.lock().await;
    if runtime.is_some() {
        return Err("mesh node is already running".to_string());
    }

    let started = mesh_llm::DesktopMeshRuntime::start(request)
        .await
        .map_err(|error| error.to_string())?;
    let status = started
        .status()
        .await
        .map_err(|error| format!("mesh node started but status probe failed: {error}"))?;
    *runtime = Some(started);
    Ok(status)
}

#[tauri::command]
pub async fn mesh_ensure_client_node(
    state: State<'_, AppState>,
    request: mesh_llm::EnsureMeshClientRequest,
) -> CmdResult<mesh_llm::MeshNodeStatus> {
    ensure_client_node_for_model(&state, request.model_id, request.endpoint_addr).await
}

pub(crate) async fn ensure_client_node_for_model(
    state: &AppState,
    model_id: impl AsRef<str>,
    endpoint_addr: Option<String>,
) -> CmdResult<mesh_llm::MeshNodeStatus> {
    let requested_model = model_id.as_ref().trim();
    if requested_model.is_empty() {
        return Err("modelId is required".to_string());
    }

    {
        let runtime = state.mesh_llm_runtime.lock().await;
        if let Some(runtime) = runtime.as_ref() {
            let status = runtime.status().await.map_err(|error| error.to_string())?;
            return match status.mode {
                Some(mesh_llm::MeshNodeMode::Client) => Ok(status),
                Some(mesh_llm::MeshNodeMode::Serve) => Err(
                    "this desktop is currently sharing compute; stop sharing before using relay mesh as a client"
                        .to_string(),
                ),
                None => Ok(status),
            };
        }
    }

    let join_token = match endpoint_addr
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        Some(value) => value,
        None => {
            let availability =
                match relay::query_relay(state, &[mesh_llm::mesh_status_filter()]).await {
                    Ok(events) => mesh_llm::availability_from_events(events),
                    Err(error) => return Err(format!("failed to read relay mesh status: {error}")),
                };
            if !availability.available {
                return Err(availability
                    .reason
                    .unwrap_or_else(|| "relay mesh is not available".to_string()));
            }
            let target = availability
                .serve_targets
                .iter()
                .find(|target| target.model_id == requested_model)
                .cloned()
                .ok_or_else(|| {
                    format!("relay mesh has no serve target for model {requested_model}")
                })?;
            target.endpoint_addr
        }
    };

    let start = mesh_llm::StartMeshNodeRequest {
        mode: mesh_llm::MeshNodeMode::Client,
        model_id: None,
        max_vram_gb: None,
        join_token: Some(join_token),
    };
    let mut runtime = state.mesh_llm_runtime.lock().await;
    if runtime.is_some() {
        return Err("mesh node changed while starting relay mesh client".to_string());
    }
    let started = mesh_llm::DesktopMeshRuntime::start(start)
        .await
        .map_err(|error| format!("mesh client failed to start: {error}"))?;
    let status = started
        .status()
        .await
        .map_err(|error| format!("mesh client started but status probe failed: {error}"))?;
    *runtime = Some(started);
    Ok(status)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshDialEndpointRequest {
    pub endpoint_addr: String,
}

#[tauri::command]
pub async fn mesh_dial_endpoint_addr(
    state: State<'_, AppState>,
    request: MeshDialEndpointRequest,
) -> CmdResult<mesh_llm::MeshNodeStatus> {
    let endpoint_addr = request.endpoint_addr.trim();
    if endpoint_addr.is_empty() {
        return Err("endpointAddr is required".to_string());
    }
    let runtime = state.mesh_llm_runtime.lock().await;
    let Some(runtime) = runtime.as_ref() else {
        return Err("mesh node is not running".to_string());
    };
    runtime
        .dial_endpoint_addr(endpoint_addr)
        .await
        .map_err(|error| format!("mesh dial failed: {error}"))?;
    runtime.status().await.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn mesh_status_report_payload(
    state: State<'_, AppState>,
) -> CmdResult<Option<serde_json::Value>> {
    let runtime = state.mesh_llm_runtime.lock().await;
    match runtime.as_ref() {
        Some(runtime) => runtime
            .status_report_payload()
            .await
            .map(Some)
            .map_err(|error| error.to_string()),
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn mesh_stop_node(state: State<'_, AppState>) -> CmdResult<mesh_llm::MeshNodeStatus> {
    let runtime = state.mesh_llm_runtime.lock().await.take();
    if let Some(runtime) = runtime {
        runtime.stop().await.map_err(|error| error.to_string())?;
    }
    Ok(mesh_llm::stopped_status())
}

#[tauri::command]
pub async fn mesh_node_status(state: State<'_, AppState>) -> CmdResult<mesh_llm::MeshNodeStatus> {
    let runtime = state.mesh_llm_runtime.lock().await;
    match runtime.as_ref() {
        Some(runtime) => runtime.status().await.map_err(|error| error.to_string()),
        None => Ok(mesh_llm::stopped_status()),
    }
}

#[tauri::command]
pub async fn mesh_installed_models(
    state: State<'_, AppState>,
) -> CmdResult<Vec<mesh_llm::MeshModelOption>> {
    let runtime = state.mesh_llm_runtime.lock().await;
    if let Some(runtime) = runtime.as_ref() {
        return runtime
            .installed_models()
            .await
            .map_err(|error| error.to_string());
    }
    Ok(Vec::new())
}

#[tauri::command]
pub fn mesh_agent_preset(
    request: mesh_llm::MeshAgentPresetRequest,
) -> CmdResult<mesh_llm::MeshAgentPreset> {
    mesh_llm::agent_preset(request)
}
