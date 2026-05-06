//! Tiny fake MCP server for integration tests.
//!
//! Reads JSON-RPC line frames on stdin and replies on stdout. Driven by
//! environment variables so tests can simulate misbehavior:
//!
//!   FAKE_MCP_HANG_INIT=1     — never reply to `initialize` (init timeout)
//!   FAKE_MCP_HANG_TOOLS=1    — never reply to `tools/list` (list timeout)
//!   FAKE_MCP_TOOL_COUNT=N    — return N tools (default: 1)
//!   FAKE_MCP_HUGE_DESC=1     — every tool description is 100 KB
//!   FAKE_MCP_DESC_SIZE=N     — every tool description is N bytes (overrides HUGE_DESC)
//!   FAKE_MCP_TOOL_DELAY=N    — `tools/call` sleeps N seconds before replying
//!                              (use a large value, e.g. 999, to simulate hang)
//!   FAKE_MCP_PID_FILE=path   — write the child PID to `path` on startup
//!                              (for tests that want to verify the child died)
//!   FAKE_MCP_SPAWN_GRANDCHILD=1
//!                            — on `tools/call`, spawn a `sleep 999`
//!                              grandchild before hanging. Its PID is
//!                              written to FAKE_MCP_GRANDCHILD_PID_FILE
//!                              so a test can verify the entire process
//!                              tree dies on timeout.
//!   FAKE_MCP_GRANDCHILD_PID_FILE=path
//!                            — path to write the grandchild PID to.

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

fn env_u64(k: &str, default: u64) -> u64 {
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

fn make_tools(count: usize, desc: &str) -> Vec<Value> {
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
    // Optional: write our own PID so a test can later check the process is gone.
    if let Ok(path) = std::env::var("FAKE_MCP_PID_FILE") {
        let pid = std::process::id().to_string();
        let _ = std::fs::write(&path, pid);
    }

    let hang_init = env_flag("FAKE_MCP_HANG_INIT");
    let hang_tools = env_flag("FAKE_MCP_HANG_TOOLS");
    let tool_count = env_usize("FAKE_MCP_TOOL_COUNT", 1);
    // FAKE_MCP_DESC_SIZE wins over FAKE_MCP_HUGE_DESC when set.
    let desc: String = if let Some(n) = std::env::var("FAKE_MCP_DESC_SIZE")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
    {
        "x".repeat(n)
    } else if env_flag("FAKE_MCP_HUGE_DESC") {
        "x".repeat(100_000)
    } else {
        "fake tool".to_owned()
    };
    let tool_delay_secs = env_u64("FAKE_MCP_TOOL_DELAY", 0);

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
                write_response(id, json!({ "tools": make_tools(tool_count, &desc) }));
            }
            "tools/call" => {
                // Optionally spawn a long-sleeping grandchild so the test
                // can verify process-group killing reaches the whole tree.
                if env_flag("FAKE_MCP_SPAWN_GRANDCHILD") {
                    let child = std::process::Command::new("sleep")
                        .arg("999")
                        .spawn()
                        .expect("spawn grandchild");
                    if let Ok(path) = std::env::var("FAKE_MCP_GRANDCHILD_PID_FILE") {
                        let _ = std::fs::write(&path, child.id().to_string());
                    }
                    // Don't reap; let it run until the parent group is killed.
                    std::mem::forget(child);
                }
                if tool_delay_secs > 0 {
                    std::thread::sleep(std::time::Duration::from_secs(tool_delay_secs));
                }
                write_response(
                    id,
                    json!({
                        "content": [{ "type": "text", "text": "ok" }],
                        "isError": false,
                    }),
                );
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
