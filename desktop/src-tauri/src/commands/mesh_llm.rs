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
