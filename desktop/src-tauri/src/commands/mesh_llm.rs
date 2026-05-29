use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use nostr::{EventBuilder, JsonUtil, Kind, Tag};
use reqwest::Method;
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
    mut request: mesh_llm::StartMeshNodeRequest,
) -> CmdResult<mesh_llm::MeshNodeStatus> {
    let mut runtime = state.mesh_llm_runtime.lock().await;
    if runtime.is_some() {
        return Err("mesh node is already running".to_string());
    }

    hydrate_private_relay_config(&state, &mut request).await?;

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

async fn hydrate_private_relay_config(
    state: &AppState,
    request: &mut mesh_llm::StartMeshNodeRequest,
) -> Result<(), String> {
    if request.iroh_relay_url.is_none() {
        request.iroh_relay_url = Some(fetch_iroh_relay_url(state).await?);
    }
    if request.iroh_relay_auth.is_none() {
        let relay = request
            .iroh_relay_url
            .as_deref()
            .ok_or_else(|| "relay did not advertise iroh_relay_url".to_string())?;
        request.iroh_relay_auth = Some(build_iroh_relay_bearer(state, relay)?);
    }
    Ok(())
}

async fn fetch_iroh_relay_url(state: &AppState) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct Nip11Info {
        iroh_relay_url: Option<String>,
    }

    let url = relay::relay_api_base_url_with_override(state);
    let response = state
        .http_client
        .get(&url)
        .header("Accept", "application/nostr+json")
        .send()
        .await
        .map_err(|error| format!("failed to fetch relay NIP-11: {error}"))?;
    if !response.status().is_success() {
        return Err(relay::relay_error_message(response).await);
    }
    let info = response
        .json::<Nip11Info>()
        .await
        .map_err(|error| format!("failed to parse relay NIP-11: {error}"))?;
    info.iroh_relay_url
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "relay NIP-11 does not advertise iroh_relay_url".to_string())
}

fn build_iroh_relay_bearer(state: &AppState, relay_url: &str) -> Result<String, String> {
    let canonical = canonical_iroh_relay_auth_url(relay_url)?;
    let tags = vec![
        Tag::parse(vec!["u", canonical.as_str()])
            .map_err(|error| format!("url tag failed: {error}"))?,
        Tag::parse(vec!["method", Method::GET.as_str()])
            .map_err(|error| format!("method tag failed: {error}"))?,
    ];
    let keys = state.keys.lock().map_err(|error| error.to_string())?;
    let event = EventBuilder::new(Kind::HttpAuth, "")
        .tags(tags)
        .sign_with_keys(&keys)
        .map_err(|error| format!("sign failed: {error}"))?;
    Ok(BASE64.encode(event.as_json().as_bytes()))
}

fn canonical_iroh_relay_auth_url(relay_url: &str) -> Result<String, String> {
    let mut parsed = url::Url::parse(relay_url)
        .map_err(|error| format!("invalid iroh relay URL {relay_url:?}: {error}"))?;
    parsed.set_query(None);
    parsed.set_fragment(None);
    let mut path = parsed.path().trim_end_matches('/').to_string();
    if !path.ends_with("/relay") {
        if path.is_empty() {
            path = "/relay".to_string();
        } else {
            path.push_str("/relay");
        }
    }
    parsed.set_path(&path);
    Ok(parsed.to_string().trim_end_matches('/').to_string())
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
