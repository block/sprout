//! The tool loop. Append-only history. One in-flight prompt.
//!
//! ┌─────────┐  ┌──────┐  ┌──────────┐  ┌─────┐
//! │ history │─►│ LLM  │─►│ tool_use │─►│ MCP │─► result → history → loop
//! └─────────┘  └──────┘  └──────────┘  └─────┘
//!
//! No tool_use ⇒ end_turn.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::oneshot;

use crate::llm::Llm;
use crate::mcp::{truncate_for_context, McpRegistry};
use crate::types::{
    AgentError, Config, ContentBlock, HistoryItem, McpContent, ProviderStop, StopReason, ToolCall,
    ToolResult,
};

/// Channel from the agent loop to the writer task.
pub type AcpOut = tokio::sync::mpsc::Sender<AcpEvent>;

/// Outbound events the agent emits. The writer task serializes them to stdout.
pub enum AcpEvent {
    /// `session/update` notification with arbitrary `update` payload.
    Update { session_id: String, update: Value },
    /// `session/request_permission` request — wait for the response on `reply`.
    Permission {
        session_id: String,
        tool_call: Value,
        reply: oneshot::Sender<PermissionOutcome>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PermissionOutcome {
    Allow,
    Deny,
    Cancelled,
}

pub struct Session {
    pub id: String,
    pub cancelled: Arc<AtomicBool>,
}

/// Run one prompt to completion, returning the ACP stop reason.
pub async fn run_prompt(
    cfg: &Config,
    session: &Session,
    out: &AcpOut,
    llm: &Llm,
    mcp: &McpRegistry,
    history: &mut Vec<HistoryItem>,
    prompt: Vec<ContentBlock>,
) -> Result<StopReason, AgentError> {
    let user_text = prompt
        .into_iter()
        .filter_map(|b| match b {
            ContentBlock::Text { text } => Some(text),
            ContentBlock::Other => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    if user_text.len() > cfg.max_prompt_bytes {
        return Err(AgentError::InvalidParams(format!(
            "prompt exceeds {} bytes",
            cfg.max_prompt_bytes
        )));
    }

    history.push(HistoryItem::User { text: user_text });

    let tools = mcp.tools();

    for _round in 0..cfg.max_rounds {
        if session.cancelled.load(Ordering::Acquire) {
            return Ok(StopReason::Cancelled);
        }

        let response = llm.complete(cfg, history, tools).await?;

        history.push(HistoryItem::Assistant {
            text: response.text.clone(),
            tool_calls: response.tool_calls.clone(),
        });

        if response.tool_calls.is_empty() {
            // Debug-only: emit one agent_message_chunk with the buffered text.
            if !response.text.is_empty() {
                let _ = out
                    .send(AcpEvent::Update {
                        session_id: session.id.clone(),
                        update: json!({
                            "sessionUpdate": "agent_message_chunk",
                            "content": { "type": "text", "text": response.text },
                        }),
                    })
                    .await;
            }
            return Ok(map_stop(response.stop_reason));
        }

        for call in response.tool_calls {
            if session.cancelled.load(Ordering::Acquire) {
                return Ok(StopReason::Cancelled);
            }

            // 1) tool_call (pending)
            send_update(
                out,
                &session.id,
                json!({
                    "sessionUpdate": "tool_call",
                    "toolCallId": call.provider_id,
                    "title": call.name,
                    "kind": "mcp",
                    "status": "pending",
                    "rawInput": call.arguments,
                }),
            )
            .await;

            // 2) request_permission
            let outcome = request_permission(out, &session.id, &call).await;

            match outcome {
                PermissionOutcome::Cancelled => {
                    session.cancelled.store(true, Ordering::Release);
                    return Ok(StopReason::Cancelled);
                }
                PermissionOutcome::Deny => {
                    send_update(
                        out,
                        &session.id,
                        json!({
                            "sessionUpdate": "tool_call_update",
                            "toolCallId": call.provider_id,
                            "status": "failed",
                            "rawOutput": { "error": "permission denied" },
                        }),
                    )
                    .await;
                    history.push(HistoryItem::ToolResult(ToolResult::synthetic(
                        &call,
                        "permission denied",
                        true,
                    )));
                    continue;
                }
                PermissionOutcome::Allow => {}
            }

            // 3) in_progress
            send_update(
                out,
                &session.id,
                json!({
                    "sessionUpdate": "tool_call_update",
                    "toolCallId": call.provider_id,
                    "status": "in_progress",
                }),
            )
            .await;

            // 4) call MCP with timeout
            let result = call_one(cfg, mcp, &call).await;

            // 5) emit terminal status + push history
            let update = if result.infrastructure_failed {
                json!({
                    "sessionUpdate": "tool_call_update",
                    "toolCallId": call.provider_id,
                    "status": "failed",
                    "rawOutput": { "error": result.summary() },
                })
            } else {
                json!({
                    "sessionUpdate": "tool_call_update",
                    "toolCallId": call.provider_id,
                    "status": "completed",
                    "content": acp_content_blocks(&result.content),
                })
            };
            send_update(out, &session.id, update).await;
            history.push(HistoryItem::ToolResult(result));
        }
    }

    Ok(StopReason::MaxTurnRequests)
}

async fn call_one(cfg: &Config, mcp: &McpRegistry, call: &ToolCall) -> ToolResult {
    if !mcp.has(&call.name) {
        return ToolResult::synthetic(call, format!("unknown tool: {}", call.name), true);
    }
    match tokio::time::timeout(cfg.tool_timeout, mcp.call(&call.name, &call.arguments)).await {
        Ok(Ok(res)) => {
            let (truncated, content) = truncate_for_context(res.content, cfg.max_tool_result_bytes);
            ToolResult {
                provider_id: call.provider_id.clone(),
                name: call.name.clone(),
                content,
                is_error: res.is_error,
                infrastructure_failed: false,
                truncated,
            }
        }
        Ok(Err(e)) => ToolResult::synthetic(call, format!("mcp error: {e}"), true),
        Err(_) => ToolResult::synthetic(call, "tool timeout".to_string(), true),
    }
}

async fn request_permission(out: &AcpOut, session_id: &str, call: &ToolCall) -> PermissionOutcome {
    let (tx, rx) = oneshot::channel();
    let tool_call = json!({
        "toolCallId": call.provider_id,
        "title": call.name,
        "kind": "mcp",
        "rawInput": call.arguments,
    });
    if out
        .send(AcpEvent::Permission {
            session_id: session_id.to_string(),
            tool_call,
            reply: tx,
        })
        .await
        .is_err()
    {
        return PermissionOutcome::Cancelled;
    }
    rx.await.unwrap_or(PermissionOutcome::Cancelled)
}

async fn send_update(out: &AcpOut, session_id: &str, update: Value) {
    let _ = out
        .send(AcpEvent::Update {
            session_id: session_id.to_string(),
            update,
        })
        .await;
}

fn acp_content_blocks(content: &[McpContent]) -> Vec<Value> {
    content
        .iter()
        .map(|c| match c {
            McpContent::Text { text } => json!({
                "type": "content",
                "content": { "type": "text", "text": text },
            }),
            McpContent::Image { data, mime_type } => json!({
                "type": "content",
                "content": { "type": "image", "data": data, "mimeType": mime_type },
            }),
            McpContent::Audio { data, mime_type } => json!({
                "type": "content",
                "content": { "type": "audio", "data": data, "mimeType": mime_type },
            }),
            McpContent::ResourceLink { uri } => json!({
                "type": "content",
                "content": { "type": "resource_link", "uri": uri },
            }),
            McpContent::Other(v) => json!({
                "type": "content",
                "content": { "type": "text", "text": serde_json::to_string(v).unwrap_or_default() },
            }),
        })
        .collect()
}

fn map_stop(p: ProviderStop) -> StopReason {
    match p {
        ProviderStop::EndTurn => StopReason::EndTurn,
        ProviderStop::ToolUse => StopReason::EndTurn, // shouldn't reach here w/ no tool calls
        ProviderStop::MaxTokens => StopReason::MaxTokens,
        ProviderStop::Refusal => StopReason::Refusal,
        ProviderStop::Other => StopReason::EndTurn,
    }
}
