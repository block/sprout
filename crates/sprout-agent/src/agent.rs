use std::collections::HashMap;
use std::sync::Arc;

use serde_json::json;
use tokio::sync::{oneshot, watch, Mutex};

use crate::config::{Config, MAX_PROMPT_BYTES, MAX_TOOL_CALLS_PER_TURN, MAX_TOOL_RESULT_BYTES};
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
}

impl RunCtx<'_> {
    pub async fn run(&mut self, prompt: Vec<ContentBlock>) -> Result<StopReason, AgentError> {
        let user_text = prompt_to_text(prompt)?;
        if user_text.len() > MAX_PROMPT_BYTES {
            return Err(AgentError::InvalidParams(format!(
                "prompt: exceeds {MAX_PROMPT_BYTES} bytes"
            )));
        }
        self.history.push(HistoryItem::User(user_text));

        for _ in 0..self.cfg.max_rounds {
            if *self.cancel.borrow() {
                return Ok(StopReason::Cancelled);
            }
            truncate_history(self.history, self.cfg.max_history_bytes);

            let response = tokio::select! {
                biased;
                _ = self.cancel.changed() => return Ok(StopReason::Cancelled),
                r = self.llm.complete(self.cfg, self.history, self.mcp.tools()) => r?,
            };

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
        Ok(StopReason::MaxTurnRequests)
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
        let outcome = tokio::select! {
            biased;
            _ = cancel.changed() => PermissionOutcome::Cancelled,
            o = rx => o.unwrap_or(PermissionOutcome::Cancelled),
        };
        if outcome == PermissionOutcome::Cancelled {
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
        let poison = |reason: &str| {
            if let Some(server) = self.mcp.server_of(&call.name) {
                self.mcp.poison(server, reason);
            }
        };
        let result = tokio::select! {
            biased;
            _ = self.cancel.changed() => {
                poison("cancelled during tool call");
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
                    let m = e.to_string();
                    poison(&format!("transport error: {m}"));
                    emit_failed(self.wire, self.session_id, call, &m).await;
                    self.history.push(synthetic_error(call, m));
                    return Ok(None);
                }
                Err(_) => {
                    poison("tool timeout");
                    emit_failed(self.wire, self.session_id, call, "tool: timeout").await;
                    self.history.push(synthetic_error(call, "tool: timeout".into()));
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

fn item_bytes(item: &HistoryItem) -> usize {
    match item {
        HistoryItem::User(s) => s.len(),
        HistoryItem::Assistant { text, tool_calls } => {
            text.len()
                + tool_calls
                    .iter()
                    .map(|c| {
                        c.provider_id.len()
                            + c.name.len()
                            + serde_json::to_vec(&c.arguments)
                                .map(|b| b.len())
                                .unwrap_or(0)
                    })
                    .sum::<usize>()
        }
        HistoryItem::ToolResult(r) => r.provider_id.len() + r.text.len(),
    }
}

fn truncate_history(history: &mut Vec<HistoryItem>, max_bytes: usize) {
    let mut total: usize = history.iter().map(item_bytes).sum();
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
        let dropped: usize = history[..end].iter().map(item_bytes).sum();
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
