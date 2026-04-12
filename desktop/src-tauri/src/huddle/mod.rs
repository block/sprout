//! Huddle (voice/video) state machine and Tauri commands.
//!
//! Mental model:
//!   parent channel → start_huddle → ephemeral channel + LiveKit token
//!   other clients  → join_huddle  → LiveKit token
//!   any client     → leave_huddle → lifecycle event, clear local state
//!   creator        → end_huddle   → archive ephemeral channel, clear state
//!
//! HuddleState is stored in AppState and serialized for get_huddle_state.

pub mod agents;
pub mod models;
pub mod preprocessing;
pub mod stt;
pub mod tts;

use reqwest::Method;
use serde::{Deserialize, Serialize};
use std::sync::{atomic::AtomicBool, Arc, Mutex};
use tauri::State;
use uuid::Uuid;

use nostr::JsonUtil;

use crate::{
    app_state::AppState,
    events,
    relay::{api_path, build_authed_request, send_json_request, submit_event},
};

// ── State types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum HuddlePhase {
    Idle,
    Creating,
    Connecting,
    Active,
    Leaving,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HuddleState {
    pub phase: HuddlePhase,
    pub parent_channel_id: Option<String>,
    pub ephemeral_channel_id: Option<String>,
    pub livekit_token: Option<String>,
    pub livekit_url: Option<String>,
    pub livekit_room: Option<String>,
    /// Participant pubkey hex strings (all members, including humans).
    pub participants: Vec<String>,
    /// Agent pubkeys only — used as p-tags on transcribed messages.
    ///
    /// Stored as `Arc<Mutex<Vec<String>>>` so the transcription task can clone
    /// the `Arc` and read the current list at post time without holding the
    /// outer `HuddleState` lock across an await point.
    ///
    /// Populated from `member_pubkeys` in `start_huddle` (the UI sends agent
    /// pubkeys specifically). Joiners don't add agents — they were already
    /// added by the creator. Serialized as a plain `Vec<String>` for the
    /// frontend via the custom `Serialize`/`Deserialize` impls below.
    #[serde(
        serialize_with = "serialize_agent_pubkeys",
        deserialize_with = "deserialize_agent_pubkeys"
    )]
    pub agent_pubkeys: Arc<Mutex<Vec<String>>>,
    /// Active STT pipeline — not serialized, not cloned.
    #[serde(skip)]
    pub stt_pipeline: Option<Arc<stt::SttPipeline>>,
    /// Active TTS pipeline — not serialized, not cloned.
    #[serde(skip)]
    pub tts_pipeline: Option<Arc<tts::TtsPipeline>>,
    /// Whether TTS output is enabled (user-toggled).
    pub tts_enabled: bool,
    /// Shared flag: true while TTS is playing audio.
    /// Shared with the STT pipeline for barge-in / echo gating.
    #[serde(skip)]
    pub tts_active: Arc<AtomicBool>,
}

fn serialize_agent_pubkeys<S>(v: &Arc<Mutex<Vec<String>>>, s: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeSeq;
    let guard = v.lock().unwrap_or_else(|e| e.into_inner());
    let mut seq = s.serialize_seq(Some(guard.len()))?;
    for item in guard.iter() {
        seq.serialize_element(item)?;
    }
    seq.end()
}

fn deserialize_agent_pubkeys<'de, D>(d: D) -> Result<Arc<Mutex<Vec<String>>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v: Vec<String> = serde::Deserialize::deserialize(d)?;
    Ok(Arc::new(Mutex::new(v)))
}

impl Clone for HuddleState {
    fn clone(&self) -> Self {
        let agent_pubkeys_snapshot = self
            .agent_pubkeys
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        Self {
            phase: self.phase.clone(),
            parent_channel_id: self.parent_channel_id.clone(),
            ephemeral_channel_id: self.ephemeral_channel_id.clone(),
            livekit_token: self.livekit_token.clone(),
            livekit_url: self.livekit_url.clone(),
            livekit_room: self.livekit_room.clone(),
            participants: self.participants.clone(),
            agent_pubkeys: Arc::new(Mutex::new(agent_pubkeys_snapshot)),
            stt_pipeline: None, // Never clone the pipeline handle.
            tts_pipeline: None, // Never clone the pipeline handle.
            tts_enabled: self.tts_enabled,
            tts_active: Arc::clone(&self.tts_active),
        }
    }
}

impl Default for HuddleState {
    fn default() -> Self {
        Self {
            phase: HuddlePhase::Idle,
            parent_channel_id: None,
            ephemeral_channel_id: None,
            livekit_token: None,
            livekit_url: None,
            livekit_room: None,
            participants: Vec::new(),
            agent_pubkeys: Arc::new(Mutex::new(Vec::new())),
            stt_pipeline: None,
            tts_pipeline: None,
            tts_enabled: true,
            tts_active: Arc::new(AtomicBool::new(false)),
        }
    }
}

// ── Response types ────────────────────────────────────────────────────────────

/// Returned by start_huddle and join_huddle.
#[derive(Debug, Serialize, Deserialize)]
pub struct HuddleJoinInfo {
    pub ephemeral_channel_id: String,
    pub livekit_token: String,
    pub livekit_url: String,
    pub livekit_room: String,
}

/// Raw response from `POST /api/huddles/{channel_id}/token`.
#[derive(Debug, Deserialize)]
struct LiveKitTokenResponse {
    pub token: String,
    pub url: String,
    pub room: String,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_channel_uuid(channel_id: &str) -> Result<Uuid, String> {
    Uuid::parse_str(channel_id).map_err(|_| format!("invalid channel UUID: {channel_id}"))
}

/// Fetch a LiveKit token from the relay for the given channel.
async fn fetch_livekit_token(
    channel_id: &str,
    state: &AppState,
) -> Result<LiveKitTokenResponse, String> {
    let path = api_path(&["huddles", channel_id, "token"]);
    let request = build_authed_request(&state.http_client, Method::POST, &path, state)?;
    send_json_request(request).await
}

/// Attempt to start the STT pipeline if models are present.
/// Silently skips if models are missing — huddle continues as voice-only.
///
/// Creates the shared `tts_active` flag and passes it to the STT pipeline
/// for barge-in / echo gating. The same flag is later passed to the TTS
/// pipeline so it can signal when audio is playing.
async fn maybe_start_stt_pipeline(state: &AppState, ephemeral_channel_id: &str) {
    if !models::is_moonshine_ready() {
        return; // Models not downloaded yet — voice-only mode.
    }
    let model_dir = match models::moonshine_model_dir() {
        Some(d) => d,
        None => return,
    };

    // Grab the shared tts_active flag from HuddleState.
    let tts_active = {
        let hs = match state.huddle_state.lock() {
            Ok(h) => h,
            Err(_) => return,
        };
        Arc::clone(&hs.tts_active)
    };

    // Grab the TTS cancel flag so STT can trigger barge-in.
    let tts_cancel = {
        let hs = match state.huddle_state.lock() {
            Ok(h) => h,
            Err(_) => return,
        };
        hs.tts_pipeline.as_ref().map(|p| Arc::clone(&p.cancel))
    };

    let pipeline = match stt::SttPipeline::new(model_dir, tts_active, tts_cancel) {
        Ok(p) => Arc::new(p),
        Err(e) => {
            eprintln!("sprout-desktop: STT pipeline failed to start: {e}");
            return;
        }
    };

    let channel_uuid = match parse_channel_uuid(ephemeral_channel_id) {
        Ok(u) => u,
        Err(_) => return,
    };

    // Clone the Arc<Mutex<Vec<String>>> BEFORE storing the pipeline, so we
    // can pass it to the transcription task without holding the state lock.
    let agent_pubkeys_arc = {
        let hs = match state.huddle_state.lock() {
            Ok(h) => h,
            Err(_) => return,
        };
        Arc::clone(&hs.agent_pubkeys)
    };

    // Store the pipeline.
    {
        let mut hs = match state.huddle_state.lock() {
            Ok(h) => h,
            Err(_) => return,
        };
        hs.stt_pipeline = Some(Arc::clone(&pipeline));
    }

    spawn_transcription_task(pipeline, channel_uuid, agent_pubkeys_arc, state);
}

/// Attempt to start the TTS pipeline if Kokoro models are present and TTS is enabled.
/// Silently skips if models are missing or TTS is disabled.
async fn maybe_start_tts_pipeline(state: &AppState) {
    if !models::is_kokoro_ready() {
        return; // Kokoro not downloaded yet — TTS unavailable.
    }
    let model_dir = match models::kokoro_model_dir() {
        Some(d) => d,
        None => return,
    };

    let (tts_active, tts_enabled) = {
        let hs = match state.huddle_state.lock() {
            Ok(h) => h,
            Err(_) => return,
        };
        (Arc::clone(&hs.tts_active), hs.tts_enabled)
    };

    if !tts_enabled {
        return;
    }

    let pipeline = match tts::TtsPipeline::new(model_dir, tts_active) {
        Ok(p) => Arc::new(p),
        Err(e) => {
            eprintln!("sprout-desktop: TTS pipeline failed to start: {e}");
            return;
        }
    };

    {
        let mut hs = match state.huddle_state.lock() {
            Ok(h) => h,
            Err(_) => return,
        };
        hs.tts_pipeline = Some(pipeline);
    }
}

/// Spawn a tokio task that reads text_rx and posts kind:9 events.
///
/// Fix 1: `agent_pubkeys_arc` is an `Arc<Mutex<Vec<String>>>` cloned from
///        `HuddleState` — the task reads it at post time so p-tags are always
///        current, not a stale snapshot.
/// Fix 3: no `.unwrap()` on mutex — poisoned locks are recovered gracefully.
/// Fix 4: `recv_timeout` instead of `try_recv` + sleep — no busy-polling.
fn spawn_transcription_task(
    pipeline: Arc<stt::SttPipeline>,
    channel_uuid: Uuid,
    agent_pubkeys_arc: Arc<Mutex<Vec<String>>>,
    state: &AppState,
) {
    let http_client = state.http_client.clone();
    let keys = match state.keys.lock() {
        Ok(k) => k.clone(),
        Err(_) => return,
    };
    let configured_api_token = state.configured_api_token.clone();

    tauri::async_runtime::spawn(async move {
        loop {
            // Fix 3: recover from a poisoned mutex rather than panicking.
            // Fix 4: recv_timeout blocks the thread efficiently; Disconnected
            //        means the pipeline worker has exited — stop the task.
            let text = {
                let rx = pipeline.text_rx.lock().unwrap_or_else(|e| e.into_inner());
                match rx.recv_timeout(std::time::Duration::from_millis(100)) {
                    Ok(t) => Some(t),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None,
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            };

            let t = match text {
                Some(t) if !t.is_empty() => t,
                _ => continue,
            };

            // Fix 1: read current agent pubkeys at post time.
            let agent_pubkeys: Vec<String> = agent_pubkeys_arc
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();

            let p_tags: Vec<&str> = agent_pubkeys.iter().map(|s| s.as_str()).collect();
            let builder = match events::build_message(channel_uuid, &t, None, &p_tags, &[]) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("sprout-desktop: STT build_message: {e}");
                    continue;
                }
            };
            let event = match builder.sign_with_keys(&keys) {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("sprout-desktop: STT sign event: {e}");
                    continue;
                }
            };
            let event_json = event.as_json();
            let auth_header = match configured_api_token.as_deref() {
                Some(token) => format!("Bearer {token}"),
                None => format!("X-Pubkey {}", keys.public_key().to_hex()),
            };
            let url = format!("{}/api/events", crate::relay::relay_api_base_url());
            let req = if auth_header.starts_with("Bearer ") {
                http_client.post(&url).header("Authorization", &auth_header)
            } else {
                let pk = auth_header.strip_prefix("X-Pubkey ").unwrap_or("");
                http_client.post(&url).header("X-Pubkey", pk)
            }
            .header("Content-Type", "application/json")
            .body(event_json);

            if let Err(e) = req.send().await {
                eprintln!("sprout-desktop: STT kind:9 post failed: {e}");
            }
        }
    });
}

// ── Tauri commands ────────────────────────────────────────────────────────────

/// Start a new huddle in the given parent channel.
///
/// Steps:
/// 1. Create an ephemeral channel (kind 9007, ttl=3600).
/// 2. Add each invited member to the ephemeral channel (kind 9000).
/// 3. Fetch a LiveKit token from the relay.
/// 4. Emit KIND_HUDDLE_STARTED to the parent channel (kind 48100) — only after
///    token is confirmed, so no phantom announcement on failure.
/// 5. Store state and return join info.
///
/// If ANY step fails (including channel creation), the orphaned ephemeral
/// channel is archived (best-effort) and state is reset to Idle.
#[tauri::command]
pub async fn start_huddle(
    parent_channel_id: String,
    member_pubkeys: Vec<String>,
    state: State<'_, AppState>,
) -> Result<HuddleJoinInfo, String> {
    // Transition to Creating.
    {
        let mut hs = state.huddle_state.lock().map_err(|e| e.to_string())?;
        if hs.phase != HuddlePhase::Idle {
            return Err(format!(
                "cannot start huddle: already in phase {:?}",
                hs.phase
            ));
        }
        hs.phase = HuddlePhase::Creating;
        hs.parent_channel_id = Some(parent_channel_id.clone());
    }

    let ephemeral_uuid = Uuid::new_v4();
    let ephemeral_channel_id = ephemeral_uuid.to_string();
    let short_id = &ephemeral_channel_id[..8];
    let channel_name = format!("huddle-{short_id}");

    // All steps wrapped so we can roll back on ANY failure, including step 1.
    // channel_was_created tracks whether we need to archive on rollback.
    let mut channel_was_created = false;

    let result: Result<(LiveKitTokenResponse, Vec<String>), String> = async {
        // 1. Create ephemeral channel.
        let create_builder = events::build_create_channel(
            ephemeral_uuid,
            &channel_name,
            "private",
            "stream",
            None,
            Some(3600),
        )?;
        submit_event(create_builder, &state).await?;
        channel_was_created = true;

        // 2. Add members to the ephemeral channel; only keep successfully enrolled ones.
        let mut successful_agents: Vec<String> = Vec::new();
        for pubkey in &member_pubkeys {
            let add_builder = events::build_add_member(ephemeral_uuid, pubkey, None)?;
            match submit_event(add_builder, &state).await {
                Ok(_) => successful_agents.push(pubkey.clone()),
                Err(e) => {
                    eprintln!("sprout-desktop: huddle add_member failed for {pubkey}: {e}");
                    // Intentionally not added — policy rejected this agent.
                }
            }
        }

        // 3. Fetch LiveKit token BEFORE emitting HUDDLE_STARTED.
        //    This prevents a phantom announcement if the token fetch fails.
        let lk = fetch_livekit_token(&ephemeral_channel_id, &state).await?;

        // 4. Emit HUDDLE_STARTED to parent channel — only now that token is confirmed.
        let started_builder =
            events::build_huddle_started(&parent_channel_id, &ephemeral_channel_id, &lk.room)?;
        submit_event(started_builder, &state).await?;

        // 5. Post voice-mode guidelines as a regular kind:9 message.
        //    We do NOT use kind:40099 — that is relay-signed; the client must not mint it.
        //    Best-effort: don't fail the huddle if this fails.
        if let Ok(msg_builder) = events::build_message(
            ephemeral_uuid,
            &format!("[System] {}", agents::VOICE_MODE_GUIDELINES),
            None,
            &[],
            &[],
        ) {
            if let Err(e) = submit_event(msg_builder, &state).await {
                eprintln!("sprout-desktop: voice-mode guidelines message failed: {e}");
            }
        }

        Ok((lk, successful_agents))
    }
    .await;

    match result {
        Ok((lk, successful_agents)) => {
            // 5. Store active state.
            {
                let mut hs = state.huddle_state.lock().map_err(|e| e.to_string())?;
                hs.phase = HuddlePhase::Active;
                hs.ephemeral_channel_id = Some(ephemeral_channel_id.clone());
                hs.livekit_token = Some(lk.token.clone());
                hs.livekit_url = Some(lk.url.clone());
                hs.livekit_room = Some(lk.room.clone());
                // Only store agents that were successfully enrolled (Fix 1).
                *hs.agent_pubkeys.lock().unwrap_or_else(|e| e.into_inner()) =
                    successful_agents.clone();
                // Include the current user + any invited members as participants.
                let own_pubkey = state
                    .keys
                    .lock()
                    .map(|k| k.public_key().to_hex())
                    .unwrap_or_default();
                let mut participants = member_pubkeys;
                if !own_pubkey.is_empty() && !participants.contains(&own_pubkey) {
                    participants.insert(0, own_pubkey);
                }
                hs.participants = participants;
            }

            // 6. Auto-start STT and TTS pipelines if models are ready.
            maybe_start_stt_pipeline(&state, &ephemeral_channel_id).await;
            maybe_start_tts_pipeline(&state).await;

            Ok(HuddleJoinInfo {
                ephemeral_channel_id,
                livekit_token: lk.token,
                livekit_url: lk.url,
                livekit_room: lk.room,
            })
        }
        Err(e) => {
            // Rollback: archive the orphaned ephemeral channel if it was created.
            if channel_was_created {
                if let Ok(archive_builder) = events::build_archive(ephemeral_uuid) {
                    if let Err(ae) = submit_event(archive_builder, &state).await {
                        eprintln!(
                            "sprout-desktop: rollback archive of {ephemeral_channel_id} failed: {ae}"
                        );
                    }
                }
            }
            // Reset state to Idle so the user can retry.
            if let Ok(mut hs) = state.huddle_state.lock() {
                *hs = HuddleState::default();
            }
            Err(e)
        }
    }
}

/// Join an existing huddle in the given parent channel.
///
/// Steps:
/// 1. Fetch a LiveKit token from the relay for the ephemeral channel.
/// 2. Emit KIND_HUDDLE_PARTICIPANT_JOINED to the parent channel (best-effort).
/// 3. Store state and return join info.
#[tauri::command]
pub async fn join_huddle(
    parent_channel_id: String,
    ephemeral_channel_id: String,
    livekit_room: String,
    state: State<'_, AppState>,
) -> Result<HuddleJoinInfo, String> {
    // Transition to Connecting.
    {
        let mut hs = state.huddle_state.lock().map_err(|e| e.to_string())?;
        if hs.phase != HuddlePhase::Idle {
            return Err(format!(
                "cannot join huddle: already in phase {:?}",
                hs.phase
            ));
        }
        hs.phase = HuddlePhase::Connecting;
        hs.parent_channel_id = Some(parent_channel_id.clone());
        hs.ephemeral_channel_id = Some(ephemeral_channel_id.clone());
        hs.livekit_room = Some(livekit_room.clone());
    }

    // 1. Fetch LiveKit token. On failure, reset state to Idle so user can retry.
    let lk = match fetch_livekit_token(&ephemeral_channel_id, &state).await {
        Ok(lk) => lk,
        Err(e) => {
            if let Ok(mut hs) = state.huddle_state.lock() {
                *hs = HuddleState::default();
            }
            return Err(e);
        }
    };

    // 2. Emit PARTICIPANT_JOINED to parent channel (best-effort — don't fail the join).
    if let Ok(joined_builder) =
        events::build_huddle_participant_joined(&parent_channel_id, &ephemeral_channel_id)
    {
        if let Err(e) = submit_event(joined_builder, &state).await {
            eprintln!("sprout-desktop: huddle_participant_joined event failed: {e}");
        }
    }

    // 3. Store active state.
    {
        let mut hs = state.huddle_state.lock().map_err(|e| e.to_string())?;
        hs.phase = HuddlePhase::Active;
        hs.livekit_token = Some(lk.token.clone());
        hs.livekit_url = Some(lk.url.clone());
        hs.livekit_room = Some(lk.room.clone());
        // Note: agent_pubkeys stays empty for joiners — agents were added by the creator.
    }

    // 4. Auto-start STT and TTS pipelines if models are ready.
    maybe_start_stt_pipeline(&state, &ephemeral_channel_id).await;
    maybe_start_tts_pipeline(&state).await;

    Ok(HuddleJoinInfo {
        ephemeral_channel_id,
        livekit_token: lk.token,
        livekit_url: lk.url,
        livekit_room: lk.room,
    })
}

/// Leave the current huddle.
///
/// Steps:
/// 1. Emit KIND_HUDDLE_PARTICIPANT_LEFT to the parent channel.
/// 2. Shut down the STT pipeline (Fix 5).
/// 3. Clear local huddle state.
#[tauri::command]
pub async fn leave_huddle(state: State<'_, AppState>) -> Result<(), String> {
    let (parent_channel_id, ephemeral_channel_id) = {
        let mut hs = state.huddle_state.lock().map_err(|e| e.to_string())?;
        if hs.phase == HuddlePhase::Idle {
            return Ok(()); // Nothing to leave.
        }
        hs.phase = HuddlePhase::Leaving;
        (
            hs.parent_channel_id.clone().unwrap_or_default(),
            hs.ephemeral_channel_id.clone().unwrap_or_default(),
        )
    };

    // Emit PARTICIPANT_LEFT (best-effort).
    if !parent_channel_id.is_empty() && !ephemeral_channel_id.is_empty() {
        if let Ok(left_builder) =
            events::build_huddle_participant_left(&parent_channel_id, &ephemeral_channel_id)
        {
            if let Err(e) = submit_event(left_builder, &state).await {
                eprintln!("sprout-desktop: huddle_participant_left event failed: {e}");
            }
        }
    }

    // Signal the STT and TTS pipelines to stop before dropping state.
    // The pipelines' Drop impls will join worker threads for a clean exit.
    {
        let hs = state.huddle_state.lock().map_err(|e| e.to_string())?;
        if let Some(ref pipeline) = hs.stt_pipeline {
            pipeline.shutdown();
        }
        if let Some(ref pipeline) = hs.tts_pipeline {
            pipeline.shutdown();
        }
    }

    // Clear state.
    {
        let mut hs = state.huddle_state.lock().map_err(|e| e.to_string())?;
        *hs = HuddleState::default();
    }

    Ok(())
}

/// End the current huddle (creator only).
///
/// Steps:
/// 1. Emit KIND_HUDDLE_ENDED to the parent channel.
/// 2. Archive the ephemeral channel.
/// 3. Shut down the STT pipeline (Fix 5).
/// 4. Clear local huddle state.
#[tauri::command]
pub async fn end_huddle(state: State<'_, AppState>) -> Result<(), String> {
    let (parent_channel_id, ephemeral_channel_id) = {
        let mut hs = state.huddle_state.lock().map_err(|e| e.to_string())?;
        if hs.phase == HuddlePhase::Idle {
            return Ok(()); // Nothing to end.
        }
        hs.phase = HuddlePhase::Leaving;
        (
            hs.parent_channel_id.clone().unwrap_or_default(),
            hs.ephemeral_channel_id.clone().unwrap_or_default(),
        )
    };

    // Emit HUDDLE_ENDED (best-effort).
    if !parent_channel_id.is_empty() && !ephemeral_channel_id.is_empty() {
        if let Ok(ended_builder) =
            events::build_huddle_ended(&parent_channel_id, &ephemeral_channel_id)
        {
            if let Err(e) = submit_event(ended_builder, &state).await {
                eprintln!("sprout-desktop: huddle_ended event failed: {e}");
            }
        }
    }

    // Archive the ephemeral channel (best-effort).
    if !ephemeral_channel_id.is_empty() {
        if let Ok(uuid) = parse_channel_uuid(&ephemeral_channel_id) {
            if let Ok(archive_builder) = events::build_archive(uuid) {
                if let Err(e) = submit_event(archive_builder, &state).await {
                    eprintln!("sprout-desktop: huddle archive ephemeral channel failed: {e}");
                }
            }
        }
    }

    // Signal the STT and TTS pipelines to stop before dropping state.
    {
        let hs = state.huddle_state.lock().map_err(|e| e.to_string())?;
        if let Some(ref pipeline) = hs.stt_pipeline {
            pipeline.shutdown();
        }
        if let Some(ref pipeline) = hs.tts_pipeline {
            pipeline.shutdown();
        }
    }

    // Clear state.
    {
        let mut hs = state.huddle_state.lock().map_err(|e| e.to_string())?;
        *hs = HuddleState::default();
    }

    Ok(())
}

/// Return the current HuddleState (serialized for the frontend).
#[tauri::command]
pub fn get_huddle_state(state: State<'_, AppState>) -> Result<HuddleState, String> {
    let hs = state.huddle_state.lock().map_err(|e| e.to_string())?;
    Ok(hs.clone())
}

/// Receive raw PCM audio bytes from the AudioWorklet and feed the STT pipeline.
///
/// Expects a raw binary body of f32 LE samples at 48 kHz mono.
/// If no STT pipeline is active, the bytes are silently discarded.
#[tauri::command]
pub fn push_audio_pcm(
    request: tauri::ipc::Request<'_>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    match request.body() {
        tauri::ipc::InvokeBody::Raw(bytes) => {
            if let Ok(hs) = state.huddle_state.lock() {
                if let Some(ref pipeline) = hs.stt_pipeline {
                    pipeline.push_audio(bytes.to_vec())?;
                }
            }
            Ok(())
        }
        _ => Err("expected raw binary body".to_string()),
    }
}

/// Start the STT pipeline for the active huddle.
///
/// Creates the pipeline, stores it in HuddleState, and spawns a tokio task
/// that reads transcribed text and posts kind:9 events to the ephemeral
/// channel.
///
/// No-op if models are not present — huddle continues as voice-only.
/// Safe to call multiple times: replaces the existing pipeline if already running.
#[tauri::command]
pub async fn start_stt_pipeline(state: State<'_, AppState>) -> Result<(), String> {
    if !models::is_moonshine_ready() {
        return Err("Moonshine model not ready".to_string());
    }
    let model_dir = models::moonshine_model_dir()
        .ok_or_else(|| "Moonshine model directory not found".to_string())?;

    let (ephemeral_channel_id, agent_pubkeys_arc) = {
        let hs = state.huddle_state.lock().map_err(|e| e.to_string())?;
        (
            hs.ephemeral_channel_id.clone(),
            Arc::clone(&hs.agent_pubkeys),
        )
    };

    let ephemeral_channel_id =
        ephemeral_channel_id.ok_or("no active huddle — start or join a huddle first")?;
    let channel_uuid = parse_channel_uuid(&ephemeral_channel_id)?;

    let (tts_active, tts_cancel) = {
        let hs = state.huddle_state.lock().map_err(|e| e.to_string())?;
        let cancel = hs.tts_pipeline.as_ref().map(|p| Arc::clone(&p.cancel));
        (Arc::clone(&hs.tts_active), cancel)
    };

    let pipeline = Arc::new(stt::SttPipeline::new(model_dir, tts_active, tts_cancel)?);

    {
        let mut hs = state.huddle_state.lock().map_err(|e| e.to_string())?;
        hs.stt_pipeline = Some(Arc::clone(&pipeline));
    }

    spawn_transcription_task(pipeline, channel_uuid, agent_pubkeys_arc, &state);
    Ok(())
}

/// Trigger a background download of voice models (Moonshine STT + Kokoro TTS).
///
/// Returns immediately — downloads run in tokio background tasks.
/// Poll `get_model_status` to track progress.
/// Safe to call multiple times: no-op if already downloading or ready.
#[tauri::command]
pub async fn download_voice_models(state: State<'_, AppState>) -> Result<(), String> {
    let manager = models::global_model_manager()
        .ok_or("model manager unavailable (home directory could not be resolved)")?;
    manager.start_moonshine_download(state.http_client.clone());
    manager.start_kokoro_download(state.http_client.clone());
    Ok(())
}

/// Return the current download status for all voice models.
#[tauri::command]
pub fn get_model_status(_state: State<'_, AppState>) -> Result<models::VoiceModelStatus, String> {
    let manager = models::global_model_manager()
        .ok_or("model manager unavailable (home directory could not be resolved)")?;
    Ok(models::VoiceModelStatus {
        moonshine: manager.moonshine_status(),
        kokoro: manager.kokoro_status(),
    })
}

/// Enable or disable TTS output.
///
/// When disabled, the TTS pipeline is shut down and audio output stops.
/// When re-enabled, the pipeline is restarted if Kokoro models are available.
#[tauri::command]
pub async fn set_tts_enabled(enabled: bool, state: State<'_, AppState>) -> Result<(), String> {
    {
        let mut hs = state.huddle_state.lock().map_err(|e| e.to_string())?;
        hs.tts_enabled = enabled;
        if !enabled {
            // Shut down the TTS pipeline immediately.
            if let Some(ref pipeline) = hs.tts_pipeline {
                pipeline.shutdown();
            }
            hs.tts_pipeline = None;
        }
    }

    if enabled {
        // Re-start TTS pipeline if models are available and huddle is active.
        let phase = {
            let hs = state.huddle_state.lock().map_err(|e| e.to_string())?;
            hs.phase.clone()
        };
        if phase == HuddlePhase::Active {
            maybe_start_tts_pipeline(&state).await;
        }
    }

    Ok(())
}

/// Speak an agent message via TTS.
///
/// Called by the WebView when it receives an incoming agent kind:9 message.
/// The WebView already subscribes to channel messages — it calls this command
/// for messages from non-human pubkeys so the agent's voice is heard.
///
/// No-op if TTS is disabled or no pipeline is active.
#[tauri::command]
pub fn speak_agent_message(text: String, state: State<'_, AppState>) -> Result<(), String> {
    let hs = state.huddle_state.lock().map_err(|e| e.to_string())?;
    if hs.tts_enabled {
        if let Some(ref pipeline) = hs.tts_pipeline {
            pipeline.speak(text)?;
        }
    }
    Ok(())
}

/// Add an agent to the active huddle.
///
/// Steps:
/// 1. Validates the huddle is in the Active phase.
/// 2. Adds the agent to both the ephemeral and parent channels (kind:9000).
/// 3. Only appends the agent pubkey to `agent_pubkeys` if the ephemeral add
///    succeeded — failed adds (policy rejection) are NOT p-tagged.
///
/// Returns a structured `AgentAddResult` so the frontend can surface
/// parent-channel errors without treating them as hard failures.
///
/// The running ACP process for this agent auto-subscribes when it receives
/// the kind:9000 membership notification — no separate process spawn needed.
#[tauri::command]
pub async fn add_agent_to_huddle(
    agent_pubkey: String,
    state: State<'_, AppState>,
) -> Result<agents::AgentAddResult, String> {
    let (eph_id, parent_id) = {
        let hs = state.huddle_state.lock().map_err(|e| e.to_string())?;
        if hs.phase != HuddlePhase::Active {
            return Err("no active huddle".to_string());
        }
        let eph = hs
            .ephemeral_channel_id
            .clone()
            .ok_or("no ephemeral channel")?;
        let parent = hs.parent_channel_id.clone().ok_or("no parent channel")?;
        (eph, parent)
    };

    let eph_uuid = Uuid::parse_str(&eph_id).map_err(|e| e.to_string())?;
    let parent_uuid = Uuid::parse_str(&parent_id).map_err(|e| e.to_string())?;

    // Returns Err only if the ephemeral add fails — parent failure is in the result.
    let result = agents::add_agent_to_huddle(eph_uuid, parent_uuid, &agent_pubkey, &state).await?;

    // Ephemeral add succeeded — safe to register for p-tagging.
    // Clone the Arc first so we can drop the outer HuddleState lock before
    // acquiring the inner pubkeys lock (avoids the E0597 borrow-checker error).
    {
        let agent_pubkeys_arc = {
            let hs = state.huddle_state.lock().map_err(|e| e.to_string())?;
            Arc::clone(&hs.agent_pubkeys)
        };
        let mut pubkeys = agent_pubkeys_arc.lock().unwrap_or_else(|e| e.into_inner());
        if !pubkeys.contains(&agent_pubkey) {
            pubkeys.push(agent_pubkey.clone());
        }
    }

    // Also add the agent to the visible participants list.
    {
        let mut hs = state.huddle_state.lock().map_err(|e| e.to_string())?;
        if !hs.participants.contains(&agent_pubkey) {
            hs.participants.push(agent_pubkey);
        }
    }

    Ok(result)
}
