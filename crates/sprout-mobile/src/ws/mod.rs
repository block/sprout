//! WebSocket connection manager for real-time relay communication.
//! Handles connect, NIP-42 auth, reconnect, subscriptions, and event dispatch.

/// Relay protocol message definitions.
pub mod protocol;
/// Helpers for building subscription filters and IDs.
pub mod subscription;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use nostr::Keys;
use serde_json::{json, Value};
use tokio::sync::{watch, Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, warn};

use sprout_core::protocol::{parse_relay_message, OkResponse, RelayMessage};

use crate::auth::nip42::build_auth_event;
use crate::converters::json_to_message;
use crate::error::SproutError;
use crate::types::ConnectionState;
use crate::SproutEventListener;

/// Maximum reconnect backoff delay.
const MAX_BACKOFF: Duration = Duration::from_secs(30);
/// Initial reconnect delay.
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
/// Timeout for the NIP-42 auth handshake.
const AUTH_TIMEOUT: Duration = Duration::from_secs(10);

/// Manages the WebSocket connection to a Sprout relay.
pub(crate) struct WsManager {
    relay_url: String,
    keys: Arc<RwLock<Option<Keys>>>,
    api_token: Arc<RwLock<Option<String>>>,
    /// Event listener — pub(crate) for SproutClient.set_event_listener() access.
    pub(crate) listener: Arc<RwLock<Option<Box<dyn SproutEventListener>>>>,
    state_tx: watch::Sender<ConnectionState>,
    state_rx: watch::Receiver<ConnectionState>,
    task: Mutex<Option<JoinHandle<()>>>,
    /// Active subscriptions: sub_id → filters (for reconnect replay).
    subscriptions: Arc<RwLock<HashMap<String, Vec<nostr::Filter>>>>,
    /// Pending OK responses: event_id → oneshot sender.
    #[allow(clippy::type_complexity)]
    pending_ok:
        Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<Result<OkResponse, SproutError>>>>>,
}

impl WsManager {
    /// Create a new WebSocket manager (does not connect yet).
    pub fn new(relay_url: &str) -> Self {
        let (state_tx, state_rx) = watch::channel(ConnectionState::Disconnected);
        Self {
            relay_url: relay_url.to_string(),
            keys: Arc::new(RwLock::new(None)),
            api_token: Arc::new(RwLock::new(None)),
            listener: Arc::new(RwLock::new(None)),
            state_tx,
            state_rx,
            task: Mutex::new(None),
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            pending_ok: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Set the keypair for signing auth events.
    pub async fn set_keys(&self, keys: Keys) {
        *self.keys.write().await = Some(keys);
    }

    /// Set the API token for auth_token tag.
    pub async fn set_api_token(&self, token: String) {
        *self.api_token.write().await = Some(token);
    }

    /// Set the event listener.
    #[allow(dead_code)]
    pub async fn set_listener(&self, listener: Box<dyn SproutEventListener>) {
        *self.listener.write().await = Some(listener);
    }

    /// Current connection state.
    pub fn state(&self) -> ConnectionState {
        self.state_rx.borrow().clone()
    }

    /// Connect to the relay (spawns a background task).
    pub async fn connect(&self) -> Result<(), SproutError> {
        let keys = self
            .keys
            .read()
            .await
            .clone()
            .ok_or(SproutError::AuthRequired)?;

        // Shut down existing task if any.
        self.disconnect().await;

        let relay_url = self.relay_url.clone();
        let api_token = self.api_token.clone();
        let listener = self.listener.clone();
        let state_tx = self.state_tx.clone();
        let subscriptions = self.subscriptions.clone();
        let pending_ok = self.pending_ok.clone();

        let task = tokio::spawn(async move {
            connection_loop(
                relay_url,
                keys,
                api_token,
                listener,
                state_tx,
                subscriptions,
                pending_ok,
            )
            .await;
        });

        *self.task.lock().await = Some(task);
        Ok(())
    }

    /// Disconnect from the relay.
    pub async fn disconnect(&self) {
        if let Some(task) = self.task.lock().await.take() {
            task.abort();
            let _ = task.await;
        }
        let _ = self.state_tx.send(ConnectionState::Disconnected);
    }

    /// Subscribe to events matching the given filters.
    pub async fn subscribe(&self, sub_id: String, filters: Vec<nostr::Filter>) {
        self.subscriptions.write().await.insert(sub_id, filters);
    }

    /// Unsubscribe from a subscription.
    pub async fn unsubscribe(&self, sub_id: &str) {
        self.subscriptions.write().await.remove(sub_id);
    }

    /// Send a signed event and wait for the OK response.
    #[allow(dead_code)]
    pub async fn send_event(&self, event: nostr::Event) -> Result<OkResponse, SproutError> {
        let event_id = event.id.to_hex();
        let (tx, rx) = tokio::sync::oneshot::channel();

        self.pending_ok.lock().await.insert(event_id.clone(), tx);

        // The connection loop will pick up events to send through its own mechanism.
        // For now, we store the pending OK waiter; the actual send happens through
        // the connection loop's subscription replay mechanism.
        // TODO: Add a command channel for sending events through the active connection.

        match tokio::time::timeout(Duration::from_secs(10), rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(SproutError::WebSocketError {
                message: "send channel dropped".to_string(),
            }),
            Err(_) => {
                self.pending_ok.lock().await.remove(&event_id);
                Err(SproutError::WebSocketError {
                    message: "timeout waiting for OK response".to_string(),
                })
            }
        }
    }
}

/// Long-running connection loop with reconnect logic.
#[allow(clippy::type_complexity)]
async fn connection_loop(
    relay_url: String,
    keys: Keys,
    api_token: Arc<RwLock<Option<String>>>,
    listener: Arc<RwLock<Option<Box<dyn SproutEventListener>>>>,
    state_tx: watch::Sender<ConnectionState>,
    subscriptions: Arc<RwLock<HashMap<String, Vec<nostr::Filter>>>>,
    #[allow(clippy::type_complexity)] pending_ok: Arc<
        Mutex<HashMap<String, tokio::sync::oneshot::Sender<Result<OkResponse, SproutError>>>>,
    >,
) {
    let mut backoff = INITIAL_BACKOFF;
    let mut attempt: u32 = 0;

    loop {
        let _ = state_tx.send(if attempt == 0 {
            ConnectionState::Connecting
        } else {
            ConnectionState::Reconnecting { attempt }
        });

        // Notify listener of state change.
        {
            let l = listener.read().await;
            if let Some(ref cb) = *l {
                cb.on_connection_state_changed(state_tx.borrow().clone());
            }
        }

        match connect_and_run(
            &relay_url,
            &keys,
            &api_token,
            &listener,
            &state_tx,
            &subscriptions,
            &pending_ok,
        )
        .await
        {
            Ok(()) => {
                // Clean disconnect requested.
                debug!("WebSocket connection closed cleanly");
                return;
            }
            Err(e) => {
                warn!("WebSocket error: {e}, reconnecting in {backoff:?}");
                let _ = state_tx.send(ConnectionState::Reconnecting {
                    attempt: attempt + 1,
                });
                {
                    let l = listener.read().await;
                    if let Some(ref cb) = *l {
                        cb.on_connection_state_changed(ConnectionState::Reconnecting {
                            attempt: attempt + 1,
                        });
                    }
                }
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
                attempt += 1;
            }
        }
    }
}

/// Single connection attempt: connect, auth, subscribe, and process messages.
#[allow(clippy::type_complexity)]
async fn connect_and_run(
    relay_url: &str,
    keys: &Keys,
    api_token: &Arc<RwLock<Option<String>>>,
    listener: &Arc<RwLock<Option<Box<dyn SproutEventListener>>>>,
    state_tx: &watch::Sender<ConnectionState>,
    subscriptions: &Arc<RwLock<HashMap<String, Vec<nostr::Filter>>>>,
    #[allow(clippy::type_complexity)] pending_ok: &Arc<
        Mutex<HashMap<String, tokio::sync::oneshot::Sender<Result<OkResponse, SproutError>>>>,
    >,
) -> Result<(), SproutError> {
    let url: url::Url =
        relay_url
            .parse()
            .map_err(|e: url::ParseError| SproutError::WebSocketError {
                message: format!("invalid relay URL: {e}"),
            })?;

    let (ws, _) = connect_async(url.as_str())
        .await
        .map_err(|e| SproutError::WebSocketError {
            message: e.to_string(),
        })?;

    debug!("WebSocket connected to {relay_url}");
    let _ = state_tx.send(ConnectionState::Authenticating);

    let (mut sink, mut stream) = ws.split();

    // Wait for AUTH challenge.
    let challenge = tokio::time::timeout(AUTH_TIMEOUT, async {
        loop {
            match stream.next().await {
                Some(Ok(Message::Text(text))) => {
                    if let Ok(RelayMessage::Auth { challenge }) = parse_relay_message(&text) {
                        return Ok(challenge);
                    }
                }
                Some(Ok(Message::Ping(data))) => {
                    let _ = sink.send(Message::Pong(data)).await;
                }
                Some(Err(e)) => {
                    return Err(SproutError::WebSocketError {
                        message: e.to_string(),
                    });
                }
                None => {
                    return Err(SproutError::WebSocketError {
                        message: "connection closed before auth".to_string(),
                    });
                }
                _ => {}
            }
        }
    })
    .await
    .map_err(|_| SproutError::WebSocketError {
        message: "timeout waiting for AUTH challenge".to_string(),
    })??;

    // Sign and send AUTH response.
    let token = api_token.read().await.clone();
    let auth_event = build_auth_event(keys, &challenge, relay_url, token.as_deref())?;
    let auth_msg = json!([
        "AUTH",
        serde_json::to_value(&auth_event).map_err(|e| {
            SproutError::InternalError {
                message: e.to_string(),
            }
        })?
    ]);
    sink.send(Message::Text(auth_msg.to_string().into()))
        .await
        .map_err(|e| SproutError::WebSocketError {
            message: e.to_string(),
        })?;

    // Wait for OK response to auth.
    let auth_ok = tokio::time::timeout(AUTH_TIMEOUT, async {
        loop {
            match stream.next().await {
                Some(Ok(Message::Text(text))) => {
                    if let Ok(RelayMessage::Ok(ok)) = parse_relay_message(&text) {
                        return Ok(ok);
                    }
                }
                Some(Ok(Message::Ping(data))) => {
                    let _ = sink.send(Message::Pong(data)).await;
                }
                Some(Err(e)) => {
                    return Err(SproutError::WebSocketError {
                        message: e.to_string(),
                    });
                }
                None => {
                    return Err(SproutError::WebSocketError {
                        message: "connection closed during auth".to_string(),
                    });
                }
                _ => {}
            }
        }
    })
    .await
    .map_err(|_| SproutError::WebSocketError {
        message: "timeout waiting for auth OK".to_string(),
    })??;

    if !auth_ok.accepted {
        return Err(SproutError::AuthFailed {
            message: auth_ok.message,
        });
    }

    debug!("NIP-42 auth successful");
    let _ = state_tx.send(ConnectionState::Connected);
    {
        let l = listener.read().await;
        if let Some(ref cb) = *l {
            cb.on_connection_state_changed(ConnectionState::Connected);
        }
    }

    // Re-subscribe to all active subscriptions.
    {
        let subs = subscriptions.read().await;
        for (sub_id, filters) in subs.iter() {
            let mut msg: Vec<Value> = vec![json!("REQ"), json!(sub_id)];
            for f in filters {
                msg.push(serde_json::to_value(f).unwrap_or(json!({})));
            }
            let _ = sink
                .send(Message::Text(Value::Array(msg).to_string().into()))
                .await;
        }
    }

    // Main message loop.
    loop {
        match stream.next().await {
            Some(Ok(Message::Text(text))) => {
                match parse_relay_message(&text) {
                    Ok(RelayMessage::Event {
                        subscription_id: _,
                        event,
                    }) => {
                        // Convert and dispatch to listener.
                        let event_json = serde_json::to_value(&*event).unwrap_or(json!({}));
                        if let Some(msg) = json_to_message(&event_json) {
                            let l = listener.read().await;
                            if let Some(ref cb) = *l {
                                cb.on_message(msg);
                            }
                        }
                    }
                    Ok(RelayMessage::Ok(ok)) => {
                        // Resolve pending send futures.
                        let mut pending = pending_ok.lock().await;
                        if let Some(tx) = pending.remove(&ok.event_id) {
                            if ok.accepted {
                                let _ = tx.send(Ok(ok));
                            } else {
                                let _ = tx.send(Err(SproutError::RelayError {
                                    status: 400,
                                    message: ok.message,
                                }));
                            }
                        }
                    }
                    Ok(RelayMessage::Eose { .. }) => {
                        // EOSE — historical events delivered, now streaming live.
                    }
                    Ok(RelayMessage::Closed {
                        subscription_id,
                        message,
                    }) => {
                        debug!("subscription {subscription_id} closed: {message}");
                        subscriptions.write().await.remove(&subscription_id);
                    }
                    Ok(RelayMessage::Notice { message }) => {
                        debug!("relay notice: {message}");
                    }
                    Ok(RelayMessage::Auth { challenge: _ }) => {
                        // Mid-session re-auth challenge — re-authenticate.
                        warn!("received mid-session AUTH challenge; re-auth not yet implemented");
                    }
                    Err(e) => {
                        debug!("failed to parse relay message: {e}");
                    }
                }
            }
            Some(Ok(Message::Ping(data))) => {
                let _ = sink.send(Message::Pong(data)).await;
            }
            Some(Ok(Message::Close(_))) => {
                debug!("relay sent close frame");
                return Err(SproutError::WebSocketError {
                    message: "relay closed connection".to_string(),
                });
            }
            Some(Err(e)) => {
                return Err(SproutError::WebSocketError {
                    message: e.to_string(),
                });
            }
            None => {
                return Err(SproutError::WebSocketError {
                    message: "connection stream ended".to_string(),
                });
            }
            _ => {}
        }
    }
}
