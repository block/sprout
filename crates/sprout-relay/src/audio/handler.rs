//! WebSocket audio handler: NIP-42 auth → room join → frame relay → cleanup.
//!
//! ```text
//! ws_audio_handler
//!   └─ handle_audio_connection
//!        ├─ send challenge, await auth (5s timeout)
//!        ├─ ensure_membership (auto-add for ephemeral channels)
//!        ├─ room.add_peer → broadcast joined
//!        ├─ spawn send_loop + heartbeat_loop
//!        ├─ run recv_loop (blocks until disconnect)
//!        └─ cleanup: remove peer, broadcast left, emit lifecycle events
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message as WsMessage, WebSocket};
use axum::{
    extract::{Path, State, WebSocketUpgrade},
    response::IntoResponse,
};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use nostr::{EventBuilder, Filter, Kind, Tag};
use serde::Deserialize;
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};
use uuid::Uuid;

use sprout_auth::generate_challenge;
use sprout_db::channel::MemberRole;

use sprout_core::StoredEvent;

use crate::audio::room::PeerCtrl;
use crate::state::AppState;

/// Maximum binary frame size: 4 KB is generous for a single Opus packet.
const MAX_AUDIO_FRAME_BYTES: usize = 4096;

/// Maximum text frame size: 8 KB bounds auth/control JSON parsing.
const MAX_TEXT_FRAME_BYTES: usize = 8192;

/// Heartbeat interval.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// Missed pong limit before disconnect.
const MAX_MISSED_PONGS: u8 = 3;

/// Auth timeout.
const AUTH_TIMEOUT: Duration = Duration::from_secs(5);

// ── Route handler ─────────────────────────────────────────────────────────────

/// WebSocket upgrade handler for `/huddle/:channel_id/audio`.
pub async fn ws_audio_handler(
    State(state): State<Arc<AppState>>,
    Path(channel_id): Path<Uuid>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_audio_connection(socket, state, channel_id))
}

// ── Auth message shape ────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct AuthMsg {
    #[serde(rename = "type")]
    msg_type: String,
    event: nostr::Event,
    parent_channel_id: Option<Uuid>,
}

// ── Core connection lifecycle ─────────────────────────────────────────────────

async fn handle_audio_connection(socket: WebSocket, state: Arc<AppState>, channel_id: Uuid) {
    let (mut ws_send, mut ws_recv) = socket.split();

    // ── Step 1: send challenge ────────────────────────────────────────────────
    let challenge = generate_challenge();
    let challenge_msg =
        serde_json::json!({"type": "challenge", "challenge": challenge}).to_string();
    if ws_send
        .send(WsMessage::Text(challenge_msg.into()))
        .await
        .is_err()
    {
        return;
    }

    // ── Step 2: await auth (5s timeout) ──────────────────────────────────────
    let auth_result = tokio::time::timeout(AUTH_TIMEOUT, async {
        while let Some(Ok(msg)) = ws_recv.next().await {
            if let WsMessage::Text(text) = msg {
                if text.len() > MAX_TEXT_FRAME_BYTES {
                    warn!(channel_id = %channel_id, "auth text frame too large — dropping");
                    continue;
                }
                if let Ok(auth) = serde_json::from_str::<AuthMsg>(&text) {
                    if auth.msg_type == "auth" {
                        return Some(auth);
                    }
                }
            }
        }
        None
    })
    .await;

    let auth_msg = match auth_result {
        Ok(Some(a)) => a,
        _ => {
            debug!(channel_id = %channel_id, "audio auth timeout or disconnect");
            return;
        }
    };

    // Extract NIP-OA auth tag for NIP-AA before the event is consumed.
    // Delegates to the canonical extract_single_auth_tag from nip_aa.rs:
    //   Ok(None)     → zero auth tags — not a NIP-AA attempt
    //   Ok(Some(t))  → exactly one auth tag — serialize to JSON for later use
    //   Err(reason)  → multiple auth tags — malformed, reject immediately
    let auth_tag_json =
        match crate::handlers::nip_aa::extract_single_auth_tag(auth_msg.event.tags.as_slice()) {
            Ok(None) => None,
            Ok(Some(tag)) => serde_json::to_string(&tag.as_slice()).ok(),
            Err(reason) => {
                warn!(channel_id = %channel_id, "audio: multiple auth tags in NIP-42 event");
                let _ = ws_send
                    .send(WsMessage::Text(
                        serde_json::json!({"type": "error", "message": reason})
                            .to_string()
                            .into(),
                    ))
                    .await;
                return;
            }
        };
    let event_created_at = auth_msg.event.created_at.as_u64();

    // Extract auth_token tag before the event is consumed — API tokens and
    // Okta JWTs carry an `auth_token` tag; NIP-AA agents do not.
    let audio_auth_token = auth_msg.event.tags.iter().find_map(|tag| {
        let v = tag.as_slice();
        if v.len() >= 2 && v[0] == "auth_token" {
            Some(v[1].to_string())
        } else {
            None
        }
    });

    let relay_url = state.config.relay_url.clone();

    // ── NIP-AA agent auth path ───────────────────────────────────────────────
    // If there is no auth_token but there IS a NIP-OA auth tag, this *might*
    // be a NIP-AA agent. Verify NIP-42 binding first, then check relay
    // membership to confirm it is a virtual member — not just a direct member
    // with a dummy auth tag trying to bypass verify_auth_event.
    // Track whether this pubkey was granted access via NIP-AA virtual membership.
    // Virtual members already passed the relay membership check inside verify_nip_aa
    // (Step 5), so the direct-only membership gate below must be skipped for them.
    // owner_pubkey is Some only for NIP-AA virtual members; retained for the
    // duration of the audio session for audit and future owner-scoped tracking.
    let (pubkey, is_nip_aa_virtual, owner_pubkey): (
        nostr::PublicKey,
        bool,
        Option<nostr::PublicKey>,
    ) = if audio_auth_token.is_none() && auth_tag_json.is_some() {
        let event_for_verify = auth_msg.event.clone();
        let challenge_owned = challenge.clone();
        let relay_owned = relay_url.clone();
        match tokio::task::spawn_blocking(move || {
            sprout_auth::verify_nip42_event(&event_for_verify, &challenge_owned, &relay_owned)
        })
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                warn!(channel_id = %channel_id, "audio NIP-AA: NIP-42 binding failed: {e}");
                let _ = ws_send
                    .send(WsMessage::Text(
                        serde_json::json!({"type":"error","message":"auth failed"})
                            .to_string()
                            .into(),
                    ))
                    .await;
                return;
            }
            Err(e) => {
                warn!(channel_id = %channel_id, "audio NIP-AA: NIP-42 verify task panicked: {e}");
                let _ = ws_send
                    .send(WsMessage::Text(
                        serde_json::json!({"type":"error","message":"auth failed"})
                            .to_string()
                            .into(),
                    ))
                    .await;
                return;
            }
        }

        // NIP-AA Step 2: check direct membership first. If the agent is already a
        // direct member, skip NIP-AA entirely and fall through to verify_auth_event.
        // This matches the main WS auth handler's ordering.
        let candidate_pubkey = auth_msg.event.pubkey;
        let candidate_pubkey_hex = candidate_pubkey.to_hex();
        let is_direct_member = if state.config.require_relay_membership {
            // Fail closed on DB errors — a DB failure must not silently grant
            // access. The main WS auth handler (auth.rs) uses the same pattern.
            match state.db.is_relay_member(&candidate_pubkey_hex).await {
                Ok(v) => v,
                Err(e) => {
                    warn!(channel_id = %channel_id, "audio: DB error checking relay membership: {e}");
                    let _ = ws_send
                        .send(WsMessage::Text(
                            serde_json::json!({"type":"error","message":"auth failed"})
                                .to_string()
                                .into(),
                        ))
                        .await;
                    return;
                }
            }
        } else {
            true // membership not required — treat as member
        };

        if is_direct_member {
            // Direct member with an auth tag — go through verify_auth_event to
            // enforce token requirements (same as the Ok(None) branch below).
            let auth_ctx = match state
                .auth
                .verify_auth_event(auth_msg.event, &challenge, &relay_url)
                .await
            {
                Ok(ctx) => ctx,
                Err(e) => {
                    warn!(channel_id = %channel_id, "audio auth failed for direct member: {e}");
                    let _ = ws_send
                        .send(WsMessage::Text(
                            serde_json::json!({"type":"error","message":"auth failed"})
                                .to_string()
                                .into(),
                        ))
                        .await;
                    return;
                }
            };
            // Direct member — not a NIP-AA virtual member.
            (auth_ctx.pubkey, false, None)
        } else {
            // Not a direct member — attempt NIP-AA (Steps 3-5).
            let event_tags_slice: Vec<nostr::Tag> = auth_msg.event.tags.clone().to_vec();
            let nip_aa_result = crate::handlers::nip_aa::verify_nip_aa(
                &state,
                &candidate_pubkey,
                &event_tags_slice,
                event_created_at,
            )
            .await;

            match nip_aa_result {
                Ok(Some(result)) => {
                    // NIP-AA virtual member — NIP-42 binding already verified above.
                    // verify_nip_aa already confirmed the owner is an active relay member
                    // (Step 5), so the direct membership gate below must be skipped.
                    //
                    // owner_pubkey is persisted in AudioPeer for the full session lifetime
                    // for audit, quota aggregation, and future owner-scoped
                    // enumeration/termination (NIP-AA §6).
                    let nip_aa_owner = result.owner_pubkey;
                    tracing::info!(
                        channel_id = %channel_id,
                        agent = %candidate_pubkey.to_hex(),
                        owner = %nip_aa_owner.to_hex(),
                        "NIP-AA: audio virtual membership granted"
                    );
                    (candidate_pubkey, true, Some(nip_aa_owner))
                }
                Ok(None) => {
                    // No auth tag (or direct member) — must go through verify_auth_event
                    // to enforce token requirements.
                    let auth_ctx = match state
                        .auth
                        .verify_auth_event(auth_msg.event, &challenge, &relay_url)
                        .await
                    {
                        Ok(ctx) => ctx,
                        Err(e) => {
                            warn!(channel_id = %channel_id, "audio auth failed for direct member: {e}");
                            let _ = ws_send
                                .send(WsMessage::Text(
                                    serde_json::json!({"type":"error","message":"auth failed"})
                                        .to_string()
                                        .into(),
                                ))
                                .await;
                            return;
                        }
                    };
                    (auth_ctx.pubkey, false, None)
                }
                Err(reason) => {
                    // Auth tag present but invalid, or owner not a member — deny.
                    warn!(channel_id = %candidate_pubkey.to_hex(), reason = %reason, "audio NIP-AA: denied");
                    let _ = ws_send
                        .send(WsMessage::Text(
                            serde_json::json!({"type": "error", "message": reason})
                                .to_string()
                                .into(),
                        ))
                        .await;
                    return;
                }
            }
        } // close else { // Not a direct member — attempt NIP-AA
    } else {
        // ── Standard auth path (Okta JWT / API token / pubkey-only) ─────────
        let auth_ctx = match state
            .auth
            .verify_auth_event(auth_msg.event, &challenge, &relay_url)
            .await
        {
            Ok(ctx) => ctx,
            Err(e) => {
                warn!(channel_id = %channel_id, "audio auth failed: {e}");
                let _ = ws_send
                    .send(WsMessage::Text(
                        serde_json::json!({"type":"error","message":"auth failed"})
                            .to_string()
                            .into(),
                    ))
                    .await;
                return;
            }
        };
        (auth_ctx.pubkey, false, None)
    };
    let pubkey_hex = pubkey.to_hex();
    let pubkey_bytes = pubkey.serialize().to_vec();
    let parent_channel_id = auth_msg.parent_channel_id;

    // owner_pubkey is Some for NIP-AA virtual members; persisted in AudioPeer
    // for the full session lifetime for audit, quota aggregation, and future
    // owner-scoped enumeration/termination.
    if let Some(ref owner) = owner_pubkey {
        debug!(
            channel_id = %channel_id,
            agent = %pubkey_hex,
            owner = %owner.to_hex(),
            "audio session: NIP-AA owner retained for session"
        );
    }

    // ── Relay membership gate (NIP-43) ────────────────────────────────────────
    // NIP-AA virtual members already passed the relay membership check inside
    // verify_nip_aa (Step 5 — owner is an active member). Skip the direct gate
    // for them; only check direct membership for non-virtual-member connections.
    if !is_nip_aa_virtual
        && crate::api::relay_members::enforce_relay_membership(&state, &pubkey.serialize())
            .await
            .is_err()
    {
        warn!(channel_id = %channel_id, pubkey = %pubkey_hex, "audio: relay membership denied");
        let _ = ws_send
            .send(WsMessage::Text(
                serde_json::json!({"type": "error", "message": "restricted: not a relay member"})
                    .to_string()
                    .into(),
            ))
            .await;
        return;
    }

    // ── Step 3: membership check / auto-add ───────────────────────────────────
    if let Err(e) = ensure_membership(&state, channel_id, &pubkey_bytes, parent_channel_id).await {
        warn!(channel_id = %channel_id, pubkey = %pubkey_hex, "audio membership denied: {e}");
        let _ = ws_send
            .send(WsMessage::Text(
                serde_json::json!({"type":"error","message":"not a member"})
                    .to_string()
                    .into(),
            ))
            .await;
        return;
    }

    // ── Step 4: join room ─────────────────────────────────────────────────────
    let room = state.audio_rooms.get_or_create(channel_id);

    // Re-check archived status after obtaining the room. This closes the
    // cross-boundary race: a joiner that passed ensure_membership before
    // the last peer archived the channel could get a fresh room via
    // get_or_create (the old room was already cleaned up). This DB check
    // catches that case. The room-level ended flag (checked inside add_peer)
    // handles the same-room case.
    match state.db.get_channel(channel_id).await {
        Ok(ch) if ch.archived_at.is_some() => {
            debug!(channel_id = %channel_id, "channel archived before room join");
            let _ = ws_send
                .send(WsMessage::Text(
                    serde_json::json!({"type":"error","message":"huddle has ended"})
                        .to_string()
                        .into(),
                ))
                .await;
            state.audio_rooms.cleanup_if_empty(channel_id);
            return;
        }
        Err(e) => {
            warn!(channel_id = %channel_id, "pre-join channel check failed (fail-closed): {e}");
            state.audio_rooms.cleanup_if_empty(channel_id);
            return;
        }
        Ok(_) => {} // Channel exists and is not archived — proceed.
    }

    let (peer_id, peer_index, audio_rx, peer_ctrl_rx) = match room.add_peer(
        pubkey_hex.clone(),
        owner_pubkey.as_ref().map(|pk| pk.to_hex()),
    ) {
        Some(v) => v,
        None => {
            warn!(channel_id = %channel_id, "audio room full (255 peers exhausted)");
            let _ = ws_send
                .send(WsMessage::Text(
                    serde_json::json!({"type":"error","code":"room_full","message":"peer index space exhausted"})
                        .to_string().into(),
                ))
                .await;
            return;
        }
    };

    info!(
        channel_id = %channel_id,
        pubkey = %pubkey_hex,
        peer_index,
        "audio peer joined"
    );

    // ── Register in ConnectionManager ─────────────────────────────────────────
    // Audio connections are registered in the shared ConnectionManager so that:
    //   1. `connection_ids_for_owner()` sees NIP-AA audio sessions, enabling
    //      owner-scoped termination when an owner is removed from the relay.
    //   2. `connection_ids_for_pubkey()` can enumerate all live sessions for
    //      a given agent pubkey, including audio-only connections.
    //
    // We register using `peer_id` as the connection ID (already a UUIDv4) and
    // `ctrl_tx` as the outbound sender so that `conn_manager.send_to()` routes
    // control messages to the priority channel (matching the main WS handler).
    //
    // Audio connections carry no Nostr subscriptions — the empty map is correct.
    //
    // Scope intersection note: NIP-AA §5 grants virtual members read/write
    // scopes but not admin. The audio handler only relays binary frames and
    // emits relay-signed lifecycle events (kinds 48101–48103) — it never
    // publishes events on behalf of the agent. Scope enforcement is therefore
    // N/A here; it applies in the main WebSocket event handler.
    let (ctrl_tx, ctrl_rx) = mpsc::channel::<WsMessage>(8);
    let cancel = CancellationToken::new();
    let conn_backpressure = Arc::new(AtomicU8::new(0));
    let conn_subscriptions = Arc::new(Mutex::new(HashMap::<String, Vec<Filter>>::new()));

    state.conn_manager.register(
        peer_id,
        ctrl_tx.clone(),
        cancel.clone(),
        Arc::clone(&conn_backpressure),
        conn_subscriptions,
    );
    state
        .conn_manager
        .set_authenticated_pubkey(peer_id, pubkey_bytes.clone());
    if let Some(ref owner) = owner_pubkey {
        state
            .conn_manager
            .set_owner_pubkey(peer_id, owner.serialize().to_vec());
    }

    // ── Step 5: broadcast joined + send welcome ───────────────────────────────
    let peers_snapshot: Vec<serde_json::Value> = room
        .peer_pubkeys()
        .into_iter()
        .map(|(pk, idx)| serde_json::json!({"pubkey": pk, "peer_index": idx}))
        .collect();

    let joined_msg = serde_json::json!({
        "type": "joined",
        "pubkey": pubkey_hex,
        "peer_index": peer_index,
        "peers": peers_snapshot,
    })
    .to_string();

    room.broadcast_control(joined_msg);

    // ── Step 6: emit kind:48101 (PARTICIPANT_JOINED) ──────────────────────────
    let parent_id_for_event = parent_channel_id.unwrap_or(channel_id);
    emit_participant_event(
        &state,
        Kind::Custom(48101),
        channel_id,
        parent_id_for_event,
        &pubkey_hex,
    )
    .await;

    // ── Step 7: spawn send + heartbeat loops ──────────────────────────────────
    // cancel, ctrl_tx, and ctrl_rx were created above during ConnectionManager
    // registration. data_tx/data_rx are audio-only and not needed for conn_manager.
    let missed_pongs = Arc::new(AtomicU8::new(0));

    // Dual-channel pattern (matches connection.rs): data channel for audio,
    // control channel for Ping/Pong/Close/control JSON with priority drain.
    let (data_tx, data_rx) = mpsc::channel::<WsMessage>(16);

    let send_cancel = cancel.child_token();
    let send_task = tokio::spawn(send_loop(ws_send, data_rx, ctrl_rx, send_cancel));

    let hb_cancel = cancel.clone();
    let hb_missed = Arc::clone(&missed_pongs);
    let heartbeat_task = tokio::spawn(heartbeat_loop(ctrl_tx.clone(), hb_missed, hb_cancel));

    // ── Step 8: audio forward loop (room channels → WS send channels) ────────
    let fwd_cancel = cancel.child_token();
    let forward_task = tokio::spawn(audio_forward_loop(
        audio_rx,
        peer_ctrl_rx,
        data_tx,
        ctrl_tx.clone(),
        fwd_cancel,
    ));

    // ── Step 9: recv loop (blocks until disconnect) ───────────────────────────
    recv_loop(
        ws_recv,
        Arc::clone(&room),
        peer_id,
        ctrl_tx,
        Arc::clone(&missed_pongs),
        cancel.clone(),
    )
    .await;

    // ── Cleanup ───────────────────────────────────────────────────────────────
    cancel.cancel();
    let _ = send_task.await;
    let _ = heartbeat_task.await;
    let _ = forward_task.await;

    // Deregister from ConnectionManager — mirrors the main WS handler cleanup.
    // This removes the audio session from owner-scoped and pubkey-scoped lookups.
    state.conn_manager.deregister(peer_id);

    // Atomic remove + end check: remove_peer_and_check_ended holds the
    // AdmissionGuard lock across index recycling AND the is_empty + ended=true
    // check. This is the SAME lock that add_peer holds across its ended check
    // + insert. So they are mutually exclusive — no concurrent add_peer can
    // succeed between the removal and the ended flag being set.
    let (_, should_auto_end) = room
        .remove_peer_and_check_ended(peer_id)
        .unwrap_or((peer_index, false));

    let left_msg = serde_json::json!({
        "type": "left",
        "pubkey": pubkey_hex,
        "peer_index": peer_index,
    })
    .to_string();
    room.broadcast_control(left_msg);

    emit_participant_event(
        &state,
        Kind::Custom(48102),
        channel_id,
        parent_id_for_event,
        &pubkey_hex,
    )
    .await;

    if should_auto_end {
        info!(channel_id = %channel_id, "audio room empty — auto-ending huddle");

        match state.db.archive_channel(channel_id).await {
            Err(e) => {
                warn!(channel_id = %channel_id, "auto-archive failed, huddle stays alive: {e}");
                room.clear_ended();
            }
            Ok(()) => {
                state.audio_rooms.cleanup_if_empty(channel_id);

                emit_participant_event(
                    &state,
                    Kind::Custom(48103),
                    channel_id,
                    parent_id_for_event,
                    &pubkey_hex,
                )
                .await;
            }
        }
    } else {
        state.audio_rooms.cleanup_if_empty(channel_id);
    }

    info!(
        channel_id = %channel_id,
        pubkey = %pubkey_hex,
        "audio peer left"
    );
}

// ── Recv loop ─────────────────────────────────────────────────────────────────

async fn recv_loop(
    mut ws_recv: futures_util::stream::SplitStream<WebSocket>,
    room: Arc<crate::audio::room::Room>,
    peer_id: Uuid,
    ctrl_tx: mpsc::Sender<WsMessage>,
    missed_pongs: Arc<AtomicU8>,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => break,
            msg = ws_recv.next() => {
                match msg {
                    Some(Ok(WsMessage::Binary(data))) => {
                        if data.len() > MAX_AUDIO_FRAME_BYTES {
                            warn!(peer_id = %peer_id, bytes = data.len(), "audio frame too large — dropping");
                            continue;
                        }
                        room.broadcast_frame(peer_id, data);
                    }
                    Some(Ok(WsMessage::Text(text))) => {
                        if text.len() > MAX_TEXT_FRAME_BYTES {
                            warn!(peer_id = %peer_id, bytes = text.len(), "control text frame too large — dropping");
                            continue;
                        }
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                            if v.get("type").and_then(|t| t.as_str()) == Some("leave") {
                                break;
                            }
                        }
                    }
                    Some(Ok(WsMessage::Pong(_))) => {
                        missed_pongs.store(0, Ordering::Relaxed);
                    }
                    Some(Ok(WsMessage::Ping(data))) => {
                        // Pong goes through the control channel — priority delivery.
                        let _ = ctrl_tx.try_send(WsMessage::Pong(data));
                    }
                    Some(Ok(WsMessage::Close(_))) | None => break,
                    Some(Err(e)) => {
                        debug!(peer_id = %peer_id, "ws error: {e}");
                        break;
                    }
                }
            }
        }
    }
}

// ── Send loop ─────────────────────────────────────────────────────────────────

/// Outbound send loop with control-frame priority (matches connection.rs pattern).
///
/// Control frames (Ping, Pong, Close, control JSON) are drained first on every
/// iteration, so heartbeat pings are never starved by audio backpressure.
async fn send_loop(
    mut ws_send: futures_util::stream::SplitSink<WebSocket, WsMessage>,
    mut data_rx: mpsc::Receiver<WsMessage>,
    mut ctrl_rx: mpsc::Receiver<WsMessage>,
    cancel: CancellationToken,
) {
    loop {
        // Priority: drain all pending control frames before data.
        while let Ok(ctrl_msg) = ctrl_rx.try_recv() {
            if ws_send.send(ctrl_msg).await.is_err() {
                return;
            }
        }

        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                let _ = ws_send.send(WsMessage::Close(None)).await;
                break;
            }
            Some(ctrl_msg) = ctrl_rx.recv() => {
                if ws_send.send(ctrl_msg).await.is_err() { break; }
            }
            Some(msg) = data_rx.recv() => {
                if ws_send.send(msg).await.is_err() { break; }
            }
        }
    }
}

// ── Audio forward loop ────────────────────────────────────────────────────────
// Bridges the room's mpsc channel to the WS send channel.

/// Bridges room per-peer channels → WS send channels.
/// Audio frames (from room audio_rx) go to data_tx.
/// Control messages (from room ctrl_rx) go to ws ctrl_tx (priority path).
/// Two separate room channels ensure control is never starved by audio backpressure.
async fn audio_forward_loop(
    mut audio_rx: mpsc::Receiver<Bytes>,
    mut peer_ctrl_rx: mpsc::Receiver<PeerCtrl>,
    data_tx: mpsc::Sender<WsMessage>,
    ctrl_tx: mpsc::Sender<WsMessage>,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => break,
            // Control messages get priority over audio in the select.
            msg = peer_ctrl_rx.recv() => {
                match msg {
                    Some(PeerCtrl::Json(json)) => {
                        let _ = ctrl_tx.try_send(WsMessage::Text(json.into()));
                    }
                    Some(PeerCtrl::Close) | None => break,
                }
            }
            frame = audio_rx.recv() => {
                match frame {
                    Some(bytes) => {
                        let _ = data_tx.try_send(WsMessage::Binary(bytes));
                    }
                    None => break,
                }
            }
        }
    }
}

// ── Heartbeat loop ────────────────────────────────────────────────────────────

async fn heartbeat_loop(
    ws_tx: mpsc::Sender<WsMessage>,
    missed_pongs: Arc<AtomicU8>,
    cancel: CancellationToken,
) {
    let mut interval = tokio::time::interval(HEARTBEAT_INTERVAL);
    loop {
        tokio::select! {
            _ = interval.tick() => {
                // fetch_add returns the previous value; +1 gives the current count.
                let missed = missed_pongs.fetch_add(1, Ordering::Relaxed) + 1;
                if missed >= MAX_MISSED_PONGS {
                    warn!("audio: {missed} missed pongs — closing connection");
                    cancel.cancel();
                    break;
                }
                if ws_tx.try_send(WsMessage::Ping(axum::body::Bytes::new())).is_err() {
                    cancel.cancel();
                    break;
                }
            }
            _ = cancel.cancelled() => break,
        }
    }
}

// ── Membership helper ─────────────────────────────────────────────────────────

async fn ensure_membership(
    state: &AppState,
    channel_id: Uuid,
    pubkey_bytes: &[u8],
    parent_channel_id: Option<Uuid>,
) -> Result<(), String> {
    // Load channel first — reject archived channels before any membership check.
    // This ensures auto-ended huddles can't be rejoined by existing members.
    let channel = state
        .db
        .get_channel(channel_id)
        .await
        .map_err(|e| format!("db error: {e}"))?;

    if channel.archived_at.is_some() {
        return Err("channel is archived".into());
    }

    // Fast path: already a member.
    let is_member = state
        .is_member_cached(channel_id, pubkey_bytes)
        .await
        .map_err(|e| format!("db error: {e}"))?;

    if is_member {
        return Ok(());
    }

    if channel.visibility == "open" {
        return Ok(());
    }

    // Auto-add path: private ephemeral channel + caller is member of parent.
    //
    // TODO(security): parent_channel_id is client-supplied and unverified.
    // We don't confirm it's the *actual* parent of this ephemeral channel.
    // Security relies on the ephemeral UUID being unguessable (UUIDv4) and
    // only discoverable via the kind:48100 event in the real parent channel
    // — which requires parent membership. A future hardening pass should
    // verify the parent→ephemeral linkage by checking that a kind:48100
    // event exists in the claimed parent channel referencing this channel ID.
    if channel.ttl_seconds.is_some() {
        if let Some(parent_id) = parent_channel_id {
            let parent_member = state
                .is_member_cached(parent_id, pubkey_bytes)
                .await
                .map_err(|e| format!("db error: {e}"))?;

            if parent_member {
                state
                    .db
                    .add_member(
                        channel_id,
                        pubkey_bytes,
                        MemberRole::Member,
                        Some(&channel.created_by),
                    )
                    .await
                    .map_err(|e| format!("auto-add failed: {e}"))?;
                state.invalidate_membership(channel_id, pubkey_bytes);

                return Ok(());
            }
        }
    }

    Err("not a member".into())
}

// ── Lifecycle event helper ────────────────────────────────────────────────────

async fn emit_participant_event(
    state: &AppState,
    kind: Kind,
    channel_id: Uuid,
    parent_channel_id: Uuid,
    participant_pubkey: &str,
) {
    let content = serde_json::json!({"ephemeral_channel_id": channel_id.to_string()}).to_string();

    let h_tag = match Tag::parse(&["h", &parent_channel_id.to_string()]) {
        Ok(t) => t,
        Err(e) => {
            warn!("audio: failed to parse h tag: {e}");
            return;
        }
    };
    let p_tag = match Tag::parse(&["p", participant_pubkey]) {
        Ok(t) => t,
        Err(e) => {
            warn!("audio: failed to parse p tag: {e}");
            return;
        }
    };
    let tags = vec![h_tag, p_tag];

    let event = match EventBuilder::new(kind, content, tags).sign_with_keys(&state.relay_keypair) {
        Ok(e) => e,
        Err(e) => {
            warn!("audio: failed to sign lifecycle event: {e}");
            return;
        }
    };

    let event_id_hex = event.id.to_hex();

    // 1. Persist to DB so late-joining clients can reconstruct huddle state
    //    from historical queries. Without this, lifecycle events only exist
    //    for the duration of the Redis pub/sub delivery and are lost forever.
    let stored = match state.db.insert_event(&event, Some(parent_channel_id)).await {
        Ok((stored, true)) => stored,
        Ok((_, false)) => {
            // Duplicate — already persisted (e.g. concurrent emit). Skip fan-out
            // to avoid double-delivery, matching the side_effects.rs pattern.
            debug!(
                event_id = %event_id_hex,
                channel_id = %parent_channel_id,
                "audio lifecycle event already persisted — skipping fan-out"
            );
            return;
        }
        Err(e) => {
            // DB failure during disconnect cleanup. Still broadcast so live
            // subscribers see the leave/end event immediately — suppressing it
            // would leave connected clients stale. Late joiners will have an
            // inconsistent view until the next huddle lifecycle event lands.
            warn!(
                event_id = %event_id_hex,
                channel_id = %parent_channel_id,
                kind = %event.kind.as_u16(),
                "audio: failed to persist lifecycle event: {e}"
            );
            StoredEvent::new(event.clone(), Some(parent_channel_id))
        }
    };

    // 2. Mark as locally-published before Redis broadcast to prevent
    //    double-delivery when the event echoes back through the subscriber loop.
    state.mark_local_event(&event.id);

    // 3. Local fan-out to WS subscribers on this node (same pattern as
    //    dispatch_persistent_event in the ingest handler).
    let matches = state.sub_registry.fan_out(&stored);
    if !matches.is_empty() {
        let event_json = serde_json::to_string(&event)
            .expect("nostr::Event serialization is infallible for well-formed events");
        let mut drop_count = 0u32;
        for (target_conn_id, sub_id) in &matches {
            let msg = format!(r#"["EVENT","{}",{}]"#, sub_id, event_json);
            if !state.conn_manager.send_to(*target_conn_id, msg) {
                drop_count += 1;
            }
        }
        if drop_count > 0 {
            warn!(
                event_id = %event_id_hex,
                drop_count,
                "audio lifecycle fan-out: {drop_count} connection(s) dropped"
            );
        }
    }

    // 4. Cross-node broadcast via Redis pub/sub.
    if let Err(e) = state.pubsub.publish_event(parent_channel_id, &event).await {
        state.local_event_ids.invalidate(&event.id.to_bytes());
        warn!(
            event_id = %event_id_hex,
            channel_id = %parent_channel_id,
            "audio: failed to publish lifecycle event: {e}"
        );
    }
}
