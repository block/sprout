//! Local observer and control endpoint for ACP session activity.
//!
//! This is intentionally process-local infrastructure: it lets the desktop app
//! watch the raw ACP JSON-RPC stream and send tightly-scoped control commands
//! without sending private execution detail through the Sprout relay.

use std::{
    collections::VecDeque,
    convert::Infallible,
    net::SocketAddr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Json, Router,
};
use futures_util::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, oneshot};

const OBSERVER_BUFFER_CAP: usize = 1_000;

/// Best-effort metadata attached to observer events.
#[derive(Clone, Debug, Default)]
pub struct ObserverContext {
    /// Sprout channel UUID for the current turn, when channel-scoped.
    pub channel_id: Option<String>,
    /// ACP session ID associated with the current turn, once known.
    pub session_id: Option<String>,
    /// Local UUID for one prompt turn.
    pub turn_id: Option<String>,
}

/// Handle used by the harness to publish local observer events.
#[derive(Clone)]
pub struct ObserverHandle {
    inner: Arc<ObserverInner>,
}

struct ObserverInner {
    tx: broadcast::Sender<ObserverEvent>,
    buffer: Mutex<VecDeque<ObserverEvent>>,
    seq: AtomicU64,
    addr: SocketAddr,
}

/// Event delivered over the local observer SSE stream.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObserverEvent {
    /// Monotonic process-local sequence number.
    pub seq: u64,
    /// RFC3339 UTC timestamp.
    pub timestamp: String,
    /// Observer event kind, for example `acp_read` or `turn_started`.
    pub kind: String,
    /// Pool slot index for the agent process that emitted the event.
    pub agent_index: Option<usize>,
    /// Sprout channel UUID for channel-scoped events.
    pub channel_id: Option<String>,
    /// ACP session ID when known.
    pub session_id: Option<String>,
    /// Local UUID for one prompt turn.
    pub turn_id: Option<String>,
    /// Raw or semantic event payload.
    pub payload: serde_json::Value,
}

/// Commands accepted by the observer control loop.
#[derive(Debug)]
pub enum ObserverControlCommand {
    /// Stop the active turn for a channel, if one exists.
    CancelTurn {
        channel_id: uuid::Uuid,
        respond_to: oneshot::Sender<CancelTurnResponse>,
    },
}

/// Response returned by observer control commands.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelTurnResponse {
    /// Result of attempting to send the cancel signal.
    pub status: CancelTurnStatus,
}

/// Status for a cancel-turn request.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelTurnStatus {
    /// A cancel signal was sent to an in-flight channel turn.
    Sent,
    /// The channel had no active turn at the time of the request.
    NoActiveTurn,
}

#[derive(Clone)]
struct ObserverServerState {
    observer: ObserverHandle,
    token: Option<String>,
    control_tx: Option<mpsc::Sender<ObserverControlCommand>>,
}

#[derive(Deserialize)]
struct EventQuery {
    token: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CancelTurnRequest {
    channel_id: uuid::Uuid,
}

/// Start the loopback observer HTTP server.
pub async fn spawn_observer_server(
    bind_addr: &str,
    token: Option<String>,
    control_tx: Option<mpsc::Sender<ObserverControlCommand>>,
) -> anyhow::Result<ObserverHandle> {
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    let addr = listener.local_addr()?;
    if !addr.ip().is_loopback() {
        anyhow::bail!("observer bind address must be loopback, got {addr}");
    }

    let (tx, _) = broadcast::channel(OBSERVER_BUFFER_CAP);
    let observer = ObserverHandle {
        inner: Arc::new(ObserverInner {
            tx,
            buffer: Mutex::new(VecDeque::with_capacity(OBSERVER_BUFFER_CAP)),
            seq: AtomicU64::new(1),
            addr,
        }),
    };
    let state = ObserverServerState {
        observer: observer.clone(),
        token,
        control_tx,
    };
    let app = Router::new()
        .route("/events", get(events_handler))
        .route("/health", get(health_handler))
        .route("/control/cancel", post(cancel_turn_handler))
        .with_state(state);

    tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, app).await {
            tracing::warn!(target: "observer", "observer server stopped: {error}");
        }
    });

    Ok(observer)
}

impl ObserverHandle {
    /// Return the bound loopback address.
    pub fn addr(&self) -> SocketAddr {
        self.inner.addr
    }

    /// Subscribe to live observer events.
    pub fn subscribe(&self) -> broadcast::Receiver<ObserverEvent> {
        self.inner.tx.subscribe()
    }

    /// Return the current replay buffer.
    pub fn snapshot(&self) -> Vec<ObserverEvent> {
        match self.inner.buffer.lock() {
            Ok(buffer) => buffer.iter().cloned().collect(),
            Err(error) => {
                tracing::warn!(target: "observer", "observer replay buffer lock poisoned: {error}");
                Vec::new()
            }
        }
    }

    /// Emit a local observer event.
    pub fn emit(
        &self,
        kind: impl Into<String>,
        agent_index: Option<usize>,
        context: &ObserverContext,
        payload: serde_json::Value,
    ) {
        let event = ObserverEvent {
            seq: self.inner.seq.fetch_add(1, Ordering::Relaxed),
            timestamp: chrono::Utc::now().to_rfc3339(),
            kind: kind.into(),
            agent_index,
            channel_id: context.channel_id.clone(),
            session_id: context.session_id.clone(),
            turn_id: context.turn_id.clone(),
            payload,
        };

        match self.inner.buffer.lock() {
            Ok(mut buffer) => {
                if buffer.len() >= OBSERVER_BUFFER_CAP {
                    buffer.pop_front();
                }
                buffer.push_back(event.clone());
            }
            Err(error) => {
                tracing::warn!(target: "observer", "observer replay buffer lock poisoned: {error}");
            }
        }

        let _ = self.inner.tx.send(event);
    }
}

async fn health_handler(State(state): State<ObserverServerState>) -> Response {
    serde_json::json!({
        "ok": true,
        "addr": state.observer.addr().to_string(),
        "control": state.control_tx.is_some(),
    })
    .to_string()
    .into_response()
}

async fn events_handler(
    State(state): State<ObserverServerState>,
    Query(query): Query<EventQuery>,
    headers: HeaderMap,
) -> Response {
    if !authorized(state.token.as_deref(), query.token.as_deref(), &headers) {
        return (StatusCode::UNAUTHORIZED, "invalid observer token").into_response();
    }

    let origin = headers.get(header::ORIGIN).cloned();
    if !origin_allowed(origin.as_ref()) {
        return (StatusCode::FORBIDDEN, "forbidden: invalid origin").into_response();
    }

    let replay = state.observer.snapshot();
    let replay_stream = stream::iter(replay.into_iter().map(event_to_sse));
    let live_rx = state.observer.subscribe();
    let live_stream = stream::unfold(live_rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(event) => return Some((event_to_sse(event), rx)),
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    });

    let stream = replay_stream.chain(live_stream);
    let sse = Sse::new(stream).keep_alive(KeepAlive::default());
    let mut response = sse.into_response();
    if let Some(origin) = origin {
        response
            .headers_mut()
            .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    }
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, no-cache"),
    );
    response
}

async fn cancel_turn_handler(
    State(state): State<ObserverServerState>,
    Query(query): Query<EventQuery>,
    headers: HeaderMap,
    Json(request): Json<CancelTurnRequest>,
) -> Response {
    if !authorized(state.token.as_deref(), query.token.as_deref(), &headers) {
        return (StatusCode::UNAUTHORIZED, "invalid observer token").into_response();
    }

    if !origin_allowed(headers.get(header::ORIGIN)) {
        return (StatusCode::FORBIDDEN, "forbidden: invalid origin").into_response();
    }

    let Some(control_tx) = state.control_tx else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "observer control is not available",
        )
            .into_response();
    };

    let (respond_to, response_rx) = oneshot::channel();
    let command = ObserverControlCommand::CancelTurn {
        channel_id: request.channel_id,
        respond_to,
    };

    if control_tx.send(command).await.is_err() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "observer control loop is not available",
        )
            .into_response();
    }

    match tokio::time::timeout(std::time::Duration::from_secs(2), response_rx).await {
        Ok(Ok(response)) => Json(response).into_response(),
        Ok(Err(_)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "observer control response was dropped",
        )
            .into_response(),
        Err(_) => (StatusCode::GATEWAY_TIMEOUT, "observer control timed out").into_response(),
    }
}

fn authorized(expected: Option<&str>, query_token: Option<&str>, headers: &HeaderMap) -> bool {
    token_matches(expected, query_token) || token_matches(expected, bearer_token(headers))
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    value.strip_prefix("Bearer ")
}

fn token_matches(expected: Option<&str>, actual: Option<&str>) -> bool {
    match expected {
        Some(expected) => actual == Some(expected),
        None => true,
    }
}

fn origin_allowed(origin: Option<&HeaderValue>) -> bool {
    let Some(origin) = origin.and_then(|value| value.to_str().ok()) else {
        return true;
    };

    origin == "tauri://localhost"
        || origin == "http://tauri.localhost"
        || origin == "https://tauri.localhost"
        || origin.starts_with("http://localhost:")
        || origin.starts_with("http://127.0.0.1:")
}

fn event_to_sse(event: ObserverEvent) -> Result<Event, Infallible> {
    let data = serde_json::to_string(&event).unwrap_or_else(|error| {
        serde_json::json!({
            "seq": event.seq,
            "timestamp": event.timestamp,
            "kind": "observer_serialize_error",
            "payload": {"error": error.to_string()},
        })
        .to_string()
    });
    Ok(Event::default().id(event.seq.to_string()).data(data))
}

/// Build observer context values from optional channel/session/turn IDs.
pub fn context_for(
    channel_id: Option<uuid::Uuid>,
    session_id: Option<String>,
    turn_id: Option<String>,
) -> ObserverContext {
    ObserverContext {
        channel_id: channel_id.map(|id| id.to_string()),
        session_id,
        turn_id,
    }
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue};

    use super::{authorized, origin_allowed, token_matches};

    #[test]
    fn observer_token_requires_exact_match_when_configured() {
        assert!(token_matches(Some("secret"), Some("secret")));
        assert!(!token_matches(Some("secret"), Some("wrong")));
        assert!(!token_matches(Some("secret"), None));
        assert!(token_matches(None, None));
        assert!(token_matches(None, Some("anything")));
    }

    #[test]
    fn observer_authorization_accepts_bearer_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer secret"),
        );
        assert!(authorized(Some("secret"), None, &headers));
        assert!(!authorized(Some("other"), None, &headers));
    }

    #[test]
    fn observer_origin_allows_tauri_and_loopback_only() {
        assert!(origin_allowed(None));
        assert!(origin_allowed(Some(&HeaderValue::from_static(
            "tauri://localhost"
        ))));
        assert!(origin_allowed(Some(&HeaderValue::from_static(
            "http://localhost:1420"
        ))));
        assert!(origin_allowed(Some(&HeaderValue::from_static(
            "http://127.0.0.1:1420"
        ))));
        assert!(!origin_allowed(Some(&HeaderValue::from_static(
            "https://example.com"
        ))));
    }
}
