//! Huddle (voice/video) state machine and Tauri commands.
//!
//! Mental model:
//!   parent channel → start_huddle → ephemeral channel + LiveKit token
//!   other clients  → join_huddle  → LiveKit token
//!   any client     → leave_huddle → lifecycle event, clear local state
//!   creator        → end_huddle   → archive ephemeral channel, clear state
//!
//! HuddleState is stored in AppState and serialized for get_huddle_state.

use reqwest::Method;
use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HuddleState {
    pub phase: HuddlePhase,
    pub parent_channel_id: Option<String>,
    pub ephemeral_channel_id: Option<String>,
    pub livekit_token: Option<String>,
    pub livekit_url: Option<String>,
    pub livekit_room: Option<String>,
    /// Participant pubkey hex strings.
    pub participants: Vec<String>,
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

    let result: Result<LiveKitTokenResponse, String> = async {
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

        // 2. Add members to the ephemeral channel (best-effort).
        for pubkey in &member_pubkeys {
            let add_builder = events::build_add_member(ephemeral_uuid, pubkey, None)?;
            if let Err(e) = submit_event(add_builder, &state).await {
                eprintln!("sprout-desktop: huddle add_member failed for {pubkey}: {e}");
            }
        }

        // 3. Fetch LiveKit token BEFORE emitting HUDDLE_STARTED.
        //    This prevents a phantom announcement if the token fetch fails.
        let lk = fetch_livekit_token(&ephemeral_channel_id, &state).await?;

        // 4. Emit HUDDLE_STARTED to parent channel — only now that token is confirmed.
        let started_builder = events::build_huddle_started(
            &parent_channel_id,
            &ephemeral_channel_id,
            &lk.room,
        )?;
        submit_event(started_builder, &state).await?;

        Ok(lk)
    }
    .await;

    match result {
        Ok(lk) => {
            // 5. Store active state.
            {
                let mut hs = state.huddle_state.lock().map_err(|e| e.to_string())?;
                hs.phase = HuddlePhase::Active;
                hs.ephemeral_channel_id = Some(ephemeral_channel_id.clone());
                hs.livekit_token = Some(lk.token.clone());
                hs.livekit_url = Some(lk.url.clone());
                hs.livekit_room = Some(lk.room.clone());
                hs.participants = member_pubkeys;
            }

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
    }

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
/// 2. Clear local huddle state.
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
/// 3. Clear local huddle state.
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

/// Receive raw PCM audio bytes from the AudioWorklet.
/// Phase 1 stub — logs receipt. Phase 2 will feed to STT pipeline.
#[tauri::command]
pub fn push_audio_pcm(request: tauri::ipc::Request<'_>) -> Result<(), String> {
    match request.body() {
        tauri::ipc::InvokeBody::Raw(bytes) => {
            // Phase 1: just acknowledge receipt. Phase 2 will process.
            let sample_count = bytes.len() / 4; // f32 = 4 bytes
            if sample_count > 0 {
                // Log occasionally to avoid spam.
                static COUNTER: std::sync::atomic::AtomicU64 =
                    std::sync::atomic::AtomicU64::new(0);
                let count = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if count % 100 == 0 {
                    eprintln!(
                        "sprout-desktop: push_audio_pcm received {sample_count} samples (batch #{count})"
                    );
                }
            }
            Ok(())
        }
        _ => Err("expected raw binary body".to_string()),
    }
}
