//! Tauri commands for the mesh-LLM frontend surface.
//!
//! All commands deal with *the local user's own* mesh-LLM state:
//! - the persisted iroh endpoint id,
//! - the persisted compute-sharing preferences (the avatar-menu sliders),
//! - explicit toggle/save calls invoked when the user changes the prefs,
//! - publishing / deleting the user's kind:31990 compute-offer event.
//!
//! Discovering *other* members' offers happens through the relay
//! WebSocket pipeline already exposed by `relayClientSession.ts`.

use nostr::{EventBuilder, Kind, Tag};
use serde::Serialize;
use sprout_core::kind::KIND_MESH_LLM_DISCOVERY;
use tauri::{AppHandle, State};

use crate::app_state::AppState;
use crate::mesh_llm;
use crate::relay::submit_event;

/// Result type for mesh-LLM commands: errors are surfaced as user-facing
/// strings by the frontend.
type CmdResult<T> = Result<T, String>;

/// Stable identifier of the local iroh endpoint, in iroh's canonical
/// Display form. Returned to the frontend so the user can see *which*
/// machine identity they're publishing under (useful when one user has
/// multiple devices each running Sprout).
#[derive(Debug, Clone, Serialize)]
pub struct MeshEndpointInfo {
    /// Iroh endpoint id (= public key) as displayed by `iroh-base`.
    pub endpoint_id: String,
}

/// Returns the local mesh-LLM iroh endpoint id, creating + persisting the
/// keypair on first call.
#[tauri::command]
pub fn mesh_get_endpoint_id(app: AppHandle) -> CmdResult<MeshEndpointInfo> {
    let key = mesh_llm::load_or_create_endpoint_key(&app).map_err(|e| e.to_string())?;
    Ok(MeshEndpointInfo {
        endpoint_id: key.public().to_string(),
    })
}

/// Returns the persisted compute-sharing preferences for the avatar menu.
#[tauri::command]
pub fn mesh_get_sharing_prefs(app: AppHandle) -> CmdResult<mesh_llm::ComputeSharingPrefs> {
    mesh_llm::offer::load_prefs(&app).map_err(|e| e.to_string())
}

/// Replaces the persisted compute-sharing preferences. The caller is
/// responsible for republishing or deleting the kind:31990 offer to reflect
/// the change — this command only touches local state.
#[tauri::command]
pub fn mesh_set_sharing_prefs(
    app: AppHandle,
    prefs: mesh_llm::ComputeSharingPrefs,
) -> CmdResult<()> {
    mesh_llm::offer::save_prefs(&app, &prefs).map_err(|e| e.to_string())
}

/// Probe the connected relay's NIP-11 for an `iroh_relay_url`.
///
/// Returns:
/// - `Ok(Some(url))` if the relay advertises one,
/// - `Ok(None)` if it doesn't, or if the relay is unreachable / malformed.
/// - `Err(_)` only for caller-side errors (e.g. bad WS URL shape).
#[tauri::command]
pub async fn mesh_relay_iroh_url(
    _state: State<'_, AppState>,
    relay_ws_url: String,
) -> CmdResult<Option<String>> {
    mesh_llm::fetch_iroh_relay_url(&relay_ws_url)
        .await
        .map_err(|e| e.to_string())
}

/// Start a local mesh-llm node inside the Sprout process.
///
/// This exposes the same localhost OpenAI-compatible API shape as
/// `mesh-llm serve`, but it calls the mesh-llm Rust SDK directly instead of
/// launching a sidecar binary.
#[tauri::command]
pub async fn mesh_start_node(
    app: AppHandle,
    state: State<'_, AppState>,
    request: mesh_llm::runtime::StartMeshNodeRequest,
) -> CmdResult<mesh_llm::runtime::MeshNodeStatus> {
    let prefs = mesh_llm::offer::load_prefs(&app).map_err(|e| e.to_string())?;
    let mut runtime = state.mesh_llm_runtime.lock().await;
    if runtime.is_some() {
        return Err("mesh-llm node is already running".to_string());
    }

    let started = mesh_llm::runtime::SproutMeshRuntime::start(request, &prefs)
        .await
        .map_err(|e| e.to_string())?;
    let status = match started.status().await {
        Ok(status) => status,
        Err(error) => {
            let _ = started.stop().await;
            return Err(error.to_string());
        }
    };
    *runtime = Some(started);
    Ok(status)
}

/// Stop the in-process mesh-llm node if Sprout started one.
#[tauri::command]
pub async fn mesh_stop_node(state: State<'_, AppState>) -> CmdResult<()> {
    let runtime = state.mesh_llm_runtime.lock().await.take();
    if let Some(runtime) = runtime {
        runtime.stop().await.map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Return the embedded node status, including the local OpenAI API URL.
#[tauri::command]
pub async fn mesh_node_status(
    state: State<'_, AppState>,
) -> CmdResult<mesh_llm::runtime::MeshNodeStatus> {
    let runtime = state.mesh_llm_runtime.lock().await;
    match runtime.as_ref() {
        Some(runtime) => runtime.status().await.map_err(|e| e.to_string()),
        None => Ok(mesh_llm::runtime::stopped_status()),
    }
}

// ── Publisher ──────────────────────────────────────────────────────────────

/// Result of `mesh_publish_offer` — surface enough state so the frontend
/// can show the user *which* offer just went on the wire.
#[derive(Debug, Clone, Serialize)]
pub struct PublishOfferResult {
    /// `event_id` returned by the relay on accept.
    pub event_id: String,
    /// `true` if compute-sharing is currently enabled. When false, the
    /// command publishes an *empty-content* kind:31990 event at the same
    /// `(pubkey, d_tag)` address, which under NIP-33 is the canonical way
    /// to indicate "this offer is no longer active". Consumers that observe
    /// the empty content drop the offer from their cache.
    pub published_offer: bool,
}

/// Publish (or revoke) the user's kind:31990 compute-offer event.
///
/// Reads the current prefs from disk and the local iroh endpoint id. If
/// `enabled = true`, builds a kind:31990 with the offer envelope content
/// and the matching `d` tag; signs and POSTs via the existing
/// [`submit_event`] pipeline (NIP-98-authenticated to the configured relay).
/// If `enabled = false`, publishes the *same address* with empty content
/// to tell consumers the offer has been retired.
///
/// `iroh_relay_url` should be the relay's NIP-11 `iroh_relay_url` (fetched
/// via [`mesh_relay_iroh_url`] at session start). The offer envelope
/// carries it so consumers know where to dial.
#[tauri::command]
pub async fn mesh_publish_offer(
    app: AppHandle,
    state: State<'_, AppState>,
    iroh_relay_url: String,
) -> CmdResult<PublishOfferResult> {
    // Load prefs + endpoint key. These are sync; complete before any await.
    let prefs = mesh_llm::offer::load_prefs(&app).map_err(|e| e.to_string())?;
    let endpoint_key = mesh_llm::load_or_create_endpoint_key(&app).map_err(|e| e.to_string())?;
    let endpoint_id_str = endpoint_key.public().to_string();

    let d_tag = prefs.d_tag.clone();
    let d_tag_tag = Tag::parse(["d", &d_tag]).map_err(|e| format!("d tag: {e}"))?;

    let (content, published_offer) = if prefs.enabled {
        // expires_at = now + OFFER_TTL_SECS. The frontend hook re-invokes
        // mesh_publish_offer on a heartbeat well before the deadline; if
        // the publisher crashes before the next heartbeat, consumers reap
        // the offer once `now > expires_at` (see MeshLlmOffer::is_expired).
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| format!("system clock: {e}"))?
            .as_secs();
        let expires_at = now + mesh_llm::offer::OFFER_TTL_SECS;
        let offer = prefs
            .build_offer(&endpoint_id_str, &iroh_relay_url, expires_at)
            .ok_or_else(|| {
                "build_offer returned None despite enabled=true (logic bug)".to_string()
            })?;
        if !offer.is_publishable() {
            return Err("offer envelope failed publishable check".to_string());
        }
        let json = serde_json::to_string(&offer).map_err(|e| format!("serialise: {e}"))?;
        (json, true)
    } else {
        // NIP-33 "delete by replace": same (pubkey, kind, d) address, empty
        // content. Consumers must treat an empty content as 'offer
        // withdrawn'.
        (String::new(), false)
    };

    let builder = EventBuilder::new(Kind::Custom(KIND_MESH_LLM_DISCOVERY as u16), content)
        .tags(vec![d_tag_tag]);

    let res = submit_event(builder, &state).await?;
    Ok(PublishOfferResult {
        event_id: res.event_id,
        published_offer,
    })
}
