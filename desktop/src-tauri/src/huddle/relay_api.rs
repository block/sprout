//! Relay HTTP helpers for huddle operations.
//!
//! Thin wrappers around the relay REST API for channel membership queries,
//! human participant counting, and the audio relay WebSocket connection.
//!
//! Mental model:
//!
//! ```text
//! connect_audio_relay(channel_id)
//!   → WS /huddle/{id}/audio
//!   → recv challenge → sign NIP-42 kind:22242 → send auth
//!   → recv joined
//!   → spawn audio_relay_task:
//!       send loop: pcm_rx → Opus encode → WS binary frame
//!       recv loop: WS binary frame → Opus decode (per-peer) → rodio playback
//! ```

use futures_util::{SinkExt, StreamExt};
use reqwest::Method;
use serde::Deserialize;
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMsg};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::relay::{api_path, build_authed_request, send_json_request};

/// Maximum number of agents that can be invited to a single huddle.
pub(crate) const MAX_HUDDLE_AGENTS: usize = 20;

/// Validate that a string looks like a Nostr pubkey hex (64 hex chars).
pub(crate) fn validate_pubkey_hex(pubkey: &str) -> Result<(), String> {
    if pubkey.len() != 64 || !pubkey.chars().all(|c| c.is_ascii_hexdigit()) {
        let preview: String = pubkey.chars().take(16).collect();
        return Err(format!("invalid pubkey hex: {preview}"));
    }
    Ok(())
}

pub(crate) fn parse_channel_uuid(channel_id: &str) -> Result<Uuid, String> {
    Uuid::parse_str(channel_id).map_err(|_| format!("invalid channel UUID: {channel_id}"))
}

/// Handshake timeout: max time to wait for challenge/joined from the relay.
/// Matches the server's AUTH_TIMEOUT so both sides give up at roughly the same time.
const HANDSHAKE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Connect to the relay's audio WebSocket and run the Opus encode/decode pipeline.
///
/// Returns `(cancel_token, pcm_sender)` — caller stores both in `HuddleState`.
/// Dropping the sender or calling `cancel.cancel()` shuts down the relay task.
pub(crate) async fn connect_audio_relay(
    channel_id: &str,
    parent_channel_id: Option<&str>,
    state: &AppState,
) -> Result<(CancellationToken, tokio::sync::mpsc::Sender<Vec<u8>>), String> {
    use nostr::JsonUtil;

    let relay_url = crate::relay::relay_ws_url();
    let ws_url = format!("{relay_url}/huddle/{channel_id}/audio");

    let keys = state.keys.lock().map_err(|e| e.to_string())?.clone();

    // Grab app handle for emitting active-speaker events to the frontend.
    let app_handle = state.app_handle.lock().ok().and_then(|g| g.clone());

    // ── Synchronous handshake: connect + auth + wait for joined ──────────────
    // This runs BEFORE returning, so callers get a real error on failure.
    let (ws_stream, _) = connect_async(&ws_url)
        .await
        .map_err(|e| format!("audio WS connect failed: {e}"))?;
    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    // Receive challenge (with timeout — don't hang on a half-open socket).
    let challenge = tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
        loop {
            match ws_rx.next().await {
                Some(Ok(WsMsg::Text(text))) => {
                    let v: serde_json::Value = serde_json::from_str(&text)
                        .map_err(|e| format!("bad challenge JSON: {e}"))?;
                    if v["type"] == "challenge" {
                        break v["challenge"]
                            .as_str()
                            .ok_or_else(|| "missing challenge string".to_string())
                            .map(|s| s.to_string());
                    }
                }
                Some(Ok(WsMsg::Close(_))) | None => {
                    break Err("connection closed before challenge".into());
                }
                _ => continue,
            }
        }
    })
    .await
    .map_err(|_| "timeout waiting for challenge from relay".to_string())?
    .map_err(|e: String| e)?;

    // Sign NIP-42 auth event (kind:22242).
    let tags = vec![
        nostr::Tag::parse(["relay", &relay_url]).map_err(|e| format!("tag relay: {e}"))?,
        nostr::Tag::parse(["challenge", &challenge]).map_err(|e| format!("tag challenge: {e}"))?,
    ];
    let event = nostr::EventBuilder::new(nostr::Kind::Custom(22242), "")
        .tags(tags)
        .sign_with_keys(&keys)
        .map_err(|e| format!("sign: {e}"))?;

    // Send auth message.
    let event_json: serde_json::Value = serde_json::from_str(&event.as_json())
        .map_err(|e| format!("failed to serialize auth event: {e}"))?;
    let auth_msg = serde_json::json!({
        "type": "auth",
        "event": event_json,
        "parent_channel_id": parent_channel_id,
    });
    ws_tx
        .send(WsMsg::Text(auth_msg.to_string().into()))
        .await
        .map_err(|e| format!("send auth: {e}"))?;

    // Wait for joined — capture initial peer map (with timeout).
    let initial_peers: Vec<(u8, String)> = tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
        loop {
            match ws_rx.next().await {
                Some(Ok(WsMsg::Text(text))) => {
                    let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
                    match v["type"].as_str() {
                        Some("joined") => {
                            let peers = v["peers"]
                                .as_array()
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|p| {
                                            Some((
                                                p["peer_index"].as_u64()? as u8,
                                                p["pubkey"].as_str()?.to_string(),
                                            ))
                                        })
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default();
                            break Ok(peers);
                        }
                        Some("error") => {
                            break Err(format!("audio relay auth error: {}", v["message"]));
                        }
                        _ => continue,
                    }
                }
                Some(Ok(WsMsg::Close(_))) | None => {
                    break Err("connection closed before joined".into());
                }
                _ => continue,
            }
        }
    })
    .await
    .map_err(|_| "timeout waiting for joined from relay".to_string())?
    .map_err(|e: String| e)?;

    // ── Handshake succeeded — spawn background encode/decode loops ────────────
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    let (pcm_tx, pcm_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(50);

    tokio::spawn(async move {
        if let Err(e) = audio_relay_pipeline(
            ws_tx,
            ws_rx,
            pcm_rx,
            cancel_clone.clone(),
            app_handle.clone(),
            initial_peers,
        )
        .await
        {
            eprintln!("sprout-desktop: audio relay pipeline exited: {e}");
        }

        // Only emit the disconnect event for UNEXPECTED exits.
        // If teardown_huddle intentionally cancelled the pipeline, the token
        // is already cancelled when we get here — skip the event to avoid a
        // duplicate leaveHuddle() racing with the in-progress teardown.
        if !cancel_clone.is_cancelled() {
            cancel_clone.cancel();
            // Notify the frontend so it can trigger leaveHuddle() — replaces
            // the old LiveKit onDisconnected callback removed in this PR.
            if let Some(ref app) = app_handle {
                use tauri::Emitter;
                let _ = app.emit("huddle-audio-disconnected", ());
            }
        }
    });

    Ok((cancel, pcm_tx))
}

/// Background Opus encode/decode pipeline. Called after the handshake succeeds
/// in `connect_audio_relay` with the already-split WS halves and initial peer map.
type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn audio_relay_pipeline(
    ws_tx: futures_util::stream::SplitSink<WsStream, WsMsg>,
    mut ws_rx: futures_util::stream::SplitStream<WsStream>,
    mut pcm_rx: tokio::sync::mpsc::Receiver<Vec<u8>>,
    cancel: CancellationToken,
    app_handle: Option<tauri::AppHandle>,
    initial_peers: Vec<(u8, String)>,
) -> Result<(), String> {
    // ── Setup Opus encoder (48 kHz mono, VOIP application) ───────────────────
    let mut encoder = opus::Encoder::new(48000, opus::Channels::Mono, opus::Application::Voip)
        .map_err(|e| format!("opus encoder: {e}"))?;
    encoder
        .set_bitrate(opus::Bitrate::Bits(32000))
        .map_err(|e| format!("opus bitrate: {e}"))?;
    encoder
        .set_dtx(true)
        .map_err(|e| format!("opus dtx: {e}"))?;

    // Persistent rodio player — created once, reused for all decoded packets.
    let sink_handle =
        rodio::DeviceSinkBuilder::open_default_sink().map_err(|e| format!("audio output: {e}"))?;
    let player = rodio::Player::connect_new(&sink_handle.mixer());

    // Per-peer decoders: peer_id (u8) → opus::Decoder
    let decoders: std::collections::HashMap<u8, opus::Decoder> = std::collections::HashMap::new();

    // Opus frame size: 20 ms at 48 kHz = 960 samples.
    const FRAME_SAMPLES: usize = 960;
    // Output buffer for decoded PCM (f32).
    let decode_buf = vec![0f32; FRAME_SAMPLES];

    // Run send/recv loops concurrently, sharing the WS sender via a tokio Mutex.
    use std::sync::Arc as StdArc;
    let ws_tx = StdArc::new(tokio::sync::Mutex::new(ws_tx));
    let ws_tx_send = StdArc::clone(&ws_tx);
    let cancel_send = cancel.clone();

    // Send task: PCM → Opus encode → WS binary frame.
    let send_task = tokio::spawn(async move {
        let mut encoder = encoder; // Move encoder into task.
        const FRAME_SAMPLES: usize = 960;
        let mut out_buf = vec![0u8; 4000];

        loop {
            // Poll cancel and pcm_rx together using futures_util::future::select.
            let pcm_bytes = {
                use futures_util::future::Either;
                let cancelled = std::pin::pin!(cancel_send.cancelled());
                let recv = std::pin::pin!(pcm_rx.recv());
                match futures_util::future::select(cancelled, recv).await {
                    Either::Left(_) => break, // Cancelled.
                    Either::Right((Some(b), _)) => b,
                    Either::Right((None, _)) => break, // Sender dropped.
                }
            };

            if pcm_bytes.len() % 4 != 0 {
                continue; // Malformed batch.
            }
            let samples: Vec<f32> = pcm_bytes
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect();

            let mut tx = ws_tx_send.lock().await;
            for chunk in samples.chunks(FRAME_SAMPLES) {
                let encode_result = if chunk.len() == FRAME_SAMPLES {
                    encoder.encode_float(chunk, &mut out_buf)
                } else {
                    let mut padded = chunk.to_vec();
                    padded.resize(FRAME_SAMPLES, 0.0);
                    encoder.encode_float(&padded, &mut out_buf)
                };
                let n = match encode_result {
                    Ok(n) => n,
                    Err(e) => {
                        eprintln!("sprout-desktop: opus encode error: {e}");
                        continue;
                    }
                };
                if n > 0 {
                    // Send raw Opus bytes — the relay prepends the peer_index.
                    if tx
                        .send(WsMsg::Binary(out_buf[..n].to_vec().into()))
                        .await
                        .is_err()
                    {
                        return; // WS closed.
                    }
                }
            }
        }
        // Send close on clean exit.
        let mut tx = ws_tx_send.lock().await;
        let _ = tx.send(WsMsg::Close(None)).await;
    });

    // Recv task: WS binary frame → Opus decode → rodio playback + active speakers.
    let recv_task = tokio::spawn(async move {
        let mut decoders = decoders;
        let mut decode_buf = decode_buf;
        let cancel_recv = cancel;
        let ws_tx_recv = ws_tx;

        // Active speaker tracking (client-side): map peer_index → pubkey,
        // track which indices sent audio recently, emit Tauri event every 500ms.
        let mut index_to_pubkey: std::collections::HashMap<u8, String> =
            initial_peers.into_iter().collect();
        let mut active_indices: std::collections::HashSet<u8> = std::collections::HashSet::new();
        let mut speaker_tick = tokio::time::interval(std::time::Duration::from_millis(500));
        speaker_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            use futures_util::future::Either;
            let cancelled = std::pin::pin!(cancel_recv.cancelled());
            let next = std::pin::pin!(ws_rx.next());
            let tick = std::pin::pin!(speaker_tick.tick());

            // Three-way select: cancel, WS message, or speaker tick.
            // Use nested select since futures_util doesn't have select3.
            let ws_or_tick = std::pin::pin!(futures_util::future::select(next, tick));
            match futures_util::future::select(cancelled, ws_or_tick).await {
                Either::Left(_) => break, // Cancelled.
                Either::Right((Either::Right((_, _)), _)) => {
                    // Speaker tick: emit active speakers and reset.
                    if let Some(ref app) = app_handle {
                        use tauri::Emitter;
                        let pubkeys: Vec<String> = active_indices
                            .iter()
                            .filter_map(|idx| index_to_pubkey.get(idx).cloned())
                            .collect();
                        let _ = app.emit("huddle-active-speakers", &pubkeys);
                    }
                    active_indices.clear();
                    continue;
                }
                Either::Right((Either::Left((Some(Ok(msg)), _)), _)) => {
                    // WS message — process below.
                    match msg {
                        WsMsg::Binary(data) => {
                            if data.len() < 2 {
                                continue;
                            }
                            let peer_idx = data[0];
                            let opus_bytes = &data[1..];
                            active_indices.insert(peer_idx);
                            let decoder = match decoders.entry(peer_idx) {
                                std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                                std::collections::hash_map::Entry::Vacant(e) => {
                                    match opus::Decoder::new(48000, opus::Channels::Mono) {
                                        Ok(d) => e.insert(d),
                                        Err(err) => {
                                            eprintln!("sprout-desktop: opus decoder init peer {peer_idx}: {err}");
                                            continue;
                                        }
                                    }
                                }
                            };
                            match decoder.decode_float(opus_bytes, &mut decode_buf, false) {
                                Ok(n) if n > 0 => {
                                    use rodio::buffer::SamplesBuffer;
                                    use std::num::NonZero;
                                    let channels = NonZero::new(1u16).unwrap();
                                    let rate = NonZero::new(48000u32).unwrap();
                                    player.append(SamplesBuffer::new(
                                        channels,
                                        rate,
                                        decode_buf[..n].to_vec(),
                                    ));
                                }
                                Ok(_) => {} // DTX silence.
                                Err(e) => {
                                    eprintln!("sprout-desktop: opus decode peer {peer_idx}: {e}");
                                }
                            }
                        }
                        WsMsg::Text(text) => {
                            // Parse control messages to build index→pubkey map.
                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                                match v["type"].as_str() {
                                    Some("joined") => {
                                        // Build/update the peer index → pubkey map from the peers array.
                                        if let Some(peers) = v["peers"].as_array() {
                                            for p in peers {
                                                if let (Some(pk), Some(idx)) =
                                                    (p["pubkey"].as_str(), p["peer_index"].as_u64())
                                                {
                                                    index_to_pubkey
                                                        .insert(idx as u8, pk.to_string());
                                                }
                                            }
                                        }
                                    }
                                    Some("left") => {
                                        if let Some(idx) = v["peer_index"].as_u64() {
                                            index_to_pubkey.remove(&(idx as u8));
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        WsMsg::Ping(data) => {
                            let mut tx = ws_tx_recv.lock().await;
                            let _ = tx.send(WsMsg::Pong(data)).await;
                        }
                        WsMsg::Close(_) => break,
                        _ => {}
                    }
                }
                // WS error or closed.
                Either::Right((Either::Left(_), _)) => break,
            }
        }
    });

    // Wait for either task to finish, then abort the survivor.
    // Without this, the send_task can sit forever on pcm_rx.recv() after
    // the recv_task exits (WS closed), leaking resources.
    use futures_util::future::Either;
    match futures_util::future::select(std::pin::pin!(send_task), std::pin::pin!(recv_task)).await {
        Either::Left((_, recv_handle)) => recv_handle.abort(),
        Either::Right((_, send_handle)) => send_handle.abort(),
    }

    Ok(())
}

/// Fetch channel members with their roles from the relay.
/// Returns (pubkey, role) tuples — the authoritative source for both
/// `fetch_channel_members` (filtered by role) and `count_human_members`.
pub(crate) async fn fetch_channel_members_with_roles(
    channel_id: &str,
    state: &AppState,
) -> Result<Vec<(String, Option<String>)>, String> {
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
        .map(|m| (m.pubkey, m.role))
        .collect())
}

/// Fetch channel members from the relay. If `role_filter` is Some, only return
/// members with that role (e.g., "bot" for agents). Returns all members if None.
pub(crate) async fn fetch_channel_members(
    channel_id: &str,
    role_filter: Option<&str>,
    state: &AppState,
) -> Result<Vec<String>, String> {
    let all = fetch_channel_members_with_roles(channel_id, state).await?;
    Ok(all
        .into_iter()
        .filter(|(_, role)| role_filter.map_or(true, |r| role.as_deref() == Some(r)))
        .map(|(pubkey, _)| pubkey)
        .collect())
}

/// Count human (non-bot) members remaining in a channel.
/// Built on `fetch_channel_members_with_roles` — fetches all members then counts non-bots.
pub(crate) async fn count_human_members(
    channel_id: &str,
    state: &AppState,
) -> Result<usize, String> {
    let all = fetch_channel_members_with_roles(channel_id, state).await?;
    Ok(all
        .iter()
        .filter(|(_, role)| role.as_deref() != Some("bot"))
        .count())
}
