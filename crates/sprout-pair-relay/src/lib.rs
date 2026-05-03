//! Ephemeral sidecar relay for NIP-AB device pairing handshakes.
//!
//! Accepts WebSocket connections, matches incoming kind:24134 events against
//! live `#p`-filtered subscriptions, and forwards matches to the subscriber.
//! No persistence. No auth. No history.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::header::{
    HeaderValue, CONNECTION, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY, SEC_WEBSOCKET_VERSION,
    UPGRADE,
};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::upgrade::Upgraded;
use hyper::{Method, Request, Response, StatusCode, Version};
use hyper_util::rt::TokioIo;
use parking_lot::Mutex;
use serde_json::Value;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;
use tokio_tungstenite::tungstenite::protocol::{Message, Role, WebSocketConfig};
use tokio_tungstenite::WebSocketStream;
use tokio_util::sync::CancellationToken;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Hard per-connection lifetime. `pub(crate)` for test access.
pub(crate) const CONN_TIMEOUT: Duration = Duration::from_secs(120);

const MAX_CONNS: u32 = 128;
const CHANNEL_CAP: usize = 4;
const KIND_PAIR: u64 = 24134;
/// Max WebSocket frame/message size. NIP-AB handshake payloads are small
/// (ephemeral pubkeys + encrypted session data), well under 4 KiB.
const MAX_FRAME: usize = 4096;
const RATE_WINDOW: Duration = Duration::from_secs(10);
const RATE_MSG_MAX: u32 = 20;
const RATE_EVENT_MAX: u32 = 10;
const MAX_TAGS: usize = 16;
const MAX_TAG_STR: usize = 128;
const SUB_ID_MAX: usize = 64;

// ── Core types ────────────────────────────────────────────────────────────────

enum OutMsg {
    Text(String),
    Pong(Vec<u8>),
    Close,
}

struct Sub {
    conn_id: u64,
    sub_id: String,
    p_value: [u8; 32],
    writer_tx: mpsc::Sender<OutMsg>,
}

pub struct Relay {
    subs: Mutex<Vec<Sub>>,
    conn_count: AtomicU32,
    next_conn_id: AtomicU64,
}

impl Default for Relay {
    fn default() -> Self {
        Self::new()
    }
}

impl Relay {
    pub fn new() -> Self {
        Self {
            subs: Mutex::new(Vec::new()),
            conn_count: AtomicU32::new(0),
            next_conn_id: AtomicU64::new(0),
        }
    }

    /// Fan-out: send event to all subscribers whose `p_value` matches.
    fn fanout(&self, p_value: &[u8; 32], event: &Value) {
        let subs = self.subs.lock();
        for sub in subs.iter() {
            if &sub.p_value == p_value {
                let msg = Value::Array(vec![
                    Value::String("EVENT".into()),
                    Value::String(sub.sub_id.clone()),
                    event.clone(),
                ]);
                if let Ok(s) = serde_json::to_string(&msg) {
                    let _ = sub.writer_tx.try_send(OutMsg::Text(s));
                }
            }
        }
    }

    fn remove_sub(&self, conn_id: u64) {
        self.subs.lock().retain(|s| s.conn_id != conn_id);
    }
}

// ── RAII connection guard ─────────────────────────────────────────────────────

struct ConnGuard {
    relay: Arc<Relay>,
    conn_id: u64,
}

impl Drop for ConnGuard {
    fn drop(&mut self) {
        self.relay.remove_sub(self.conn_id);
        self.relay.conn_count.fetch_sub(1, Ordering::Relaxed);
        eprintln!(
            "conn closed conn_id={} active={}",
            self.conn_id,
            self.relay.conn_count.load(Ordering::Relaxed)
        );
    }
}

// ── Rate limiter ──────────────────────────────────────────────────────────────

struct RateWindow {
    count: u32,
    window_start: tokio::time::Instant,
}

impl RateWindow {
    fn new() -> Self {
        Self {
            count: 0,
            window_start: tokio::time::Instant::now(),
        }
    }

    fn tick(&mut self) -> u32 {
        if self.window_start.elapsed() >= RATE_WINDOW {
            self.count = 0;
            self.window_start = tokio::time::Instant::now();
        }
        self.count += 1;
        self.count
    }
}

// ── JSON helpers ──────────────────────────────────────────────────────────────

fn jarr(v: Vec<Value>) -> String {
    Value::Array(v).to_string()
}

fn make_ok(id: &str, ok: bool, msg: &str) -> String {
    jarr(vec![
        Value::String("OK".into()),
        Value::String(id.into()),
        Value::Bool(ok),
        Value::String(msg.into()),
    ])
}

fn make_closed(sub_id: &str, msg: &str) -> String {
    jarr(vec![
        Value::String("CLOSED".into()),
        Value::String(sub_id.into()),
        Value::String(msg.into()),
    ])
}

fn make_eose(sub_id: &str) -> String {
    jarr(vec![
        Value::String("EOSE".into()),
        Value::String(sub_id.into()),
    ])
}

fn make_notice(msg: &str) -> String {
    jarr(vec![
        Value::String("NOTICE".into()),
        Value::String(msg.into()),
    ])
}

// ── Validation ────────────────────────────────────────────────────────────────

fn is_lower_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn decode_hex32(s: &str) -> Option<[u8; 32]> {
    if !is_lower_hex(s, 64) {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
        let hi = (chunk[0] as char).to_digit(16)?;
        let lo = (chunk[1] as char).to_digit(16)?;
        out[i] = (hi * 16 + lo) as u8;
    }
    Some(out)
}

/// Validate a REQ filter. Returns `Ok(p_value)` or `Err(reason)`.
fn validate_filter(filter: &Value) -> Result<[u8; 32], &'static str> {
    let obj = filter.as_object().ok_or("filter must be an object")?;
    for key in obj.keys() {
        match key.as_str() {
            "kinds" | "#p" => {}
            _ => return Err("unsupported filter field"),
        }
    }
    if let Some(kinds) = obj.get("kinds") {
        let arr = kinds.as_array().ok_or("kinds must be an array")?;
        if arr.len() != 1 || arr[0].as_u64() != Some(KIND_PAIR) {
            return Err("kinds must be [24134]");
        }
    }
    let p_arr = obj
        .get("#p")
        .and_then(|v| v.as_array())
        .ok_or("#p filter required")?;
    if p_arr.len() != 1 {
        return Err("#p must have exactly one value");
    }
    let p_str = p_arr[0].as_str().ok_or("#p value must be a string")?;
    decode_hex32(p_str).ok_or("#p value must be 64 lowercase hex chars")
}

/// Validate an EVENT object. Returns `Ok((event_id, p_value))` or `Err(reason)`.
fn validate_event(ev: &Value) -> Result<(String, [u8; 32]), &'static str> {
    let obj = ev.as_object().ok_or("event must be an object")?;
    let id = obj.get("id").and_then(|v| v.as_str()).ok_or("missing id")?;
    if !is_lower_hex(id, 64) {
        return Err("id must be 64 lowercase hex chars");
    }
    let pubkey = obj
        .get("pubkey")
        .and_then(|v| v.as_str())
        .ok_or("missing pubkey")?;
    if !is_lower_hex(pubkey, 64) {
        return Err("pubkey must be 64 lowercase hex chars");
    }
    if obj.get("kind").and_then(|v| v.as_u64()) != Some(KIND_PAIR) {
        return Err("kind must be 24134");
    }
    obj.get("created_at")
        .and_then(|v| v.as_i64())
        .ok_or("missing created_at")?;
    obj.get("content")
        .and_then(|v| v.as_str())
        .ok_or("missing content")?;
    let sig = obj
        .get("sig")
        .and_then(|v| v.as_str())
        .ok_or("missing sig")?;
    if !is_lower_hex(sig, 128) {
        return Err("sig must be 128 lowercase hex chars");
    }

    let tags = obj
        .get("tags")
        .and_then(|v| v.as_array())
        .ok_or("missing tags")?;
    if tags.len() > MAX_TAGS {
        return Err("too many tags");
    }

    let mut p_bytes: Option<[u8; 32]> = None;
    let mut p_count = 0usize;
    for tag in tags {
        let arr = tag.as_array().ok_or("tag must be an array")?;
        if arr.is_empty() {
            return Err("tag must be non-empty");
        }
        for elem in arr {
            if elem.as_str().ok_or("tag elements must be strings")?.len() > MAX_TAG_STR {
                return Err("tag string too long");
            }
        }
        if arr[0].as_str() == Some("p") {
            p_count += 1;
            if p_count > 1 {
                return Err("event must have exactly one p tag");
            }
            let p_str = arr
                .get(1)
                .and_then(|v| v.as_str())
                .ok_or("p tag missing value")?;
            p_bytes =
                Some(decode_hex32(p_str).ok_or("p tag value must be 64 lowercase hex chars")?);
        }
    }
    if p_count != 1 {
        return Err("event must have exactly one p tag");
    }
    // p_count == 1 guarantees p_bytes is Some; use ok_or for a clean error path.
    let p_bytes = p_bytes.ok_or("event must have exactly one p tag")?;
    Ok((id.to_string(), p_bytes))
}

fn safe_event_id(ev: &Value) -> String {
    ev.get("id")
        .and_then(|v| v.as_str())
        .filter(|s| is_lower_hex(s, 64))
        .unwrap_or("")
        .to_string()
}

// ── Writer task ───────────────────────────────────────────────────────────────

type WsSink = futures_util::stream::SplitSink<WebSocketStream<TokioIo<Upgraded>>, Message>;

async fn writer_task(mut sink: WsSink, mut rx: mpsc::Receiver<OutMsg>, cancel: CancellationToken) {
    loop {
        let msg = tokio::select! {
            _ = cancel.cancelled() => break,
            m = rx.recv() => match m { Some(m) => m, None => break },
        };
        let ws_msg = match msg {
            OutMsg::Text(s) => Message::Text(s.into()),
            OutMsg::Pong(d) => Message::Pong(d.into()),
            OutMsg::Close => Message::Close(None),
        };
        let result = tokio::select! {
            _ = cancel.cancelled() => break,
            r = timeout(Duration::from_secs(5), sink.send(ws_msg)) => r,
        };
        match result {
            Err(_) => break,     // timeout
            Ok(Err(_)) => break, // send error
            Ok(Ok(())) => {}     // success
        }
    }
}

// ── Connection handler ────────────────────────────────────────────────────────

async fn handle_conn(relay: Arc<Relay>, conn_id: u64, stream: WebSocketStream<TokioIo<Upgraded>>) {
    let _guard = ConnGuard {
        relay: Arc::clone(&relay),
        conn_id,
    };
    let (sink, mut source) = stream.split();
    let (tx, rx) = mpsc::channel::<OutMsg>(CHANNEL_CAP);
    let cancel = CancellationToken::new();
    let writer_handle = tokio::spawn(writer_task(sink, rx, cancel.clone()));
    tokio::pin!(writer_handle);

    let mut msg_rate = RateWindow::new();
    let mut event_rate = RateWindow::new();
    let mut sub_id: Option<String> = None;
    let deadline = tokio::time::sleep(CONN_TIMEOUT);
    tokio::pin!(deadline);

    'conn: loop {
        let frame = tokio::select! {
            _ = &mut deadline => break 'conn,
            _ = &mut writer_handle => break 'conn,  // writer died → close
            f = source.next() => match f { Some(f) => f, None => break 'conn },
        };

        // All inbound frames count toward the message rate limit.
        if msg_rate.tick() > RATE_MSG_MAX {
            eprintln!("conn_id={} rate-limited (msg)", conn_id);
            break 'conn;
        }

        let frame = match frame {
            Ok(f) => f,
            Err(_) => break 'conn,
        };

        match frame {
            Message::Binary(_) | Message::Frame(_) => break 'conn,

            Message::Ping(data) => {
                if tx.try_send(OutMsg::Pong(data.to_vec())).is_err() {
                    break 'conn;
                }
            }

            Message::Pong(_) => {}

            Message::Close(_) => {
                let _ = tx.try_send(OutMsg::Close);
                break 'conn;
            }

            Message::Text(text) => {
                let arr: Vec<Value> = match serde_json::from_str::<Value>(text.as_str()).ok() {
                    Some(Value::Array(a)) if !a.is_empty() => a,
                    _ => {
                        let _ = tx.try_send(OutMsg::Text(make_notice("error: invalid message")));
                        continue;
                    }
                };

                let verb = match arr[0].as_str() {
                    Some(v) => v.to_string(),
                    None => {
                        let _ = tx.try_send(OutMsg::Text(make_notice("error: invalid message")));
                        continue;
                    }
                };

                match verb.as_str() {
                    "REQ" => {
                        // Structural validation first (before we have a valid sub_id).
                        if arr.len() < 3 || !arr[1].is_string() {
                            let _ = tx.try_send(OutMsg::Text(make_notice("error: invalid REQ")));
                            continue;
                        }
                        // Now we know arr[1] is a string.
                        let client_sub_id = match arr[1].as_str() {
                            Some(s) if s.len() <= SUB_ID_MAX => s.to_string(),
                            Some(_) => {
                                let _ = tx.try_send(OutMsg::Text(make_closed(
                                    "",
                                    "error: sub_id too long",
                                )));
                                continue;
                            }
                            None => {
                                continue;
                            } // arr[1].is_string() checked above
                        };
                        if arr.len() > 3 {
                            let _ = tx.try_send(OutMsg::Text(make_closed(
                                &client_sub_id,
                                "error: multiple filters not supported",
                            )));
                            continue;
                        }
                        if !arr[2].is_object() {
                            let _ = tx.try_send(OutMsg::Text(make_closed(
                                &client_sub_id,
                                "error: invalid filter",
                            )));
                            continue;
                        }
                        if sub_id.is_some() {
                            let _ = tx.try_send(OutMsg::Text(make_closed(
                                &client_sub_id,
                                "error: already subscribed, send CLOSE first",
                            )));
                            continue;
                        }
                        let p_value = match validate_filter(&arr[2]) {
                            Ok(p) => p,
                            Err(reason) => {
                                let _ = tx.try_send(OutMsg::Text(make_closed(
                                    &client_sub_id,
                                    &format!("error: {reason}"),
                                )));
                                continue;
                            }
                        };
                        // Send EOSE before registering.
                        if tx
                            .try_send(OutMsg::Text(make_eose(&client_sub_id)))
                            .is_err()
                        {
                            break 'conn;
                        }
                        relay.subs.lock().push(Sub {
                            conn_id,
                            sub_id: client_sub_id.clone(),
                            p_value,
                            writer_tx: tx.clone(),
                        });
                        sub_id = Some(client_sub_id);
                    }

                    "EVENT" => {
                        if arr.len() != 2 {
                            let _ = tx.try_send(OutMsg::Text(make_notice("error: invalid EVENT")));
                            continue;
                        }
                        // Rate check before validation (rate check takes priority).
                        if event_rate.tick() > RATE_EVENT_MAX {
                            let safe_id = safe_event_id(&arr[1]);
                            let _ =
                                tx.try_send(OutMsg::Text(make_ok(&safe_id, false, "rate-limited")));
                            continue;
                        }
                        if !arr[1].is_object() {
                            let _ = tx.try_send(OutMsg::Text(make_ok(
                                "",
                                false,
                                "invalid: malformed event",
                            )));
                            continue;
                        }
                        match validate_event(&arr[1]) {
                            Ok((event_id, p_value)) => {
                                relay.fanout(&p_value, &arr[1]);
                                let _ = tx.try_send(OutMsg::Text(make_ok(&event_id, true, "")));
                            }
                            Err(reason) => {
                                let safe_id = safe_event_id(&arr[1]);
                                let _ = tx.try_send(OutMsg::Text(make_ok(
                                    &safe_id,
                                    false,
                                    &format!("invalid: {reason}"),
                                )));
                            }
                        }
                    }

                    "CLOSE" => {
                        if arr.len() != 2 {
                            let _ = tx.try_send(OutMsg::Text(make_notice("error: invalid CLOSE")));
                            continue;
                        }
                        match arr[1].as_str() {
                            Some(sid) => {
                                if sub_id.as_deref() == Some(sid) {
                                    relay.remove_sub(conn_id);
                                    sub_id = None;
                                }
                                // Silently ignore unknown sub_id per NIP-01.
                            }
                            None => {
                                let _ =
                                    tx.try_send(OutMsg::Text(make_notice("error: invalid CLOSE")));
                            }
                        }
                    }

                    _ => {
                        let _ =
                            tx.try_send(OutMsg::Text(make_notice("error: unsupported message")));
                    }
                }
            }
        }
    }

    // Remove the subscription first so its cloned writer_tx is dropped.
    // This allows the channel to close when we drop our local tx.
    relay.remove_sub(conn_id);

    // Drop the sender so the writer can drain any queued messages (including
    // Close frames), then cancel after a brief grace period.
    drop(tx);

    // Only await the writer if it hasn't already completed (avoid double-poll panic).
    if !writer_handle.is_finished() {
        let _ = tokio::time::timeout(Duration::from_millis(100), &mut writer_handle).await;
    }
    cancel.cancel();
}

// ── HTTP upgrade ──────────────────────────────────────────────────────────────

async fn http_service(
    relay: Arc<Relay>,
    mut req: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let headers = req.headers();
    let key = headers.get(SEC_WEBSOCKET_KEY).cloned();
    let is_ws = req.method() == Method::GET
        && req.version() >= Version::HTTP_11
        && headers
            .get(CONNECTION)
            .and_then(|h| h.to_str().ok())
            .map(|h| {
                h.split([' ', ','])
                    .any(|p| p.eq_ignore_ascii_case("upgrade"))
            })
            .unwrap_or(false)
        && headers
            .get(UPGRADE)
            .and_then(|h| h.to_str().ok())
            .map(|h| h.eq_ignore_ascii_case("websocket"))
            .unwrap_or(false)
        && headers
            .get(SEC_WEBSOCKET_VERSION)
            .map(|h| h == "13")
            .unwrap_or(false)
        && key
            .as_ref()
            .map(|k| k.len() == 24 && k.as_bytes().iter().all(|&b| b.is_ascii()))
            .unwrap_or(false);

    if !is_ws {
        let mut r = Response::new(Full::default());
        *r.status_mut() = StatusCode::BAD_REQUEST;
        return Ok(r);
    }

    // Reserve slot before upgrading.
    if relay.conn_count.fetch_add(1, Ordering::Relaxed) >= MAX_CONNS {
        relay.conn_count.fetch_sub(1, Ordering::Relaxed);
        let mut r = Response::new(Full::default());
        *r.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
        return Ok(r);
    }

    let conn_id = relay.next_conn_id.fetch_add(1, Ordering::Relaxed);
    eprintln!(
        "conn opened conn_id={} active={}",
        conn_id,
        relay.conn_count.load(Ordering::Relaxed)
    );

    let accept = derive_accept_key(key.as_ref().map(|k| k.as_bytes()).unwrap_or(b""));
    let relay_clone = Arc::clone(&relay);

    tokio::spawn(async move {
        match hyper::upgrade::on(&mut req).await {
            Ok(upgraded) => {
                let io = TokioIo::new(upgraded);
                let mut ws_config = WebSocketConfig::default();
                ws_config.max_frame_size = Some(MAX_FRAME);
                ws_config.max_message_size = Some(MAX_FRAME);
                let stream =
                    WebSocketStream::from_raw_socket(io, Role::Server, Some(ws_config)).await;
                handle_conn(relay_clone, conn_id, stream).await;
            }
            Err(e) => {
                eprintln!("upgrade error: {e}");
                relay_clone.conn_count.fetch_sub(1, Ordering::Relaxed);
            }
        }
    });

    let mut resp = Response::new(Full::default());
    *resp.status_mut() = StatusCode::SWITCHING_PROTOCOLS;
    resp.headers_mut()
        .insert(CONNECTION, HeaderValue::from_static("Upgrade"));
    resp.headers_mut()
        .insert(UPGRADE, HeaderValue::from_static("websocket"));
    if let Ok(val) = HeaderValue::from_str(&accept) {
        resp.headers_mut().insert(SEC_WEBSOCKET_ACCEPT, val);
    }
    Ok(resp)
}

// ── Server loop (extracted for testability) ───────────────────────────────────

/// Run the relay accept loop on the given listener.
/// Public for integration tests that bind to `:0`.
pub async fn run_server(listener: TcpListener, relay: Arc<Relay>) {
    let addr = listener.local_addr().ok();
    if let Some(a) = addr {
        eprintln!("sprout-pair-relay listening on {a}");
    }
    loop {
        let (tcp, _peer) = match listener.accept().await {
            Ok(pair) => pair,
            Err(e) => {
                eprintln!("accept error: {e}");
                continue;
            }
        };
        let relay = Arc::clone(&relay);
        tokio::spawn(async move {
            let io = TokioIo::new(tcp);
            let svc = service_fn(move |req| http_service(Arc::clone(&relay), req));
            if let Err(e) = http1::Builder::new()
                .serve_connection(io, svc)
                .with_upgrades()
                .await
            {
                eprintln!("http error: {e}");
            }
        });
    }
}
