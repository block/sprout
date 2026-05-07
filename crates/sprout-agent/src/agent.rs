use std::collections::HashMap;
use std::sync::Arc;

use serde_json::json;
use tokio::sync::{oneshot, watch, Mutex};

use crate::config::{
    Config, HANDOFF_MAX_OUTPUT_TOKENS, HANDOFF_MAX_TOOL_NAMES, HANDOFF_ORIGINAL_TASK_MAX_BYTES,
    HANDOFF_PROMPT_MAX_BYTES, HANDOFF_TAIL_ITEMS, HANDOFF_THRESHOLD, MAX_PROMPT_BYTES,
    MAX_TOOL_CALLS_PER_TURN, MAX_TOOL_RESULT_BYTES,
};
use crate::llm::Llm;
use crate::mcp::McpRegistry;
use crate::types::{
    AgentError, ContentBlock, HistoryItem, PermissionOutcome, ProviderStop, StopReason, ToolCall,
    ToolResult,
};
use crate::wire::{self, WireMsg, WireSender};

pub type PendingMap = Arc<Mutex<HashMap<i64, oneshot::Sender<PermissionOutcome>>>>;

pub struct RunCtx<'a> {
    pub cfg: &'a Config,
    pub session_id: &'a str,
    pub llm: &'a Llm,
    pub mcp: &'a McpRegistry,
    pub wire: &'a WireSender,
    pub pending: &'a PendingMap,
    pub next_id: &'a Arc<Mutex<i64>>,
    pub cancel: &'a mut watch::Receiver<bool>,
    pub history: &'a mut Vec<HistoryItem>,
    /// First user prompt verbatim. Set on the first invocation of `run` and
    /// preserved across context handoffs so the agent never loses sight of
    /// the original task.
    pub original_task: &'a mut Option<String>,
    /// Number of internal handoffs performed this session.
    pub handoff_count: &'a mut usize,
}

impl RunCtx<'_> {
    pub async fn run(&mut self, prompt: Vec<ContentBlock>) -> Result<StopReason, AgentError> {
        let user_text = prompt_to_text(prompt)?;
        if user_text.len() > MAX_PROMPT_BYTES {
            return Err(AgentError::InvalidParams(format!(
                "prompt: exceeds {MAX_PROMPT_BYTES} bytes"
            )));
        }
        if self.original_task.is_none() {
            *self.original_task = Some(user_text.clone());
        }
        self.history.push(HistoryItem::User(user_text));

        let mut round = 0u32;
        loop {
            if self.cfg.max_rounds > 0 && round >= self.cfg.max_rounds {
                return Ok(StopReason::MaxTurnRequests);
            }
            round = round.saturating_add(1);
            if *self.cancel.borrow() {
                return Ok(StopReason::Cancelled);
            }
            // Try internal handoff first; if it ran, history is already a
            // single fresh-summary message and we skip truncation. If it was
            // skipped or failed, fall back to the existing truncation path.
            if !self.maybe_handoff().await {
                truncate_history(self.history, self.cfg.max_history_bytes);
            }

            let tools = self.mcp.tools();
            let response = tokio::select! {
                biased;
                _ = self.cancel.changed() => return Ok(StopReason::Cancelled),
                r = self.llm.complete(self.cfg, self.history, &tools) => r?,
            };

            if !response.text.is_empty() {
                wire::send(
                    self.wire,
                    wire::session_update(
                        self.session_id,
                        json!({
                            "sessionUpdate": "agent_message_chunk",
                            "content": { "type": "text", "text": &response.text }
                        }),
                    ),
                )
                .await;
            }

            if response.tool_calls.is_empty() {
                if response.stop == ProviderStop::ToolUse {
                    return Err(AgentError::Llm(
                        "provider: stop=tool_use but zero tool_calls".into(),
                    ));
                }
                self.history.push(HistoryItem::Assistant {
                    text: response.text,
                    tool_calls: Vec::new(),
                });
                return Ok(map_stop(response.stop));
            }

            let mut calls = response.tool_calls.clone();
            if calls.len() > MAX_TOOL_CALLS_PER_TURN {
                eprintln!(
                    "sprout-agent: agent: capping tool_calls {} -> {MAX_TOOL_CALLS_PER_TURN}",
                    calls.len()
                );
                calls.truncate(MAX_TOOL_CALLS_PER_TURN);
            }
            self.history.push(HistoryItem::Assistant {
                text: response.text,
                tool_calls: calls.clone(),
            });

            if let Some(stop) = self.execute_calls(&calls).await? {
                return Ok(stop);
            }
        }
    }

    /// Returns true if a handoff was performed (history was reset to a
    /// summary). Returns false if not needed, capped, or the summary call
    /// failed — in which case the caller should fall back to truncation.
    async fn maybe_handoff(&mut self) -> bool {
        if !self.should_handoff() {
            return false;
        }
        if *self.handoff_count >= self.cfg.max_handoffs {
            eprintln!(
                "sprout-agent: agent: handoff cap reached ({}); using truncation",
                self.cfg.max_handoffs
            );
            return false;
        }
        let prompt = self.build_handoff_prompt();
        let summary = tokio::select! {
            biased;
            _ = self.cancel.changed() => return false,
            r = self.llm.summarize(
                self.cfg,
                HANDOFF_SYSTEM_PROMPT,
                &prompt,
                HANDOFF_MAX_OUTPUT_TOKENS,
            ) => match r {
                Ok(s) if !s.trim().is_empty() => s,
                Ok(_) => {
                    eprintln!("sprout-agent: agent: handoff returned empty summary; truncating");
                    return false;
                }
                Err(e) => {
                    eprintln!("sprout-agent: agent: handoff failed: {e}; truncating");
                    return false;
                }
            },
        };
        let prior = self.history.len();
        self.history.clear();
        self.history
            .push(HistoryItem::User(format!("[Context Handoff]\n{summary}")));
        *self.handoff_count += 1;
        eprintln!(
            "sprout-agent: agent: handoff #{} (history {prior} -> 1 item)",
            *self.handoff_count
        );
        true
    }

    fn should_handoff(&self) -> bool {
        let usage: usize = self.history.iter().map(HistoryItem::estimated_bytes).sum();
        let threshold = (self.cfg.max_history_bytes as f64 * HANDOFF_THRESHOLD) as usize;
        usage > threshold
    }

    fn build_handoff_prompt(&self) -> String {
        // Header + original task + tools list. Original task is clamped to a
        // hard cap so a 1MB initial prompt can't dominate the summary call.
        let mut head = String::new();
        head.push_str(&format!(
            "[Internal handoff #{} — context reset]\n\n",
            *self.handoff_count + 1
        ));
        head.push_str("# Original Task\n");
        let task = self.original_task.as_deref().unwrap_or("(unknown)");
        head.push_str(&clamp_bytes(task, HANDOFF_ORIGINAL_TASK_MAX_BYTES));
        head.push_str("\n\n# Available Tools\n");
        let all_tools = self.mcp.tools();
        let total = all_tools.len();
        if total == 0 {
            head.push_str("(none)\n");
        } else {
            let shown = total.min(HANDOFF_MAX_TOOL_NAMES);
            let names: Vec<&str> = all_tools[..shown].iter().map(|t| t.name.as_str()).collect();
            head.push_str(&names.join(", "));
            if shown < total {
                head.push_str(&format!(", … (+{} more)", total - shown));
            }
            head.push('\n');
        }

        // Assemble snippets for the trailing history window, then drop oldest
        // snippets until the whole prompt fits HANDOFF_PROMPT_MAX_BYTES. The
        // head and tail (instructions) are always preserved.
        let tail = "\n# Instructions\n\
             Produce a context handoff summary covering: (1) original task, \
             (2) what was accomplished, (3) key decisions, (4) what remains, \
             (5) one concrete next step. Be concise but thorough. Plain text.\n";
        let history_header = "\n# Recent History (most recent last)\n";

        let start = self.history.len().saturating_sub(HANDOFF_TAIL_ITEMS);
        let mut snippets: Vec<String> = self.history[start..]
            .iter()
            .map(|item| {
                let mut s = String::new();
                push_history_snippet(&mut s, item);
                s
            })
            .collect();

        let fixed = head.len() + history_header.len() + tail.len();
        let mut snippets_bytes: usize = snippets.iter().map(String::len).sum();
        let mut dropped = 0usize;
        while fixed + snippets_bytes > HANDOFF_PROMPT_MAX_BYTES && !snippets.is_empty() {
            // Drop oldest snippet first (front of vec).
            let removed = snippets.remove(0);
            snippets_bytes -= removed.len();
            dropped += 1;
        }
        if dropped > 0 {
            eprintln!("sprout-agent: agent: handoff prompt cap, dropped {dropped} oldest snippets");
        }

        let mut out =
            String::with_capacity(fixed + snippets_bytes + if dropped > 0 { 32 } else { 0 });
        out.push_str(&head);
        out.push_str(history_header);
        if dropped > 0 {
            out.push_str(&format!("(… {dropped} older items omitted)\n"));
        }
        for s in &snippets {
            out.push_str(s);
        }
        out.push_str(tail);
        out
    }

    async fn execute_calls(
        &mut self,
        calls: &[ToolCall],
    ) -> Result<Option<StopReason>, AgentError> {
        for (idx, call) in calls.iter().enumerate() {
            if *self.cancel.borrow() {
                fill_cancelled(self.history, &calls[idx..]);
                return Ok(Some(StopReason::Cancelled));
            }
            if !self.mcp.has(&call.name) {
                let err = format!("unknown tool: {}", call.name);
                emit_failed(self.wire, self.session_id, call, &err).await;
                self.history.push(synthetic_error(call, err));
                continue;
            }
            self.emit_pending(call).await;
            if !self.cfg.auto_approve {
                match self.ask_permission(call).await {
                    PermissionOutcome::Cancelled => {
                        emit_failed(self.wire, self.session_id, call, "cancelled").await;
                        fill_cancelled(self.history, &calls[idx..]);
                        return Ok(Some(StopReason::Cancelled));
                    }
                    PermissionOutcome::Deny => {
                        emit_failed(self.wire, self.session_id, call, "permission denied").await;
                        self.history
                            .push(synthetic_error(call, "permission denied".into()));
                        continue;
                    }
                    PermissionOutcome::Allow => {}
                }
            }
            wire::send(
                self.wire,
                wire::session_update(
                    self.session_id,
                    json!({
                        "sessionUpdate": "tool_call_update",
                        "toolCallId": call.provider_id,
                        "status": "in_progress",
                    }),
                ),
            )
            .await;
            if let Some(stop) = self.invoke_tool(call, calls, idx).await? {
                return Ok(Some(stop));
            }
        }
        Ok(None)
    }

    async fn emit_pending(&self, call: &ToolCall) {
        wire::send(
            self.wire,
            wire::session_update(
                self.session_id,
                json!({
                    "sessionUpdate": "tool_call",
                    "toolCallId": call.provider_id,
                    "title": call.name,
                    "kind": "other",
                    "status": "pending",
                    "rawInput": call.arguments,
                }),
            ),
        )
        .await;
    }

    async fn ask_permission(&self, call: &ToolCall) -> PermissionOutcome {
        let perm_id = {
            let mut n = self.next_id.lock().await;
            let v = *n;
            *n += 1;
            v
        };
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(perm_id, tx);
        let params = wire::permission_request(
            self.session_id,
            &call.provider_id,
            &call.name,
            &call.arguments,
        );
        if self
            .wire
            .send(WireMsg::Permission {
                id: perm_id,
                params,
            })
            .await
            .is_err()
        {
            self.pending.lock().await.remove(&perm_id);
            return PermissionOutcome::Cancelled;
        }
        let mut cancel = self.cancel.clone();
        // A non-responsive client must not wedge the session forever.
        // Bound the wait by tool_timeout and treat expiry as Deny.
        let outcome = tokio::select! {
            biased;
            _ = cancel.changed() => PermissionOutcome::Cancelled,
            _ = tokio::time::sleep(self.cfg.tool_timeout) => PermissionOutcome::Deny,
            o = rx => o.unwrap_or(PermissionOutcome::Cancelled),
        };
        if outcome != PermissionOutcome::Allow {
            self.pending.lock().await.remove(&perm_id);
        }
        outcome
    }

    async fn invoke_tool(
        &mut self,
        call: &ToolCall,
        calls: &[ToolCall],
        idx: usize,
    ) -> Result<Option<StopReason>, AgentError> {
        let result = tokio::select! {
            biased;
            _ = self.cancel.changed() => {
                // Session ending — kill first (while pgid is still in Healthy
                // state), then mark dead for consistent state.
                if let Some(server) = self.mcp.server_of(&call.name).map(str::to_owned) {
                    self.mcp.kill_server(&server);
                    self.mcp.mark_dead(&server, "cancelled");
                }
                emit_failed(self.wire, self.session_id, call, "cancelled").await;
                fill_cancelled(self.history, &calls[idx..]);
                return Ok(Some(StopReason::Cancelled));
            }
            r = tokio::time::timeout(
                self.cfg.tool_timeout,
                self.mcp.call(&call.name, &call.provider_id, &call.arguments, MAX_TOOL_RESULT_BYTES),
            ) => match r {
                Ok(Ok(r)) => r,
                Ok(Err(e)) => {
                    // Tool call failed. mcp.call() has already marked the
                    // server dead if it was a transport error. Surface the
                    // error to the LLM; lazy restart happens on the next call.
                    let m = e.to_string();
                    emit_failed(self.wire, self.session_id, call, &m).await;
                    self.history.push(synthetic_error(call, m));
                    return Ok(None);
                }
                Err(_) => {
                    // Tool timed out — kill the pgid to unstick the child,
                    // then mark the server dead. Lazy restart on next call.
                    if let Some(server) = self.mcp.server_of(&call.name).map(str::to_owned) {
                        self.mcp.kill_server(&server);
                        self.mcp.mark_dead(&server, "tool timeout");
                    }
                    let m = format!(
                        "tool: timeout after {}s. The command took too long. Try a faster approach.",
                        self.cfg.tool_timeout.as_secs()
                    );
                    emit_failed(self.wire, self.session_id, call, &m).await;
                    self.history.push(synthetic_error(call, m));
                    return Ok(None);
                }
            },
        };
        wire::send(
            self.wire,
            wire::session_update(
                self.session_id,
                json!({
                    "sessionUpdate": "tool_call_update",
                    "toolCallId": call.provider_id,
                    "status": "completed",
                    "content": [{ "type": "content", "content": { "type": "text", "text": result.text } }],
                    "rawOutput": { "isError": result.is_error },
                }),
            ),
        )
        .await;
        self.history.push(HistoryItem::ToolResult(result));
        Ok(None)
    }
}

fn prompt_to_text(prompt: Vec<ContentBlock>) -> Result<String, AgentError> {
    let mut parts = Vec::with_capacity(prompt.len());
    for block in prompt {
        match block {
            ContentBlock::Text { text } => parts.push(text),
            ContentBlock::ResourceLink { uri } => parts.push(format!("[resource: {uri}]")),
            ContentBlock::Unsupported => {
                return Err(AgentError::InvalidParams(
                    "prompt: unsupported content block (only text and resource_link are advertised)".into(),
                ));
            }
        }
    }
    Ok(parts.join("\n"))
}

fn synthetic_error(call: &ToolCall, msg: String) -> HistoryItem {
    HistoryItem::ToolResult(ToolResult {
        provider_id: call.provider_id.clone(),
        text: msg,
        is_error: true,
    })
}

fn fill_cancelled(history: &mut Vec<HistoryItem>, remaining: &[ToolCall]) {
    for call in remaining {
        history.push(synthetic_error(call, "cancelled".into()));
    }
}

const HANDOFF_SYSTEM_PROMPT: &str = "You are generating a context handoff summary for the next \
turn of an autonomous agent. Be concise but thorough. Cover: what the original task was, what \
you accomplished, key decisions made, what remains, and one concrete next step. Output plain \
text only — no tool calls, no JSON. Stay under 8192 tokens.";

const HANDOFF_SNIPPET_BYTES: usize = 2048;

fn push_history_snippet(out: &mut String, item: &HistoryItem) {
    match item {
        HistoryItem::User(s) => {
            out.push_str("[user] ");
            out.push_str(&clamp_for_snippet(s));
            out.push('\n');
        }
        HistoryItem::Assistant { text, tool_calls } => {
            out.push_str("[assistant] ");
            if !text.is_empty() {
                out.push_str(&clamp_for_snippet(text));
            }
            for c in tool_calls {
                out.push_str(&format!(" tool:{}", c.name));
            }
            out.push('\n');
        }
        HistoryItem::ToolResult(r) => {
            out.push_str(if r.is_error { "[tool_err] " } else { "[tool] " });
            out.push_str(&clamp_for_snippet(&r.text));
            out.push('\n');
        }
    }
}

fn clamp_for_snippet(s: &str) -> String {
    clamp_bytes(s, HANDOFF_SNIPPET_BYTES)
}

/// Truncate `s` to at most `max_bytes`, snapping back to a UTF-8 char
/// boundary, and append an ellipsis if truncated.
fn clamp_bytes(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_owned();
    }
    if max_bytes < 4 {
        return s[..max_bytes.min(s.len())].to_owned();
    }
    let target = max_bytes - "…".len();
    let mut cut = target;
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}…", &s[..cut])
}

fn truncate_history(history: &mut Vec<HistoryItem>, max_bytes: usize) {
    let mut total: usize = history.iter().map(HistoryItem::estimated_bytes).sum();
    if total <= max_bytes {
        return;
    }
    let original_len = history.len();
    while total > max_bytes && !history.is_empty() {
        let mut end = 1usize;
        while end < history.len() && !matches!(history[end], HistoryItem::User(_)) {
            end += 1;
        }
        if end >= history.len() {
            break;
        }
        let dropped: usize = history[..end]
            .iter()
            .map(HistoryItem::estimated_bytes)
            .sum();
        history.drain(..end);
        total = total.saturating_sub(dropped);
    }
    if history.len() < original_len {
        eprintln!(
            "sprout-agent: agent: history truncated {original_len} -> {} items ({total} bytes)",
            history.len()
        );
    }
}

async fn emit_failed(wire: &WireSender, sid: &str, call: &ToolCall, err: &str) {
    wire::send(
        wire,
        wire::session_update(
            sid,
            json!({
                "sessionUpdate": "tool_call_update",
                "toolCallId": call.provider_id,
                "status": "failed",
                "rawOutput": { "error": err },
            }),
        ),
    )
    .await;
}

fn map_stop(p: ProviderStop) -> StopReason {
    match p {
        ProviderStop::EndTurn | ProviderStop::ToolUse | ProviderStop::Other => StopReason::EndTurn,
        ProviderStop::MaxTokens => StopReason::MaxTokens,
        ProviderStop::Refusal => StopReason::Refusal,
    }
}
