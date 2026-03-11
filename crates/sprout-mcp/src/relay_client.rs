use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use nostr::{Event, EventBuilder, Filter, Keys, Kind, Tag, Url};
use serde_json::{json, Value};
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use tracing::debug;

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

type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

struct Inner {
    ws: WsStream,
    buffer: VecDeque<RelayMessage>,
    pending_challenge: Option<String>,
}

impl Inner {
    async fn send_raw(&mut self, value: &Value) -> Result<(), RelayClientError> {
        let text = serde_json::to_string(value)?;
        self.ws.send(Message::Text(text.into())).await?;
        Ok(())
    }

    // wait_for_auth_challenge, wait_for_ok, and collect_until_eose share a similar
    // deadline-loop structure but differ in termination condition and what they do
    // with interleaved messages, so they cannot be collapsed into a single helper.
    async fn wait_for_auth_challenge(
        &mut self,
        timeout_dur: Duration,
    ) -> Result<String, RelayClientError> {
        if let Some(challenge) = self.pending_challenge.take() {
            return Ok(challenge);
        }

        if let Some(idx) = self
            .buffer
            .iter()
            .position(|m| matches!(m, RelayMessage::Auth { .. }))
        {
            if let Some(RelayMessage::Auth { challenge }) = self.buffer.remove(idx) {
                return Ok(challenge);
            }
        }

        let deadline = tokio::time::Instant::now() + timeout_dur;

        loop {
            let remaining = deadline
                .checked_duration_since(tokio::time::Instant::now())
                .unwrap_or(Duration::ZERO);

            if remaining.is_zero() {
                return Err(RelayClientError::NoAuthChallenge);
            }

            let raw = timeout(remaining, self.ws.next())
                .await
                .map_err(|_| RelayClientError::NoAuthChallenge)?
                .ok_or(RelayClientError::ConnectionClosed)?
                .map_err(RelayClientError::WebSocket)?;

            match raw {
                Message::Text(text) => {
                    let msg = parse_relay_message(&text)?;
                    match msg {
                        RelayMessage::Auth { challenge } => return Ok(challenge),
                        other => self.buffer.push_back(other),
                    }
                }
                Message::Ping(data) => {
                    self.ws.send(Message::Pong(data)).await?;
                }
                Message::Close(_) => return Err(RelayClientError::ConnectionClosed),
                _ => {}
            }
        }
    }

    async fn wait_for_ok(
        &mut self,
        event_id: &str,
        timeout_dur: Duration,
    ) -> Result<OkResponse, RelayClientError> {
        let deadline = tokio::time::Instant::now() + timeout_dur;

        if let Some(idx) = self
            .buffer
            .iter()
            .position(|m| matches!(m, RelayMessage::Ok(ok) if ok.event_id == event_id))
        {
            if let Some(RelayMessage::Ok(ok)) = self.buffer.remove(idx) {
                return Ok(ok);
            }
        }

        loop {
            let remaining = deadline
                .checked_duration_since(tokio::time::Instant::now())
                .unwrap_or(Duration::ZERO);

            if remaining.is_zero() {
                return Err(RelayClientError::Timeout);
            }

            let raw = timeout(remaining, self.ws.next())
                .await
                .map_err(|_| RelayClientError::Timeout)?
                .ok_or(RelayClientError::ConnectionClosed)?
                .map_err(RelayClientError::WebSocket)?;

            match raw {
                Message::Text(text) => {
                    let msg = parse_relay_message(&text)?;
                    match msg {
                        RelayMessage::Ok(ok) if ok.event_id == event_id => return Ok(ok),
                        RelayMessage::Auth { ref challenge } => {
                            self.pending_challenge = Some(challenge.clone());
                            self.buffer.push_back(msg);
                        }
                        other => self.buffer.push_back(other),
                    }
                }
                Message::Ping(data) => {
                    self.ws.send(Message::Pong(data)).await?;
                }
                Message::Close(_) => return Err(RelayClientError::ConnectionClosed),
                _ => {}
            }
        }
    }

    async fn collect_until_eose(
        &mut self,
        sub_id: &str,
        timeout_dur: Duration,
    ) -> Result<Vec<Event>, RelayClientError> {
        let deadline = tokio::time::Instant::now() + timeout_dur;
        let mut events = Vec::new();

        let old_buffer = std::mem::take(&mut self.buffer);
        let mut found_eose = false;
        for msg in old_buffer {
            if found_eose {
                self.buffer.push_back(msg);
                continue;
            }
            match msg {
                RelayMessage::Event {
                    subscription_id,
                    event,
                } if subscription_id == sub_id => {
                    events.push(*event);
                }
                RelayMessage::Eose { subscription_id } if subscription_id == sub_id => {
                    found_eose = true;
                }
                other => self.buffer.push_back(other),
            }
        }
        if found_eose {
            return Ok(events);
        }

        loop {
            let remaining = deadline
                .checked_duration_since(tokio::time::Instant::now())
                .unwrap_or(Duration::ZERO);

            if remaining.is_zero() {
                return Err(RelayClientError::Timeout);
            }

            let raw = timeout(remaining, self.ws.next())
                .await
                .map_err(|_| RelayClientError::Timeout)?
                .ok_or(RelayClientError::ConnectionClosed)?
                .map_err(RelayClientError::WebSocket)?;

            match raw {
                Message::Text(text) => {
                    let msg = parse_relay_message(&text)?;
                    match msg {
                        RelayMessage::Event {
                            subscription_id,
                            event,
                        } if subscription_id == sub_id => {
                            events.push(*event);
                        }
                        RelayMessage::Eose { subscription_id } if subscription_id == sub_id => {
                            return Ok(events);
                        }
                        RelayMessage::Auth { ref challenge } => {
                            self.pending_challenge = Some(challenge.clone());
                            self.buffer.push_back(msg);
                        }
                        other => self.buffer.push_back(other),
                    }
                }
                Message::Ping(data) => {
                    self.ws.send(Message::Pong(data)).await?;
                }
                Message::Close(_) => return Err(RelayClientError::ConnectionClosed),
                _ => {}
            }
        }
    }
}

/// Clone-able WebSocket client for the Sprout relay.
///
/// All clones share the same underlying connection via `Arc<Mutex<Inner>>`.
/// Active subscriptions are tracked so they can be resubmitted after a reconnect.
#[derive(Clone)]
pub struct RelayClient {
    inner: Arc<Mutex<Inner>>,
    keys: Keys,
    /// WebSocket URL of the relay (e.g. "ws://localhost:3000").
    relay_url: String,
    /// Shared reqwest client for REST API calls.
    http: reqwest::Client,
    /// Optional API token for Bearer auth on REST endpoints.
    /// When present, REST calls send `Authorization: Bearer <token>` instead of `X-Pubkey`.
    api_token: Option<String>,
    /// Active subscriptions: sub_id → filters. Used to resubscribe after reconnect.
    active_subscriptions: Arc<Mutex<HashMap<String, Vec<Filter>>>>,
}

impl RelayClient {
    /// Perform a single connection + NIP-42 authentication attempt.
    /// Returns the authenticated `Inner` on success.
    async fn try_connect(
        relay_url: &str,
        keys: &Keys,
        api_token: Option<&str>,
    ) -> Result<Inner, RelayClientError> {
        let parsed = relay_url
            .parse::<url::Url>()
            .map_err(|e| RelayClientError::Url(e.to_string()))?;

        let (ws, _response) = connect_async(parsed.as_str())
            .await
            .map_err(RelayClientError::WebSocket)?;

        debug!("connected to relay at {relay_url}");

        let mut inner = Inner {
            ws,
            buffer: VecDeque::new(),
            pending_challenge: None,
        };

        let challenge = inner
            .wait_for_auth_challenge(Duration::from_secs(5))
            .await?;

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
        // Log only the event ID, never the full AUTH payload which may contain tokens.
        debug!("sending AUTH event {event_id}");
        let msg = json!(["AUTH", auth_event]);
        inner.send_raw(&msg).await?;

        let ok = inner.wait_for_ok(&event_id, Duration::from_secs(5)).await?;

        if !ok.accepted {
            return Err(RelayClientError::AuthFailed(ok.message));
        }

        debug!("NIP-42 authentication successful");
        Ok(inner)
    }

    /// Connect to the relay with exponential-backoff retry.
    ///
    /// Attempts `try_connect` in a loop, doubling the delay on each failure
    /// (1 s → 2 s → 4 s → … → 30 s max). Returns only when a connection
    /// and NIP-42 auth handshake succeed.
    async fn connect_with_retry(relay_url: &str, keys: &Keys, api_token: Option<&str>) -> Inner {
        let mut delay = Duration::from_secs(1);
        let max_delay = Duration::from_secs(30);
        loop {
            match Self::try_connect(relay_url, keys, api_token).await {
                Ok(inner) => {
                    tracing::info!("connected to relay at {relay_url}");
                    return inner;
                }
                Err(e) => {
                    tracing::warn!("connection failed: {e}, retrying in {delay:?}");
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(max_delay);
                }
            }
        }
    }

    /// Connect to the relay (first connection; returns an error rather than retrying
    /// so the caller can surface a startup failure immediately).
    pub async fn connect(
        relay_url: &str,
        keys: &Keys,
        api_token: Option<&str>,
    ) -> Result<Self, RelayClientError> {
        let inner = Self::try_connect(relay_url, keys, api_token).await?;

        Ok(Self {
            keys: keys.clone(),
            relay_url: relay_url.to_string(),
            http: reqwest::Client::new(),
            inner: Arc::new(Mutex::new(inner)),
            api_token: api_token.map(|t| t.to_string()),
            active_subscriptions: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Reconnect after a connection loss: replace the inner WebSocket with a fresh
    /// authenticated connection (using exponential backoff), then resubscribe to all
    /// subscriptions that were active at the time of the disconnect.
    pub async fn reconnect(&self) {
        tracing::warn!("relay connection lost — reconnecting…");
        let new_inner =
            Self::connect_with_retry(&self.relay_url, &self.keys, self.api_token.as_deref()).await;

        {
            let mut inner = self.inner.lock().await;
            *inner = new_inner;
        }

        let subs = self.active_subscriptions.lock().await.clone();
        if !subs.is_empty() {
            tracing::info!("resubscribing to {} active subscription(s)", subs.len());
            for (sub_id, filters) in &subs {
                let mut inner = self.inner.lock().await;
                let mut msg: Vec<Value> = Vec::with_capacity(2 + filters.len());
                msg.push(json!("REQ"));
                msg.push(json!(sub_id));
                for f in filters {
                    match serde_json::to_value(f) {
                        Ok(v) => msg.push(v),
                        Err(e) => {
                            tracing::warn!("failed to serialize filter for {sub_id}: {e}");
                        }
                    }
                }
                if let Err(e) = inner.send_raw(&Value::Array(msg)).await {
                    tracing::warn!("failed to resubscribe to {sub_id}: {e}");
                }
            }
        }
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
        let mut inner = self.inner.lock().await;
        let event_id = event.id.to_hex();
        let msg = json!(["EVENT", event]);
        inner.send_raw(&msg).await?;
        inner.wait_for_ok(&event_id, Duration::from_secs(10)).await
    }

    /// Open a subscription with the given filters and collect all stored events until `EOSE`.
    pub async fn subscribe(
        &self,
        sub_id: &str,
        filters: Vec<Filter>,
    ) -> Result<Vec<Event>, RelayClientError> {
        // Track this subscription so it can be resubmitted after a reconnect.
        self.active_subscriptions
            .lock()
            .await
            .insert(sub_id.to_string(), filters.clone());

        let mut inner = self.inner.lock().await;

        let mut msg: Vec<Value> = Vec::with_capacity(2 + filters.len());
        msg.push(json!("REQ"));
        msg.push(json!(sub_id));
        for f in &filters {
            msg.push(serde_json::to_value(f)?);
        }
        inner.send_raw(&Value::Array(msg)).await?;

        inner
            .collect_until_eose(sub_id, Duration::from_secs(10))
            .await
    }

    /// Send a `CLOSE` message to the relay and remove the subscription from the active set.
    pub async fn close_subscription(&self, sub_id: &str) -> Result<(), RelayClientError> {
        // Remove from active subscriptions — no longer needs to be resubscribed.
        self.active_subscriptions.lock().await.remove(sub_id);

        let mut inner = self.inner.lock().await;
        let msg = json!(["CLOSE", sub_id]);
        inner.send_raw(&msg).await
    }

    /// Perform a clean WebSocket close handshake.
    pub async fn close(&self) -> Result<(), RelayClientError> {
        let mut inner = self.inner.lock().await;
        inner.ws.close(None).await?;
        Ok(())
    }
}

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
