//! Harness-side Sprout relay client.
//!
//! Connects to the Sprout relay via NIP-01 WebSocket, authenticates via NIP-42,
//! discovers channels via REST API, and streams matching events back to the
//! harness main loop.
//!
//! This is a simplified receive-only client adapted from `sprout-mcp`'s
//! `relay_client.rs`. It does not publish events or perform queries — it only
//! subscribes and receives.
//!
//! ## Architecture
//!
//! A background tokio task owns the WebSocket stream. It:
//! - Responds to Ping frames with Pong (preventing relay disconnect on long turns)
//! - Forwards `SproutEvent`s through an `mpsc` channel
//! - Handles reconnection with `since` filters to avoid event loss
//! - Responds to mid-session AUTH challenges
//!
//! `HarnessRelay` communicates with the background task via a `RelayCommand`
//! channel. `next_event()` reads from the event receiver.

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Duration;

// ─── Named constants ──────────────────────────────────────────────────────────

/// Default capacity of the event channel from background task to harness.
/// Override with `SPROUT_ACP_EVENT_BUFFER` env var at startup.
const EVENT_CHANNEL_CAPACITY_DEFAULT: usize = 256;
/// Capacity of the command channel from harness to background task.
const CMD_CHANNEL_CAPACITY: usize = 64;

/// Read the event channel capacity from the environment, falling back to the
/// compiled-in default. Parsed once at call-site (connect time).
fn event_channel_capacity() -> usize {
    std::env::var("SPROUT_ACP_EVENT_BUFFER")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(EVENT_CHANNEL_CAPACITY_DEFAULT)
}
/// Maximum number of seen event IDs before the dedup set is cleared.
const SEEN_ID_LIMIT: usize = 12_000;
/// Seconds subtracted from `since` on resubscribe to tolerate clock skew.
const SINCE_SKEW_SECS: u64 = 5;
/// Timeout for the NIP-42 auth handshake steps.
const AUTH_TIMEOUT: Duration = Duration::from_secs(5);

use futures_util::{SinkExt, StreamExt};
use nostr::{Event, EventBuilder, Keys, Kind, Tag, Url as NostrUrl};
use serde_json::{json, Value};
use sprout_core::kind::{KIND_MEMBER_ADDED_NOTIFICATION, KIND_MEMBER_REMOVED_NOTIFICATION};
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::config::ChannelFilter;

// ── Types ─────────────────────────────────────────────────────────────────────

/// Events the harness cares about.
#[derive(Debug, Clone)]
pub struct SproutEvent {
    /// Which channel this event belongs to.
    pub channel_id: Uuid,
    /// The underlying Nostr event.
    pub event: Event,
}

/// Errors from relay operations.
#[derive(Debug, thiserror::Error)]
pub enum RelayError {
    #[error("WebSocket error: {0}")]
    WebSocket(Box<tokio_tungstenite::tungstenite::Error>),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Auth failed: {0}")]
    AuthFailed(String),

    #[error("No auth challenge received")]
    NoAuthChallenge,

    #[error("Connection closed")]
    ConnectionClosed,

    #[error("Timeout")]
    Timeout,

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Unexpected message: {0}")]
    UnexpectedMessage(String),
}

impl From<nostr::event::builder::Error> for RelayError {
    fn from(e: nostr::event::builder::Error) -> Self {
        RelayError::AuthFailed(e.to_string())
    }
}

// ── Internal relay message types ──────────────────────────────────────────────

/// A parsed NIP-01 relay message.
#[derive(Debug, Clone)]
enum RelayMessage {
    Event {
        subscription_id: String,
        event: Box<Event>,
    },
    Ok {
        event_id: String,
        accepted: bool,
        message: String,
    },
    Eose {
        subscription_id: String,
    },
    Closed {
        subscription_id: String,
        message: String,
    },
    Notice {
        message: String,
    },
    Auth {
        challenge: String,
    },
}

// ── Commands sent from HarnessRelay to the background task ───────────────────

/// Subscription ID for the global membership notification subscription.
const MEMBERSHIP_NOTIF_SUB_ID: &str = "membership-notif";

/// Commands sent from `HarnessRelay` to the background WebSocket task.
enum RelayCommand {
    /// Subscribe to a channel (sends a NIP-01 REQ) with the given filter.
    Subscribe {
        channel_id: Uuid,
        filter: ChannelFilter,
    },
    /// Unsubscribe from a channel (sends a NIP-01 CLOSE).
    #[allow(dead_code)]
    Unsubscribe { channel_id: Uuid },
    /// Reconnect to the relay (re-authenticate and resubscribe).
    Reconnect,
    /// Shut down the background task.
    Shutdown,
    /// Subscribe to global membership notifications.
    SubscribeMembership,
}

// ── WebSocket stream type alias ───────────────────────────────────────────────

type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

// ── HarnessRelay ──────────────────────────────────────────────────────────────

/// Harness-side relay client.
///
/// Connects to the Sprout relay, authenticates via NIP-42, and streams
/// matching events for subscribed channels.
///
/// A background tokio task owns the WebSocket connection and responds to
/// Ping frames, preventing disconnection during long agent turns.
pub struct HarnessRelay {
    /// Receiver for events forwarded by the background task.
    event_rx: mpsc::Receiver<Option<SproutEvent>>,
    /// Sender for commands to the background task.
    cmd_tx: mpsc::Sender<RelayCommand>,
    /// HTTP client for REST API calls.
    http: reqwest::Client,
    /// WebSocket URL of the relay.
    relay_url: String,
    /// Optional API token for Bearer auth.
    api_token: Option<String>,
    /// Keys used for NIP-42 signing.
    keys: Keys,
    /// Agent public key (hex) used as the `#p` filter on subscriptions.
    #[allow(dead_code)]
    agent_pubkey_hex: String,
    /// Handle to the background task (for clean shutdown).
    bg_handle: tokio::task::JoinHandle<()>,
}

impl HarnessRelay {
    // ── Public API ────────────────────────────────────────────────────────────

    /// Connect to relay and authenticate via NIP-42.
    pub async fn connect(
        relay_url: &str,
        keys: &Keys,
        api_token: Option<&str>,
        agent_pubkey_hex: &str,
    ) -> Result<Self, RelayError> {
        // Perform the initial connection and auth handshake.
        let (ws, _buffer) = do_connect(relay_url, keys, api_token).await?;

        let (event_tx, event_rx) = mpsc::channel::<Option<SproutEvent>>(event_channel_capacity());
        let (cmd_tx, cmd_rx) = mpsc::channel::<RelayCommand>(CMD_CHANNEL_CAPACITY);

        let bg_keys = keys.clone();
        let bg_relay_url = relay_url.to_string();
        let bg_api_token = api_token.map(|t| t.to_string());
        let bg_agent_pubkey_hex = agent_pubkey_hex.to_string();

        let bg_handle = tokio::spawn(async move {
            run_background_task(
                ws,
                event_tx,
                cmd_rx,
                bg_keys,
                bg_relay_url,
                bg_api_token,
                bg_agent_pubkey_hex,
            )
            .await;
        });

        Ok(Self {
            event_rx,
            cmd_tx,
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .connect_timeout(std::time::Duration::from_secs(5))
                .build()
                .map_err(|e| RelayError::Http(format!("failed to build HTTP client: {e}")))?,
            relay_url: relay_url.to_string(),
            api_token: api_token.map(|t| t.to_string()),
            keys: keys.clone(),
            agent_pubkey_hex: agent_pubkey_hex.to_string(),
            bg_handle,
        })
    }

    /// Discover channels the harness is a member of via `GET /api/channels`.
    pub async fn discover_channels(&self) -> Result<Vec<Uuid>, RelayError> {
        let http_url = relay_ws_to_http(&self.relay_url);
        let url = format!("{http_url}/api/channels");

        let builder = self.http.get(&url);
        let builder = apply_auth(builder, &self.api_token, &self.keys);

        let resp = builder
            .send()
            .await
            .map_err(|e| RelayError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(RelayError::Http(format!(
                "GET /api/channels returned HTTP {}",
                resp.status()
            )));
        }

        let body: Value = resp
            .json()
            .await
            .map_err(|e| RelayError::Http(e.to_string()))?;

        let channels = body
            .as_array()
            .ok_or_else(|| RelayError::Http("expected JSON array from /api/channels".into()))?;

        let mut ids = Vec::with_capacity(channels.len());
        for ch in channels {
            if let Some(id_str) = ch.get("id").and_then(|v| v.as_str()) {
                match id_str.parse::<Uuid>() {
                    Ok(uuid) => ids.push(uuid),
                    Err(e) => {
                        warn!("skipping channel with unparseable id {id_str:?}: {e}");
                    }
                }
            }
        }

        debug!("discovered {} channel(s)", ids.len());
        Ok(ids)
    }

    /// Subscribe to events in a channel using the given filter.
    ///
    /// Sends a `Subscribe` command to the background task, which issues the
    /// NIP-01 `REQ` built from `filter`. Subscription ID is `ch-<uuid>`.
    pub async fn subscribe_channel(
        &mut self,
        channel_id: Uuid,
        filter: ChannelFilter,
    ) -> Result<(), RelayError> {
        self.cmd_tx
            .send(RelayCommand::Subscribe { channel_id, filter })
            .await
            .map_err(|_| RelayError::ConnectionClosed)?;
        debug!("queued subscribe for channel {channel_id}");
        Ok(())
    }

    /// Subscribe to membership notifications for this agent.
    pub async fn subscribe_membership_notifications(&mut self) -> Result<(), RelayError> {
        self.cmd_tx
            .send(RelayCommand::SubscribeMembership)
            .await
            .map_err(|_| RelayError::ConnectionClosed)?;
        Ok(())
    }

    /// Unsubscribe from a channel.
    #[allow(dead_code)]
    pub async fn unsubscribe_channel(&mut self, channel_id: Uuid) -> Result<(), RelayError> {
        self.cmd_tx
            .send(RelayCommand::Unsubscribe { channel_id })
            .await
            .map_err(|_| RelayError::ConnectionClosed)?;
        debug!("queued unsubscribe for channel {channel_id}");
        Ok(())
    }

    /// Wait for the next event from any subscribed channel.
    ///
    /// Reads from the background task's event channel. Returns `None` on
    /// connection loss — the caller should call [`reconnect`](Self::reconnect).
    pub async fn next_event(&mut self) -> Option<SproutEvent> {
        // The background task sends `None` to signal connection loss.
        self.event_rx.recv().await.flatten()
    }

    /// Reconnect after connection loss. Instructs the background task to
    /// re-authenticate and resubscribe to all previously active channels.
    pub async fn reconnect(&mut self) -> Result<(), RelayError> {
        warn!("relay connection lost — reconnecting…");
        self.cmd_tx
            .send(RelayCommand::Reconnect)
            .await
            .map_err(|_| RelayError::ConnectionClosed)?;
        Ok(())
    }
}

impl Drop for HarnessRelay {
    fn drop(&mut self) {
        // Best-effort shutdown signal; ignore errors (task may already be done).
        let _ = self.cmd_tx.try_send(RelayCommand::Shutdown);
        self.bg_handle.abort();
    }
}

// ── Background task ───────────────────────────────────────────────────────────

/// State maintained by the background WebSocket task.
struct BgState {
    /// Active subscriptions: channel_id → subscription_id string.
    active_subscriptions: HashMap<Uuid, String>,
    /// Most recent `created_at` timestamp seen per channel (for `since` filter).
    last_seen: HashMap<Uuid, u64>,
    /// Set of event IDs seen, for deduplication.
    seen_ids: HashSet<String>,
    /// Per-channel filter used on subscribe (for resubscribe after reconnect).
    active_filters: HashMap<Uuid, ChannelFilter>,
    /// Oldest timestamp of a membership notification that was dropped due to
    /// backpressure. If set, reconnect replay must start from this timestamp
    /// (minus skew) to re-deliver the lost event. Reset on successful reconnect.
    membership_dropped_since: Option<u64>,
    /// Newest successfully-enqueued membership notification timestamp.
    /// Used as the `since` for reconnect replay when no events were dropped.
    membership_last_seen: Option<u64>,
    /// Whether the membership notification subscription is active.
    membership_sub_active: bool,
    /// Oldest dropped channel-event timestamp per channel, keyed by channel_id.
    /// Mirrors `membership_dropped_since` but for ordinary channel events.
    /// On reconnect resubscribe, `since` = min(last_seen, channel_dropped_since).
    /// Cleared per-channel after a successful resubscribe.
    channel_dropped_since: HashMap<Uuid, u64>,
}

impl BgState {
    fn new() -> Self {
        Self {
            active_subscriptions: HashMap::new(),
            last_seen: HashMap::new(),
            seen_ids: HashSet::new(),
            active_filters: HashMap::new(),
            membership_dropped_since: None,
            membership_last_seen: None,
            membership_sub_active: false,
            channel_dropped_since: HashMap::new(),
        }
    }

    /// Record a received event for dedup and `since` tracking.
    /// Returns `true` if the event is new (not a duplicate).
    fn record_event(&mut self, channel_id: Uuid, event: &Event) -> bool {
        let id_hex = event.id.to_hex();

        // Deduplicate.
        if !self.seen_ids.insert(id_hex) {
            return false;
        }

        // Bound seen_ids to prevent unbounded memory growth.
        if self.seen_ids.len() > SEEN_ID_LIMIT {
            // HashSet has no ordering, so we clear and re-insert the current
            // event to avoid a false-negative dedup gap for this event.
            let current_id = event.id.to_hex();
            self.seen_ids.clear();
            self.seen_ids.insert(current_id);
        }

        // Update last_seen timestamp.
        let ts = event.created_at.as_u64();
        self.last_seen
            .entry(channel_id)
            .and_modify(|t| *t = (*t).max(ts))
            .or_insert(ts);

        true
    }
}

/// The main background task loop.
///
/// Owns the WebSocket stream, responds to Pings, forwards events, and handles
/// reconnection.
async fn run_background_task(
    mut ws: WsStream,
    event_tx: mpsc::Sender<Option<SproutEvent>>,
    mut cmd_rx: mpsc::Receiver<RelayCommand>,
    keys: Keys,
    relay_url: String,
    api_token: Option<String>,
    agent_pubkey_hex: String,
) {
    let mut state = BgState::new();

    loop {
        tokio::select! {
            // ── Incoming WebSocket message ────────────────────────────────────
            raw = ws.next() => {
                match raw {
                    Some(Ok(msg)) => {
                        if !handle_ws_message(
                            msg,
                            &mut ws,
                            &event_tx,
                            &mut state,
                            &keys,
                            &relay_url,
                            api_token.as_deref(),
                        )
                        .await
                        {
                            // handle_ws_message returns false on connection loss.
                            // Signal the caller, then attempt autonomous reconnect.
                            let _ = event_tx.send(None).await;
                            let reconnected = try_autonomous_reconnect(
                                &mut ws,
                                &mut state,
                                &keys,
                                &relay_url,
                                api_token.as_deref(),
                                &agent_pubkey_hex,
                            )
                            .await;
                            if !reconnected {
                                wait_for_reconnect(
                                    &mut ws,
                                    &mut cmd_rx,
                                    &mut state,
                                    &keys,
                                    &relay_url,
                                    api_token.as_deref(),
                                    &agent_pubkey_hex,
                                    false,
                                )
                                .await;
                            }
                        }
                    }
                    Some(Err(e)) => {
                        warn!("WebSocket error in background task: {e}");
                        let _ = event_tx.send(None).await;
                        let reconnected = try_autonomous_reconnect(
                            &mut ws,
                            &mut state,
                            &keys,
                            &relay_url,
                            api_token.as_deref(),
                            &agent_pubkey_hex,
                        )
                        .await;
                        if !reconnected {
                            wait_for_reconnect(
                                &mut ws,
                                &mut cmd_rx,
                                &mut state,
                                &keys,
                                &relay_url,
                                api_token.as_deref(),
                                &agent_pubkey_hex,
                                false,
                            )
                            .await;
                        }
                    }
                    None => {
                        debug!("WebSocket stream ended");
                        let _ = event_tx.send(None).await;
                        let reconnected = try_autonomous_reconnect(
                            &mut ws,
                            &mut state,
                            &keys,
                            &relay_url,
                            api_token.as_deref(),
                            &agent_pubkey_hex,
                        )
                        .await;
                        if !reconnected {
                            wait_for_reconnect(
                                &mut ws,
                                &mut cmd_rx,
                                &mut state,
                                &keys,
                                &relay_url,
                                api_token.as_deref(),
                                &agent_pubkey_hex,
                                false,
                            )
                            .await;
                        }
                    }
                }
            }

            // ── Command from HarnessRelay ─────────────────────────────────────
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(RelayCommand::Subscribe { channel_id, filter }) => {
                        send_subscribe(&mut ws, &state, channel_id, &agent_pubkey_hex, None, &filter).await;
                        state.active_subscriptions.insert(channel_id, channel_sub_id(channel_id));
                        state.active_filters.insert(channel_id, filter);
                    }
                    Some(RelayCommand::Unsubscribe { channel_id }) => {
                        if let Some(sub_id) = state.active_subscriptions.remove(&channel_id) {
                            let msg = json!(["CLOSE", sub_id]);
                            if let Ok(text) = serde_json::to_string(&msg) {
                                let _ = ws.send(Message::Text(text.into())).await;
                            }
                            debug!("unsubscribed from channel {channel_id}");
                        }
                    }
                    Some(RelayCommand::SubscribeMembership) => {
                        let _ =
                            send_membership_subscribe(&mut ws, &agent_pubkey_hex, None).await;
                        state.membership_sub_active = true;
                        // Seed the watermark so reconnect replays from this point
                        // rather than falling back to since=now (which could miss
                        // notifications during the reconnect gap).
                        if state.membership_last_seen.is_none() {
                            state.membership_last_seen = Some(
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs(),
                            );
                        }
                    }
                    Some(RelayCommand::Reconnect) => {
                        // Reconnect command already consumed — skip the drain loop.
                        wait_for_reconnect(
                            &mut ws,
                            &mut cmd_rx,
                            &mut state,
                            &keys,
                            &relay_url,
                            api_token.as_deref(),
                            &agent_pubkey_hex,
                            true, // skip_drain: command already consumed
                        )
                        .await;
                    }
                    Some(RelayCommand::Shutdown) | None => {
                        debug!("background task shutting down");
                        return;
                    }
                }
            }
        }
    }
}

/// Handle a single WebSocket message in the background task.
///
/// Returns `false` if the connection has been lost (Close frame or unrecoverable
/// error), `true` otherwise.
async fn handle_ws_message(
    msg: Message,
    ws: &mut WsStream,
    event_tx: &mpsc::Sender<Option<SproutEvent>>,
    state: &mut BgState,
    keys: &Keys,
    relay_url: &str,
    api_token: Option<&str>,
) -> bool {
    match msg {
        Message::Text(text) => {
            let relay_msg = match parse_relay_message(&text) {
                Ok(m) => m,
                Err(e) => {
                    warn!("failed to parse relay message: {e} — raw: {text}");
                    return true;
                }
            };

            match relay_msg {
                RelayMessage::Event {
                    subscription_id,
                    event,
                } => {
                    if subscription_id == MEMBERSHIP_NOTIF_SUB_ID {
                        // Membership notification — extract channel UUID from h tag.
                        let channel_uuid = match extract_h_tag_uuid(&event) {
                            Some(uuid) => uuid,
                            None => {
                                warn!("membership notification missing h tag — dropping");
                                return true;
                            }
                        };
                        let ts = event.created_at.as_u64();
                        let sprout_event = SproutEvent {
                            channel_id: channel_uuid,
                            event: *event,
                        };
                        match event_tx.try_send(Some(sprout_event)) {
                            Ok(()) => {
                                state.membership_last_seen =
                                    Some(state.membership_last_seen.unwrap_or(0).max(ts));
                            }
                            Err(mpsc::error::TrySendError::Full(_)) => {
                                // Track the oldest dropped timestamp so reconnect
                                // replay starts early enough to re-deliver it.
                                state.membership_dropped_since =
                                    Some(state.membership_dropped_since.map_or(ts, |d| d.min(ts)));
                                warn!(
                                    channel_id = %channel_uuid,
                                    ts,
                                    "membership notification dropped (backpressure) — will replay from {ts} on reconnect"
                                );
                            }
                            Err(mpsc::error::TrySendError::Closed(_)) => return false,
                        }
                    } else if let Some(channel_id) = channel_id_from_sub_id(&subscription_id) {
                        let ts = event.created_at.as_u64();
                        if state.record_event(channel_id, &event) {
                            let sprout_event = SproutEvent {
                                channel_id,
                                event: *event,
                            };
                            match event_tx.try_send(Some(sprout_event)) {
                                Ok(()) => {}
                                Err(mpsc::error::TrySendError::Full(_)) => {
                                    // Track the oldest dropped timestamp so reconnect
                                    // replay starts early enough to re-deliver it.
                                    state
                                        .channel_dropped_since
                                        .entry(channel_id)
                                        .and_modify(|d| *d = (*d).min(ts))
                                        .or_insert(ts);
                                    warn!(
                                        channel_id = %channel_id,
                                        ts,
                                        "event channel full — dropping event for channel {channel_id} — will replay from {ts} on reconnect"
                                    );
                                }
                                Err(mpsc::error::TrySendError::Closed(_)) => {
                                    // Receiver dropped — shut down.
                                    return false;
                                }
                            }
                        } else {
                            debug!("dropping duplicate event for channel {channel_id}");
                        }
                    } else {
                        warn!("received EVENT for unknown subscription {subscription_id}");
                    }
                }
                RelayMessage::Eose { subscription_id } => {
                    debug!("EOSE for subscription {subscription_id}");
                }
                RelayMessage::Notice { message } => {
                    // Fix 4: NOTICE at warn level.
                    tracing::warn!("relay NOTICE: {message}");
                }
                RelayMessage::Closed {
                    subscription_id,
                    message,
                } => {
                    warn!("subscription {subscription_id} closed by relay: {message}");
                }
                RelayMessage::Auth { challenge } => {
                    // Fix 5: Handle mid-session AUTH challenge by re-authenticating.
                    debug!("received mid-session AUTH challenge — re-authenticating");
                    if let Err(e) =
                        send_auth_response(ws, &challenge, relay_url, keys, api_token).await
                    {
                        warn!("failed to respond to mid-session AUTH challenge: {e}");
                    }
                }
                RelayMessage::Ok {
                    event_id,
                    accepted,
                    message,
                } => {
                    debug!("OK for event {event_id}: accepted={accepted} message={message}");
                }
            }
            true
        }
        Message::Ping(data) => {
            if let Err(e) = ws.send(Message::Pong(data)).await {
                warn!("failed to send pong: {e}");
                return false;
            }
            true
        }
        Message::Close(_) => {
            debug!("relay sent Close frame");
            false
        }
        // Binary, Pong, Frame — ignore
        _ => true,
    }
}

/// Attempt autonomous reconnect on socket loss — 3 attempts with 1s→2s→4s backoff.
///
/// If any attempt succeeds, resubscribes all active channels and membership
/// notifications (same logic as `wait_for_reconnect`) and returns `true`.
/// If all 3 attempts fail, returns `false` so the caller can fall back to
/// `wait_for_reconnect` (which blocks until the caller sends a `Reconnect`).
#[allow(clippy::too_many_arguments)]
async fn try_autonomous_reconnect(
    ws: &mut WsStream,
    state: &mut BgState,
    keys: &Keys,
    relay_url: &str,
    api_token: Option<&str>,
    agent_pubkey_hex: &str,
) -> bool {
    let backoffs = [
        Duration::from_secs(1),
        Duration::from_secs(2),
        Duration::from_secs(4),
    ];

    for (attempt, delay) in backoffs.iter().enumerate() {
        info!(
            "autonomous reconnect attempt {}/{} to {relay_url}…",
            attempt + 1,
            backoffs.len()
        );
        match do_connect(relay_url, keys, api_token).await {
            Ok((new_ws, _buffer)) => {
                *ws = new_ws;
                info!("autonomous reconnect succeeded (attempt {})", attempt + 1);

                // Resubscribe channels with since = min(last_seen, channel_dropped_since).
                let channels: Vec<Uuid> = state.active_subscriptions.keys().copied().collect();
                if !channels.is_empty() {
                    info!("resubscribing to {} channel(s) after autonomous reconnect", channels.len());
                    for channel_id in channels {
                        let last_seen = state.last_seen.get(&channel_id).copied();
                        let dropped = state.channel_dropped_since.get(&channel_id).copied();
                        let since = match (last_seen, dropped) {
                            (Some(l), Some(d)) => Some(l.min(d)),
                            (Some(l), None) => Some(l),
                            (None, Some(d)) => Some(d),
                            (None, None) => None,
                        };
                        let filter = state.active_filters.get(&channel_id).cloned().unwrap_or(
                            ChannelFilter { kinds: None, require_mention: false },
                        );
                        send_subscribe(ws, state, channel_id, agent_pubkey_hex, since, &filter).await;
                        state.channel_dropped_since.remove(&channel_id);
                    }
                }

                // Resubscribe membership notifications.
                if state.membership_sub_active {
                    let replay_since = match (state.membership_dropped_since, state.membership_last_seen) {
                        (Some(d), Some(l)) => Some(d.min(l)),
                        (Some(d), None) => Some(d),
                        (None, Some(l)) => Some(l),
                        (None, None) => None,
                    };
                    let sent = send_membership_subscribe(ws, agent_pubkey_hex, replay_since).await;
                    if sent {
                        state.membership_dropped_since = None;
                    }
                }

                return true;
            }
            Err(e) => {
                warn!(
                    "autonomous reconnect attempt {} failed: {e} — {}",
                    attempt + 1,
                    if attempt + 1 < backoffs.len() {
                        format!("retrying in {}s", delay.as_secs())
                    } else {
                        "falling back to caller-driven reconnect".to_string()
                    }
                );
                tokio::time::sleep(*delay).await;
            }
        }
    }

    false
}

/// Attempt reconnection with exponential backoff. Resubscribes all active
/// channels with `since` filters on success.
///
/// If `skip_drain` is `false`, drains the command channel until a `Reconnect`
/// command arrives (used when called from the WS-error path where the caller
/// hasn't sent Reconnect yet). If `true`, skips the drain and reconnects
/// immediately (used when called from the `RelayCommand::Reconnect` arm where
/// the command was already consumed).
#[allow(clippy::too_many_arguments)]
async fn wait_for_reconnect(
    ws: &mut WsStream,
    cmd_rx: &mut mpsc::Receiver<RelayCommand>,
    state: &mut BgState,
    keys: &Keys,
    relay_url: &str,
    api_token: Option<&str>,
    agent_pubkey_hex: &str,
    skip_drain: bool,
) {
    if !skip_drain {
        // Drain commands until we get Reconnect (or Shutdown).
        loop {
            match cmd_rx.recv().await {
                Some(RelayCommand::Reconnect) => break,
                Some(RelayCommand::Shutdown) | None => return,
                // Apply Subscribe/Unsubscribe to state so reconnect reflects
                // the latest caller intent (not just pre-disconnect state).
                Some(RelayCommand::Subscribe { channel_id, filter }) => {
                    state
                        .active_subscriptions
                        .insert(channel_id, channel_sub_id(channel_id));
                    state.active_filters.insert(channel_id, filter);
                }
                Some(RelayCommand::Unsubscribe { channel_id }) => {
                    state.active_subscriptions.remove(&channel_id);
                    state.active_filters.remove(&channel_id);
                }
                Some(RelayCommand::SubscribeMembership) => {
                    state.membership_sub_active = true;
                }
            }
        }
    }

    // Attempt reconnection with backoff.
    let mut delay = Duration::from_secs(1);
    loop {
        info!("attempting relay reconnect to {relay_url}…");
        match do_connect(relay_url, keys, api_token).await {
            Ok((new_ws, _buffer)) => {
                *ws = new_ws;
                info!("relay reconnected to {relay_url}");

                // Resubscribe all active channels with `since` filter.
                // Use min(last_seen, channel_dropped_since) so dropped events
                // are replayed — same pattern as membership notifications.
                let channels: Vec<Uuid> = state.active_subscriptions.keys().copied().collect();
                if !channels.is_empty() {
                    info!("resubscribing to {} channel(s)", channels.len());
                    for channel_id in channels {
                        let last_seen = state.last_seen.get(&channel_id).copied();
                        let dropped = state.channel_dropped_since.get(&channel_id).copied();
                        let since = match (last_seen, dropped) {
                            (Some(l), Some(d)) => Some(l.min(d)),
                            (Some(l), None) => Some(l),
                            (None, Some(d)) => Some(d),
                            (None, None) => None,
                        };
                        let filter = state.active_filters.get(&channel_id).cloned().unwrap_or(
                            ChannelFilter {
                                kinds: None,
                                require_mention: false,
                            },
                        );
                        send_subscribe(ws, state, channel_id, agent_pubkey_hex, since, &filter)
                            .await;
                        // Clear the drop tracker now that we've resubscribed
                        // with a since that covers the dropped window.
                        state.channel_dropped_since.remove(&channel_id);
                    }
                }

                // Resubscribe to membership notifications if active.
                // Use the oldest of (dropped, last_seen) so dropped events are replayed.
                if state.membership_sub_active {
                    let replay_since =
                        match (state.membership_dropped_since, state.membership_last_seen) {
                            (Some(d), Some(l)) => Some(d.min(l)),
                            (Some(d), None) => Some(d),
                            (None, Some(l)) => Some(l),
                            (None, None) => None,
                        };
                    let sent = send_membership_subscribe(ws, agent_pubkey_hex, replay_since).await;
                    if sent {
                        // Only clear drop tracker if the REQ was actually sent —
                        // if send failed, retain it so the next reconnect retries.
                        state.membership_dropped_since = None;
                    }
                }

                return;
            }
            Err(e) => {
                warn!(
                    "relay reconnect failed: {e} — retrying in {}s",
                    delay.as_secs()
                );
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(60));
            }
        }
    }
}

/// Send a NIP-01 REQ for a channel, built from a [`ChannelFilter`].
///
/// - `kinds` is included only when `filter.kinds` is `Some`; `None` = wildcard.
/// - `#p` is included only when `filter.require_mention` is `true`.
/// - `#h` is always included (channel-scoped subscription).
/// - On first subscribe (`since` is `None`) adds `since=now` to avoid replaying
///   history. On reconnect (`since` is `Some`) subtracts [`SINCE_SKEW_SECS`].
async fn send_subscribe(
    ws: &mut WsStream,
    _state: &BgState,
    channel_id: Uuid,
    agent_pubkey_hex: &str,
    since: Option<u64>,
    filter: &ChannelFilter,
) {
    let sub_id = channel_sub_id(channel_id);

    let mut req_filter = serde_json::Map::new();

    // kinds — omit entirely for wildcard subscriptions.
    if let Some(ref kinds) = filter.kinds {
        req_filter.insert("kinds".into(), json!(kinds));
    }

    // #h — always present (channel scope).
    req_filter.insert("#h".into(), json!([channel_id.to_string()]));

    // #p — only when require_mention is true.
    if filter.require_mention {
        req_filter.insert("#p".into(), json!([agent_pubkey_hex]));
    }

    // since — on first subscribe use current time to skip history; on reconnect
    // subtract skew buffer to catch events missed during the disconnect window.
    let since_ts = match since {
        Some(ts) => ts.saturating_sub(SINCE_SKEW_SECS),
        None => std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };
    req_filter.insert("since".into(), json!(since_ts));

    let req = json!(["REQ", sub_id, Value::Object(req_filter)]);

    match serde_json::to_string(&req) {
        Ok(text) => {
            if let Err(e) = ws.send(Message::Text(text.into())).await {
                warn!("failed to send REQ for channel {channel_id}: {e}");
            } else {
                debug!(
                    "subscribed to channel {channel_id}{}",
                    if since.is_some() {
                        " (with since filter)"
                    } else {
                        " (since=now)"
                    }
                );
            }
        }
        Err(e) => {
            warn!("failed to serialize REQ for channel {channel_id}: {e}");
        }
    }
}

/// Send a NIP-01 REQ for membership notifications (kind:44100+44101, global, #p=[agent_pubkey]).
/// Returns `true` if the REQ was successfully written to the WebSocket.
async fn send_membership_subscribe(
    ws: &mut WsStream,
    agent_pubkey_hex: &str,
    since: Option<u64>,
) -> bool {
    let mut req_filter = serde_json::Map::new();
    req_filter.insert(
        "kinds".into(),
        json!([
            KIND_MEMBER_ADDED_NOTIFICATION,
            KIND_MEMBER_REMOVED_NOTIFICATION
        ]),
    );
    req_filter.insert("#p".into(), json!([agent_pubkey_hex]));

    let since_ts = match since {
        Some(ts) => ts.saturating_sub(SINCE_SKEW_SECS),
        None => std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };
    req_filter.insert("since".into(), json!(since_ts));

    let req = json!(["REQ", MEMBERSHIP_NOTIF_SUB_ID, Value::Object(req_filter)]);
    match serde_json::to_string(&req) {
        Ok(text) => match ws.send(Message::Text(text.into())).await {
            Ok(()) => {
                debug!("subscribed to membership notifications (since={since_ts})");
                true
            }
            Err(e) => {
                warn!("failed to send membership notification REQ: {e}");
                false
            }
        },
        Err(e) => {
            warn!("failed to serialize membership notification REQ: {e}");
            false
        }
    }
}

/// Extract a channel UUID from the h tag of a Nostr event.
fn extract_h_tag_uuid(event: &nostr::Event) -> Option<Uuid> {
    event.tags.iter().find_map(|tag| {
        let tag_vec = tag.as_slice();
        if tag_vec.len() >= 2 && tag_vec[0] == "h" {
            tag_vec[1].parse::<Uuid>().ok()
        } else {
            None
        }
    })
}

/// Build and send a NIP-42 AUTH response event.
async fn send_auth_response(
    ws: &mut WsStream,
    challenge: &str,
    relay_url: &str,
    keys: &Keys,
    api_token: Option<&str>,
) -> Result<(), RelayError> {
    let relay_nostr_url: NostrUrl = relay_url
        .parse()
        .map_err(|e: url::ParseError| RelayError::Http(format!("invalid relay URL: {e}")))?;

    let auth_event = if let Some(token) = api_token {
        let tags = vec![
            Tag::parse(&["relay", relay_url]).map_err(|e| RelayError::AuthFailed(e.to_string()))?,
            Tag::parse(&["challenge", challenge])
                .map_err(|e| RelayError::AuthFailed(e.to_string()))?,
            Tag::parse(&["auth_token", token])
                .map_err(|e| RelayError::AuthFailed(e.to_string()))?,
        ];
        EventBuilder::new(Kind::Authentication, "", tags).sign_with_keys(keys)?
    } else {
        EventBuilder::auth(challenge, relay_nostr_url).sign_with_keys(keys)?
    };

    let auth_msg = serde_json::to_string(&json!(["AUTH", auth_event]))?;
    ws.send(Message::Text(auth_msg.into()))
        .await
        .map_err(|e| RelayError::WebSocket(Box::new(e)))?;
    debug!("sent AUTH response for challenge");
    Ok(())
}

// ── Free functions ────────────────────────────────────────────────────────────

/// Convert a WebSocket URL to its HTTP equivalent.
///
/// `ws://host:port` → `http://host:port`
/// `wss://host:port` → `https://host:port`
/// Trailing slashes are stripped.
pub(crate) fn relay_ws_to_http(url: &str) -> String {
    url.replace("wss://", "https://")
        .replace("ws://", "http://")
        .trim_end_matches('/')
        .to_string()
}

/// Build the subscription ID for a channel: `ch-<uuid>`.
pub(crate) fn channel_sub_id(channel_id: Uuid) -> String {
    format!("ch-{channel_id}")
}

/// Extract a channel UUID from a subscription ID of the form `ch-<uuid>`.
/// Returns `None` if the format doesn't match or the UUID is invalid.
fn channel_id_from_sub_id(sub_id: &str) -> Option<Uuid> {
    sub_id
        .strip_prefix("ch-")
        .and_then(|s| s.parse::<Uuid>().ok())
}

/// Apply the appropriate auth header to a reqwest request builder.
fn apply_auth(
    builder: reqwest::RequestBuilder,
    api_token: &Option<String>,
    keys: &Keys,
) -> reqwest::RequestBuilder {
    if let Some(ref token) = api_token {
        builder.header("Authorization", format!("Bearer {token}"))
    } else {
        builder.header("X-Pubkey", keys.public_key().to_hex())
    }
}

/// Parse a raw relay text frame into a typed [`RelayMessage`].
#[allow(private_interfaces)]
pub(crate) fn parse_relay_message(text: &str) -> Result<RelayMessage, RelayError> {
    let arr: Vec<Value> = serde_json::from_str(text)?;

    let msg_type = arr
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| RelayError::UnexpectedMessage(text.to_string()))?;

    match msg_type {
        "EVENT" => {
            let sub_id = arr
                .get(1)
                .and_then(|v| v.as_str())
                .ok_or_else(|| RelayError::UnexpectedMessage(text.to_string()))?
                .to_string();
            let event: Event = serde_json::from_value(
                arr.get(2)
                    .cloned()
                    .ok_or_else(|| RelayError::UnexpectedMessage(text.to_string()))?,
            )?;
            Ok(RelayMessage::Event {
                subscription_id: sub_id,
                event: Box::new(event),
            })
        }
        "OK" => {
            let event_id = arr
                .get(1)
                .and_then(|v| v.as_str())
                .ok_or_else(|| RelayError::UnexpectedMessage(text.to_string()))?
                .to_string();
            let accepted = arr.get(2).and_then(|v| v.as_bool()).unwrap_or(false);
            let message = arr
                .get(3)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Ok(RelayMessage::Ok {
                event_id,
                accepted,
                message,
            })
        }
        "EOSE" => {
            let sub_id = arr
                .get(1)
                .and_then(|v| v.as_str())
                .ok_or_else(|| RelayError::UnexpectedMessage(text.to_string()))?
                .to_string();
            Ok(RelayMessage::Eose {
                subscription_id: sub_id,
            })
        }
        "CLOSED" => {
            let sub_id = arr
                .get(1)
                .and_then(|v| v.as_str())
                .ok_or_else(|| RelayError::UnexpectedMessage(text.to_string()))?
                .to_string();
            let message = arr
                .get(2)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Ok(RelayMessage::Closed {
                subscription_id: sub_id,
                message,
            })
        }
        "NOTICE" => {
            let message = arr
                .get(1)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Ok(RelayMessage::Notice { message })
        }
        "AUTH" => {
            let challenge = arr
                .get(1)
                .and_then(|v| v.as_str())
                .ok_or_else(|| RelayError::UnexpectedMessage(text.to_string()))?
                .to_string();
            Ok(RelayMessage::Auth { challenge })
        }
        other => Err(RelayError::UnexpectedMessage(format!(
            "unknown message type: {other}"
        ))),
    }
}

// ── Connection helpers ────────────────────────────────────────────────────────

/// Perform a single WebSocket connect + NIP-42 auth handshake.
///
/// Returns `(ws, buffer)` on success.
async fn do_connect(
    relay_url: &str,
    keys: &Keys,
    api_token: Option<&str>,
) -> Result<(WsStream, VecDeque<RelayMessage>), RelayError> {
    let parsed = relay_url
        .parse::<url::Url>()
        .map_err(|e| RelayError::Http(format!("invalid relay URL: {e}")))?;

    let (ws, _response) = connect_async(parsed.as_str())
        .await
        .map_err(|e| RelayError::WebSocket(Box::new(e)))?;
    debug!("connected to relay at {relay_url}");

    let mut ws = ws;
    let mut buffer: VecDeque<RelayMessage> = VecDeque::new();

    // ── Step 1: Wait for AUTH challenge ───────────────────────────────────
    let challenge = wait_for_auth_challenge(&mut ws, &mut buffer, AUTH_TIMEOUT).await?;

    // ── Step 2: Build and send kind:22242 auth event ──────────────────────
    send_auth_response(&mut ws, &challenge, relay_url, keys, api_token).await?;

    // ── Step 3: Wait for OK ───────────────────────────────────────────────
    let event_id = {
        // We need the event_id that was just sent. Re-derive it by signing again
        // just to get the ID — but that's wasteful. Instead, parse the last sent
        // message. Simpler: wait_for_ok accepts any OK (we just sent one event).
        // The event_id in the OK will match whatever we sent.
        // We'll accept the first OK we receive.
        let ok = wait_for_any_ok(&mut ws, &mut buffer, AUTH_TIMEOUT).await?;
        if !ok.accepted {
            return Err(RelayError::AuthFailed(ok.message));
        }
        ok.event_id
    };

    debug!("NIP-42 authentication successful (event {event_id})");
    Ok((ws, buffer))
}

/// Wait for an `AUTH` challenge from the relay, buffering any other messages.
async fn wait_for_auth_challenge(
    ws: &mut WsStream,
    buffer: &mut VecDeque<RelayMessage>,
    timeout_dur: Duration,
) -> Result<String, RelayError> {
    // Check if there's already one buffered.
    if let Some(idx) = buffer
        .iter()
        .position(|m| matches!(m, RelayMessage::Auth { .. }))
    {
        if let Some(RelayMessage::Auth { challenge }) = buffer.remove(idx) {
            return Ok(challenge);
        }
    }

    let deadline = tokio::time::Instant::now() + timeout_dur;

    loop {
        let remaining = deadline
            .checked_duration_since(tokio::time::Instant::now())
            .unwrap_or(Duration::ZERO);

        if remaining.is_zero() {
            return Err(RelayError::NoAuthChallenge);
        }

        let raw = timeout(remaining, ws.next())
            .await
            .map_err(|_| RelayError::NoAuthChallenge)?
            .ok_or(RelayError::ConnectionClosed)?
            .map_err(|e| RelayError::WebSocket(Box::new(e)))?;

        match raw {
            Message::Text(text) => {
                let msg = parse_relay_message(&text)?;
                match msg {
                    RelayMessage::Auth { challenge } => return Ok(challenge),
                    other => buffer.push_back(other),
                }
            }
            Message::Ping(data) => {
                ws.send(Message::Pong(data))
                    .await
                    .map_err(|e| RelayError::WebSocket(Box::new(e)))?;
            }
            Message::Close(_) => return Err(RelayError::ConnectionClosed),
            _ => {}
        }
    }
}

/// Response from an `OK` relay message.
struct OkResponse {
    event_id: String,
    accepted: bool,
    message: String,
}

/// Wait for the first `OK` message from the relay (used after sending AUTH).
async fn wait_for_any_ok(
    ws: &mut WsStream,
    buffer: &mut VecDeque<RelayMessage>,
    timeout_dur: Duration,
) -> Result<OkResponse, RelayError> {
    // Check if there's already one buffered.
    if let Some(idx) = buffer
        .iter()
        .position(|m| matches!(m, RelayMessage::Ok { .. }))
    {
        if let Some(RelayMessage::Ok {
            event_id,
            accepted,
            message,
        }) = buffer.remove(idx)
        {
            return Ok(OkResponse {
                event_id,
                accepted,
                message,
            });
        }
    }

    let deadline = tokio::time::Instant::now() + timeout_dur;

    loop {
        let remaining = deadline
            .checked_duration_since(tokio::time::Instant::now())
            .unwrap_or(Duration::ZERO);

        if remaining.is_zero() {
            return Err(RelayError::Timeout);
        }

        let raw = timeout(remaining, ws.next())
            .await
            .map_err(|_| RelayError::Timeout)?
            .ok_or(RelayError::ConnectionClosed)?
            .map_err(|e| RelayError::WebSocket(Box::new(e)))?;

        match raw {
            Message::Text(text) => {
                let msg = parse_relay_message(&text)?;
                match msg {
                    RelayMessage::Ok {
                        event_id,
                        accepted,
                        message,
                    } => {
                        return Ok(OkResponse {
                            event_id,
                            accepted,
                            message,
                        });
                    }
                    other => buffer.push_back(other),
                }
            }
            Message::Ping(data) => {
                ws.send(Message::Pong(data))
                    .await
                    .map_err(|e| RelayError::WebSocket(Box::new(e)))?;
            }
            Message::Close(_) => return Err(RelayError::ConnectionClosed),
            _ => {}
        }
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── relay_ws_to_http ──────────────────────────────────────────────────────

    #[test]
    fn relay_ws_to_http_plain() {
        assert_eq!(
            relay_ws_to_http("ws://localhost:3000"),
            "http://localhost:3000"
        );
    }

    #[test]
    fn relay_ws_to_http_secure() {
        assert_eq!(
            relay_ws_to_http("wss://relay.example.com"),
            "https://relay.example.com"
        );
    }

    #[test]
    fn relay_ws_to_http_strips_trailing_slash() {
        assert_eq!(
            relay_ws_to_http("ws://localhost:3000/"),
            "http://localhost:3000"
        );
    }

    #[test]
    fn relay_ws_to_http_with_path() {
        assert_eq!(
            relay_ws_to_http("wss://relay.example.com/nostr"),
            "https://relay.example.com/nostr"
        );
    }

    #[test]
    fn relay_ws_to_http_with_port_and_path() {
        assert_eq!(
            relay_ws_to_http("wss://relay.example.com:4000/ws"),
            "https://relay.example.com:4000/ws"
        );
    }

    // ── channel_sub_id ────────────────────────────────────────────────────────

    #[test]
    fn channel_sub_id_format() {
        let uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        assert_eq!(
            channel_sub_id(uuid),
            "ch-550e8400-e29b-41d4-a716-446655440000"
        );
    }

    #[test]
    fn channel_id_from_sub_id_roundtrip() {
        let uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let sub_id = channel_sub_id(uuid);
        let recovered = channel_id_from_sub_id(&sub_id).unwrap();
        assert_eq!(recovered, uuid);
    }

    #[test]
    fn channel_id_from_sub_id_invalid_prefix() {
        assert!(channel_id_from_sub_id("sub-550e8400-e29b-41d4-a716-446655440000").is_none());
    }

    #[test]
    fn channel_id_from_sub_id_invalid_uuid() {
        assert!(channel_id_from_sub_id("ch-not-a-uuid").is_none());
    }

    #[test]
    fn channel_id_from_sub_id_empty() {
        assert!(channel_id_from_sub_id("").is_none());
    }

    // ── parse_relay_message ───────────────────────────────────────────────────

    #[test]
    fn parse_ok_accepted() {
        let text = r#"["OK","abc123",true,""]"#;
        let msg = parse_relay_message(text).unwrap();
        match msg {
            RelayMessage::Ok {
                event_id,
                accepted,
                message,
            } => {
                assert_eq!(event_id, "abc123");
                assert!(accepted);
                assert_eq!(message, "");
            }
            _ => panic!("expected Ok"),
        }
    }

    #[test]
    fn parse_ok_rejected() {
        let text = r#"["OK","abc123",false,"blocked: spam"]"#;
        let msg = parse_relay_message(text).unwrap();
        match msg {
            RelayMessage::Ok {
                event_id,
                accepted,
                message,
            } => {
                assert_eq!(event_id, "abc123");
                assert!(!accepted);
                assert_eq!(message, "blocked: spam");
            }
            _ => panic!("expected Ok"),
        }
    }

    #[test]
    fn parse_eose() {
        let text = r#"["EOSE","sub-1"]"#;
        let msg = parse_relay_message(text).unwrap();
        match msg {
            RelayMessage::Eose { subscription_id } => {
                assert_eq!(subscription_id, "sub-1");
            }
            _ => panic!("expected Eose"),
        }
    }

    #[test]
    fn parse_notice() {
        let text = r#"["NOTICE","hello from relay"]"#;
        let msg = parse_relay_message(text).unwrap();
        match msg {
            RelayMessage::Notice { message } => {
                assert_eq!(message, "hello from relay");
            }
            _ => panic!("expected Notice"),
        }
    }

    #[test]
    fn parse_notice_empty() {
        let text = r#"["NOTICE"]"#;
        let msg = parse_relay_message(text).unwrap();
        match msg {
            RelayMessage::Notice { message } => {
                assert_eq!(message, "");
            }
            _ => panic!("expected Notice"),
        }
    }

    #[test]
    fn parse_auth() {
        let text = r#"["AUTH","some-challenge-string"]"#;
        let msg = parse_relay_message(text).unwrap();
        match msg {
            RelayMessage::Auth { challenge } => {
                assert_eq!(challenge, "some-challenge-string");
            }
            _ => panic!("expected Auth"),
        }
    }

    #[test]
    fn parse_closed() {
        let text = r#"["CLOSED","sub-2","error: rate-limited"]"#;
        let msg = parse_relay_message(text).unwrap();
        match msg {
            RelayMessage::Closed {
                subscription_id,
                message,
            } => {
                assert_eq!(subscription_id, "sub-2");
                assert_eq!(message, "error: rate-limited");
            }
            _ => panic!("expected Closed"),
        }
    }

    #[test]
    fn parse_closed_no_message() {
        let text = r#"["CLOSED","sub-3"]"#;
        let msg = parse_relay_message(text).unwrap();
        match msg {
            RelayMessage::Closed {
                subscription_id,
                message,
            } => {
                assert_eq!(subscription_id, "sub-3");
                assert_eq!(message, "");
            }
            _ => panic!("expected Closed"),
        }
    }

    #[test]
    fn parse_unknown_type_returns_error() {
        let text = r#"["UNKNOWN","data"]"#;
        let result = parse_relay_message(text);
        assert!(result.is_err());
        match result.unwrap_err() {
            RelayError::UnexpectedMessage(msg) => {
                assert!(msg.contains("unknown message type"));
            }
            e => panic!("expected UnexpectedMessage, got {e:?}"),
        }
    }

    #[test]
    fn parse_invalid_json_returns_error() {
        let text = "not json at all";
        let result = parse_relay_message(text);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RelayError::Json(_)));
    }

    #[test]
    fn parse_empty_array_returns_error() {
        let text = "[]";
        let result = parse_relay_message(text);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RelayError::UnexpectedMessage(_)
        ));
    }

    #[test]
    fn parse_auth_missing_challenge_returns_error() {
        let text = r#"["AUTH"]"#;
        let result = parse_relay_message(text);
        assert!(result.is_err());
    }

    #[test]
    fn parse_eose_missing_sub_id_returns_error() {
        let text = r#"["EOSE"]"#;
        let result = parse_relay_message(text);
        assert!(result.is_err());
    }

    // ── channel_sub_id subscription format ───────────────────────────────────

    #[test]
    fn subscription_id_starts_with_ch_prefix() {
        let uuid = Uuid::new_v4();
        let sub_id = channel_sub_id(uuid);
        assert!(sub_id.starts_with("ch-"));
    }

    #[test]
    fn subscription_id_contains_full_uuid() {
        let uuid = Uuid::parse_str("12345678-1234-5678-1234-567812345678").unwrap();
        let sub_id = channel_sub_id(uuid);
        assert_eq!(sub_id, "ch-12345678-1234-5678-1234-567812345678");
    }

    // ── BgState: seen_ids deduplication ──────────────────────────────────────

    /// Build a real signed Nostr event for testing BgState.
    ///
    /// Uses `custom_created_at` so tests can control the timestamp.
    /// The event ID is determined by the nostr signing process — we don't
    /// control it, but we return it so callers can use it for dedup tests.
    fn make_test_event(keys: &nostr::Keys, created_at_secs: u64) -> Event {
        let ts = nostr::Timestamp::from(created_at_secs);
        EventBuilder::new(nostr::Kind::TextNote, "test", [])
            .custom_created_at(ts)
            .sign_with_keys(keys)
            .expect("signing should succeed")
    }

    #[test]
    fn bg_state_dedup_first_event_accepted() {
        let mut state = BgState::new();
        let channel_id = Uuid::new_v4();
        let keys = nostr::Keys::generate();
        let event = make_test_event(&keys, 1_000_000);
        assert!(
            state.record_event(channel_id, &event),
            "first event should be accepted"
        );
    }

    #[test]
    fn bg_state_dedup_duplicate_rejected() {
        let mut state = BgState::new();
        let channel_id = Uuid::new_v4();
        let keys = nostr::Keys::generate();
        let event = make_test_event(&keys, 1_000_000);
        assert!(
            state.record_event(channel_id, &event),
            "first should be accepted"
        );
        assert!(
            !state.record_event(channel_id, &event),
            "duplicate should be rejected"
        );
    }

    #[test]
    fn bg_state_dedup_different_ids_both_accepted() {
        let mut state = BgState::new();
        let channel_id = Uuid::new_v4();
        // Two different keys → two different event IDs.
        let keys1 = nostr::Keys::generate();
        let keys2 = nostr::Keys::generate();
        let event1 = make_test_event(&keys1, 1_000_000);
        let event2 = make_test_event(&keys2, 1_000_001);
        assert!(state.record_event(channel_id, &event1));
        assert!(state.record_event(channel_id, &event2));
    }

    // ── BgState: last_seen tracking ───────────────────────────────────────────

    #[test]
    fn bg_state_last_seen_set_on_first_event() {
        let mut state = BgState::new();
        let channel_id = Uuid::new_v4();
        let keys = nostr::Keys::generate();
        let event = make_test_event(&keys, 1_700_000);
        state.record_event(channel_id, &event);
        assert_eq!(state.last_seen.get(&channel_id).copied(), Some(1_700_000));
    }

    #[test]
    fn bg_state_last_seen_advances_on_newer_event() {
        let mut state = BgState::new();
        let channel_id = Uuid::new_v4();
        let keys1 = nostr::Keys::generate();
        let keys2 = nostr::Keys::generate();
        let event1 = make_test_event(&keys1, 1_700_000);
        let event2 = make_test_event(&keys2, 1_800_000);
        state.record_event(channel_id, &event1);
        state.record_event(channel_id, &event2);
        assert_eq!(state.last_seen.get(&channel_id).copied(), Some(1_800_000));
    }

    #[test]
    fn bg_state_last_seen_does_not_regress_on_older_event() {
        let mut state = BgState::new();
        let channel_id = Uuid::new_v4();
        let keys1 = nostr::Keys::generate();
        let keys2 = nostr::Keys::generate();
        let event_new = make_test_event(&keys1, 1_800_000);
        let event_old = make_test_event(&keys2, 1_700_000);
        state.record_event(channel_id, &event_new);
        state.record_event(channel_id, &event_old);
        // last_seen should remain at the higher timestamp
        assert_eq!(state.last_seen.get(&channel_id).copied(), Some(1_800_000));
    }

    #[test]
    fn bg_state_last_seen_independent_per_channel() {
        let mut state = BgState::new();
        let ch1 = Uuid::new_v4();
        let ch2 = Uuid::new_v4();
        let keys1 = nostr::Keys::generate();
        let keys2 = nostr::Keys::generate();
        let event1 = make_test_event(&keys1, 1_000_000);
        let event2 = make_test_event(&keys2, 2_000_000);
        state.record_event(ch1, &event1);
        state.record_event(ch2, &event2);
        assert_eq!(state.last_seen.get(&ch1).copied(), Some(1_000_000));
        assert_eq!(state.last_seen.get(&ch2).copied(), Some(2_000_000));
    }

    #[test]
    fn bg_state_seen_ids_cleared_at_limit() {
        let mut state = BgState::new();
        let channel_id = Uuid::new_v4();

        // Pre-populate seen_ids to just below the threshold (12_000 - 1).
        // Use synthetic hex strings — we're testing the clear logic, not signing.
        for i in 0u64..11_999 {
            state.seen_ids.insert(format!("{:0>64x}", i));
        }
        assert_eq!(state.seen_ids.len(), 11_999);

        // Now insert two real events. The first will bring us to 12_000 (no
        // clear yet), the second will push us to 12_001 and trigger the clear.
        let keys = nostr::Keys::generate();
        let event1 = make_test_event(&keys, 1_000_000);
        let keys2 = nostr::Keys::generate();
        let event2 = make_test_event(&keys2, 1_000_001);

        // First insert: 12_000 entries — no clear triggered yet.
        state.record_event(channel_id, &event1);
        assert_eq!(
            state.seen_ids.len(),
            12_000,
            "should be at 12_000 before clear"
        );

        // Second insert: 12_001 entries — triggers clear, then re-inserts.
        state.record_event(channel_id, &event2);
        // After clear + re-insert of event2, seen_ids should be very small.
        assert!(
            state.seen_ids.len() < 12_000,
            "seen_ids should have been cleared, got {}",
            state.seen_ids.len()
        );
    }

    // ── Bug 5: channel_dropped_since tracking ─────────────────────────────────

    /// Test 8: channel_dropped_since records the OLDEST dropped timestamp.
    ///
    /// Simulates the backpressure path directly on BgState:
    /// - First drop at ts=1000 → entry is 1000
    /// - Second drop at ts=2000 (later) → entry stays 1000 (min)
    /// - Third drop at ts=500 (earlier) → entry updates to 500 (min)
    #[test]
    fn acp_records_channel_dropped_since_on_backpressure() {
        let mut state = BgState::new();
        let channel_id = Uuid::new_v4();

        // Simulate the backpressure path: record ts=1000.
        let ts1: u64 = 1_000;
        state
            .channel_dropped_since
            .entry(channel_id)
            .and_modify(|d| *d = (*d).min(ts1))
            .or_insert(ts1);
        assert_eq!(
            state.channel_dropped_since.get(&channel_id).copied(),
            Some(1_000),
            "first drop should record ts=1000"
        );

        // Later timestamp (2000) — entry should stay at 1000.
        let ts2: u64 = 2_000;
        state
            .channel_dropped_since
            .entry(channel_id)
            .and_modify(|d| *d = (*d).min(ts2))
            .or_insert(ts2);
        assert_eq!(
            state.channel_dropped_since.get(&channel_id).copied(),
            Some(1_000),
            "later drop should not overwrite earlier timestamp"
        );

        // Earlier timestamp (500) — entry should update to 500.
        let ts3: u64 = 500;
        state
            .channel_dropped_since
            .entry(channel_id)
            .and_modify(|d| *d = (*d).min(ts3))
            .or_insert(ts3);
        assert_eq!(
            state.channel_dropped_since.get(&channel_id).copied(),
            Some(500),
            "earlier drop should update entry to 500"
        );
    }

    /// Test 9: reconnect since filter = min(last_seen, channel_dropped_since) - SINCE_SKEW_SECS.
    ///
    /// With last_seen=1000 and channel_dropped_since=900, the effective since
    /// passed to send_subscribe should be min(1000, 900) - SINCE_SKEW_SECS = 895.
    #[test]
    fn acp_reconnect_uses_dropped_since_for_replay() {
        let mut state = BgState::new();
        let channel_id = Uuid::new_v4();

        // Set up state: last_seen=1000, channel_dropped_since=900.
        state.last_seen.insert(channel_id, 1_000);
        state.channel_dropped_since.insert(channel_id, 900);

        // Compute the since value the reconnect path would use.
        let last_seen = state.last_seen.get(&channel_id).copied();
        let dropped = state.channel_dropped_since.get(&channel_id).copied();
        let since = match (last_seen, dropped) {
            (Some(l), Some(d)) => Some(l.min(d)),
            (Some(l), None) => Some(l),
            (None, Some(d)) => Some(d),
            (None, None) => None,
        };

        // The since passed to send_subscribe (which subtracts SINCE_SKEW_SECS internally).
        assert_eq!(since, Some(900), "since should be min(1000, 900) = 900");

        // After subtracting skew (as send_subscribe does), the REQ filter value is:
        let req_since = since.unwrap().saturating_sub(SINCE_SKEW_SECS);
        assert_eq!(
            req_since,
            895,
            "REQ since filter should be 900 - {} = 895",
            SINCE_SKEW_SECS
        );

        // Simulate clearing after resubscribe.
        state.channel_dropped_since.remove(&channel_id);
        assert!(
            state.channel_dropped_since.get(&channel_id).is_none(),
            "channel_dropped_since should be cleared after resubscribe"
        );
    }
}
