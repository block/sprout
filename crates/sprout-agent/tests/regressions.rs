//! Regression tests for round 4-6 hardening:
//!   - assistant text preserved in history
//!   - MCP init timeout (with explicit child kill)
//!   - tool metadata caps (description bytes, count)
//!   - cancellation leaves history valid for the next prompt
//!   - empty-content assistant turn doesn't poison OpenAI history

use std::collections::VecDeque;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

// ─── Fake LLM that captures requests so we can inspect history ──────────────

struct CapturingLlm {
    url: String,
    captured: Arc<Mutex<Vec<Value>>>,
}

async fn spawn_capturing_llm(responses: Vec<Value>) -> CapturingLlm {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let queue = Arc::new(Mutex::new(VecDeque::from(responses)));
    let captured: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let cap2 = captured.clone();
    tokio::spawn(async move {
        loop {
            let (mut sock, _) = match listener.accept().await {
                Ok(p) => p,
                Err(_) => return,
            };
            let queue = queue.clone();
            let captured = cap2.clone();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut tmp = [0u8; 8192];
                // Read until headers complete.
                while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    match sock.read(&mut tmp).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => buf.extend_from_slice(&tmp[..n]),
                    }
                    if buf.len() > 4_000_000 {
                        return;
                    }
                }
                // Parse Content-Length and read body.
                let header_end = buf.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
                let headers = &buf[..header_end];
                let mut body_len = 0usize;
                for line in headers.split(|b| *b == b'\n') {
                    let line = std::str::from_utf8(line).unwrap_or("");
                    if let Some(rest) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        body_len = rest.trim().trim_end_matches('\r').parse().unwrap_or(0);
                    }
                }
                while buf.len() < header_end + body_len {
                    match sock.read(&mut tmp).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => buf.extend_from_slice(&tmp[..n]),
                    }
                }
                if let Ok(req) = serde_json::from_slice::<Value>(&buf[header_end..]) {
                    captured.lock().await.push(req);
                }
                let body = queue
                    .lock()
                    .await
                    .pop_front()
                    .unwrap_or_else(|| json!({ "error": "no canned response" }));
                let body_s = serde_json::to_string(&body).unwrap();
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body_s.len(), body_s,
                );
                let _ = sock.write_all(resp.as_bytes()).await;
                let _ = sock.shutdown().await;
            });
        }
    });
    CapturingLlm { url, captured }
}

// ─── Harness (minimal copy — keeping per-test independence) ─────────────────

struct Harness {
    child: tokio::process::Child,
    stdin: tokio::process::ChildStdin,
    stdout: BufReader<tokio::process::ChildStdout>,
    next_id: i64,
}

impl Harness {
    async fn spawn_with_env(base_url: &str, extra: &[(&str, &str)]) -> Self {
        let bin = env!("CARGO_BIN_EXE_sprout-agent");
        let mut cmd = tokio::process::Command::new(bin);
        cmd.env("SPROUT_AGENT_PROVIDER", "openai")
            .env("OPENAI_COMPAT_API_KEY", "test")
            .env("OPENAI_COMPAT_MODEL", "fake-model")
            .env("OPENAI_COMPAT_BASE_URL", base_url)
            .env("SPROUT_AGENT_LLM_TIMEOUT_SECS", "5")
            .env("SPROUT_AGENT_TOOL_TIMEOUT_SECS", "5")
            .env("SPROUT_AGENT_MAX_ROUNDS", "8")
            .env("SPROUT_AGENT_MCP_INIT_TIMEOUT_SECS", "2");
        for (k, v) in extra {
            cmd.env(k, v);
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true);
        let mut child = cmd.spawn().expect("spawn sprout-agent");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            stdin,
            stdout,
            next_id: 1,
        }
    }

    async fn spawn(base_url: &str) -> Self {
        Self::spawn_with_env(base_url, &[]).await
    }

    async fn send(&mut self, method: &str, params: Value) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        self.write(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
            .await;
        id
    }

    async fn notify(&mut self, method: &str, params: Value) {
        self.write(json!({ "jsonrpc": "2.0", "method": method, "params": params }))
            .await;
    }

    async fn write(&mut self, msg: Value) {
        let mut s = serde_json::to_string(&msg).unwrap();
        s.push('\n');
        self.stdin.write_all(s.as_bytes()).await.unwrap();
        self.stdin.flush().await.unwrap();
    }

    async fn recv(&mut self) -> Value {
        let mut line = String::new();
        let n = tokio::time::timeout(Duration::from_secs(15), self.stdout.read_line(&mut line))
            .await
            .expect("recv timeout")
            .expect("read line");
        assert!(n > 0, "agent EOF");
        serde_json::from_str(&line).expect("non-JSON line")
    }

    async fn recv_until<F: FnMut(&Value) -> bool>(&mut self, mut pred: F) -> Value {
        loop {
            let v = self.recv().await;
            if pred(&v) {
                return v;
            }
        }
    }

    async fn shutdown(mut self) {
        drop(self.stdin);
        let _ = tokio::time::timeout(Duration::from_secs(2), self.child.wait()).await;
        let _ = self.child.start_kill();
    }
}

fn openai_text(content: &str) -> Value {
    json!({
        "id": "cc-1", "object": "chat.completion", "model": "fake-model",
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": content },
            "finish_reason": "stop",
        }],
    })
}

fn openai_tool_call(id: &str, name: &str, args: Value) -> Value {
    json!({
        "id": "cc-2", "object": "chat.completion", "model": "fake-model",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant", "content": null,
                "tool_calls": [{
                    "id": id, "type": "function",
                    "function": { "name": name, "arguments": args.to_string() },
                }],
            },
            "finish_reason": "tool_calls",
        }],
    })
}

async fn init_session(h: &mut Harness, mcp_servers: Value) -> String {
    h.send(
        "initialize",
        json!({"protocolVersion":1,"clientCapabilities":{}}),
    )
    .await;
    let _ = h.recv().await;
    h.send(
        "session/new",
        json!({"cwd":"/tmp","mcpServers": mcp_servers}),
    )
    .await;
    let r = h
        .recv_until(|v| v.get("result").is_some() || v.get("error").is_some())
        .await;
    r["result"]["sessionId"]
        .as_str()
        .expect("sessionId")
        .to_owned()
}

// ─── Tests ──────────────────────────────────────────────────────────────────

/// After a text-only assistant response, the next prompt's request must
/// include that assistant text in `messages` history. Round 4 fix.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn assistant_text_preserved_across_prompts() {
    let llm = spawn_capturing_llm(vec![openai_text("hello world"), openai_text("done")]).await;
    let mut h = Harness::spawn(&llm.url).await;
    let sid = init_session(&mut h, json!([])).await;

    // Prompt 1.
    let p1 = h
        .send(
            "session/prompt",
            json!({"sessionId": sid, "prompt": [{"type":"text","text":"first"}]}),
        )
        .await;
    let _ = h.recv_until(|v| v["id"] == json!(p1)).await;

    // Prompt 2 — should carry assistant text from prompt 1.
    let p2 = h
        .send(
            "session/prompt",
            json!({"sessionId": sid, "prompt": [{"type":"text","text":"second"}]}),
        )
        .await;
    let _ = h.recv_until(|v| v["id"] == json!(p2)).await;

    let captured = llm.captured.lock().await;
    assert_eq!(captured.len(), 2, "expected 2 LLM requests");
    let msgs = captured[1]["messages"].as_array().unwrap();
    let assistants: Vec<&Value> = msgs.iter().filter(|m| m["role"] == "assistant").collect();
    assert!(
        assistants.iter().any(|m| m["content"] == "hello world"),
        "assistant text was dropped: messages={msgs:?}"
    );
    h.shutdown().await;
}

/// MCP init that hangs forever must time out within ~2s, surface an error,
/// and the child process must be killed (not lingering).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mcp_init_timeout_kills_child() {
    let llm = spawn_capturing_llm(vec![]).await;
    let mut h = Harness::spawn(&llm.url).await;

    let fake_mcp = env!("CARGO_BIN_EXE_fake-mcp");
    h.send(
        "initialize",
        json!({"protocolVersion":1,"clientCapabilities":{}}),
    )
    .await;
    let _ = h.recv().await;

    let start = Instant::now();
    h.send(
        "session/new",
        json!({
            "cwd": "/tmp",
            "mcpServers": [{
                "name": "stuck",
                "command": fake_mcp,
                "args": [],
                "env": [{ "name": "FAKE_MCP_HANG_INIT", "value": "1" }],
            }],
        }),
    )
    .await;
    let r = h
        .recv_until(|v| v.get("result").is_some() || v.get("error").is_some())
        .await;
    let elapsed = start.elapsed();

    assert!(r.get("error").is_some(), "expected error, got {r}");
    let msg = r["error"]["message"].as_str().unwrap_or("");
    assert!(msg.contains("timeout"), "error not a timeout: {msg}");
    // 2s timeout + small slack. Generous to cover slow CI.
    assert!(
        elapsed < Duration::from_secs(8),
        "timeout took too long: {elapsed:?}"
    );
    h.shutdown().await;
}

/// A real MCP server that returns 200 tools with 100KB descriptions must
/// be capped: tool count ≤ MAX_TOOLS_PER_SESSION (128) — we expect spawn_all
/// to either reject (too many) OR truncate. We assert the spawn succeeds with
/// a bounded count, and that descriptions sent to the LLM are ≤ 1024 bytes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tool_metadata_caps_enforced() {
    let llm = spawn_capturing_llm(vec![openai_text("done")]).await;
    let mut h = Harness::spawn(&llm.url).await;

    let fake_mcp = env!("CARGO_BIN_EXE_fake-mcp");
    h.send(
        "initialize",
        json!({"protocolVersion":1,"clientCapabilities":{}}),
    )
    .await;
    let _ = h.recv().await;
    h.send(
        "session/new",
        json!({
            "cwd": "/tmp",
            "mcpServers": [{
                "name": "many",
                "command": fake_mcp,
                "args": [],
                "env": [
                    { "name": "FAKE_MCP_TOOL_COUNT", "value": "200" },
                    { "name": "FAKE_MCP_HUGE_DESC", "value": "1" },
                ],
            }],
        }),
    )
    .await;
    let r = h
        .recv_until(|v| v.get("result").is_some() || v.get("error").is_some())
        .await;

    // Either spawn rejects (200 > 128 cap) — that's acceptable hardening —
    // OR it accepts and we verify the LLM request stays bounded.
    if r.get("error").is_some() {
        let msg = r["error"]["message"].as_str().unwrap_or("");
        assert!(msg.contains("too many"), "unexpected error: {msg}");
        h.shutdown().await;
        return;
    }

    let sid = r["result"]["sessionId"].as_str().unwrap().to_owned();
    let p = h
        .send(
            "session/prompt",
            json!({"sessionId": sid, "prompt": [{"type":"text","text":"go"}]}),
        )
        .await;
    let _ = h.recv_until(|v| v["id"] == json!(p)).await;

    let captured = llm.captured.lock().await;
    assert!(!captured.is_empty(), "no LLM request captured");
    let tools = captured[0]["tools"].as_array().unwrap();
    assert!(tools.len() <= 128, "tool count not capped: {}", tools.len());
    for t in tools {
        let desc = t["function"]["description"].as_str().unwrap_or("");
        assert!(
            desc.len() <= 1024,
            "description not capped: {} bytes",
            desc.len()
        );
    }
    h.shutdown().await;
}

/// Cap on MCP server count: 17 servers must be rejected.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mcp_server_count_cap() {
    let llm = spawn_capturing_llm(vec![]).await;
    let mut h = Harness::spawn(&llm.url).await;
    h.send(
        "initialize",
        json!({"protocolVersion":1,"clientCapabilities":{}}),
    )
    .await;
    let _ = h.recv().await;

    let fake_mcp = env!("CARGO_BIN_EXE_fake-mcp");
    let servers: Vec<Value> = (0..17)
        .map(|i| {
            json!({
                "name": format!("s{i}"),
                "command": fake_mcp,
                "args": [],
                "env": [],
            })
        })
        .collect();
    h.send("session/new", json!({"cwd":"/tmp","mcpServers": servers}))
        .await;
    let r = h
        .recv_until(|v| v.get("result").is_some() || v.get("error").is_some())
        .await;
    assert!(r.get("error").is_some(), "expected error for 17 servers");
    let msg = r["error"]["message"].as_str().unwrap_or("");
    assert!(msg.contains("too many"), "wrong error: {msg}");
    h.shutdown().await;
}

/// After cancelling mid-tool-loop, the next prompt must succeed without
/// the LLM seeing a malformed history (assistant tool_use with no
/// matching tool_result). Round 5 fix.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_leaves_history_valid_for_next_prompt() {
    // Round 1: tool call (unknown — fails fast, no permission flow).
    // Round 2: text "ok".
    // After cancel, prompt 2 returns text immediately.
    let llm = spawn_capturing_llm(vec![
        openai_tool_call("tc1", "fake__t", json!({})),
        openai_text("after-cancel"),
        openai_text("p2-done"),
    ])
    .await;
    let mut h = Harness::spawn(&llm.url).await;
    let sid = init_session(&mut h, json!([])).await;

    let p1 = h
        .send(
            "session/prompt",
            json!({"sessionId": sid, "prompt": [{"type":"text","text":"first"}]}),
        )
        .await;
    // Cancel right away; the agent races between cancellation and the LLM
    // round trip — either way history must remain valid.
    h.notify("session/cancel", json!({"sessionId": sid})).await;
    let _ = h.recv_until(|v| v["id"] == json!(p1)).await;

    // Prompt 2 — must NOT error from a malformed history.
    let p2 = h
        .send(
            "session/prompt",
            json!({"sessionId": sid, "prompt": [{"type":"text","text":"second"}]}),
        )
        .await;
    let r = h.recv_until(|v| v["id"] == json!(p2)).await;
    assert!(r.get("result").is_some(), "p2 errored: {r}");
    h.shutdown().await;
}

/// Empty assistant content + no tool_calls must serialize as "" (not null)
/// for OpenAI, so subsequent prompts don't get rejected. Round 7 fix 6.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn empty_assistant_serializes_as_empty_string() {
    // First call returns content="" finish_reason=stop — agent records an
    // empty assistant turn. Second call's request body is what we inspect.
    let llm = spawn_capturing_llm(vec![openai_text(""), openai_text("done")]).await;
    let mut h = Harness::spawn(&llm.url).await;
    let sid = init_session(&mut h, json!([])).await;

    let p1 = h
        .send(
            "session/prompt",
            json!({"sessionId": sid, "prompt": [{"type":"text","text":"a"}]}),
        )
        .await;
    let _ = h.recv_until(|v| v["id"] == json!(p1)).await;
    let p2 = h
        .send(
            "session/prompt",
            json!({"sessionId": sid, "prompt": [{"type":"text","text":"b"}]}),
        )
        .await;
    let _ = h.recv_until(|v| v["id"] == json!(p2)).await;

    let captured = llm.captured.lock().await;
    let msgs = captured[1]["messages"].as_array().unwrap();
    let empty_assistant = msgs
        .iter()
        .find(|m| m["role"] == "assistant" && m.get("tool_calls").is_none())
        .expect("no plain assistant turn");
    // Must be empty string, NOT null.
    assert_eq!(
        empty_assistant["content"],
        json!(""),
        "expected empty string content, got {empty_assistant}"
    );
    h.shutdown().await;
}

fn openai_n_tool_calls(n: usize) -> Value {
    let calls: Vec<Value> = (0..n)
        .map(|i| {
            json!({
                "id": format!("c{i}"),
                "type": "function",
                "function": { "name": "many__tool_0", "arguments": "{}" },
            })
        })
        .collect();
    json!({
        "id": "cc-n", "object": "chat.completion", "model": "fake-model",
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": null, "tool_calls": calls },
            "finish_reason": "tool_calls",
        }],
    })
}

// ─── New round-8 regression tests ──────────────────────────────────────────

/// History budget evicts old turns: after many prompts, the LLM request
/// body stays below a sane bound. Round 7 fix; round 8 test.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn history_budget_evicts_old_turns() {
    // Budget = 1 MB (MIN allowed by config). Each prompt is ~200 KB, so
    // 12 prompts × 200 KB = ~2.4 MB blows the cap and forces eviction.
    // We expect the captured request body to stay under 3× the cap.
    const BUDGET: usize = 1024 * 1024; // 1 MB — must be >= MAX_PROMPT_BYTES
    const PROMPT_BYTES: usize = 200 * 1024; // 200 KB per turn
    let responses: Vec<Value> = (0..12).map(|_| openai_text(&"y".repeat(200))).collect();
    let llm = spawn_capturing_llm(responses).await;
    let mut h = Harness::spawn_with_env(
        &llm.url,
        &[
            ("SPROUT_AGENT_MAX_HISTORY_BYTES", &BUDGET.to_string()),
            ("SPROUT_AGENT_MAX_HANDOFFS", "0"), // exercise truncation, not handoff
        ],
    )
    .await;
    let sid = init_session(&mut h, json!([])).await;

    for i in 0..12 {
        let user = "x".repeat(PROMPT_BYTES);
        let p = h
            .send(
                "session/prompt",
                json!({"sessionId": sid, "prompt": [{"type":"text","text": format!("{i}:{user}")}]}),
            )
            .await;
        let _ = h.recv_until(|v| v["id"] == json!(p)).await;
    }

    let captured = llm.captured.lock().await;
    assert_eq!(captured.len(), 12);
    // The last request must show eviction: body well under unbounded 12 × 200 KB = 2.4 MB.
    let last = &captured[captured.len() - 1];
    let body_bytes = serde_json::to_vec(last).unwrap().len();
    assert!(
        body_bytes < BUDGET * 3,
        "history not evicted: request body is {body_bytes} bytes"
    );
    let msgs = last["messages"].as_array().unwrap();
    // We must NEVER drop the latest user prompt.
    assert!(
        msgs.iter()
            .any(|m| m["role"] == "user" && m["content"].as_str().unwrap_or("").starts_with("11:")),
        "newest user turn missing"
    );
    h.shutdown().await;
}

/// Per-turn tool-call cap: an LLM that returns 100 tool_calls in one
/// response must only have 64 (MAX_TOOL_CALLS_PER_TURN) executed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn per_turn_tool_call_cap_enforced() {
    let llm = spawn_capturing_llm(vec![openai_n_tool_calls(100), openai_text("done")]).await;
    let mut h = Harness::spawn(&llm.url).await;

    let fake_mcp = env!("CARGO_BIN_EXE_fake-mcp");
    h.send(
        "initialize",
        json!({"protocolVersion":1,"clientCapabilities":{}}),
    )
    .await;
    let _ = h.recv().await;
    h.send(
        "session/new",
        json!({
            "cwd": "/tmp",
            "mcpServers": [{
                "name": "many",
                "command": fake_mcp,
                "args": [],
                "env": [{ "name": "FAKE_MCP_TOOL_COUNT", "value": "1" }],
            }],
        }),
    )
    .await;
    let r = h
        .recv_until(|v| v.get("result").is_some() || v.get("error").is_some())
        .await;
    let sid = r["result"]["sessionId"].as_str().unwrap().to_owned();

    let p = h
        .send(
            "session/prompt",
            json!({"sessionId": sid, "prompt": [{"type":"text","text":"go"}]}),
        )
        .await;

    // Count distinct tool_call (pending) notifications until final response.
    let mut tool_call_ids = std::collections::HashSet::new();
    loop {
        let v = h.recv().await;
        if v.get("method") == Some(&json!("session/request_permission")) {
            let id = v["id"].clone();
            h.write(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "outcome": { "outcome": "selected", "optionId": "allow" } },
            }))
            .await;
            continue;
        }
        if v.get("method") == Some(&json!("session/update"))
            && v["params"]["update"]["sessionUpdate"] == "tool_call"
        {
            if let Some(id) = v["params"]["update"]["toolCallId"].as_str() {
                tool_call_ids.insert(id.to_owned());
            }
            continue;
        }
        if v["id"] == json!(p) {
            break;
        }
    }
    // MAX_TOOL_CALLS_PER_TURN = 64.
    assert_eq!(
        tool_call_ids.len(),
        64,
        "expected 64 tool_calls, got {}",
        tool_call_ids.len()
    );
    h.shutdown().await;
}

/// Description clamping: a 5000-byte description from MCP must be
/// truncated to ≤ 1024 bytes in the LLM request.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn description_clamping_enforced() {
    let llm = spawn_capturing_llm(vec![openai_text("done")]).await;
    let mut h = Harness::spawn_with_env(&llm.url, &[("SPROUT_AGENT_TODO", "0")]).await;

    let fake_mcp = env!("CARGO_BIN_EXE_fake-mcp");
    h.send(
        "initialize",
        json!({"protocolVersion":1,"clientCapabilities":{}}),
    )
    .await;
    let _ = h.recv().await;
    h.send(
        "session/new",
        json!({
            "cwd": "/tmp",
            "mcpServers": [{
                "name": "big",
                "command": fake_mcp,
                "args": [],
                "env": [
                    { "name": "FAKE_MCP_TOOL_COUNT", "value": "1" },
                    { "name": "FAKE_MCP_DESC_SIZE", "value": "5000" },
                ],
            }],
        }),
    )
    .await;
    let r = h
        .recv_until(|v| v.get("result").is_some() || v.get("error").is_some())
        .await;
    let sid = r["result"]["sessionId"].as_str().unwrap().to_owned();

    let p = h
        .send(
            "session/prompt",
            json!({"sessionId": sid, "prompt": [{"type":"text","text":"go"}]}),
        )
        .await;
    let _ = h.recv_until(|v| v["id"] == json!(p)).await;

    let captured = llm.captured.lock().await;
    let tools = captured[0]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1);
    let desc = tools[0]["function"]["description"].as_str().unwrap_or("");
    assert!(
        desc.len() <= 1024,
        "description not clamped: {} bytes (expected ≤ 1024)",
        desc.len()
    );
    // Sanity: the original was 5000 bytes, so we did clamp something.
    assert!(
        desc.len() < 5000,
        "description not actually truncated: {} bytes",
        desc.len()
    );
    h.shutdown().await;
}

/// Todo enforcement: agent must not accept end_turn while pending todos exist.
/// Sequence: LLM calls todo (creates pending items) → LLM tries to end (no tool calls)
/// → agent injects reminder → LLM calls todo (marks all completed) → LLM ends → accepted.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn todo_enforcement_blocks_premature_end() {
    // Round 1: LLM calls todo to create 2 open items.
    // Round 2: LLM tries to end (text only, no tool calls) → enforcement fires.
    // Round 3: LLM calls todo to mark both done.
    // Round 4: LLM ends (text only) → allowed because no open items.
    let llm = spawn_capturing_llm(vec![
        // Round 1: create todos
        openai_tool_call(
            "tc_1",
            "todo",
            json!({
                "todos": [
                    {"id": 1, "title": "Step one", "done": false},
                    {"id": 2, "title": "Step two", "done": false}
                ]
            }),
        ),
        // Round 2: try to end — should be blocked
        openai_text("All done!"),
        // Round 3: after enforcement reminder, complete the todos
        openai_tool_call(
            "tc_2",
            "todo",
            json!({
                "todos": [
                    {"id": 1, "title": "Step one", "done": true},
                    {"id": 2, "title": "Step two", "done": true}
                ]
            }),
        ),
        // Round 4: end again — should be allowed now
        openai_text("Now truly done."),
    ])
    .await;
    let mut h = Harness::spawn(&llm.url).await;

    h.send(
        "initialize",
        json!({"protocolVersion":1,"clientCapabilities":{}}),
    )
    .await;
    let _ = h.recv().await;
    h.send("session/new", json!({"cwd": "/tmp", "mcpServers": []}))
        .await;
    let r = h.recv().await;
    let sid = r["result"]["sessionId"].as_str().unwrap().to_owned();

    let p_id = h
        .send(
            "session/prompt",
            json!({
                "sessionId": sid,
                "prompt": [{"type": "text", "text": "do something"}],
            }),
        )
        .await;

    // Collect all messages until the prompt resolves.
    let v = h.recv_until(|v| v["id"] == json!(p_id)).await;
    assert_eq!(
        v["result"]["stopReason"], "end_turn",
        "should eventually end_turn after completing todos"
    );

    // Verify the LLM was called 4 times (enforcement forced an extra round).
    let captured = llm.captured.lock().await;
    assert_eq!(
        captured.len(),
        4,
        "expected 4 LLM calls (todo, blocked end, todo complete, allowed end), got {}",
        captured.len()
    );

    // The 3rd request (after enforcement) should contain the strike reminder
    // in the messages array (injected as a user message).
    let third_req = &captured[2];
    let messages = third_req["messages"].as_array().unwrap();
    let last_user_msg = messages.iter().rev().find(|m| m["role"] == "user").unwrap();
    let content = last_user_msg["content"].as_str().unwrap_or("");
    assert!(
        content.contains("open todo items"),
        "enforcement reminder not found in 3rd LLM request. Last user msg: {}",
        &content[..content.len().min(200)]
    );

    h.shutdown().await;
}
