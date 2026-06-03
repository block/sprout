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
            // A running runtime — in any mode — is the mesh's local OpenAI
            // ingress on `9337`. mesh-llm's router already resolves the
            // requested model to a local, remote, or split target at request
            // time (see `route_missing_local_model` -> `hosts_for_model`), so
            // "serving" and "using the mesh as a client" are not mutually
            // exclusive: a serve node can host model A and route model B to a
            // peer through the same ingress. Hand the agent the existing
            // runtime; the router decides routability per request rather than
            // this preflight second-guessing it (a `/v1/models` check here
            // would race model gossip and wrongly reject freshly-discovered
            // remote/split models).
            //
            // If the caller selected a specific target, still dial it: that is
            // how the runtime joins the chosen peer's mesh. Skipping it would
            // let a serve runtime not yet connected to that target fail its
            // first inference while the frontend has already signalled the
            // peer to expect us.
            if let Some(endpoint_addr) = endpoint_addr
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                runtime
                    .dial_endpoint_addr(endpoint_addr)
                    .await
                    .map_err(|error| format!("mesh dial failed: {error}"))?;
            }
            return runtime.status().await.map_err(|error| error.to_string());
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

#[cfg(all(test, feature = "mesh-llm"))]
mod tests {
    use super::*;
    use crate::app_state::build_app_state;

    /// Acceptance-critical regression for dropping the serve-vs-client guard.
    ///
    /// Before this change, `ensure_client_node_for_model` hard-errored whenever
    /// the running runtime was in `Serve` mode ("stop sharing before using
    /// relay mesh as a client"). That forbade the exact thing a user should be
    /// able to do: host model A while pointing an agent at a different model B
    /// through the same `9337` ingress.
    ///
    /// This test starts a real serve runtime and asserts that a follow-up
    /// preflight for a *different* model:
    ///   1. does NOT reject on mode, and
    ///   2. returns the existing runtime's status (same `9337` ingress), so the
    ///      agent keeps talking to the running node and mesh-llm's router
    ///      resolves the model per request.
    ///
    /// Hardware-gated (`#[ignore]`): loads a real model. Run with:
    ///   cargo test -p sprout-desktop --features mesh-llm \
    ///     ensure_serve_runtime_serves_other_model -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "loads a real model; run manually with --ignored"]
    async fn ensure_serve_runtime_serves_other_model() {
        const HOSTED_MODEL: &str = "jc-builds/SmolLM2-135M-Instruct-Q4_K_M-GGUF:Q4_K_M";
        const OTHER_MODEL: &str = "some/other-model-not-hosted-locally:Q4_K_M";

        let state = build_app_state();

        // Start a serve runtime hosting HOSTED_MODEL — this is the "Share
        // compute" path.
        let serve = mesh_llm::DesktopMeshRuntime::start(mesh_llm::StartMeshNodeRequest {
            mode: mesh_llm::MeshNodeMode::Serve,
            model_id: Some(HOSTED_MODEL.to_string()),
            max_vram_gb: None,
            join_token: None,
        })
        .await
        .expect("serve runtime should start");

        let serve_status = serve.status().await.expect("serve status");
        let serve_base = serve_status.api_base_url.clone();
        assert_eq!(serve_status.mode, Some(mesh_llm::MeshNodeMode::Serve));

        {
            let mut runtime = state.mesh_llm_runtime.lock().await;
            *runtime = Some(serve);
        }

        // Preflight for a DIFFERENT model with no explicit target. Old code:
        // Err(...sharing compute...). New code: reuse the running ingress.
        let status = ensure_client_node_for_model(&state, OTHER_MODEL, None)
            .await
            .expect("serve runtime must not reject a different-model preflight");

        // It returns the SAME running node — agents keep using A's 9337, and
        // the router decides routability for OTHER_MODEL per request.
        assert_eq!(
            status.mode,
            Some(mesh_llm::MeshNodeMode::Serve),
            "preflight should reuse the existing serve runtime, not spin up a client"
        );
        assert_eq!(
            status.api_base_url, serve_base,
            "agent must be pointed at the existing serve node's ingress"
        );

        // Clean up the runtime.
        let taken = state.mesh_llm_runtime.lock().await.take();
        if let Some(runtime) = taken {
            let _ = runtime.stop().await;
        }
    }
}
