//! Tiny fake MCP server for integration tests.
//!
//! Reads JSON-RPC line frames on stdin and replies on stdout. Driven by
//! environment variables so tests can simulate misbehavior:
//!
//!   FAKE_MCP_HANG_INIT=1     — never reply to `initialize` (init timeout)
//!   FAKE_MCP_HANG_TOOLS=1    — never reply to `tools/list` (list timeout)
//!   FAKE_MCP_TOOL_COUNT=N    — return N tools (default: 1)
//!   FAKE_MCP_HUGE_DESC=1     — every tool description is 100 KB

use std::io::{BufRead, Write};

use serde_json::{json, Value};

fn env_flag(k: &str) -> bool {
    std::env::var(k).map(|v| v != "0").unwrap_or(false)
}

fn env_usize(k: &str, default: usize) -> usize {
    std::env::var(k)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn write_response(id: Value, result: Value) {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "result": result });
    let mut s = serde_json::to_string(&msg).expect("serialize");
    s.push('\n');
    let mut out = std::io::stdout().lock();
    out.write_all(s.as_bytes()).expect("write");
    out.flush().expect("flush");
}

fn hang_forever() -> ! {
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}

fn make_tools(count: usize, huge_desc: bool) -> Vec<Value> {
    let desc = if huge_desc {
        "x".repeat(100_000)
    } else {
        "fake tool".to_owned()
    };
    (0..count)
        .map(|i| {
            json!({
                "name": format!("tool_{i}"),
                "description": desc,
                "inputSchema": { "type": "object", "properties": {} },
            })
        })
        .collect()
}

fn main() {
    let hang_init = env_flag("FAKE_MCP_HANG_INIT");
    let hang_tools = env_flag("FAKE_MCP_HANG_TOOLS");
    let tool_count = env_usize("FAKE_MCP_TOOL_COUNT", 1);
    let huge_desc = env_flag("FAKE_MCP_HUGE_DESC");

    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();

    while let Some(Ok(line)) = lines.next() {
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let id = msg.get("id").cloned().unwrap_or(Value::Null);

        // Notifications carry no id; ignore.
        if msg.get("id").is_none() {
            continue;
        }

        match method {
            "initialize" => {
                if hang_init {
                    hang_forever();
                }
                write_response(
                    id,
                    json!({
                        "protocolVersion": "2025-06-18",
                        "capabilities": { "tools": {} },
                        "serverInfo": { "name": "fake-mcp", "version": "0.0.0" },
                    }),
                );
            }
            "tools/list" => {
                if hang_tools {
                    hang_forever();
                }
                write_response(id, json!({ "tools": make_tools(tool_count, huge_desc) }));
            }
            _ => {
                // Unknown method: respond with an error so rmcp doesn't hang.
                let err = json!({
                    "jsonrpc": "2.0", "id": id,
                    "error": { "code": -32601, "message": format!("method not found: {method}") },
                });
                let mut s = serde_json::to_string(&err).unwrap();
                s.push('\n');
                let mut out = std::io::stdout().lock();
                let _ = out.write_all(s.as_bytes());
                let _ = out.flush();
            }
        }
    }
}
