//! WebSocket connection lifecycle: semaphore → challenge → recv/send/heartbeat loops → cleanup.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message as WsMessage, WebSocket};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, trace, warn};
use uuid::Uuid;

use nostr::Filter;
use sprout_auth::{generate_challenge, AuthContext};

use crate::handlers;
use crate::protocol::{ClientMessage, RelayMessage};
use crate::state::AppState;

/// NIP-42 authentication state for a single connection.
#[derive(Debug, Clone)]
pub enum AuthState {
    /// Challenge has been sent; awaiting a signed AUTH event from the client.
    Pending {
        /// The random challenge string sent to the client.
        challenge: String,
    },
    /// Client has successfully authenticated.
    Authenticated(AuthContext),
    /// Authentication attempt was rejected.
    Failed,
}

/// Per-connection state split by access pattern:
/// - `auth_state`: RwLock (read-heavy after initial auth)
/// - `subscriptions`: Mutex (write-heavy during REQ/CLOSE)
/// - `send_tx`, `cancel`: outside any lock (Clone+Send, no coordination needed)
pub struct ConnectionState {
    /// Unique identifier for this connection.
    pub conn_id: Uuid,
    /// Remote socket address of the client.
    pub remote_addr: SocketAddr,
    /// Current NIP-42 authentication state.
    pub auth_state: RwLock<AuthState>,
    /// Active subscriptions keyed by subscription ID.
    pub subscriptions: Mutex<HashMap<String, Vec<Filter>>>,
    /// Sender for outbound WebSocket messages.
    pub send_tx: mpsc::Sender<WsMessage>,
    /// Token used to signal graceful shutdown of this connection's tasks.
    pub cancel: CancellationToken,
}

impl ConnectionState {
    /// Sends a message to this connection's outbound channel.
    ///
    /// If the send buffer is full (slow client), cancels the connection
    /// via the `CancellationToken` to prevent unbounded memory growth.
    pub fn send(&self, msg: String) -> bool {
        match self.send_tx.try_send(WsMessage::Text(msg.into())) {
            Ok(_) => true,
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                warn!(conn_id = %self.conn_id, "send buffer full — closing slow client");
                self.cancel.cancel();
                false
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                debug!(conn_id = %self.conn_id, "send channel closed");
                false
            }
        }
    }
}

/// Entry point for a new WebSocket connection.
///
/// Acquires a connection semaphore permit, sends the NIP-42 AUTH challenge,
/// then drives the send, heartbeat, and receive loops until the connection closes.
pub async fn handle_connection(socket: WebSocket, state: Arc<AppState>, addr: SocketAddr) {
    let permit = match state.conn_semaphore.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            warn!("Connection limit reached, rejecting {addr}");
            return;
        }
    };

    let conn_id = Uuid::new_v4();
    let challenge = generate_challenge();
    let cancel = CancellationToken::new();

    let (tx, rx) = mpsc::channel::<WsMessage>(state.config.send_buffer_size);

    let conn = Arc::new(ConnectionState {
        conn_id,
        remote_addr: addr,
        auth_state: RwLock::new(AuthState::Pending {
            challenge: challenge.clone(),
        }),
        subscriptions: Mutex::new(HashMap::new()),
        send_tx: tx.clone(),
        cancel: cancel.clone(),
    });

    info!(conn_id = %conn_id, addr = %addr, "WebSocket connection established");

    let challenge_msg = RelayMessage::auth_challenge(&challenge);
    if tx
        .send(WsMessage::Text(challenge_msg.into()))
        .await
        .is_err()
    {
        warn!(conn_id = %conn_id, "Failed to send AUTH challenge — client disconnected immediately");
        return;
    }

    // Register after challenge succeeds — avoids leaked entries on early disconnect.
    state
        .conn_manager
        .register(conn_id, tx.clone(), cancel.clone());

    let (ws_send, ws_recv) = socket.split();

    let send_cancel = cancel.child_token();
    let send_task = tokio::spawn(send_loop(ws_send, rx, send_cancel));

    let missed_pongs = Arc::new(AtomicU8::new(0));
    let heartbeat_cancel = cancel.clone();
    let heartbeat_task = tokio::spawn(heartbeat_loop(
        tx.clone(),
        Arc::clone(&missed_pongs),
        heartbeat_cancel,
    ));

    recv_loop(
        ws_recv,
        Arc::clone(&conn),
        Arc::clone(&state),
        Arc::clone(&missed_pongs),
        cancel.clone(),
    )
    .await;

    cancel.cancel();
    let _ = send_task.await;
    let _ = heartbeat_task.await;

    state.sub_registry.remove_connection(conn.conn_id);
    state.conn_manager.deregister(conn.conn_id);
    info!(conn_id = %conn_id, addr = %addr, "WebSocket connection closed");

    drop(permit);
}

async fn send_loop(
    mut ws_send: futures_util::stream::SplitSink<WebSocket, WsMessage>,
    mut rx: mpsc::Receiver<WsMessage>,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            Some(msg) = rx.recv() => {
                if ws_send.send(msg).await.is_err() {
                    break;
                }
            }
            _ = cancel.cancelled() => {
                let _ = ws_send.send(WsMessage::Close(None)).await;
                break;
            }
        }
    }
}

/// 3 missed pongs → disconnect.
async fn heartbeat_loop(
    tx: mpsc::Sender<WsMessage>,
    missed_pongs: Arc<AtomicU8>,
    cancel: CancellationToken,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                // fetch_add returns the *previous* value before incrementing:
                //   prev=0 → now 1 (first miss)
                //   prev=1 → now 2 (second miss)
                //   prev=2 → now 3 (third miss → disconnect)
                let missed = missed_pongs.fetch_add(1, Ordering::Relaxed);
                if missed >= 2 {
                    warn!("3 missed pongs — closing connection");
                    cancel.cancel();
                    break;
                }
                if tx.send(WsMessage::Ping(axum::body::Bytes::new())).await.is_err() {
                    break;
                }
            }
            _ = cancel.cancelled() => break,
        }
    }
}

/// NIP-11 advertised max_message_length. Frames exceeding this are rejected.
pub const MAX_FRAME_BYTES: usize = 65536;

async fn recv_loop(
    mut ws_recv: futures_util::stream::SplitStream<WebSocket>,
    conn: Arc<ConnectionState>,
    state: Arc<AppState>,
    missed_pongs: Arc<AtomicU8>,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            msg = ws_recv.next() => {
                match msg {
                    Some(Ok(WsMessage::Text(text))) => {
                        if text.len() > MAX_FRAME_BYTES {
                            warn!(conn_id = %conn.conn_id, bytes = text.len(), "frame too large — disconnecting");
                            break;
                        }
                        trace!(len = text.len(), "frame received");
                        handle_text_message(text.to_string(), Arc::clone(&conn), Arc::clone(&state)).await;
                    }
                    Some(Ok(WsMessage::Binary(bytes))) => {
                        if bytes.len() > MAX_FRAME_BYTES {
                            warn!(conn_id = %conn.conn_id, bytes = bytes.len(), "binary frame too large — disconnecting");
                            break;
                        }
                        // Binary frames: attempt UTF-8 decode and treat as text. Some clients
                        // (notably certain Nostr libraries) send text payloads in binary frames.
                        // NIP-01 is text-only, but accepting binary is a common relay extension.
                        if let Ok(text) = String::from_utf8(bytes.to_vec()) {
                            handle_text_message(text, Arc::clone(&conn), Arc::clone(&state)).await;
                        }
                    }
                    Some(Ok(WsMessage::Pong(_))) => {
                        missed_pongs.store(0, Ordering::Relaxed);
                    }
                    Some(Ok(WsMessage::Ping(data))) => {
                        let _ = conn.send_tx.try_send(WsMessage::Pong(data));
                    }
                    Some(Ok(WsMessage::Close(_))) | None => {
                        debug!("WebSocket closed by client");
                        break;
                    }
                    Some(Err(e)) => {
                        debug!("WebSocket error: {e}");
                        break;
                    }
                }
            }
            _ = cancel.cancelled() => break,
        }
    }
}

async fn handle_text_message(text: String, conn: Arc<ConnectionState>, state: Arc<AppState>) {
    let msg = match ClientMessage::parse(&text) {
        Ok(m) => m,
        Err(e) => {
            conn.send(RelayMessage::notice(&format!("invalid message: {e}")));
            return;
        }
    };

    match msg {
        ClientMessage::Auth(event) => {
            handlers::auth::handle_auth(event, Arc::clone(&conn), Arc::clone(&state)).await;
        }
        ClientMessage::Event(event) => {
            let conn = Arc::clone(&conn);
            let state = Arc::clone(&state);
            let permit = match state.handler_semaphore.clone().try_acquire_owned() {
                Ok(p) => p,
                Err(_) => {
                    conn.send(RelayMessage::notice(
                        "rate-limited: too many concurrent requests",
                    ));
                    return;
                }
            };
            tokio::spawn(async move {
                handlers::event::handle_event(event, conn, state).await;
                drop(permit);
            });
        }
        ClientMessage::Req { sub_id, filters } => {
            let conn = Arc::clone(&conn);
            let state = Arc::clone(&state);
            let permit = match state.handler_semaphore.clone().try_acquire_owned() {
                Ok(p) => p,
                Err(_) => {
                    conn.send(RelayMessage::notice(
                        "rate-limited: too many concurrent requests",
                    ));
                    return;
                }
            };
            tokio::spawn(async move {
                handlers::req::handle_req(sub_id, filters, conn, state).await;
                drop(permit);
            });
        }
        ClientMessage::Close(sub_id) => {
            handlers::close::handle_close(sub_id, Arc::clone(&conn), Arc::clone(&state)).await;
        }
    }
}
