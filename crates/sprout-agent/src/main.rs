//! sprout-agent — minimal ACP agent over stdio.
//!
//! Architecture (one ASCII diagram, one sentence per component):
//!
//!   ┌────────┐  lines  ┌─────────┐  events  ┌────────┐  bytes  ┌────────┐
//!   │ stdin  │────────►│ reader  │─────────►│ writer │────────►│ stdout │
//!   └────────┘         └─────┬───┘   ▲      └────────┘         └────────┘
//!                            │       │ permission replies
//!                       dispatch     │
//!                            ▼       │
//!                       ┌────────────┴┐    spawn       ┌─────────┐
//!                       │ agent loop  │───────────────►│  MCP    │
//!                       └─────┬───────┘                └─────────┘
//!                             │ HTTP POST
//!                             ▼
//!                          ┌─────┐
//!                          │ LLM │
//!                          └─────┘
//!
//! `reader` reads NDJSON requests, `writer` is the sole owner of stdout
//! (single-consumer mpsc), `dispatch` maps method names to handlers. The
//! agent loop runs in its own task while the reader continues to listen
//! for `session/cancel`.

mod agent;
mod llm;
mod mcp;
mod types;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{oneshot, Mutex};

use crate::agent::{AcpEvent, AcpOut, PermissionOutcome, Session};
use crate::llm::Llm;
use crate::mcp::McpRegistry;
use crate::types::{
    AgentError, Config, HistoryItem, InitializeParams, SessionCancelParams, SessionNewParams,
    SessionPromptParams,
};

const PROTOCOL_VERSION: u32 = 1;

/// Pending permission requests, keyed by outbound JSON-RPC id.
type PermissionMap = Arc<Mutex<HashMap<i64, oneshot::Sender<PermissionOutcome>>>>;

struct State {
    cfg: Config,
    llm: Arc<Llm>,
    mcp: Arc<Mutex<Option<McpRegistry>>>,
    session: Arc<Mutex<Option<Session>>>,
    history: Arc<Mutex<Vec<HistoryItem>>>,
    /// True while a session/prompt is in flight.
    prompt_busy: Arc<AtomicBool>,
    /// Outbound permission requests awaiting reply from the client.
    pending_permissions: PermissionMap,
    /// Monotonic id for outbound requests (permissions).
    next_outbound_id: Arc<Mutex<i64>>,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let cfg = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("sprout-agent: config error: {e}");
            std::process::exit(2);
        }
    };
    if let Err(e) = cfg.validate() {
        eprintln!("sprout-agent: {e}");
        std::process::exit(2);
    }

    let llm = match Llm::new(&cfg) {
        Ok(l) => Arc::new(l),
        Err(e) => {
            eprintln!("sprout-agent: {e}");
            std::process::exit(2);
        }
    };

    // Outbound channel: agent loop → writer task. The writer is the SOLE
    // owner of stdout, so two tasks can't interleave bytes on a line.
    let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<AcpEvent>(64);
    // Final response sink: dispatcher → writer task (responses to client requests).
    let (resp_tx, mut resp_rx) = tokio::sync::mpsc::channel::<Value>(64);

    let state = Arc::new(State {
        cfg,
        llm,
        mcp: Arc::new(Mutex::new(None)),
        session: Arc::new(Mutex::new(None)),
        history: Arc::new(Mutex::new(Vec::new())),
        prompt_busy: Arc::new(AtomicBool::new(false)),
        pending_permissions: Arc::new(Mutex::new(HashMap::new())),
        next_outbound_id: Arc::new(Mutex::new(1_000_000)),
    });

    // ── Writer task ─────────────────────────────────────────────────────
    let pending_for_writer = state.pending_permissions.clone();
    let next_id_for_writer = state.next_outbound_id.clone();
    let writer = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        loop {
            tokio::select! {
                Some(resp) = resp_rx.recv() => {
                    write_line(&mut stdout, &resp).await;
                }
                Some(ev) = out_rx.recv() => {
                    match ev {
                        AcpEvent::Update { session_id, update } => {
                            let msg = json!({
                                "jsonrpc": "2.0",
                                "method": "session/update",
                                "params": {
                                    "sessionId": session_id,
                                    "update": update,
                                },
                            });
                            write_line(&mut stdout, &msg).await;
                        }
                        AcpEvent::Permission { session_id, tool_call, reply } => {
                            let id = {
                                let mut n = next_id_for_writer.lock().await;
                                let v = *n;
                                *n += 1;
                                v
                            };
                            pending_for_writer.lock().await.insert(id, reply);
                            let msg = json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "method": "session/request_permission",
                                "params": {
                                    "sessionId": session_id,
                                    "toolCall": tool_call,
                                    "options": [
                                        { "optionId": "allow", "name": "Allow", "kind": "allow_once" },
                                        { "optionId": "deny",  "name": "Deny",  "kind": "reject_once" },
                                    ],
                                },
                            });
                            write_line(&mut stdout, &msg).await;
                        }
                    }
                }
                else => break,
            }
        }
    });

    // ── Reader / dispatcher ─────────────────────────────────────────────
    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();

    while let Ok(Some(line)) = lines.next_line().await {
        if line.len() > state.cfg.max_line_bytes {
            eprintln!(
                "sprout-agent: line exceeds max ({} bytes), aborting",
                line.len()
            );
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let resp = json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": { "code": -32700, "message": format!("parse error: {e}") },
                });
                let _ = resp_tx.send(resp).await;
                continue;
            }
        };
        handle_message(&state, msg, out_tx.clone(), resp_tx.clone()).await;
    }

    // EOF: shut down. Drop senders so writer exits.
    drop(out_tx);
    drop(resp_tx);
    let _ = writer.await;
}

async fn handle_message(
    state: &Arc<State>,
    msg: Value,
    out_tx: AcpOut,
    resp_tx: tokio::sync::mpsc::Sender<Value>,
) {
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(Value::as_str).map(str::to_owned);

    // Response to one of our outbound requests (permission)?
    if method.is_none() && id.is_some() {
        if let Some(idnum) = id.as_ref().and_then(Value::as_i64) {
            let waker = state.pending_permissions.lock().await.remove(&idnum);
            if let Some(tx) = waker {
                let outcome = parse_permission_outcome(&msg);
                let _ = tx.send(outcome);
                return;
            }
        }
        eprintln!("sprout-agent: unsolicited response: {msg}");
        return;
    }

    let method = match method {
        Some(m) => m,
        None => {
            send_error(&resp_tx, id, -32600, "missing method").await;
            return;
        }
    };
    let params = msg.get("params").cloned().unwrap_or(Value::Null);

    match method.as_str() {
        "initialize" => {
            let _: InitializeParams = serde_json::from_value(params).unwrap_or(InitializeParams {
                protocol_version: None,
                client_capabilities: None,
            });
            let result = json!({
                "protocolVersion": PROTOCOL_VERSION,
                "agentCapabilities": {
                    "loadSession": false,
                    "promptCapabilities": {
                        "image": false, "audio": false, "embeddedContext": false,
                    },
                    "mcpCapabilities": { "http": false, "sse": false },
                },
                "agentInfo": {
                    "name": "sprout-agent",
                    "version": env!("CARGO_PKG_VERSION"),
                },
            });
            send_result(&resp_tx, id, result).await;
        }

        "session/new" => {
            let p: SessionNewParams = match serde_json::from_value(params) {
                Ok(p) => p,
                Err(e) => {
                    send_error(&resp_tx, id, -32602, &format!("session/new: {e}")).await;
                    return;
                }
            };
            if state.session.lock().await.is_some() {
                send_error(&resp_tx, id, -32602, "session already exists").await;
                return;
            }
            let registry = match McpRegistry::spawn_all(&p.mcp_servers).await {
                Ok(r) => r,
                Err(e) => {
                    send_error(&resp_tx, id, e.json_rpc_code(), &e.to_string()).await;
                    return;
                }
            };
            let session_id = format!("ses_{}", random_hex());
            *state.mcp.lock().await = Some(registry);
            *state.session.lock().await = Some(Session {
                id: session_id.clone(),
                cancelled: Arc::new(AtomicBool::new(false)),
            });
            send_result(&resp_tx, id, json!({ "sessionId": session_id })).await;
        }

        "session/prompt" => {
            let p: SessionPromptParams = match serde_json::from_value(params) {
                Ok(p) => p,
                Err(e) => {
                    send_error(&resp_tx, id, -32602, &format!("session/prompt: {e}")).await;
                    return;
                }
            };
            if state.prompt_busy.swap(true, Ordering::AcqRel) {
                send_error(&resp_tx, id, -32602, "prompt already in flight").await;
                return;
            }
            let state2 = state.clone();
            let resp_tx2 = resp_tx.clone();
            let out_tx2 = out_tx.clone();
            tokio::spawn(async move {
                let result = run_prompt_task(&state2, p, out_tx2).await;
                state2.prompt_busy.store(false, Ordering::Release);
                match result {
                    Ok(stop) => {
                        send_result(&resp_tx2, id, json!({ "stopReason": stop.as_wire() })).await;
                    }
                    Err(e) => {
                        send_error(&resp_tx2, id, e.json_rpc_code(), &e.to_string()).await;
                    }
                }
            });
        }

        "session/cancel" => {
            // Notification — no response.
            let p: SessionCancelParams = match serde_json::from_value(params) {
                Ok(p) => p,
                Err(_) => return,
            };
            let sess = state.session.lock().await;
            if let Some(s) = sess.as_ref() {
                if s.id == p.session_id {
                    s.cancelled.store(true, Ordering::Release);
                }
            }
        }

        _ => {
            if id.is_some() {
                send_error(&resp_tx, id, -32601, &format!("method not found: {method}")).await;
            }
        }
    }
}

async fn run_prompt_task(
    state: &Arc<State>,
    p: SessionPromptParams,
    out_tx: AcpOut,
) -> Result<crate::types::StopReason, AgentError> {
    let session = {
        let g = state.session.lock().await;
        g.as_ref()
            .filter(|s| s.id == p.session_id)
            .map(|s| Session {
                id: s.id.clone(),
                cancelled: s.cancelled.clone(),
            })
            .ok_or_else(|| {
                AgentError::InvalidParams(format!("unknown session: {}", p.session_id))
            })?
    };
    // Reset cancel flag for this turn.
    session.cancelled.store(false, Ordering::Release);

    let mcp_guard = state.mcp.lock().await;
    let mcp = mcp_guard
        .as_ref()
        .ok_or_else(|| AgentError::Internal("mcp registry missing".into()))?;
    let mut history = state.history.lock().await;

    agent::run_prompt(
        &state.cfg,
        &session,
        &out_tx,
        &state.llm,
        mcp,
        &mut history,
        p.prompt,
    )
    .await
}

fn parse_permission_outcome(msg: &Value) -> PermissionOutcome {
    let outcome = msg
        .get("result")
        .and_then(|r| r.get("outcome"))
        .cloned()
        .unwrap_or(Value::Null);
    match outcome.get("outcome").and_then(Value::as_str) {
        Some("selected") => match outcome.get("optionId").and_then(Value::as_str) {
            Some("allow") => PermissionOutcome::Allow,
            _ => PermissionOutcome::Deny,
        },
        Some("cancelled") => PermissionOutcome::Cancelled,
        _ => PermissionOutcome::Deny,
    }
}

async fn send_result(tx: &tokio::sync::mpsc::Sender<Value>, id: Option<Value>, result: Value) {
    let resp = json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "result": result,
    });
    let _ = tx.send(resp).await;
}

async fn send_error(
    tx: &tokio::sync::mpsc::Sender<Value>,
    id: Option<Value>,
    code: i32,
    message: &str,
) {
    let resp = json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "error": { "code": code, "message": message },
    });
    let _ = tx.send(resp).await;
}

async fn write_line(stdout: &mut tokio::io::Stdout, msg: &Value) {
    let mut s = match serde_json::to_string(msg) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("sprout-agent: serialize error: {e}");
            return;
        }
    };
    s.push('\n');
    if let Err(e) = stdout.write_all(s.as_bytes()).await {
        eprintln!("sprout-agent: stdout write error: {e}");
        return;
    }
    let _ = stdout.flush().await;
}

fn random_hex() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    // Mix in the address of a stack local for a tiny bit of entropy.
    let local = 0u8;
    let addr = &local as *const u8 as usize as u128;
    format!("{:016x}{:016x}", n as u64, addr as u64)
}
