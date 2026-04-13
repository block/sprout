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
pub mod supertonic;
pub mod tts;

// ── Shared utilities ──────────────────────────────────────────────────────────

/// Drain and discard all pending messages until shutdown or disconnect.
/// Shared by both the STT and TTS worker threads for graceful degradation
/// when model files are missing or initialization fails.
pub(super) fn drain_until_shutdown<T>(
    rx: std::sync::mpsc::Receiver<T>,
    shutdown: &std::sync::atomic::AtomicBool,
) {
    loop {
        if shutdown.load(std::sync::atomic::Ordering::Acquire) {
            break;
        }
        match rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(_) => continue,
            Err(_) => break,
        }
    }
}

use reqwest::Method;
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};
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
    Connected, // Backend ready, waiting for frontend media confirmation.
    Active,
    Leaving,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HuddleState {
    pub phase: HuddlePhase,
    pub parent_channel_id: Option<String>,
    pub ephemeral_channel_id: Option<String>,
    /// Skipped from serialization — the frontend gets the token from
    /// start_huddle/join_huddle return values. Exposing the LiveKit JWT
    /// on every 2-second get_huddle_state poll is unnecessary attack surface.
    #[serde(skip)]
    pub livekit_token: Option<String>,
    /// Skipped from serialization — same rationale as livekit_token.
    #[serde(skip)]
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
    /// Whether this client created the huddle (vs. joined it).
    /// Used to enforce that only the creator can end/archive the huddle.
    pub is_creator: bool,
    /// Whether TTS output is enabled (user-toggled).
    pub tts_enabled: bool,
    /// Shared flag: true while TTS is playing audio.
    /// Shared with the STT pipeline for barge-in / echo gating.
    #[serde(skip)]
    pub tts_active: Arc<AtomicBool>,
    /// Shared barge-in cancel flag. Set by STT when it detects speech during TTS.
    /// Read by TTS to stop playback. Lives in HuddleState so it survives pipeline
    /// restarts — both STT and TTS reference the same flag for the entire huddle.
    #[serde(skip)]
    pub tts_cancel: Arc<AtomicBool>,
    /// Timestamp of the last agent pubkey refresh from the relay.
    /// Used to throttle the refresh in check_pipeline_hotstart to every 15 s.
    #[serde(skip)]
    pub last_agent_refresh: Option<std::time::Instant>,
    /// Session generation — incremented on every teardown. The transcription
    /// task captures this at spawn time and checks before each POST. If the
    /// generation has changed, the task silently drops the transcript.
    #[serde(skip)]
    pub session_generation: Arc<AtomicU64>,
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
            is_creator: self.is_creator,
            tts_enabled: self.tts_enabled,
            tts_active: Arc::clone(&self.tts_active),
            tts_cancel: Arc::clone(&self.tts_cancel),
            last_agent_refresh: self.last_agent_refresh,
            session_generation: Arc::clone(&self.session_generation),
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
            is_creator: false,
            tts_enabled: true,
            tts_active: Arc::new(AtomicBool::new(false)),
            tts_cancel: Arc::new(AtomicBool::new(false)),
            last_agent_refresh: None,
            session_generation: Arc::new(AtomicU64::new(0)),
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

/// Maximum number of agents that can be invited to a single huddle.
const MAX_HUDDLE_AGENTS: usize = 20;

/// Validate that a string looks like a Nostr pubkey hex (64 hex chars).
fn validate_pubkey_hex(pubkey: &str) -> Result<(), String> {
    if pubkey.len() != 64 || !pubkey.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "invalid pubkey hex: {}",
            &pubkey[..pubkey.len().min(16)]
        ));
    }
    Ok(())
}

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

/// Fetch channel members from the relay. If `role_filter` is Some, only return
/// members with that role (e.g., "bot" for agents). Returns all members if None.
async fn fetch_channel_members(
    channel_id: &str,
    role_filter: Option<&str>,
    state: &AppState,
) -> Result<Vec<String>, String> {
    #[derive(Deserialize)]
    struct Member {
        pubkey: String,
        role: Option<String>,
    }
    #[derive(Deserialize)]
    struct MembersResponse {
        members: Vec<Member>,
    }

    let path = api_path(&["channels", channel_id, "members"]);
    let request = build_authed_request(&state.http_client, Method::GET, &path, state)?;
    let resp: MembersResponse = send_json_request(request).await.map_err(|e| {
        eprintln!("sprout-desktop: fetch channel members failed: {e}");
        e
    })?;

    Ok(resp
        .members
        .into_iter()
        .filter(|m| role_filter.map_or(true, |r| m.role.as_deref() == Some(r)))
        .map(|m| m.pubkey)
        .collect())
}

/// Common setup after a huddle connection is established (both start and join).
/// Hydrates participants from relay, ensures model downloads, starts pipelines.
async fn post_connect_setup(state: &AppState, ephemeral_channel_id: &str) -> Result<(), String> {
    // Hydrate agent pubkeys from relay (authoritative — overrides local guess).
    if let Ok(agents) = fetch_channel_members(ephemeral_channel_id, Some("bot"), state).await {
        let hs = state.huddle()?;
        *hs.agent_pubkeys.lock().unwrap_or_else(|e| e.into_inner()) = agents;
    }

    // Hydrate participants from relay (authoritative state).
    if let Ok(all_members) = fetch_channel_members(ephemeral_channel_id, None, state).await {
        if !all_members.is_empty() {
            let mut hs = state.huddle()?;
            hs.participants = all_members;
        }
    }

    // Ensure voice models are downloading (idempotent).
    if let Some(mgr) = models::global_model_manager() {
        mgr.start_moonshine_download(state.http_client.clone());
        mgr.start_supertonic_download(state.http_client.clone());
    }

    // Start pipelines: TTS first (so STT can capture tts_cancel for barge-in).
    if let Err(e) = maybe_start_tts_pipeline(state).await {
        eprintln!("sprout-desktop: TTS pipeline failed to start: {e}");
    }
    if let Err(e) = maybe_start_stt_pipeline(state, ephemeral_channel_id).await {
        eprintln!("sprout-desktop: STT pipeline failed to start: {e}");
    }

    Ok(())
}

/// Attempt to start the STT pipeline if models are present.
///
/// Returns `Ok(true)` if the pipeline was started, `Ok(false)` if models are
/// not ready (voice-only mode), or `Err` on a real failure.
///
/// Creates the shared `tts_active` flag and passes it to the STT pipeline
/// for barge-in / echo gating. The same flag is later passed to the TTS
/// pipeline so it can signal when audio is playing.
async fn maybe_start_stt_pipeline(
    state: &AppState,
    ephemeral_channel_id: &str,
) -> Result<bool, String> {
    if !models::is_moonshine_ready() {
        return Ok(false); // Models not downloaded yet — voice-only mode.
    }
    let model_dir =
        models::moonshine_model_dir().ok_or_else(|| "Moonshine model directory not found")?;

    let channel_uuid = parse_channel_uuid(ephemeral_channel_id)?;

    // Grab shared flags, agent pubkeys, and session generation from HuddleState.
    // If replacing an existing pipeline, bump generation first so the old
    // transcription task's next POST sees a stale generation and exits.
    let (tts_active, tts_cancel, agent_pubkeys_arc, session_gen) = {
        let mut hs = state.huddle()?;
        // Invalidate any existing transcription task before replacing the pipeline.
        if hs.stt_pipeline.is_some() {
            hs.session_generation.fetch_add(1, Ordering::Release);
        }
        if let Some(ref old) = hs.stt_pipeline {
            old.shutdown();
        }
        (
            Arc::clone(&hs.tts_active),
            Some(Arc::clone(&hs.tts_cancel)),
            Arc::clone(&hs.agent_pubkeys),
            Arc::clone(&hs.session_generation),
        )
    };

    let (pipeline, text_rx) = stt::SttPipeline::new(model_dir, tts_active, tts_cancel)?;
    let pipeline = Arc::new(pipeline);

    {
        let mut hs = state.huddle()?;
        hs.stt_pipeline = Some(Arc::clone(&pipeline));
    }

    spawn_transcription_task(text_rx, channel_uuid, agent_pubkeys_arc, session_gen, state);
    Ok(true)
}

/// Attempt to start the TTS pipeline if Supertonic models are present and TTS is enabled.
///
/// Returns `Ok(true)` if the pipeline was started, `Ok(false)` if preconditions
/// aren't met (model not ready, pipeline exists, TTS disabled), or `Err` on failure.
async fn maybe_start_tts_pipeline(state: &AppState) -> Result<bool, String> {
    if !models::is_supertonic_ready() {
        return Ok(false); // Supertonic not downloaded yet — TTS unavailable.
    }

    // Don't create a duplicate pipeline if one is already running.
    {
        let hs = state.huddle()?;
        if hs.tts_pipeline.is_some() {
            return Ok(false);
        }
    }

    let model_dir = match models::supertonic_model_dir() {
        Some(d) => d,
        None => return Ok(false),
    };

    let (tts_active, tts_enabled, tts_cancel) = {
        let hs = state.huddle()?;
        (
            Arc::clone(&hs.tts_active),
            hs.tts_enabled,
            Arc::clone(&hs.tts_cancel),
        )
    };

    if !tts_enabled {
        return Ok(false);
    }

    let pipeline = Arc::new(tts::TtsPipeline::new(model_dir, tts_active, tts_cancel)?);

    {
        let mut hs = state.huddle()?;
        // Re-check: another call may have created a pipeline while we were building ours.
        if hs.tts_pipeline.is_some() {
            return Ok(false); // The existing one wins.
        }
        hs.tts_pipeline = Some(pipeline);
    }

    Ok(true)
}

/// Spawn a tokio task that reads text_rx and posts kind:9 events.
///
/// Fix 1: `agent_pubkeys_arc` is an `Arc<Mutex<Vec<String>>>` cloned from
///        `HuddleState` — the task reads it at post time so p-tags are always
///        current, not a stale snapshot.
/// Fix 3: no `.unwrap()` on mutex — poisoned locks are recovered gracefully.
/// Fix 4: `text_rx` is a `tokio::sync::mpsc::Receiver` — fully async `.recv().await`
///        never blocks a Tokio worker thread (unlike std `recv_timeout`).
fn spawn_transcription_task(
    mut text_rx: tokio::sync::mpsc::Receiver<String>,
    channel_uuid: Uuid,
    agent_pubkeys_arc: Arc<Mutex<Vec<String>>>,
    session_generation: Arc<AtomicU64>,
    state: &AppState,
) {
    // Capture the current generation at spawn time.
    let spawned_gen = session_generation.load(Ordering::Acquire);

    let http_client = state.http_client.clone();
    let keys = match state.keys.lock() {
        Ok(k) => k.clone(),
        Err(_) => return,
    };
    let configured_api_token = state.configured_api_token.clone();

    tauri::async_runtime::spawn(async move {
        // recv().await yields (not blocks) until text arrives or sender is dropped.
        // When the STT worker exits and drops its Sender, recv() returns None → loop ends.
        while let Some(t) = text_rx.recv().await {
            if t.is_empty() {
                continue;
            }

            // Session guard: if the generation has changed, this task is stale.
            // Drop the transcript silently — the huddle has ended or been replaced.
            if session_generation.load(Ordering::Acquire) != spawned_gen {
                break; // Exit the loop entirely — no more posts from this task.
            }

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
            let api_token_ref = configured_api_token.as_deref();
            let pubkey_hex = keys.public_key().to_hex();

            if let Err(e) =
                crate::events::post_event_raw(&http_client, api_token_ref, &pubkey_hex, event_json)
                    .await
            {
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
    // Validate inputs at the Tauri boundary.
    if member_pubkeys.len() > MAX_HUDDLE_AGENTS {
        return Err(format!(
            "too many agents: {} (max {})",
            member_pubkeys.len(),
            MAX_HUDDLE_AGENTS
        ));
    }
    // Dedup and validate pubkey format.
    let member_pubkeys: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        let mut deduped = Vec::new();
        for pk in member_pubkeys {
            validate_pubkey_hex(&pk)?;
            if seen.insert(pk.clone()) {
                deduped.push(pk);
            }
        }
        deduped
    };

    // Transition to Creating.
    {
        let mut hs = state.huddle()?;
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
            let add_builder = events::build_add_member(ephemeral_uuid, pubkey, Some("bot"))?;
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

        // 5. Post voice-mode guidelines as kind:48106.
        //    Best-effort: don't fail the huddle if this fails.
        let guidelines = agents::voice_mode_guidelines(&parent_channel_id);
        if let Ok(guidelines_builder) =
            events::build_huddle_guidelines(&ephemeral_channel_id, &guidelines)
        {
            if let Err(e) = submit_event(guidelines_builder, &state).await {
                eprintln!("sprout-desktop: huddle guidelines (kind:48106) failed: {e}");
            }
        }

        Ok((lk, successful_agents))
    }
    .await;

    match result {
        Ok((lk, successful_agents)) => {
            // 5. Store active state.
            {
                let mut hs = state.huddle()?;
                hs.phase = HuddlePhase::Connected;
                hs.is_creator = true;
                hs.ephemeral_channel_id = Some(ephemeral_channel_id.clone());
                hs.livekit_token = Some(lk.token.clone());
                hs.livekit_url = Some(lk.url.clone());
                hs.livekit_room = Some(lk.room.clone());
                // Only store agents that were successfully enrolled (Fix 1).
                *hs.agent_pubkeys.lock().unwrap_or_else(|e| e.into_inner()) =
                    successful_agents.clone();
                // Include the current user + successfully enrolled agents as participants.
                // Use successful_agents (not member_pubkeys) so failed enrollments
                // are not reflected in the participant list.
                let own_pubkey = state
                    .keys
                    .lock()
                    .map(|k| k.public_key().to_hex())
                    .unwrap_or_default();
                let mut participants = successful_agents.clone();
                if !own_pubkey.is_empty() && !participants.contains(&own_pubkey) {
                    participants.insert(0, own_pubkey);
                }
                hs.participants = participants;
            }

            // 6. Hydrate members, download models, start pipelines.
            post_connect_setup(&state, &ephemeral_channel_id).await?;

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
        let mut hs = state.huddle()?;
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
        let mut hs = state.huddle()?;
        hs.phase = HuddlePhase::Connected;
        hs.livekit_token = Some(lk.token.clone());
        hs.livekit_url = Some(lk.url.clone());
        hs.livekit_room = Some(lk.room.clone());
        // agent_pubkeys + participants hydrated by post_connect_setup below.
        // Seed with current user as a fallback until relay responds.
        let own_pubkey = state
            .keys
            .lock()
            .map(|k| k.public_key().to_hex())
            .unwrap_or_default();
        if !own_pubkey.is_empty() {
            hs.participants = vec![own_pubkey];
        }
    }

    // 4. Hydrate members, download models, start pipelines.
    post_connect_setup(&state, &ephemeral_channel_id).await?;

    Ok(HuddleJoinInfo {
        ephemeral_channel_id,
        livekit_token: lk.token,
        livekit_url: lk.url,
        livekit_room: lk.room,
    })
}

/// Shut down all pipelines and reset huddle state to Idle.
///
/// Used by both `leave_huddle` and `end_huddle` to avoid duplicating the
/// shutdown-then-reset sequence.
fn teardown_huddle(state: &AppState) -> Result<(), String> {
    {
        let hs = state.huddle()?;
        // Increment generation first — this immediately invalidates any
        // in-flight transcription task, even before pipelines shut down.
        hs.session_generation.fetch_add(1, Ordering::Release);
        if let Some(ref pipeline) = hs.stt_pipeline {
            pipeline.shutdown();
        }
        if let Some(ref pipeline) = hs.tts_pipeline {
            pipeline.shutdown();
        }
    }
    {
        let mut hs = state.huddle()?;
        // Preserve the generation counter across reset — it must survive
        // for the old transcription task to see the incremented value.
        let gen = Arc::clone(&hs.session_generation);
        *hs = HuddleState::default();
        hs.session_generation = gen;
    }
    Ok(())
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
        let mut hs = state.huddle()?;
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

    teardown_huddle(&state)?;

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
        let mut hs = state.huddle()?;
        if hs.phase == HuddlePhase::Idle {
            return Ok(()); // Nothing to end.
        }
        if !hs.is_creator {
            return Err("only the huddle creator can end the huddle".to_string());
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

    teardown_huddle(&state)?;

    Ok(())
}

/// Confirm that the frontend has established LiveKit + AudioWorklet.
/// Transitions from Connected → Active. No-op if already Active.
#[tauri::command]
pub async fn confirm_huddle_active(state: State<'_, AppState>) -> Result<(), String> {
    let mut hs = state.huddle()?;
    match hs.phase {
        HuddlePhase::Connected => {
            hs.phase = HuddlePhase::Active;
            Ok(())
        }
        HuddlePhase::Active => Ok(()), // Already active — idempotent.
        ref other => Err(format!("cannot confirm active: phase is {:?}", other)),
    }
}

/// Return the current HuddleState (serialized for the frontend).
#[tauri::command]
pub fn get_huddle_state(state: State<'_, AppState>) -> Result<HuddleState, String> {
    let hs = state.huddle()?;
    Ok(hs.clone())
}

/// Return the authoritative list of agent (bot-role) pubkeys in the active huddle.
///
/// Fetches from the relay's channel membership API — works for both creators
/// and joiners. Returns `Ok(Vec::new())` if no huddle is active. Returns
/// `Err` on relay fetch failure so the frontend can keep `agentsLoaded = false`
/// rather than treating a failed lookup as "zero agents".
#[tauri::command]
pub async fn get_huddle_agent_pubkeys(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let eph_id = {
        let hs = state.huddle()?;
        hs.ephemeral_channel_id.clone()
    };
    match eph_id {
        Some(id) => fetch_channel_members(&id, Some("bot"), &state).await,
        None => Ok(Vec::new()),
    }
}

/// Maximum IPC audio batch size: 100 KB.
/// A 100 ms batch at 48 kHz mono f32 is ~19 KB; 100 KB allows headroom
/// without letting a malformed IPC call allocate unbounded memory.
const MAX_AUDIO_BATCH_BYTES: usize = 100 * 1024;

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
            if bytes.len() > MAX_AUDIO_BATCH_BYTES {
                return Err(format!(
                    "audio batch too large: {} bytes (max {})",
                    bytes.len(),
                    MAX_AUDIO_BATCH_BYTES
                ));
            }
            if let Ok(hs) = state.huddle() {
                if let Some(ref pipeline) = hs.stt_pipeline {
                    pipeline.push_audio(bytes.to_vec())?;
                }
            }
            Ok(())
        }
        _ => Err("expected raw binary body".to_string()),
    }
}

/// Hot-start: check if voice models just finished downloading during an active
/// huddle and start the corresponding pipelines.
///
/// Called by the frontend on a timer or after model status changes. No-op if
/// the huddle is not active or pipelines are already running.
#[tauri::command]
pub async fn check_pipeline_hotstart(state: State<'_, AppState>) -> Result<(), String> {
    let (is_active, has_stt, has_tts, ephemeral_channel_id) = {
        let hs = state.huddle()?;
        (
            matches!(hs.phase, HuddlePhase::Connected | HuddlePhase::Active),
            hs.stt_pipeline.is_some(),
            hs.tts_pipeline.is_some(),
            hs.ephemeral_channel_id.clone(),
        )
    };

    if !is_active {
        return Ok(());
    }

    // Check if models just became ready (one-shot flags).
    let moonshine_ready = models::global_model_manager()
        .map(|m| m.take_moonshine_ready())
        .unwrap_or(false);
    let supertonic_ready = models::global_model_manager()
        .map(|m| m.take_supertonic_ready())
        .unwrap_or(false);

    // Start TTS first (so STT can capture tts_cancel).
    if !has_tts && (supertonic_ready || models::is_supertonic_ready()) {
        if let Err(e) = maybe_start_tts_pipeline(&state).await {
            eprintln!("sprout-desktop: TTS hotstart failed: {e}");
        }
    }

    if !has_stt && (moonshine_ready || models::is_moonshine_ready()) {
        if let Some(eph_id) = &ephemeral_channel_id {
            if let Err(e) = maybe_start_stt_pipeline(&state, eph_id).await {
                eprintln!("sprout-desktop: STT hotstart failed: {e}");
            }
        }
    }

    // Periodically refresh agent_pubkeys from relay membership.
    // This catches mid-huddle agent additions/removals by other participants,
    // keeping STT p-tags authoritative throughout the session.
    // Throttled to every 15 s (not on every 5 s hotstart poll) — the frontend
    // already refreshes its own agentPubkeys every 10 s via get_huddle_agent_pubkeys.
    // On Ok: always replace (even with empty — agents may have been removed).
    // On Err: preserve the existing list (transient failure shouldn't zero it).
    if let Some(eph_id) = &ephemeral_channel_id {
        let should_refresh = {
            let hs = state.huddle()?;
            match hs.last_agent_refresh {
                None => true,
                Some(t) => t.elapsed() >= std::time::Duration::from_secs(15),
            }
        };
        if should_refresh {
            // Fetch agents (for STT p-tags) and all members (for participant list).
            // Sequential — tokio::join! requires the `macros` feature.
            // Only update the throttle timestamp when at least one fetch succeeds,
            // so transient failures retry immediately on the next poll cycle.
            // Fetch both lists before acquiring the lock — no lock held across await.
            let fresh_agents = fetch_channel_members(eph_id, Some("bot"), &state)
                .await
                .ok();
            let fresh_members = fetch_channel_members(eph_id, None, &state).await.ok();

            if fresh_agents.is_some() || fresh_members.is_some() {
                let mut hs = state.huddle()?;
                if let Some(agents) = fresh_agents {
                    *hs.agent_pubkeys.lock().unwrap_or_else(|e| e.into_inner()) = agents;
                }
                if let Some(members) = fresh_members {
                    hs.participants = members;
                }
                hs.last_agent_refresh = Some(std::time::Instant::now());
            }
        }
    }

    Ok(())
}

/// Start the STT pipeline for the active huddle.
///
/// Delegates to `maybe_start_stt_pipeline` — returns `Err` if models are not
/// ready or no huddle is active. Safe to call multiple times: replaces the
/// existing pipeline if already running.
#[tauri::command]
pub async fn start_stt_pipeline(state: State<'_, AppState>) -> Result<(), String> {
    let ephemeral_channel_id = {
        let hs = state.huddle()?;
        hs.ephemeral_channel_id
            .clone()
            .ok_or("no active huddle — start or join a huddle first")?
    };

    match maybe_start_stt_pipeline(&state, &ephemeral_channel_id).await {
        Ok(true) => Ok(()),
        Ok(false) => Err("Moonshine model not ready".to_string()),
        Err(e) => Err(e),
    }
}

/// Trigger a background download of voice models (Moonshine STT + Supertonic TTS).
///
/// Returns immediately — downloads run in tokio background tasks.
/// Poll `get_model_status` to track progress.
/// Safe to call multiple times: no-op if already downloading or ready.
#[tauri::command]
pub async fn download_voice_models(state: State<'_, AppState>) -> Result<(), String> {
    let manager = models::global_model_manager()
        .ok_or("model manager unavailable (home directory could not be resolved)")?;
    manager.start_moonshine_download(state.http_client.clone());
    manager.start_supertonic_download(state.http_client.clone());
    Ok(())
}

/// Return the current download status for all voice models.
#[tauri::command]
pub fn get_model_status(_state: State<'_, AppState>) -> Result<models::VoiceModelStatus, String> {
    let manager = models::global_model_manager()
        .ok_or("model manager unavailable (home directory could not be resolved)")?;
    Ok(models::VoiceModelStatus {
        moonshine: manager.moonshine_status(),
        supertonic: manager.supertonic_status(),
    })
}

/// Enable or disable TTS output.
///
/// When disabled, the TTS pipeline is shut down and audio output stops.
/// When re-enabled, the pipeline is restarted if Supertonic models are available.
#[tauri::command]
pub async fn set_tts_enabled(enabled: bool, state: State<'_, AppState>) -> Result<(), String> {
    {
        let mut hs = state.huddle()?;
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
            let hs = state.huddle()?;
            hs.phase.clone()
        };
        if matches!(phase, HuddlePhase::Connected | HuddlePhase::Active) {
            if let Err(e) = maybe_start_tts_pipeline(&state).await {
                eprintln!("sprout-desktop: TTS pipeline restart failed: {e}");
            }
        }
    }

    Ok(())
}

/// Speak an agent message via TTS.
///
/// Maximum text length accepted for TTS synthesis.
/// ~2000 chars ≈ 1–2 minutes of speech. Longer messages are truncated.
const MAX_TTS_TEXT_LEN: usize = 2000;

/// Called by the WebView when it receives an incoming agent kind:9 message.
/// Lazily starts the TTS pipeline if models are ready but the pipeline hasn't
/// been created yet (e.g. models finished downloading after huddle started).
///
/// No-op if TTS is disabled or models aren't ready.
#[tauri::command]
pub async fn speak_agent_message(text: String, state: State<'_, AppState>) -> Result<(), String> {
    // Truncate oversized messages — agents shouldn't monologue in a voice huddle.
    // Use char count (not byte length) to avoid panicking on multi-byte UTF-8.
    let text = if text.chars().count() > MAX_TTS_TEXT_LEN {
        let mut truncated: String = text.chars().take(MAX_TTS_TEXT_LEN).collect();
        truncated.push_str("... message truncated.");
        truncated
    } else {
        text
    };

    let needs_pipeline = {
        let hs = state.huddle()?;
        hs.tts_enabled
            && hs.tts_pipeline.is_none()
            && matches!(hs.phase, HuddlePhase::Connected | HuddlePhase::Active)
    };

    // Lazy-start: models may have finished downloading after the huddle began.
    if needs_pipeline {
        if let Err(e) = maybe_start_tts_pipeline(&state).await {
            eprintln!("sprout-desktop: TTS lazy-start failed: {e}");
        }
    }

    let hs = state.huddle()?;
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
/// 1. Validates the huddle is in the Connected or Active phase.
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
    validate_pubkey_hex(&agent_pubkey)?;

    let (eph_id, parent_id) = {
        let hs = state.huddle()?;
        if !matches!(hs.phase, HuddlePhase::Connected | HuddlePhase::Active) {
            return Err("no active huddle".to_string());
        }

        // Enforce agent cap on incremental adds too.
        let current_agent_count = hs
            .agent_pubkeys
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len();
        if current_agent_count >= MAX_HUDDLE_AGENTS {
            return Err(format!(
                "agent limit reached: {} (max {})",
                current_agent_count, MAX_HUDDLE_AGENTS
            ));
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
            let hs = state.huddle()?;
            Arc::clone(&hs.agent_pubkeys)
        };
        let mut pubkeys = agent_pubkeys_arc.lock().unwrap_or_else(|e| e.into_inner());
        if !pubkeys.contains(&agent_pubkey) {
            pubkeys.push(agent_pubkey.clone());
        }
    }

    // No guidelines re-post needed — the agent sees the original kind:48106
    // guidelines via EOSE replay when it subscribes to the ephemeral channel.

    // Also add the agent to the visible participants list.
    {
        let mut hs = state.huddle()?;
        if !hs.participants.contains(&agent_pubkey) {
            hs.participants.push(agent_pubkey);
        }
    }

    Ok(result)
}
