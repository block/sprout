use std::sync::Arc;

use serde_json::json;
use tokio::sync::{watch, Semaphore};
use tokio::task::JoinSet;

use crate::config::{Config, MAX_PROMPT_BYTES, MAX_TOOL_CALLS_PER_TURN, MAX_TOOL_RESULT_BYTES};
use crate::handoff::HandoffOutcome;
use crate::llm::Llm;
use crate::mcp::McpRegistry;
use crate::todo::EndTurn;
use crate::types::{
    AgentError, ContentBlock, HistoryItem, ProviderStop, StopReason, ToolCall, ToolResult,
};
use crate::wire::{self, WireSender};

const ERROR_REFLECTION_SUFFIX: &str =
    "\n\n[Reflect] Before retrying, identify the cause and change your approach.";

pub struct RunCtx<'a> {
    pub cfg: &'a Config,
    pub session_id: &'a str,
    pub llm: &'a Llm,
    pub mcp: &'a Arc<McpRegistry>,
    pub wire: &'a WireSender,
    pub cancel: &'a mut watch::Receiver<bool>,
    pub history: &'a mut Vec<HistoryItem>,
    pub original_task: &'a mut Option<String>,
    pub handoff_count: &'a mut usize,
    pub todos: &'a mut crate::todo::Todos,
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
            if *self.cancel.borrow() {
                return Ok(StopReason::Cancelled);
            }
            match self.maybe_handoff().await {
                HandoffOutcome::Cancelled => return Ok(StopReason::Cancelled),
                HandoffOutcome::Performed => {}
                HandoffOutcome::Skipped => {
                    truncate_history(self.history, self.cfg.max_history_bytes)
                }
            }

            let mut tools = self.mcp.tools();
            if let Some(td) = self.todos.tool_def() {
                tools.push(td);
            }
            round = round.saturating_add(1);
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
                let stop = map_stop(response.stop);
                // Only gate genuine end_turn — don't override max_tokens/refusal.
                if stop == StopReason::EndTurn {
                    match self.todos.check_end_turn() {
                        EndTurn::Allow => {}
                        EndTurn::Continue(msg) => {
                            // Inject reminder as a synthetic user turn and
                            // loop again. This is the "nag" path.
                            self.history.push(HistoryItem::User(msg));
                            continue;
                        }
                        EndTurn::Stop(msg) => {
                            // Surface the reason in chat then stop.
                            wire::send(
                                self.wire,
                                wire::session_update(
                                    self.session_id,
                                    json!({
                                        "sessionUpdate": "agent_message_chunk",
                                        "content": { "type": "text", "text": msg }
                                    }),
                                ),
                            )
                            .await;
                            return Ok(StopReason::EndTurn);
                        }
                    }
                }
                return Ok(stop);
            }

            let mut calls = response.tool_calls;
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

            if let Some(stop) = self.execute_calls(&calls).await {
                return Ok(stop);
            }
        }
    }

    /// Unified tool-call execution. Three phases:
    ///   1. Preflight (sequential): emit `pending`; unknown tools fail fast
    ///      with a synthetic result. Cancel here fills every still-empty
    ///      slot as cancelled.
    ///   2. Execute: spawn runnable calls into a `JoinSet` bounded by a
    ///      `Semaphore(max_parallel_tools)`. `select!` between cancel and
    ///      `join_next`. On cancel: `abort_all`, drain joined results,
    ///      synthesize cancelled for unfilled slots and emit `failed`.
    ///   3. Append: push results into history in original call order.
    ///
    /// `max_parallel_tools = 1` makes phase 2 effectively sequential
    /// (one in-flight call at a time via the semaphore). Larger values
    /// run that many calls concurrently.
    async fn execute_calls(&mut self, calls: &[ToolCall]) -> Option<StopReason> {
        let mut results: Vec<Option<ToolResult>> = vec![None; calls.len()];
        let mut runnable: Vec<usize> = Vec::with_capacity(calls.len());

        // Phase 1: preflight.
        for (idx, call) in calls.iter().enumerate() {
            if *self.cancel.borrow() {
                for (j, c) in calls.iter().enumerate() {
                    if results[j].is_none() {
                        // Calls 0..idx already had `pending` emitted; emit
                        // a terminal `failed` so the client doesn't see
                        // them stuck.
                        if j < idx {
                            emit_failed(self.wire, self.session_id, c, "cancelled").await;
                        }
                        results[j] = Some(synthetic_tool_result(c, "cancelled".into()));
                    }
                }
                self.append_results(calls, &mut results);
                return Some(StopReason::Cancelled);
            }
            emit_pending(self.wire, self.session_id, call).await;
            // Intercept the agent-side todo tool before MCP. Synchronous
            // and cheap; we emit in_progress + completed/failed ourselves
            // so the wire shape matches an MCP tool exactly.
            if call.name == crate::todo::TOOL_NAME && self.todos.is_enabled() {
                emit_in_progress(self.wire, self.session_id, call).await;
                let (result, ok) = match self.todos.handle_call(&call.arguments) {
                    Ok(text) => (
                        ToolResult {
                            provider_id: call.provider_id.clone(),
                            text,
                            is_error: false,
                        },
                        true,
                    ),
                    Err(e) => (
                        ToolResult {
                            provider_id: call.provider_id.clone(),
                            text: format!("Error: {e}"),
                            is_error: true,
                        },
                        false,
                    ),
                };
                if ok {
                    emit_completed(self.wire, self.session_id, call, &result).await;
                } else {
                    emit_failed(self.wire, self.session_id, call, &result.text).await;
                }
                results[idx] = Some(result);
                continue;
            }
            if !self.mcp.has(&call.name) {
                let err = format!("unknown tool: {}", call.name);
                emit_failed(self.wire, self.session_id, call, &err).await;
                results[idx] = Some(synthetic_tool_result(call, err));
                continue;
            }
            runnable.push(idx);
        }

        // Phase 2: execute.
        self.execute_parallel(calls, &runnable, &mut results).await;

        // Phase 3: append in original call order.
        self.append_results(calls, &mut results);

        if *self.cancel.borrow() {
            Some(StopReason::Cancelled)
        } else {
            None
        }
    }

    fn append_results(&mut self, calls: &[ToolCall], results: &mut [Option<ToolResult>]) {
        for (i, call) in calls.iter().enumerate() {
            let mut result = results[i].take().unwrap_or_else(|| ToolResult {
                provider_id: call.provider_id.clone(),
                text: "internal error: missing result".into(),
                is_error: true,
            });
            // On tool error: append a reflection prompt so the LLM
            // diagnoses the failure before blindly retrying.
            if result.is_error {
                result.text.push_str(ERROR_REFLECTION_SUFFIX);
            }
            // Single banner injection point. The todo tool's own response
            // is already the bare list; the banner (if open items remain)
            // is added here too. `decorate` is a no-op when disabled or
            // when all items are done.
            self.todos.decorate(&mut result.text);
            self.history.push(HistoryItem::ToolResult(result));
        }
    }

    async fn execute_parallel(
        &mut self,
        calls: &[ToolCall],
        runnable: &[usize],
        results: &mut [Option<ToolResult>],
    ) {
        let limit = self.cfg.max_parallel_tools.max(1);
        let sem = Arc::new(Semaphore::new(limit));
        let mut set: JoinSet<(usize, InvokeOutcome)> = JoinSet::new();

        for &i in runnable {
            let call = calls[i].clone();
            let mcp = Arc::clone(self.mcp);
            let wire = self.wire.clone();
            let session_id = self.session_id.to_owned();
            let timeout = self.cfg.tool_timeout;
            let cancel = self.cancel.clone();
            let sem = Arc::clone(&sem);
            set.spawn(async move {
                // Acquire a permit; if the semaphore is closed (cancel),
                // emit a terminal wire update and skip the call.
                let _permit = match sem.acquire_owned().await {
                    Ok(p) => p,
                    Err(_) => {
                        emit_failed(&wire, &session_id, &call, "cancelled").await;
                        return (i, InvokeOutcome::Failed("cancelled".into()));
                    }
                };
                emit_in_progress(&wire, &session_id, &call).await;
                let outcome = invoke_tool_inner(&mcp, &call, timeout, cancel).await;
                match &outcome {
                    InvokeOutcome::Done(result) => {
                        emit_completed(&wire, &session_id, &call, result).await;
                    }
                    InvokeOutcome::Failed(msg) => {
                        emit_failed(&wire, &session_id, &call, msg).await;
                    }
                }
                (i, outcome)
            });
        }

        let mut cancel_rx = self.cancel.clone();
        let mut cancelled = false;
        loop {
            tokio::select! {
                biased;
                _ = cancel_rx.changed() => {
                    // Cancel: stop accepting new permits, abort tasks.
                    // We do NOT use `set.shutdown().await` — that drops
                    // already-completed results we still need.
                    sem.close();
                    set.abort_all();
                    cancelled = true;
                    break;
                }
                next = set.join_next() => {
                    match next {
                        Some(Ok((i, outcome))) => {
                            results[i] = Some(outcome_to_result(&calls[i], outcome));
                        }
                        Some(Err(e)) => {
                            eprintln!("sprout-agent: agent: tool task join error: {e}");
                        }
                        None => break,
                    }
                }
            }
        }

        // After cancel + abort_all, drain remaining tasks. abort_all only
        // *requests* cancellation; already-completed tasks are still
        // joinable and we must record their results (otherwise the wire
        // already said "completed" but history would say "cancelled" —
        // status divergence). join_next also collects newly-aborted tasks
        // as JoinErrors.
        if cancelled {
            while let Some(joined) = set.join_next().await {
                match joined {
                    Ok((i, outcome)) => {
                        if results[i].is_none() {
                            results[i] = Some(outcome_to_result(&calls[i], outcome));
                        }
                    }
                    Err(e) => {
                        eprintln!("sprout-agent: agent: tool task join error (drain): {e}");
                    }
                }
            }
        }

        // Fill any remaining unfilled runnable slots as cancelled. These
        // tasks were aborted before reaching their own emit_failed path,
        // so emit the terminal "failed" wire update here — otherwise the
        // client sees "pending" forever.
        for &i in runnable {
            if results[i].is_none() {
                results[i] = Some(synthetic_tool_result(&calls[i], "cancelled".into()));
                emit_failed(self.wire, self.session_id, &calls[i], "cancelled").await;
            }
        }
    }
}

/// Outcome of invoking a single tool. The wire notification is emitted by
/// the caller so the spawn loop and the (degenerate, max_parallel=1) path
/// share the same logic.
enum InvokeOutcome {
    Done(ToolResult),
    Failed(String),
}

/// Standalone tool invocation. Takes only owned/cloned handles so it can
/// run inside a spawned task. On timeout, marks the offending MCP server
/// dead (the registry's lazy restart handles it on the next call) but does
/// NOT kill the process — killing races with concurrent calls / restart.
async fn invoke_tool_inner(
    mcp: &Arc<McpRegistry>,
    call: &ToolCall,
    tool_timeout: std::time::Duration,
    mut cancel: watch::Receiver<bool>,
) -> InvokeOutcome {
    // Check if already cancelled before waiting for changes (a cloned
    // receiver that starts at the current version won't fire changed()).
    if *cancel.borrow() {
        return InvokeOutcome::Failed("cancelled".into());
    }
    tokio::select! {
        biased;
        _ = cancel.changed() => InvokeOutcome::Failed("cancelled".into()),
        r = tokio::time::timeout(
            tool_timeout,
            mcp.call(&call.name, &call.provider_id, &call.arguments, MAX_TOOL_RESULT_BYTES),
        ) => match r {
            Ok(Ok(result)) => InvokeOutcome::Done(result),
            Ok(Err(e)) => InvokeOutcome::Failed(e.to_string()),
            Err(_) => {
                if let Some(server) = mcp.server_of(&call.name) {
                    mcp.mark_dead(server, "tool timeout");
                }
                let msg = format!(
                    "tool: timeout after {}s. The command took too long. Try a faster approach.",
                    tool_timeout.as_secs()
                );
                InvokeOutcome::Failed(msg)
            }
        },
    }
}

fn outcome_to_result(call: &ToolCall, outcome: InvokeOutcome) -> ToolResult {
    match outcome {
        InvokeOutcome::Done(r) => r,
        InvokeOutcome::Failed(m) => synthetic_tool_result(call, m),
    }
}

async fn emit_pending(wire: &WireSender, sid: &str, call: &ToolCall) {
    wire::send(
        wire,
        wire::session_update(
            sid,
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

async fn emit_in_progress(wire: &WireSender, sid: &str, call: &ToolCall) {
    wire::send(
        wire,
        wire::session_update(
            sid,
            json!({
                "sessionUpdate": "tool_call_update",
                "toolCallId": call.provider_id,
                "status": "in_progress",
            }),
        ),
    )
    .await;
}

async fn emit_completed(wire: &WireSender, sid: &str, call: &ToolCall, result: &ToolResult) {
    wire::send(
        wire,
        wire::session_update(
            sid,
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

fn synthetic_tool_result(call: &ToolCall, msg: String) -> ToolResult {
    ToolResult {
        provider_id: call.provider_id.clone(),
        text: msg,
        is_error: true,
    }
}

pub(crate) fn truncate_history(history: &mut Vec<HistoryItem>, max_bytes: usize) {
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

fn map_stop(p: ProviderStop) -> StopReason {
    match p {
        ProviderStop::EndTurn | ProviderStop::ToolUse | ProviderStop::Other => StopReason::EndTurn,
        ProviderStop::MaxTokens => StopReason::MaxTokens,
        ProviderStop::Refusal => StopReason::Refusal,
    }
}
