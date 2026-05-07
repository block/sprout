use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

use crate::types::{ContentBlock, McpServerStdio, PermissionOutcome};

pub const PARSE_ERROR: i32 = -32700;
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;

pub enum WireMsg {
    Notify(Value),
    Permission { id: i64, params: Value },
}

pub type WireSender = mpsc::Sender<WireMsg>;

#[derive(Debug)]
pub enum Inbound {
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
    Response {
        id: i64,
        outcome: PermissionOutcome,
    },
    Invalid {
        id: Value,
        code: i32,
        message: String,
    },
}

#[derive(Debug, Deserialize)]
pub struct InitializeParams {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: u32,
    #[serde(default, rename = "clientCapabilities")]
    pub _client_capabilities: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionNewParams {
    pub cwd: String,
    #[serde(default)]
    pub mcp_servers: Vec<McpServerStdio>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPromptParams {
    pub session_id: String,
    pub prompt: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCancelParams {
    pub session_id: String,
}

pub fn classify(msg: &Value) -> Inbound {
    if !msg.is_object() || msg.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Inbound::Invalid {
            id: msg.get("id").cloned().unwrap_or(Value::Null),
            code: INVALID_REQUEST,
            message: "jsonrpc: missing or invalid version".into(),
        };
    }
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(Value::as_str).map(str::to_owned);
    let params = msg.get("params").cloned().unwrap_or(Value::Null);

    match (method, id) {
        (Some(m), Some(id)) => Inbound::Request {
            id,
            method: m,
            params,
        },
        (Some(m), None) => Inbound::Notification { method: m, params },
        (None, Some(id)) => match id.as_i64() {
            Some(n) => Inbound::Response {
                id: n,
                outcome: parse_permission_outcome(msg),
            },
            None => Inbound::Invalid {
                id,
                code: INVALID_REQUEST,
                message: "jsonrpc: response id must be integer".into(),
            },
        },
        (None, None) => Inbound::Invalid {
            id: Value::Null,
            code: INVALID_REQUEST,
            message: "jsonrpc: missing method and id".into(),
        },
    }
}

fn parse_permission_outcome(msg: &Value) -> PermissionOutcome {
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

pub fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

pub fn err(id: Value, code: i32, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

pub fn session_update(sid: &str, update: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": { "sessionId": sid, "update": update },
    })
}

pub fn permission_request(sid: &str, call_id: &str, name: &str, raw_input: &Value) -> Value {
    json!({
        "sessionId": sid,
        "toolCall": {
            "toolCallId": call_id,
            "title": name,
            "kind": "other",
            "rawInput": raw_input,
        },
        "options": [
            { "optionId": "allow", "name": "Allow", "kind": "allow_once" },
            { "optionId": "deny",  "name": "Deny",  "kind": "reject_once" },
        ],
    })
}

pub async fn send(wire: &WireSender, msg: Value) {
    let _ = wire.send(WireMsg::Notify(msg)).await;
}

pub async fn read_bounded_line<R: AsyncBufRead + Unpin>(
    stdin: &mut R,
    max: usize,
) -> std::io::Result<Option<String>> {
    let mut buf: Vec<u8> = Vec::new();
    loop {
        let chunk = stdin.fill_buf().await?;
        if chunk.is_empty() {
            if !buf.is_empty() {
                eprintln!(
                    "sprout-agent: io: unterminated frame at EOF ({} bytes dropped)",
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
                format!("io: line exceeds max ({max} bytes)"),
            ));
        }
        buf.extend_from_slice(&chunk[..take]);
        stdin.consume(take);
        if buf.ends_with(b"\n") {
            buf.pop();
            if buf.ends_with(b"\r") {
                buf.pop();
            }
            match String::from_utf8(buf) {
                Ok(s) => return Ok(Some(s)),
                Err(_) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "io: frame contains invalid UTF-8",
                    ))
                }
            }
        }
    }
}

pub async fn writer_task(mut rx: mpsc::Receiver<WireMsg>) {
    let mut stdout = tokio::io::stdout();
    while let Some(msg) = rx.recv().await {
        let v = match msg {
            WireMsg::Notify(v) => v,
            WireMsg::Permission { id, params } => json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "session/request_permission",
                "params": params,
            }),
        };
        let mut s = match serde_json::to_string(&v) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("sprout-agent: io: serialize: {e}");
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
