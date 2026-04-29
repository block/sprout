//! Local read-only observer feed for ACP session activity.
//!
//! This is intentionally process-local infrastructure: it lets the desktop app
//! watch the raw ACP JSON-RPC stream without sending private execution detail
//! through the Sprout relay.

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
    routing::get,
    Router,
};
use futures_util::{stream, StreamExt};
use serde::Serialize;
use tokio::sync::broadcast;

const OBSERVER_BUFFER_CAP: usize = 1_000;

#[derive(Clone, Debug, Default)]
pub struct ObserverContext {
    pub channel_id: Option<String>,
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
}

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

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObserverEvent {
    pub seq: u64,
    pub timestamp: String,
    pub kind: String,
    pub agent_index: Option<usize>,
    pub channel_id: Option<String>,
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
    pub payload: serde_json::Value,
}

#[derive(Clone)]
struct ObserverServerState {
    observer: ObserverHandle,
    token: Option<String>,
}

#[derive(serde::Deserialize)]
struct EventQuery {
    token: Option<String>,
}

pub async fn spawn_observer_server(
    bind_addr: &str,
    token: Option<String>,
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
    };
    let app = Router::new()
        .route("/events", get(events_handler))
        .route("/health", get(health_handler))
        .with_state(state);

    tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, app).await {
            tracing::warn!(target: "observer", "observer server stopped: {error}");
        }
    });

    Ok(observer)
}

impl ObserverHandle {
    pub fn addr(&self) -> SocketAddr {
        self.inner.addr
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ObserverEvent> {
        self.inner.tx.subscribe()
    }

    pub fn snapshot(&self) -> Vec<ObserverEvent> {
        match self.inner.buffer.lock() {
            Ok(buffer) => buffer.iter().cloned().collect(),
            Err(error) => {
                tracing::warn!(target: "observer", "observer replay buffer lock poisoned: {error}");
                Vec::new()
            }
        }
    }

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
    })
    .to_string()
    .into_response()
}

async fn events_handler(
    State(state): State<ObserverServerState>,
    Query(query): Query<EventQuery>,
    headers: HeaderMap,
) -> Response {
    if !token_matches(state.token.as_deref(), query.token.as_deref()) {
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
    use axum::http::HeaderValue;

    use super::{origin_allowed, token_matches};

    #[test]
    fn observer_token_requires_exact_match_when_configured() {
        assert!(token_matches(Some("secret"), Some("secret")));
        assert!(!token_matches(Some("secret"), Some("wrong")));
        assert!(!token_matches(Some("secret"), None));
        assert!(token_matches(None, None));
        assert!(token_matches(None, Some("anything")));
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
