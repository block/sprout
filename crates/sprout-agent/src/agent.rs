//! The tool loop. Append-only history. One in-flight prompt.
//!
//!   history → LLM → tool_use → MCP → result → history → loop
//!   No tool calls in LLM response ⇒ end_turn. Tool calls *are* the output.
//!
//! Cancellation: every long-running await is wrapped in `tokio::select!`
//! against a per-prompt `watch::Receiver<bool>`.

use serde_json::{json, Value};
use tokio::sync::{oneshot, watch};

use crate::llm::Llm;
use crate::mcp::McpRegistry;
use crate::types::{
    AgentError, Config, ContentBlock, HistoryItem, ProviderStop, StopReason, ToolCall, ToolResult,
};

/// Single ordered channel from the agent loop to the writer task.
pub type Wire = tokio::sync::mpsc::Sender<WireMsg>;

pub enum WireMsg {
    Notify(Value),
    Permission {
        params: Value,
        reply: oneshot::Sender<PermissionOutcome>,
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

        let response = tokio::select! {
            biased;
            _ = cancel.changed() => return Ok(StopReason::Cancelled),
            r = llm.complete(cfg, history, mcp.tools()) => r?,
        };

        history.push(HistoryItem::Assistant {
            text: String::new(),
            tool_calls: response.tool_calls.clone(),
        });

        if response.tool_calls.is_empty() {
            return Ok(map_stop(response.stop));
        }

        for call in response.tool_calls {
            if *cancel.borrow() {
                return Ok(StopReason::Cancelled);
            }

            // Validate BEFORE asking permission.
            if !mcp.has(&call.name) {
                let err = format!("unknown tool: {}", call.name);
                emit_failed(wire, sid, &call, &err).await;
                history.push(synthetic_error(&call, err));
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

            // 2) request_permission
            let outcome = tokio::select! {
                biased;
                _ = cancel.changed() => PermissionOutcome::Cancelled,
                o = request_permission(wire, sid, &call) => o,
            };
            match outcome {
                PermissionOutcome::Cancelled => return Ok(StopReason::Cancelled),
                PermissionOutcome::Deny => {
                    emit_failed(wire, sid, &call, "permission denied").await;
                    history.push(synthetic_error(&call, "permission denied".into()));
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
                _ = cancel.changed() => return Ok(StopReason::Cancelled),
                r = tokio::time::timeout(
                    cfg.tool_timeout,
                    mcp.call(&call.name, &call.provider_id, &call.arguments, cfg.max_tool_result_bytes),
                ) => match r {
                    Ok(Ok(r)) => r,
                    Ok(Err(e)) => {
                        let m = e.to_string();
                        emit_failed(wire, sid, &call, &m).await;
                        history.push(synthetic_error(&call, m));
                        continue;
                    }
                    Err(_) => {
                        emit_failed(wire, sid, &call, "tool timeout").await;
                        history.push(synthetic_error(&call, "tool timeout".into()));
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

async fn request_permission(wire: &Wire, sid: &str, call: &ToolCall) -> PermissionOutcome {
    let (tx, rx) = oneshot::channel();
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
    if wire
        .send(WireMsg::Permission { params, reply: tx })
        .await
        .is_err()
    {
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
