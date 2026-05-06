//! The tool loop. Append-only history. One in-flight prompt.
//!
//!   history → LLM → tool_use → MCP → result → history → loop
//!   No tool calls in LLM response ⇒ end_turn. Tool calls *are* the output.
//!
//! Cancellation: every long-running await is wrapped in `tokio::select!`
//! against a per-prompt `watch::Receiver<bool>`.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::{oneshot, watch, Mutex};

use crate::llm::Llm;
use crate::mcp::McpRegistry;
use crate::types::{
    AgentError, Config, ContentBlock, HistoryItem, ProviderStop, StopReason, ToolCall, ToolResult,
};

/// Hard cap on tool calls accepted from a single LLM response. A misbehaving
/// model that returns thousands of tool_calls cannot be allowed to drive
/// thousands of MCP calls per turn.
const MAX_TOOL_CALLS_PER_TURN: usize = 64;

/// Single ordered channel from the agent loop to the writer task.
pub type Wire = tokio::sync::mpsc::Sender<WireMsg>;

pub type PendingMap = Arc<Mutex<HashMap<i64, oneshot::Sender<PermissionOutcome>>>>;

pub enum WireMsg {
    Notify(Value),
    /// A permission request that has already been registered in `pending`
    /// under `id`; writer just formats and writes it.
    Permission {
        id: i64,
        params: Value,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PermissionOutcome {
    Allow,
    Deny,
    Cancelled,
}

#[allow(clippy::too_many_arguments)]
pub async fn run_prompt(
    cfg: &Config,
    sid: &str,
    cancel: &mut watch::Receiver<bool>,
    wire: &Wire,
    llm: &Llm,
    mcp: &McpRegistry,
    pending: &PendingMap,
    next_id: &Arc<Mutex<i64>>,
    history: &mut Vec<HistoryItem>,
    prompt: Vec<ContentBlock>,
) -> Result<StopReason, AgentError> {
    let user_text = flatten_prompt(prompt);
    if user_text.len() > cfg.max_prompt_bytes {
        return Err(AgentError::InvalidParams(format!(
            "prompt exceeds {} bytes",
            cfg.max_prompt_bytes
        )));
    }
    history.push(HistoryItem::User(user_text));

    for _ in 0..cfg.max_rounds {
        if *cancel.borrow() {
            return Ok(StopReason::Cancelled);
        }

        // Trim oldest non-system items if history exceeds the byte budget.
        // Cheap, crude, effective: keeps sessions from growing unboundedly.
        truncate_history(history, cfg.max_history_bytes);

        let response = tokio::select! {
            biased;
            _ = cancel.changed() => return Ok(StopReason::Cancelled),
            r = llm.complete(cfg, history, mcp.tools()) => r?,
        };

        // No tool calls ⇒ end_turn. Record the assistant turn (with the
        // model's text and empty tool_calls) so history stays valid for any
        // future prompt — a dangling User with no following Assistant breaks
        // multi-turn conversations, and dropping the text loses context.
        if response.tool_calls.is_empty() {
            history.push(HistoryItem::Assistant {
                text: response.text,
                tool_calls: Vec::new(),
            });
            return Ok(map_stop(response.stop));
        }

        // Cap tool calls per turn. A misbehaving model that demands hundreds
        // of calls cannot stampede the loop.
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

        // On cancellation we MUST flush a synthetic tool_result for every
        // tool_call that didn't get one — otherwise the next LLM call sees
        // an assistant tool_use without a matching tool_result and 400s.
        let mut idx = 0usize;
        while idx < calls.len() {
            let call = &calls[idx];
            if *cancel.borrow() {
                fill_cancelled(history, &calls[idx..]);
                return Ok(StopReason::Cancelled);
            }

            // Validate BEFORE asking permission.
            if !mcp.has(&call.name) {
                let err = format!("unknown tool: {}", call.name);
                emit_failed(wire, sid, call, &err).await;
                history.push(synthetic_error(call, err));
                idx += 1;
                continue;
            }

            // 1) tool_call (pending)
            notify(
                wire,
                &update(
                    sid,
                    json!({
                        "sessionUpdate": "tool_call",
                        "toolCallId": call.provider_id,
                        "title": call.name,
                        "kind": "mcp",
                        "status": "pending",
                        "rawInput": call.arguments,
                    }),
                ),
            )
            .await;

            // 2) request_permission. Allocate the id and register pending
            // BEFORE handing off to the writer, so cancellation can clean up.
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
            // Always remove the pending entry on any non-Allow/Deny exit
            // (cancel, wire send failure, dropped oneshot). Double-remove is a
            // no-op; this guarantees no leak regardless of which path fired.
            if outcome == PermissionOutcome::Cancelled {
                pending.lock().await.remove(&perm_id);
            }
            match outcome {
                PermissionOutcome::Cancelled => {
                    // Tool call was already announced as pending — emit a
                    // terminal failed/cancelled update before bailing.
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

            // 3) in_progress
            notify(
                wire,
                &update(
                    sid,
                    json!({
                        "sessionUpdate": "tool_call_update",
                        "toolCallId": call.provider_id,
                        "status": "in_progress",
                    }),
                ),
            )
            .await;

            // 4) MCP call (timeout + cancel)
            let result = tokio::select! {
                biased;
                _ = cancel.changed() => {
                    emit_failed(wire, sid, call, "cancelled").await;
                    fill_cancelled(history, &calls[idx..]);
                    return Ok(StopReason::Cancelled);
                }
                r = tokio::time::timeout(
                    cfg.tool_timeout,
                    mcp.call(&call.name, &call.provider_id, &call.arguments, cfg.max_tool_result_bytes),
                ) => match r {
                    Ok(Ok(r)) => r,
                    Ok(Err(e)) => {
                        let m = e.to_string();
                        emit_failed(wire, sid, call, &m).await;
                        history.push(synthetic_error(call, m));
                        idx += 1;
                        continue;
                    }
                    Err(_) => {
                        // Kill and poison the MCP server: the in-flight
                        // request is abandoned but the child may still be
                        // doing work with side effects. Subsequent calls
                        // to that server fail fast instead of pretending
                        // it's healthy.
                        if let Some(server) = mcp.server_of(&call.name) {
                            mcp.poison(server, "tool timeout");
                        }
                        emit_failed(wire, sid, call, "tool timeout").await;
                        history.push(synthetic_error(call, "tool timeout".into()));
                        idx += 1;
                        continue;
                    }
                },
            };

            // 5) terminal status (completed)
            notify(wire, &update(sid, json!({
                "sessionUpdate": "tool_call_update",
                "toolCallId": call.provider_id,
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

// ─── Helpers ────────────────────────────────────────────────────────────────

fn flatten_prompt(prompt: Vec<ContentBlock>) -> String {
    prompt
        .into_iter()
        .map(|b| match b {
            ContentBlock::Text { text } => text,
            ContentBlock::ResourceLink { uri } => format!("[resource: {uri}]"),
            ContentBlock::Other => "[unsupported content block]".into(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn synthetic_error(call: &ToolCall, msg: String) -> HistoryItem {
    HistoryItem::ToolResult(ToolResult {
        provider_id: call.provider_id.clone(),
        text: msg,
        is_error: true,
    })
}

/// On cancellation, every tool_call in the just-pushed assistant turn must
/// have a matching tool_result in history — otherwise the next LLM call
/// fails (Anthropic 400, OpenAI silent coercion). Append a synthetic
/// "cancelled" result for every call we didn't get to.
fn fill_cancelled(history: &mut Vec<HistoryItem>, remaining: &[ToolCall]) {
    for call in remaining {
        history.push(synthetic_error(call, "cancelled".into()));
    }
}

/// Approximate in-memory byte size of a history item. We don't need
/// wire-accurate; we just want a cheap monotone estimate.
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

/// Drop oldest items until history fits within `max_bytes`. We must keep
/// history VALID — never strand a tool_result without its assistant turn,
/// or vice versa — so we drop in conversation pairs:
///   `User → (Assistant → ToolResult*)+` blocks.
/// Conservative implementation: drop one User-rooted block at a time from
/// the front. If after dropping all blocks history still exceeds the cap,
/// stop (the latest turn alone is over-budget; the LLM will reject it).
fn truncate_history(history: &mut Vec<HistoryItem>, max_bytes: usize) {
    let mut total: usize = history.iter().map(item_bytes).sum();
    if total <= max_bytes {
        return;
    }
    let original_len = history.len();
    while total > max_bytes && !history.is_empty() {
        // Find the next User boundary after index 0; drop [0..boundary).
        let mut end = 1usize;
        while end < history.len() && !matches!(history[end], HistoryItem::User(_)) {
            end += 1;
        }
        if end >= history.len() {
            // Only the active turn remains; can't drop more without breaking
            // tool_use/tool_result pairing.
            break;
        }
        let dropped: usize = history[..end].iter().map(item_bytes).sum();
        history.drain(..end);
        total = total.saturating_sub(dropped);
    }
    if history.len() < original_len {
        eprintln!(
            "sprout-agent: history truncated {original_len} -> {} items ({total} bytes)",
            history.len()
        );
    }
}

fn update(sid: &str, update: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": { "sessionId": sid, "update": update },
    })
}

async fn notify(wire: &Wire, msg: &Value) {
    let _ = wire.send(WireMsg::Notify(msg.clone())).await;
}

async fn emit_failed(wire: &Wire, sid: &str, call: &ToolCall, err: &str) {
    notify(
        wire,
        &update(
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

async fn request_permission(
    wire: &Wire,
    sid: &str,
    id: i64,
    call: &ToolCall,
    rx: oneshot::Receiver<PermissionOutcome>,
) -> PermissionOutcome {
    let params = json!({
        "sessionId": sid,
        "toolCall": {
            "toolCallId": call.provider_id,
            "title": call.name,
            "kind": "mcp",
            "rawInput": call.arguments,
        },
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
