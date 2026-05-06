use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::{oneshot, watch, Mutex};

use crate::llm::Llm;
use crate::mcp::McpRegistry;
use crate::types::{
    AgentError, Config, ContentBlock, HistoryItem, ProviderStop, StopReason, ToolCall, ToolResult,
    MAX_PROMPT_BYTES, MAX_TOOL_RESULT_BYTES,
};

const MAX_TOOL_CALLS_PER_TURN: usize = 64;

pub type Wire = tokio::sync::mpsc::Sender<WireMsg>;
pub type PendingMap = Arc<Mutex<HashMap<i64, oneshot::Sender<PermissionOutcome>>>>;

pub enum WireMsg {
    Notify(Value),
    Permission { id: i64, params: Value },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PermissionOutcome {
    Allow,
    Deny,
    Cancelled,
}

#[allow(clippy::too_many_arguments)]
pub async fn run_prompt(
    cfg: &Config, sid: &str, cancel: &mut watch::Receiver<bool>, wire: &Wire,
    llm: &Llm, mcp: &McpRegistry, pending: &PendingMap, next_id: &Arc<Mutex<i64>>,
    history: &mut Vec<HistoryItem>, prompt: Vec<ContentBlock>,
) -> Result<StopReason, AgentError> {
    let user_text = prompt.into_iter().map(|b| match b {
        ContentBlock::Text { text } => text,
        ContentBlock::ResourceLink { uri } => format!("[resource: {uri}]"),
        ContentBlock::Other => "[unsupported content block]".into(),
    }).collect::<Vec<_>>().join("\n");
    if user_text.len() > MAX_PROMPT_BYTES {
        return Err(AgentError::InvalidParams(format!("prompt exceeds {MAX_PROMPT_BYTES} bytes")));
    }
    history.push(HistoryItem::User(user_text));

    for _ in 0..cfg.max_rounds {
        if *cancel.borrow() {
            return Ok(StopReason::Cancelled);
        }

        truncate_history(history, cfg.max_history_bytes);

        let response = tokio::select! {
            biased;
            _ = cancel.changed() => return Ok(StopReason::Cancelled),
            r = llm.complete(cfg, history, mcp.tools()) => r?,
        };

        if response.tool_calls.is_empty() {
            history.push(HistoryItem::Assistant {
                text: response.text,
                tool_calls: Vec::new(),
            });
            return Ok(map_stop(response.stop));
        }

        let mut calls = response.tool_calls.clone();
        if calls.len() > MAX_TOOL_CALLS_PER_TURN {
            eprintln!(
                "sprout-agent: capping tool_calls {} -> {MAX_TOOL_CALLS_PER_TURN}",
                calls.len()
            );
            calls.truncate(MAX_TOOL_CALLS_PER_TURN);
        }
        history.push(HistoryItem::Assistant {
            text: response.text,
            tool_calls: calls.clone(),
        });

        let mut idx = 0usize;
        while idx < calls.len() {
            let call = &calls[idx];
            if *cancel.borrow() {
                fill_cancelled(history, &calls[idx..]);
                return Ok(StopReason::Cancelled);
            }

            if !mcp.has(&call.name) {
                let err = format!("unknown tool: {}", call.name);
                emit_failed(wire, sid, call, &err).await;
                history.push(synthetic_error(call, err));
                idx += 1;
                continue;
            }

            notify(wire, &update(sid, json!({
                "sessionUpdate": "tool_call",
                "toolCallId": call.provider_id,
                "title": call.name, "kind": "mcp",
                "status": "pending", "rawInput": call.arguments,
            }))).await;

            let perm_id = {
                let mut n = next_id.lock().await;
                let v = *n;
                *n += 1;
                v
            };
            let (tx, rx) = oneshot::channel();
            pending.lock().await.insert(perm_id, tx);
            let outcome = tokio::select! {
                biased;
                _ = cancel.changed() => PermissionOutcome::Cancelled,
                o = request_permission(wire, sid, perm_id, call, rx) => o,
            };
            if outcome == PermissionOutcome::Cancelled {
                pending.lock().await.remove(&perm_id);
            }
            match outcome {
                PermissionOutcome::Cancelled => {
                    emit_failed(wire, sid, call, "cancelled").await;
                    fill_cancelled(history, &calls[idx..]);
                    return Ok(StopReason::Cancelled);
                }
                PermissionOutcome::Deny => {
                    emit_failed(wire, sid, call, "permission denied").await;
                    history.push(synthetic_error(call, "permission denied".into()));
                    idx += 1;
                    continue;
                }
                PermissionOutcome::Allow => {}
            }

            notify(wire, &tool_status(sid, &call.provider_id, "in_progress")).await;

            let poison_and_fail = |reason: &str| {
                if let Some(server) = mcp.server_of(&call.name) {
                    mcp.poison(server, reason);
                }
            };
            let result = tokio::select! {
                biased;
                _ = cancel.changed() => {
                    poison_and_fail("cancelled during tool call");
                    emit_failed(wire, sid, call, "cancelled").await;
                    fill_cancelled(history, &calls[idx..]);
                    return Ok(StopReason::Cancelled);
                }
                r = tokio::time::timeout(cfg.tool_timeout,
                    mcp.call(&call.name, &call.provider_id, &call.arguments, MAX_TOOL_RESULT_BYTES),
                ) => match r {
                    Ok(Ok(r)) => r,
                    Ok(Err(e)) => {
                        let m = e.to_string();
                        poison_and_fail(&format!("transport error: {m}"));
                        emit_failed(wire, sid, call, &m).await;
                        history.push(synthetic_error(call, m));
                        idx += 1;
                        continue;
                    }
                    Err(_) => {
                        poison_and_fail("tool timeout");
                        emit_failed(wire, sid, call, "tool timeout").await;
                        history.push(synthetic_error(call, "tool timeout".into()));
                        idx += 1;
                        continue;
                    }
                },
            };

            notify(wire, &update(sid, json!({
                "sessionUpdate": "tool_call_update", "toolCallId": call.provider_id,
                "status": "completed",
                "content": [{ "type": "content", "content": { "type": "text", "text": result.text } }],
                "rawOutput": { "isError": result.is_error },
            }))).await;
            history.push(HistoryItem::ToolResult(result));
            idx += 1;
        }
    }

    Ok(StopReason::MaxTurnRequests)
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
            text.len() + tool_calls.iter().map(|c| {
                c.provider_id.len() + c.name.len()
                    + serde_json::to_vec(&c.arguments).map(|b| b.len()).unwrap_or(0)
            }).sum::<usize>()
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
        eprintln!("sprout-agent: history truncated {original_len} -> {} items ({total} bytes)",
            history.len());
    }
}

fn update(sid: &str, update: Value) -> Value {
    json!({ "jsonrpc": "2.0", "method": "session/update",
        "params": { "sessionId": sid, "update": update } })
}

fn tool_status(sid: &str, tool_id: &str, status: &str) -> Value {
    update(sid, json!({ "sessionUpdate": "tool_call_update",
        "toolCallId": tool_id, "status": status }))
}

async fn notify(wire: &Wire, msg: &Value) {
    let _ = wire.send(WireMsg::Notify(msg.clone())).await;
}

async fn emit_failed(wire: &Wire, sid: &str, call: &ToolCall, err: &str) {
    notify(wire, &update(sid, json!({
        "sessionUpdate": "tool_call_update", "toolCallId": call.provider_id,
        "status": "failed", "rawOutput": { "error": err },
    }))).await;
}

async fn request_permission(
    wire: &Wire, sid: &str, id: i64, call: &ToolCall,
    rx: oneshot::Receiver<PermissionOutcome>,
) -> PermissionOutcome {
    let params = json!({
        "sessionId": sid,
        "toolCall": { "toolCallId": call.provider_id, "title": call.name,
            "kind": "mcp", "rawInput": call.arguments },
        "options": [
            { "optionId": "allow", "name": "Allow", "kind": "allow_once" },
            { "optionId": "deny",  "name": "Deny",  "kind": "reject_once" },
        ],
    });
    if wire.send(WireMsg::Permission { id, params }).await.is_err() {
        return PermissionOutcome::Cancelled;
    }
    rx.await.unwrap_or(PermissionOutcome::Cancelled)
}

fn map_stop(p: ProviderStop) -> StopReason {
    match p {
        ProviderStop::EndTurn | ProviderStop::ToolUse | ProviderStop::Other => StopReason::EndTurn,
        ProviderStop::MaxTokens => StopReason::MaxTokens,
        ProviderStop::Refusal => StopReason::Refusal,
    }
}
