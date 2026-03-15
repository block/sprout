use std::collections::HashMap;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use nostr::{Event, EventBuilder, Filter, Keys, Kind, Tag, Url};
use serde_json::{json, Value};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use tracing::{debug, warn};

// ── Timeouts ──────────────────────────────────────────────────────────────────

/// How long to wait for an OK acknowledgement after sending an event.
const SEND_EVENT_TIMEOUT: Duration = Duration::from_secs(10);
/// How long to wait for EOSE after sending a REQ.
const SUBSCRIBE_TIMEOUT: Duration = Duration::from_secs(10);
/// Capacity of the command channel.
const CMD_CHANNEL_CAPACITY: usize = 64;

// ── Public error type ─────────────────────────────────────────────────────────

/// Errors that can occur when communicating with a Sprout relay.
#[derive(Debug, Error)]
pub enum RelayClientError {
    /// A WebSocket transport error occurred.
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    /// Failed to serialize or deserialize JSON.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Failed to build a Nostr event.
    #[error("Nostr event builder error: {0}")]
    EventBuilder(String),

    /// Failed to parse a URL.
    #[error("URL parse error: {0}")]
    Url(String),

    /// A relay response was not received within the allowed time.
    #[error("Timeout waiting for relay message")]
    Timeout,

    /// The WebSocket connection was closed before the operation completed.
    #[error("Connection closed unexpectedly")]
    ConnectionClosed,

    /// The relay sent a message that was not expected in the current context.
    #[error("Unexpected relay message: {0}")]
    UnexpectedMessage(String),

    /// The relay rejected the NIP-42 authentication attempt.
    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    /// No `AUTH` challenge was received from the relay within the timeout.
    #[error("No AUTH challenge received from relay")]
    NoAuthChallenge,
}

impl From<nostr::event::builder::Error> for RelayClientError {
    fn from(e: nostr::event::builder::Error) -> Self {
        RelayClientError::EventBuilder(e.to_string())
    }
}

// ── Public relay message type ─────────────────────────────────────────────────

/// A message received from a Nostr relay.
#[derive(Debug, Clone)]
pub enum RelayMessage {
    /// An event matching an active subscription.
    Event {
        /// The subscription ID this event belongs to.
        subscription_id: String,
        /// The Nostr event payload.
        event: Box<Event>,
    },
    /// Acknowledgement of a published event.
    Ok(OkResponse),
    /// End-of-stored-events marker for a subscription.
    Eose {
        /// The subscription ID that has reached end-of-stored-events.
        subscription_id: String,
    },
    /// The relay closed a subscription, usually with an error.
    Closed {
        /// The subscription ID that was closed.
        subscription_id: String,
        /// Human-readable reason for the closure.
        message: String,
    },
    /// A human-readable notice from the relay.
    Notice {
        /// The notice text.
        message: String,
    },
    /// A NIP-42 authentication challenge from the relay.
    Auth {
        /// The challenge string to sign.
        challenge: String,
    },
}

/// The relay's response to a published event (NIP-01 `OK` message).
#[derive(Debug, Clone)]
pub struct OkResponse {
    /// Hex-encoded ID of the event that was acknowledged.
    pub event_id: String,
    /// Whether the relay accepted the event.
    pub accepted: bool,
    /// Human-readable reason string (empty when accepted without comment).
    pub message: String,
}

// ── Internal types ────────────────────────────────────────────────────────────

type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

/// Commands sent from `RelayClient` to the background WebSocket task.
enum RelayCommand {
    SendEvent {
        event: Event,
        reply: oneshot::Sender<Result<OkResponse, RelayClientError>>,
    },
    Subscribe {
        sub_id: String,
        filters: Vec<Filter>,
        reply: oneshot::Sender<Result<Vec<Event>, RelayClientError>>,
    },
    CloseSubscription {
        sub_id: String,
        reply: oneshot::Sender<Result<(), RelayClientError>>,
    },
    Shutdown,
}

/// A subscription waiting for EOSE.
struct PendingSubscription {
    events: Vec<Event>,
    reply: oneshot::Sender<Result<Vec<Event>, RelayClientError>>,
    deadline: tokio::time::Instant,
}

/// State owned exclusively by the background task.
struct BgState {
    /// Active subscriptions: sub_id → filters (for reconnect replay).
    active_subscriptions: HashMap<String, Vec<Filter>>,
    /// Pending OK waiters: event_id → (reply, deadline).
    pending_ok: HashMap<String, (oneshot::Sender<Result<OkResponse, RelayClientError>>, tokio::time::Instant)>,
    /// Pending EOSE collectors: sub_id → collector.
    pending_eose: HashMap<String, PendingSubscription>,
}

impl BgState {
    fn new() -> Self {
        Self {
            active_subscriptions: HashMap::new(),
            pending_ok: HashMap::new(),
            pending_eose: HashMap::new(),
        }
    }

    /// Resolve all pending operations with `ConnectionClosed` (called on reconnect).
    fn cancel_pending(&mut self) {
        for (_, (reply, _)) in self.pending_ok.drain() {
            let _ = reply.send(Err(RelayClientError::ConnectionClosed));
        }
        for (_, sub) in self.pending_eose.drain() {
            let _ = sub.reply.send(Err(RelayClientError::ConnectionClosed));
        }
    }

    /// Expire any pending operations whose deadline has passed.
    fn expire_timed_out(&mut self) {
        let now = tokio::time::Instant::now();

        let expired_ok: Vec<String> = self
            .pending_ok
            .iter()
            .filter(|(_, (_, dl))| now >= *dl)
            .map(|(k, _)| k.clone())
            .collect();
        for k in expired_ok {
            if let Some((reply, _)) = self.pending_ok.remove(&k) {
                let _ = reply.send(Err(RelayClientError::Timeout));
            }
        }

        let expired_eose: Vec<String> = self
            .pending_eose
            .iter()
            .filter(|(_, sub)| now >= sub.deadline)
            .map(|(k, _)| k.clone())
            .collect();
        for k in expired_eose {
            if let Some(sub) = self.pending_eose.remove(&k) {
                let _ = sub.reply.send(Err(RelayClientError::Timeout));
            }
        }
    }
}

// ── Background task ───────────────────────────────────────────────────────────

/// Perform a single NIP-42 connection + auth handshake.
/// Returns the authenticated WebSocket stream on success.
async fn do_connect(
    relay_url: &str,
    keys: &Keys,
    api_token: Option<&str>,
) -> Result<WsStream, RelayClientError> {
    let parsed = relay_url
        .parse::<url::Url>()
        .map_err(|e| RelayClientError::Url(e.to_string()))?;

    let (mut ws, _) = connect_async(parsed.as_str())
        .await
        .map_err(RelayClientError::WebSocket)?;

    debug!("connected to relay at {relay_url}");

    // Wait for AUTH challenge (5s timeout).
    let challenge = wait_for_auth_challenge(&mut ws, Duration::from_secs(5)).await?;

    let relay_nostr_url: Url = relay_url
        .parse()
        .map_err(|e: url::ParseError| RelayClientError::Url(e.to_string()))?;

    let auth_event = if let Some(token) = api_token {
        let tags = vec![
            Tag::parse(&["relay", relay_url])
                .map_err(|e| RelayClientError::EventBuilder(e.to_string()))?,
            Tag::parse(&["challenge", &challenge])
                .map_err(|e| RelayClientError::EventBuilder(e.to_string()))?,
            Tag::parse(&["auth_token", token])
                .map_err(|e| RelayClientError::EventBuilder(e.to_string()))?,
        ];
        EventBuilder::new(Kind::Authentication, "", tags).sign_with_keys(keys)?
    } else {
        EventBuilder::auth(&challenge, relay_nostr_url).sign_with_keys(keys)?
    };

    let event_id = auth_event.id.to_hex();
    debug!("sending AUTH event {event_id}");
    let auth_msg = serde_json::to_string(&json!(["AUTH", auth_event]))?;
    ws.send(Message::Text(auth_msg.into())).await?;

    let ok = wait_for_ok(&mut ws, &event_id, Duration::from_secs(5)).await?;
    if !ok.accepted {
        return Err(RelayClientError::AuthFailed(ok.message));
    }

    debug!("NIP-42 authentication successful");
    Ok(ws)
}

/// Wait for an AUTH challenge frame, responding to Pings along the way.
async fn wait_for_auth_challenge(ws: &mut WsStream, timeout_dur: Duration) -> Result<String, RelayClientError> {
    let deadline = tokio::time::Instant::now() + timeout_dur;
    loop {
        let remaining = deadline
            .checked_duration_since(tokio::time::Instant::now())
            .unwrap_or(Duration::ZERO);
        if remaining.is_zero() {
            return Err(RelayClientError::NoAuthChallenge);
        }
        let raw = tokio::time::timeout(remaining, ws.next())
            .await
            .map_err(|_| RelayClientError::NoAuthChallenge)?
            .ok_or(RelayClientError::ConnectionClosed)?
            .map_err(RelayClientError::WebSocket)?;
        match raw {
            Message::Text(text) => match parse_relay_message(&text)? {
                RelayMessage::Auth { challenge } => return Ok(challenge),
                _ => {} // discard other messages during handshake
            },
            Message::Ping(data) => { ws.send(Message::Pong(data)).await?; }
            Message::Close(_) => return Err(RelayClientError::ConnectionClosed),
            _ => {}
        }
    }
}

/// Wait for an OK frame matching `event_id`, responding to Pings along the way.
async fn wait_for_ok(ws: &mut WsStream, event_id: &str, timeout_dur: Duration) -> Result<OkResponse, RelayClientError> {
    let deadline = tokio::time::Instant::now() + timeout_dur;
    loop {
        let remaining = deadline
            .checked_duration_since(tokio::time::Instant::now())
            .unwrap_or(Duration::ZERO);
        if remaining.is_zero() {
            return Err(RelayClientError::Timeout);
        }
        let raw = tokio::time::timeout(remaining, ws.next())
            .await
            .map_err(|_| RelayClientError::Timeout)?
            .ok_or(RelayClientError::ConnectionClosed)?
            .map_err(RelayClientError::WebSocket)?;
        match raw {
            Message::Text(text) => match parse_relay_message(&text)? {
                RelayMessage::Ok(ok) if ok.event_id == event_id => return Ok(ok),
                _ => {} // discard other messages during handshake
            },
            Message::Ping(data) => { ws.send(Message::Pong(data)).await?; }
            Message::Close(_) => return Err(RelayClientError::ConnectionClosed),
            _ => {}
        }
    }
}

/// Connect with exponential backoff. Never returns until successful.
async fn connect_with_retry(relay_url: &str, keys: &Keys, api_token: Option<&str>) -> WsStream {
    let mut delay = Duration::from_secs(1);
    loop {
        match do_connect(relay_url, keys, api_token).await {
            Ok(ws) => {
                tracing::info!("connected to relay at {relay_url}");
                return ws;
            }
            Err(e) => {
                warn!("connection failed: {e}, retrying in {delay:?}");
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(30));
            }
        }
    }
}

/// Send a NIP-42 AUTH response for a mid-session challenge.
async fn send_auth_response(
    ws: &mut WsStream,
    challenge: &str,
    relay_url: &str,
    keys: &Keys,
    api_token: Option<&str>,
) {
    let result: Result<(), RelayClientError> = async {
        let relay_nostr_url: Url = relay_url
            .parse()
            .map_err(|e: url::ParseError| RelayClientError::Url(e.to_string()))?;
        let auth_event = if let Some(token) = api_token {
            let tags = vec![
                Tag::parse(&["relay", relay_url])
                    .map_err(|e| RelayClientError::EventBuilder(e.to_string()))?,
                Tag::parse(&["challenge", challenge])
                    .map_err(|e| RelayClientError::EventBuilder(e.to_string()))?,
                Tag::parse(&["auth_token", token])
                    .map_err(|e| RelayClientError::EventBuilder(e.to_string()))?,
            ];
            EventBuilder::new(Kind::Authentication, "", tags).sign_with_keys(keys)?
        } else {
            EventBuilder::auth(challenge, relay_nostr_url).sign_with_keys(keys)?
        };
        let msg = serde_json::to_string(&json!(["AUTH", auth_event]))?;
        ws.send(Message::Text(msg.into())).await?;
        debug!("sent AUTH response for mid-session challenge");
        Ok(())
    }.await;
    if let Err(e) = result {
        warn!("failed to respond to mid-session AUTH challenge: {e}");
    }
}

/// Handle a single WebSocket message in the background task.
///
/// Returns `false` if the connection has been lost (Close frame or error).
async fn handle_ws_message(
    msg: Message,
    ws: &mut WsStream,
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
                    warn!("failed to parse relay message: {e}");
                    return true;
                }
            };
            match relay_msg {
                RelayMessage::Event { subscription_id, event } => {
                    if let Some(sub) = state.pending_eose.get_mut(&subscription_id) {
                        sub.events.push(*event);
                    } else {
                        debug!("EVENT for unknown/completed subscription {subscription_id}");
                    }
                }
                RelayMessage::Ok(ok) => {
                    if let Some((reply, _)) = state.pending_ok.remove(&ok.event_id) {
                        let _ = reply.send(Ok(ok));
                    } else {
                        debug!("OK for unknown event {}", ok.event_id);
                    }
                }
                RelayMessage::Eose { subscription_id } => {
                    if let Some(sub) = state.pending_eose.remove(&subscription_id) {
                        let _ = sub.reply.send(Ok(sub.events));
                    } else {
                        debug!("EOSE for unknown subscription {subscription_id}");
                    }
                }
                RelayMessage::Closed { subscription_id, message } => {
                    warn!("subscription {subscription_id} closed by relay: {message}");
                    if let Some(sub) = state.pending_eose.remove(&subscription_id) {
                        let _ = sub.reply.send(Err(RelayClientError::ConnectionClosed));
                    }
                }
                RelayMessage::Notice { message } => {
                    debug!("relay NOTICE: {message}");
                }
                RelayMessage::Auth { challenge } => {
                    debug!("received mid-session AUTH challenge — re-authenticating");
                    send_auth_response(ws, &challenge, relay_url, keys, api_token).await;
                }
            }
            true
        }
        Message::Ping(data) => {
            if let Err(e) = ws.send(Message::Pong(data)).await {
                warn!("failed to send Pong: {e}");
                return false;
            }
            true
        }
        Message::Close(_) => {
            debug!("relay sent Close frame");
            false
        }
        _ => true,
    }
}

/// Reconnect with backoff, cancel pending ops, then replay subscriptions.
async fn do_reconnect(
    ws: &mut WsStream,
    state: &mut BgState,
    keys: &Keys,
    relay_url: &str,
    api_token: Option<&str>,
) {
    warn!("relay connection lost — reconnecting…");
    state.cancel_pending();

    let new_ws = connect_with_retry(relay_url, keys, api_token).await;
    *ws = new_ws;

    // Replay active subscriptions.
    let subs: Vec<(String, Vec<Filter>)> = state.active_subscriptions
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    for (sub_id, filters) in subs {
        let mut msg: Vec<Value> = Vec::with_capacity(2 + filters.len());
        msg.push(json!("REQ"));
        msg.push(json!(sub_id));
        for f in &filters {
            match serde_json::to_value(f) {
                Ok(v) => msg.push(v),
                Err(e) => warn!("failed to serialize filter for {sub_id}: {e}"),
            }
        }
        let text = match serde_json::to_string(&Value::Array(msg)) {
            Ok(t) => t,
            Err(e) => { warn!("failed to serialize REQ for {sub_id}: {e}"); continue; }
        };
        if let Err(e) = ws.send(Message::Text(text.into())).await {
            warn!("failed to resubscribe to {sub_id}: {e}");
        }
    }
}

/// The main background task loop.
///
/// Owns the WebSocket, responds to Pings, routes relay messages to pending
/// waiters, and handles reconnection transparently.
async fn run_background_task(
    mut ws: WsStream,
    mut cmd_rx: mpsc::Receiver<RelayCommand>,
    keys: Keys,
    relay_url: String,
    api_token: Option<String>,
) {
    let mut state = BgState::new();
    // Ticker for expiring timed-out pending operations (~1s granularity).
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            // ── Incoming WebSocket message ────────────────────────────────────
            raw = ws.next() => {
                match raw {
                    Some(Ok(msg)) => {
                        let ok = handle_ws_message(
                            msg, &mut ws, &mut state, &keys, &relay_url, api_token.as_deref(),
                        ).await;
                        if !ok {
                            do_reconnect(&mut ws, &mut state, &keys, &relay_url, api_token.as_deref()).await;
                        }
                    }
                    Some(Err(e)) => {
                        warn!("WebSocket error: {e}");
                        do_reconnect(&mut ws, &mut state, &keys, &relay_url, api_token.as_deref()).await;
                    }
                    None => {
                        debug!("WebSocket stream ended");
                        do_reconnect(&mut ws, &mut state, &keys, &relay_url, api_token.as_deref()).await;
                    }
                }
            }

            // ── Command from RelayClient ──────────────────────────────────────
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(RelayCommand::SendEvent { event, reply }) => {
                        let event_id = event.id.to_hex();
                        let msg = match serde_json::to_string(&json!(["EVENT", event])) {
                            Ok(t) => t,
                            Err(e) => { let _ = reply.send(Err(e.into())); continue; }
                        };
                        if let Err(e) = ws.send(Message::Text(msg.into())).await {
                            let _ = reply.send(Err(RelayClientError::WebSocket(e)));
                            continue;
                        }
                        let deadline = tokio::time::Instant::now() + SEND_EVENT_TIMEOUT;
                        state.pending_ok.insert(event_id, (reply, deadline));
                    }

                    Some(RelayCommand::Subscribe { sub_id, filters, reply }) => {
                        // Build and send REQ — serialize all filters first, then send.
                        let mut msg: Vec<Value> = Vec::with_capacity(2 + filters.len());
                        msg.push(json!("REQ"));
                        msg.push(json!(sub_id));
                        let mut ser_err: Option<serde_json::Error> = None;
                        for f in &filters {
                            match serde_json::to_value(f) {
                                Ok(v) => msg.push(v),
                                Err(e) => { ser_err = Some(e); break; }
                            }
                        }
                        if let Some(e) = ser_err {
                            let _ = reply.send(Err(e.into()));
                            continue;
                        }
                        let text = match serde_json::to_string(&Value::Array(msg)) {
                            Ok(t) => t,
                            Err(e) => { let _ = reply.send(Err(e.into())); continue; }
                        };
                        if let Err(e) = ws.send(Message::Text(text.into())).await {
                            let _ = reply.send(Err(RelayClientError::WebSocket(e)));
                            continue;
                        }
                        // Track for reconnect replay.
                        state.active_subscriptions.insert(sub_id.clone(), filters);
                        let deadline = tokio::time::Instant::now() + SUBSCRIBE_TIMEOUT;
                        state.pending_eose.insert(sub_id, PendingSubscription {
                            events: Vec::new(),
                            reply,
                            deadline,
                        });
                    }

                    Some(RelayCommand::CloseSubscription { sub_id, reply }) => {
                        state.active_subscriptions.remove(&sub_id);
                        // If there's a pending EOSE, cancel it.
                        if let Some(sub) = state.pending_eose.remove(&sub_id) {
                            let _ = sub.reply.send(Err(RelayClientError::ConnectionClosed));
                        }
                        let msg = match serde_json::to_string(&json!(["CLOSE", sub_id])) {
                            Ok(t) => t,
                            Err(e) => { let _ = reply.send(Err(e.into())); continue; }
                        };
                        let result = ws.send(Message::Text(msg.into())).await
                            .map_err(RelayClientError::WebSocket);
                        let _ = reply.send(result);
                    }

                    Some(RelayCommand::Shutdown) | None => {
                        debug!("background task shutting down");
                        state.cancel_pending();
                        return;
                    }
                }
            }

            // ── Timeout ticker ────────────────────────────────────────────────
            _ = tick.tick() => {
                state.expire_timed_out();
            }
        }
    }
}

// ── Public client ─────────────────────────────────────────────────────────────

/// Clone-able WebSocket client for the Sprout relay.
///
/// Internally, a background tokio task owns the WebSocket connection. All
/// clones share the same `cmd_tx` channel to that task. The background task:
/// - Responds to Ping frames immediately (prevents relay disconnect)
/// - Handles mid-session AUTH challenges automatically
/// - Reconnects with exponential backoff on connection loss
/// - Replays active subscriptions after reconnect
#[derive(Clone)]
pub struct RelayClient {
    /// Channel to the background WebSocket task.
    cmd_tx: mpsc::Sender<RelayCommand>,
    /// Handle to the background task (kept alive to prevent task cancellation on drop).
    #[allow(dead_code)]
    bg_handle: std::sync::Arc<JoinHandle<()>>,
    keys: Keys,
    /// WebSocket URL of the relay (e.g. "ws://localhost:3000").
    relay_url: String,
    /// Shared reqwest client for REST API calls.
    http: reqwest::Client,
    /// Optional API token for Bearer auth on REST endpoints.
    api_token: Option<String>,
}

impl RelayClient {
    /// Connect to the relay and start the background task.
    ///
    /// Performs the initial NIP-42 handshake synchronously so startup failures
    /// are surfaced immediately. After that, reconnection is automatic.
    pub async fn connect(
        relay_url: &str,
        keys: &Keys,
        api_token: Option<&str>,
    ) -> Result<Self, RelayClientError> {
        let ws = do_connect(relay_url, keys, api_token).await?;

        let (cmd_tx, cmd_rx) = mpsc::channel(CMD_CHANNEL_CAPACITY);

        let bg_keys = keys.clone();
        let bg_relay_url = relay_url.to_string();
        let bg_api_token = api_token.map(|t| t.to_string());

        let bg_handle = tokio::spawn(async move {
            run_background_task(ws, cmd_rx, bg_keys, bg_relay_url, bg_api_token).await;
        });

        Ok(Self {
            cmd_tx,
            bg_handle: std::sync::Arc::new(bg_handle),
            keys: keys.clone(),
            relay_url: relay_url.to_string(),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .connect_timeout(std::time::Duration::from_secs(5))
                .build()
                .expect("SAFETY: default builder with only timeout config cannot fail"),
            api_token: api_token.map(|t| t.to_string()),
        })
    }

    /// Returns the Nostr keypair used for signing and authentication.
    pub fn keys(&self) -> &Keys {
        &self.keys
    }

    /// Returns the WebSocket URL the client connected to.
    pub fn relay_url(&self) -> &str {
        &self.relay_url
    }

    /// Returns the HTTP base URL for the relay's REST API.
    /// Converts ws:// → http:// and wss:// → https://, strips trailing slash.
    pub fn relay_http_url(&self) -> String {
        relay_ws_to_http(&self.relay_url)
    }

    fn pubkey_hex(&self) -> String {
        self.keys.public_key().to_hex()
    }

    /// Returns the appropriate auth header for REST requests.
    ///
    /// - If an API token is present: `Authorization: Bearer <token>` (production mode).
    /// - Otherwise: `X-Pubkey: <hex>` (dev mode, relay has `require_auth_token=false`).
    fn apply_auth(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(ref token) = self.api_token {
            builder.header("Authorization", format!("Bearer {}", token))
        } else {
            builder.header("X-Pubkey", self.pubkey_hex())
        }
    }

    /// Authenticated GET to the relay's REST API. Returns the response body.
    pub async fn get(&self, path: &str) -> anyhow::Result<String> {
        let url = format!("{}{}", self.relay_http_url(), path);
        let resp = self.apply_auth(self.http.get(&url)).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("{} {}: {}", status, url, body));
        }
        Ok(resp.text().await?)
    }

    /// Authenticated POST (JSON body) to the relay's REST API.
    pub async fn post(&self, path: &str, body: &serde_json::Value) -> anyhow::Result<String> {
        let url = format!("{}{}", self.relay_http_url(), path);
        let resp = self
            .apply_auth(self.http.post(&url))
            .json(body)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("{} {}: {}", status, url, body));
        }
        Ok(resp.text().await?)
    }

    /// Authenticated PUT (JSON body) to the relay's REST API.
    pub async fn put(&self, path: &str, body: &serde_json::Value) -> anyhow::Result<String> {
        let url = format!("{}{}", self.relay_http_url(), path);
        let resp = self
            .apply_auth(self.http.put(&url))
            .json(body)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("{} {}: {}", status, url, body));
        }
        Ok(resp.text().await?)
    }

    /// Authenticated DELETE to the relay's REST API.
    pub async fn delete(&self, path: &str) -> anyhow::Result<String> {
        let url = format!("{}{}", self.relay_http_url(), path);
        let resp = self.apply_auth(self.http.delete(&url)).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("{} {}: {}", status, url, body));
        }
        Ok(resp.text().await?)
    }

    /// Get the canvas content for a channel via REST.
    pub async fn get_canvas(&self, channel_id: &str) -> anyhow::Result<String> {
        self.get(&format!("/api/channels/{}/canvas", channel_id))
            .await
    }

    /// Set the canvas content for a channel via REST.
    pub async fn set_canvas(&self, channel_id: &str, content: &str) -> anyhow::Result<String> {
        let body = serde_json::json!({ "content": content });
        self.put(&format!("/api/channels/{}/canvas", channel_id), &body)
            .await
    }

    /// Authenticated GET to a full URL (for feed tools that build the URL themselves).
    pub async fn get_api(&self, url: &str) -> anyhow::Result<String> {
        let resp = self.apply_auth(self.http.get(url)).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("{} {}: {}", status, url, body));
        }
        Ok(resp.text().await?)
    }

    /// Publish a signed Nostr event to the relay and wait for the `OK` acknowledgement.
    pub async fn send_event(&self, event: Event) -> Result<OkResponse, RelayClientError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(RelayCommand::SendEvent { event, reply: reply_tx })
            .await
            .map_err(|_| RelayClientError::ConnectionClosed)?;
        reply_rx.await.map_err(|_| RelayClientError::ConnectionClosed)?
    }

    /// Open a subscription with the given filters and collect all stored events until `EOSE`.
    pub async fn subscribe(
        &self,
        sub_id: &str,
        filters: Vec<Filter>,
    ) -> Result<Vec<Event>, RelayClientError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(RelayCommand::Subscribe {
                sub_id: sub_id.to_string(),
                filters,
                reply: reply_tx,
            })
            .await
            .map_err(|_| RelayClientError::ConnectionClosed)?;
        reply_rx.await.map_err(|_| RelayClientError::ConnectionClosed)?
    }

    /// Send a `CLOSE` message to the relay and remove the subscription from the active set.
    pub async fn close_subscription(&self, sub_id: &str) -> Result<(), RelayClientError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(RelayCommand::CloseSubscription {
                sub_id: sub_id.to_string(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| RelayClientError::ConnectionClosed)?;
        reply_rx.await.map_err(|_| RelayClientError::ConnectionClosed)?
    }

    /// Perform a clean WebSocket close handshake.
    pub async fn close(&self) -> Result<(), RelayClientError> {
        // Signal the background task to shut down.
        let _ = self.cmd_tx.send(RelayCommand::Shutdown).await;
        Ok(())
    }
}

// ── Free functions ────────────────────────────────────────────────────────────

/// Convert a WebSocket URL to its HTTP equivalent.
/// Converts `ws://` → `http://` and `wss://` → `https://`, strips trailing slash.
///
/// Extracted as a free function so it can be unit-tested without a live connection.
pub(crate) fn relay_ws_to_http(url: &str) -> String {
    url.replace("wss://", "https://")
        .replace("ws://", "http://")
        .trim_end_matches('/')
        .to_string()
}

/// Parse a raw relay text frame into a typed [`RelayMessage`].
#[allow(clippy::result_large_err)]
pub fn parse_relay_message(text: &str) -> Result<RelayMessage, RelayClientError> {
    let arr: Vec<Value> = serde_json::from_str(text)?;

    let msg_type = arr
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| RelayClientError::UnexpectedMessage(text.to_string()))?;

    match msg_type {
        "EVENT" => {
            let sub_id = arr
                .get(1)
                .and_then(|v| v.as_str())
                .ok_or_else(|| RelayClientError::UnexpectedMessage(text.to_string()))?
                .to_string();
            let event: Event = serde_json::from_value(
                arr.get(2)
                    .cloned()
                    .ok_or_else(|| RelayClientError::UnexpectedMessage(text.to_string()))?,
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
                .ok_or_else(|| RelayClientError::UnexpectedMessage(text.to_string()))?
                .to_string();
            let accepted = arr.get(2).and_then(|v| v.as_bool()).unwrap_or(false);
            let message = arr
                .get(3)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Ok(RelayMessage::Ok(OkResponse {
                event_id,
                accepted,
                message,
            }))
        }
        "EOSE" => {
            let sub_id = arr
                .get(1)
                .and_then(|v| v.as_str())
                .ok_or_else(|| RelayClientError::UnexpectedMessage(text.to_string()))?
                .to_string();
            Ok(RelayMessage::Eose {
                subscription_id: sub_id,
            })
        }
        "CLOSED" => {
            let sub_id = arr
                .get(1)
                .and_then(|v| v.as_str())
                .ok_or_else(|| RelayClientError::UnexpectedMessage(text.to_string()))?
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
                .ok_or_else(|| RelayClientError::UnexpectedMessage(text.to_string()))?
                .to_string();
            Ok(RelayMessage::Auth { challenge })
        }
        other => Err(RelayClientError::UnexpectedMessage(format!(
            "unknown message type: {other}"
        ))),
    }
}

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

    // ── parse_relay_message ───────────────────────────────────────────────────

    #[test]
    fn parse_ok_accepted() {
        let text = r#"["OK","abc123",true,""]"#;
        let msg = parse_relay_message(text).unwrap();
        match msg {
            RelayMessage::Ok(ok) => {
                assert_eq!(ok.event_id, "abc123");
                assert!(ok.accepted);
                assert_eq!(ok.message, "");
            }
            _ => panic!("expected Ok"),
        }
    }

    #[test]
    fn parse_ok_rejected() {
        let text = r#"["OK","abc123",false,"blocked: spam"]"#;
        let msg = parse_relay_message(text).unwrap();
        match msg {
            RelayMessage::Ok(ok) => {
                assert_eq!(ok.event_id, "abc123");
                assert!(!ok.accepted);
                assert_eq!(ok.message, "blocked: spam");
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
        // NOTICE with no message field — should default to empty string.
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
            RelayClientError::UnexpectedMessage(msg) => {
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
        assert!(matches!(result.unwrap_err(), RelayClientError::Json(_)));
    }

    #[test]
    fn parse_empty_array_returns_error() {
        let text = "[]";
        let result = parse_relay_message(text);
        assert!(result.is_err());
        match result.unwrap_err() {
            RelayClientError::UnexpectedMessage(_) => {}
            e => panic!("expected UnexpectedMessage, got {e:?}"),
        }
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
}
