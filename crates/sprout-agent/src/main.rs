//! sprout-agent — minimal ACP agent over stdio.
//!
//!   stdin → reader → App ─► writer → stdout
//!                       │
//!                       └► run_prompt → LLM + MCP
//!
//! Reader reads bounded NDJSON. Writer is the SOLE owner of stdout and
//! serializes a single channel of messages: notifications, permission
//! requests, and final responses. The agent loop runs on a tokio task; the
//! reader keeps listening for `session/cancel` and permission replies.

mod agent;
mod llm;
mod mcp;
mod types;

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, watch, Mutex};

use crate::agent::{PendingMap, PermissionOutcome, Wire, WireMsg};
use crate::llm::Llm;
use crate::mcp::McpRegistry;
use crate::types::{Config, SessionCancelParams, SessionNewParams, SessionPromptParams};

const PROTOCOL_VERSION: u32 = 1;

struct App {
    cfg: Config,
    llm: Arc<Llm>,
    state: Mutex<Option<Session>>,
    pending: PendingMap,
    next_id: Arc<Mutex<i64>>,
}

struct Session {
    id: String,
    mcp: Arc<McpRegistry>,
    /// Owned by the in-flight prompt task. `None` between prompts.
    history: Option<Vec<crate::types::HistoryItem>>,
    cancel_tx: watch::Sender<bool>,
    busy: bool,
}

fn die(msg: String) -> ! {
    eprintln!("sprout-agent: {msg}");
    std::process::exit(2);
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let cfg = Config::from_env().unwrap_or_else(|e| die(format!("config: {e}")));
    let llm = Arc::new(Llm::new(&cfg).unwrap_or_else(|e| die(e.to_string())));
    let max_line = cfg.max_line_bytes;
    let app = Arc::new(App {
        cfg,
        llm,
        state: Mutex::new(None),
        pending: Arc::new(Mutex::new(HashMap::new())),
        next_id: Arc::new(Mutex::new(1_000_000)),
    });

    let (wire_tx, wire_rx) = mpsc::channel::<WireMsg>(64);
    let writer = tokio::spawn(writer_task(wire_rx, app.clone()));

    let stdin = BufReader::new(tokio::io::stdin());
    if let Err(e) = reader_loop(stdin, app, wire_tx, max_line).await {
        eprintln!("sprout-agent: reader: {e}");
    }
    let _ = writer.await;
}

// ─── Reader ─────────────────────────────────────────────────────────────────

async fn reader_loop<R: tokio::io::AsyncBufRead + Unpin>(
    mut stdin: R,
    app: Arc<App>,
    wire: Wire,
    max_line: usize,
) -> std::io::Result<()> {
    loop {
        match read_bounded_line(&mut stdin, max_line).await? {
            None => return Ok(()),
            Some(line) if line.trim().is_empty() => continue,
            Some(line) => match serde_json::from_str::<Value>(&line) {
                Ok(msg) => handle_message(&app, msg, &wire).await,
                Err(e) => send(&wire, jrpc_err(Value::Null, -32700, &format!("parse: {e}"))).await,
            },
        }
    }
}

/// Read one `\n`-terminated line, rejecting BEFORE allocation grows past `max`.
async fn read_bounded_line<R: tokio::io::AsyncBufRead + Unpin>(
    stdin: &mut R,
    max: usize,
) -> std::io::Result<Option<String>> {
    let mut buf: Vec<u8> = Vec::new();
    loop {
        let chunk = stdin.fill_buf().await?;
        if chunk.is_empty() {
            // EOF. A partially-buffered, unterminated frame is NOT a valid
            // message — treat it as a connection close. ACP frames must be
            // newline-terminated.
            if !buf.is_empty() {
                eprintln!(
                    "sprout-agent: dropping unterminated partial frame at EOF ({} bytes)",
                    buf.len()
                );
            }
            return Ok(None);
        }
        let take = chunk
            .iter()
            .position(|b| *b == b'\n')
            .map_or(chunk.len(), |i| i + 1);
        if buf.len().saturating_add(take) > max {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("line exceeds max ({max} bytes)"),
            ));
        }
        buf.extend_from_slice(&chunk[..take]);
        stdin.consume(take);
        if buf.ends_with(b"\n") {
            buf.pop();
            if buf.ends_with(b"\r") {
                buf.pop();
            }
            return Ok(Some(String::from_utf8_lossy(&buf).into_owned()));
        }
    }
}

async fn handle_message(app: &Arc<App>, msg: Value, wire: &Wire) {
    let has_id = msg.get("id").is_some();
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(Value::as_str).map(str::to_owned);
    let has_params = msg.get("params").is_some();

    match (method, has_id) {
        // Response to one of OUR outbound requests (permission reply).
        (None, true) => {
            if let Some(idnum) = id.as_ref().and_then(Value::as_i64) {
                if let Some(tx) = app.pending.lock().await.remove(&idnum) {
                    let _ = tx.send(parse_permission(&msg));
                }
            }
        }
        // Malformed: neither method nor id.
        (None, false) => {
            eprintln!("sprout-agent: malformed message (no method, no id)");
        }
        // Notification: method without id. Never respond.
        (Some(method), false) => {
            if method == "session/cancel" {
                let params = msg.get("params").cloned().unwrap_or(Value::Null);
                if let Ok(p) = serde_json::from_value::<SessionCancelParams>(params) {
                    if let Some(s) = app.state.lock().await.as_ref() {
                        if s.id == p.session_id {
                            let _ = s.cancel_tx.send(true);
                        }
                    }
                }
            }
            // Other notifications: ignored silently per JSON-RPC.
        }
        // Request: method + id. Dispatch and respond.
        (Some(method), true) => {
            let id = id.unwrap_or(Value::Null);
            let params = msg.get("params").cloned().unwrap_or(Value::Null);
            // Methods that require params: -32600 if missing rather than silently coercing to null.
            let needs_params = matches!(method.as_str(), "session/new" | "session/prompt");
            if needs_params && !has_params {
                return send(wire, jrpc_err(id, -32600, "missing params")).await;
            }
            match method.as_str() {
                "initialize" => send(wire, jrpc_ok(id, json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "agentCapabilities": {
                        "loadSession": false,
                        "promptCapabilities": { "image": false, "audio": false, "embeddedContext": false },
                        "mcpCapabilities": { "http": false, "sse": false },
                    },
                    "agentInfo": { "name": "sprout-agent", "version": env!("CARGO_PKG_VERSION") },
                }))).await,
                "session/new" => handle_session_new(app, id, params, wire).await,
                "session/prompt" => spawn_prompt(app.clone(), id, params, wire.clone()),
                // session/cancel is a notification; if a client sends it as a request we still ack.
                "session/cancel" => {
                    if let Ok(p) = serde_json::from_value::<SessionCancelParams>(params) {
                        if let Some(s) = app.state.lock().await.as_ref() {
                            if s.id == p.session_id {
                                let _ = s.cancel_tx.send(true);
                            }
                        }
                    }
                    send(wire, jrpc_ok(id, Value::Null)).await;
                }
                _ => send(wire, jrpc_err(id, -32601, &format!("method not found: {method}"))).await,
            }
        }
    }
}

async fn handle_session_new(app: &Arc<App>, id: Value, params: Value, wire: &Wire) {
    let p: SessionNewParams = match serde_json::from_value(params) {
        Ok(p) => p,
        Err(e) => return send(wire, jrpc_err(id, -32602, &format!("session/new: {e}"))).await,
    };
    // Don't hold app.state across MCP child spawning. We re-check after.
    if app.state.lock().await.is_some() {
        return send(wire, jrpc_err(id, -32602, "session already exists")).await;
    }
    let mcp = match McpRegistry::spawn_all(&p.mcp_servers).await {
        Ok(m) => Arc::new(m),
        Err(e) => return send(wire, jrpc_err(id, e.json_rpc_code(), &e.to_string())).await,
    };
    let session_id = format!("ses_{}", session_token());
    let (cancel_tx, _) = watch::channel(false);
    let mut st = app.state.lock().await;
    if st.is_some() {
        return send(wire, jrpc_err(id, -32602, "session already exists")).await;
    }
    *st = Some(Session {
        id: session_id.clone(),
        mcp,
        history: Some(Vec::new()),
        cancel_tx,
        busy: false,
    });
    send(wire, jrpc_ok(id, json!({ "sessionId": session_id }))).await;
}

fn spawn_prompt(app: Arc<App>, id: Value, params: Value, wire: Wire) {
    tokio::spawn(async move {
        let p: SessionPromptParams = match serde_json::from_value(params) {
            Ok(p) => p,
            Err(e) => {
                return send(&wire, jrpc_err(id, -32602, &format!("session/prompt: {e}"))).await
            }
        };

        // Lift everything we need from the session under one lock, take
        // ownership of history (moving it out of the session), and mark busy.
        let (sid, mcp, mut history, mut cancel_rx) = {
            let mut st = app.state.lock().await;
            let s = match st.as_mut() {
                Some(s) if s.id == p.session_id => s,
                _ => return send(&wire, jrpc_err(id, -32602, "unknown session")).await,
            };
            if s.busy {
                return send(&wire, jrpc_err(id, -32602, "prompt already in flight")).await;
            }
            s.busy = true;
            let (tx, rx) = watch::channel(false);
            s.cancel_tx = tx;
            // history is `Some` whenever a session exists and no prompt is in flight.
            let hist = s.history.take().unwrap_or_default();
            (s.id.clone(), s.mcp.clone(), hist, rx)
        };

        let result = agent::run_prompt(
            &app.cfg,
            &sid,
            &mut cancel_rx,
            &wire,
            &app.llm,
            &mcp,
            &app.pending,
            &app.next_id,
            &mut history,
            p.prompt,
        )
        .await;

        if let Some(s) = app.state.lock().await.as_mut() {
            s.busy = false;
            s.history = Some(history);
        }

        match result {
            Ok(stop) => send(&wire, jrpc_ok(id, json!({ "stopReason": stop.as_wire() }))).await,
            Err(e) => send(&wire, jrpc_err(id, e.json_rpc_code(), &e.to_string())).await,
        }
    });
}

// ─── Writer ─────────────────────────────────────────────────────────────────

async fn writer_task(mut rx: mpsc::Receiver<WireMsg>, _app: Arc<App>) {
    let mut stdout = tokio::io::stdout();
    while let Some(msg) = rx.recv().await {
        let to_write = match msg {
            WireMsg::Notify(v) => v,
            WireMsg::Permission { id, params } => json!({
                "jsonrpc": "2.0", "id": id,
                "method": "session/request_permission", "params": params,
            }),
        };
        let mut s = match serde_json::to_string(&to_write) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("sprout-agent: serialize: {e}");
                continue;
            }
        };
        s.push('\n');
        if stdout.write_all(s.as_bytes()).await.is_err() {
            return;
        }
        let _ = stdout.flush().await;
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────────

async fn send(wire: &Wire, msg: Value) {
    let _ = wire.send(WireMsg::Notify(msg)).await;
}

fn jrpc_ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}
fn jrpc_err(id: Value, code: i32, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn parse_permission(msg: &Value) -> PermissionOutcome {
    let o = msg
        .get("result")
        .and_then(|r| r.get("outcome"))
        .cloned()
        .unwrap_or(Value::Null);
    match (
        o.get("outcome").and_then(Value::as_str),
        o.get("optionId").and_then(Value::as_str),
    ) {
        (Some("selected"), Some("allow")) => PermissionOutcome::Allow,
        (Some("cancelled"), _) => PermissionOutcome::Cancelled,
        _ => PermissionOutcome::Deny,
    }
}

/// 16 hex chars (8 bytes) from `/dev/urandom`, falling back to nanos^pid.
fn session_token() -> String {
    use std::io::Read;
    let mut b = [0u8; 8];
    if std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut b))
        .is_err()
    {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        b = (nanos ^ ((std::process::id() as u64) << 32)).to_le_bytes();
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}
