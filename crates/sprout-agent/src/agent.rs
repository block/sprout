use std::collections::HashMap;
use std::sync::Arc;

use serde_json::json;
use tokio::sync::{oneshot, watch, Mutex, Semaphore};
use tokio::task::JoinSet;

use crate::config::{Config, MAX_PROMPT_BYTES, MAX_TOOL_CALLS_PER_TURN, MAX_TOOL_RESULT_BYTES};
use crate::handoff::HandoffOutcome;
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
    pub mcp: &'a Arc<McpRegistry>,
    pub wire: &'a WireSender,
    pub pending: &'a PendingMap,
    pub next_id: &'a Arc<Mutex<i64>>,
    pub cancel: &'a mut watch::Receiver<bool>,
    pub history: &'a mut Vec<HistoryItem>,
    pub original_task: &'a mut Option<String>,
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
            match self.maybe_handoff().await {
                HandoffOutcome::Cancelled => return Ok(StopReason::Cancelled),
                HandoffOutcome::Performed => {}
                HandoffOutcome::Skipped => {
                    truncate_history(self.history, self.cfg.max_history_bytes)
                }
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

            if let Some(stop) = self.execute_calls(&calls).await? {
                return Ok(stop);
            }
        }
    }

    async fn execute_calls(
        &mut self,
        calls: &[ToolCall],
    ) -> Result<Option<StopReason>, AgentError> {
        // Decide path up front. Sequential is a verbatim restoration of the
        // pre-refactor per-call loop; parallel uses the three-phase design.
        if self.cfg.parallel_tools && self.cfg.auto_approve {
            self.execute_calls_parallel(calls).await
        } else {
            self.execute_calls_sequential(calls).await
        }
    }

    /// Sequential path: emit pending → permission → in_progress → invoke →
    /// completed → push to history, one call at a time. Behavior-preserving
    /// with respect to the pre-refactor implementation.
    async fn execute_calls_sequential(
        &mut self,
        calls: &[ToolCall],
    ) -> Result<Option<StopReason>, AgentError> {
        for (idx, call) in calls.iter().enumerate() {
            if *self.cancel.borrow() {
                fill_cancelled(self.history, &calls[idx..]);
                return Ok(Some(StopReason::Cancelled));
            }
            self.emit_pending(call).await;
            if !self.mcp.has(&call.name) {
                let err = format!("unknown tool: {}", call.name);
                emit_failed(self.wire, self.session_id, call, &err).await;
                self.history.push(synthetic_error(call, err));
                continue;
            }
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
            emit_in_progress(self.wire, self.session_id, call).await;
            // Sequential path: kill the MCP server on cancel/timeout (safe —
            // only one call is in flight at a time).
            let outcome = invoke_tool_inner(
                self.mcp,
                call,
                self.cfg.tool_timeout,
                self.cancel.clone(),
                /* kill_on_failure */ true,
            )
            .await;
            match outcome {
                InvokeOutcome::Done(result) => {
                    emit_completed(self.wire, self.session_id, call, &result).await;
                    self.history.push(HistoryItem::ToolResult(result));
                }
                InvokeOutcome::Failed(msg) => {
                    emit_failed(self.wire, self.session_id, call, &msg).await;
                    self.history.push(synthetic_error(call, msg));
                }
                InvokeOutcome::Timeout { msg, .. } => {
                    // Sequential path: invoke_tool_inner already killed +
                    // mark_dead'd the server (kill_on_failure=true).
                    emit_failed(self.wire, self.session_id, call, &msg).await;
                    self.history.push(synthetic_error(call, msg));
                }
                InvokeOutcome::Cancelled => {
                    emit_failed(self.wire, self.session_id, call, "cancelled").await;
                    fill_cancelled(self.history, &calls[idx..]);
                    return Ok(Some(StopReason::Cancelled));
                }
            }
        }
        Ok(None)
    }

    /// Parallel path: three phases.
    /// 1. Preflight (sequential): emit pending, check tool, ask permission
    ///    (no-op since auto_approve is required). Each call either becomes
    ///    runnable (slot stays None) or its slot is filled with a synthetic
    ///    result.
    /// 2. Execute: spawn runnable calls into a JoinSet bounded by a
    ///    semaphore. select! between cancel and join_next.
    /// 3. Append: results are pushed to history in original call order.
    async fn execute_calls_parallel(
        &mut self,
        calls: &[ToolCall],
    ) -> Result<Option<StopReason>, AgentError> {
        let mut results: Vec<Option<ToolResult>> = vec![None; calls.len()];
        let mut runnable: Vec<usize> = Vec::with_capacity(calls.len());

        // Phase 1: preflight.
        for (idx, call) in calls.iter().enumerate() {
            if *self.cancel.borrow() {
                // Cancel during preflight: fill ALL still-empty slots
                // (including ones earlier than idx that were marked runnable).
                // Emit terminal wire updates for any that already had "pending" emitted.
                for (j, c) in calls.iter().enumerate() {
                    if results[j].is_none() {
                        // Calls 0..idx already had emit_pending called.
                        if j < idx {
                            emit_failed(self.wire, self.session_id, c, "cancelled").await;
                        }
                        results[j] = Some(synthetic_tool_result(c, "cancelled".into()));
                    }
                }
                self.append_results(calls, &mut results);
                return Ok(Some(StopReason::Cancelled));
            }
            self.emit_pending(call).await;
            if !self.mcp.has(&call.name) {
                let err = format!("unknown tool: {}", call.name);
                emit_failed(self.wire, self.session_id, call, &err).await;
                results[idx] = Some(synthetic_tool_result(call, err));
                continue;
            }
            // auto_approve is required for the parallel path, so no
            // ask_permission round-trip.
            runnable.push(idx);
        }

        // Phase 2: execute.
        self.execute_parallel(calls, &runnable, &mut results).await;

        // Phase 3: append in original call order.
        self.append_results(calls, &mut results);

        if *self.cancel.borrow() {
            Ok(Some(StopReason::Cancelled))
        } else {
            Ok(None)
        }
    }

    fn append_results(&mut self, calls: &[ToolCall], results: &mut [Option<ToolResult>]) {
        for (i, call) in calls.iter().enumerate() {
            let result = results[i].take().unwrap_or_else(|| ToolResult {
                provider_id: call.provider_id.clone(),
                text: "internal error: missing result".into(),
                is_error: true,
            });
            self.history.push(HistoryItem::ToolResult(result));
        }
    }

    async fn execute_parallel(
        &mut self,
        calls: &[ToolCall],
        runnable: &[usize],
        results: &mut [Option<ToolResult>],
    ) {
        let limit = self.cfg.parallel_tools_limit.max(1);
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
                // Acquire a permit; if the semaphore is closed treat as cancelled.
                let _permit = match sem.acquire_owned().await {
                    Ok(p) => p,
                    Err(_) => return (i, InvokeOutcome::Cancelled),
                };
                emit_in_progress(&wire, &session_id, &call).await;
                // Parallel path: do NOT kill the MCP server on cancel/timeout.
                // With concurrent calls to the same server, killing here
                // races with another in-flight call's restart. The MCP
                // registry's lazy restart already handles dead servers.
                let outcome = invoke_tool_inner(
                    &mcp, &call, timeout, cancel, /* kill_on_failure */ false,
                )
                .await;
                match &outcome {
                    InvokeOutcome::Done(result) => {
                        emit_completed(&wire, &session_id, &call, result).await;
                    }
                    InvokeOutcome::Failed(msg) => {
                        emit_failed(&wire, &session_id, &call, msg).await;
                    }
                    InvokeOutcome::Timeout { server_name, msg } => {
                        // Parallel path: don't kill (races with concurrent
                        // calls / lazy restart) but DO mark dead so the next
                        // call triggers a lazy restart.
                        if let Some(server) = server_name {
                            mcp.mark_dead(server, "tool timeout");
                        }
                        emit_failed(&wire, &session_id, &call, msg).await;
                    }
                    InvokeOutcome::Cancelled => {
                        emit_failed(&wire, &session_id, &call, "cancelled").await;
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
                    // Cancel: stop accepting permits, abort tasks, drain.
                    // NOTE: we do NOT use `set.shutdown().await` here — that
                    // calls abort_all() and then drains until empty,
                    // throwing away the joined results. We need those
                    // results so already-completed tasks (whose wire
                    // "completed" notification was already sent) don't get
                    // overwritten with synthetic "cancelled".
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
                            // JoinError (panic or cancellation). We don't know the index;
                            // best we can do is log and continue. The post-loop fill
                            // catches any missing slots.
                            eprintln!("sprout-agent: agent: parallel tool task join error: {e}");
                        }
                        None => break, // all done
                    }
                }
            }
        }

        // After cancel + abort_all, drain remaining tasks. abort_all() only
        // *requests* cancellation; already-completed tasks are still
        // joinable and we must record their results (otherwise the wire
        // already said "completed" but history would say "cancelled" —
        // status divergence). join_next().await also collects newly-aborted
        // tasks as JoinErrors (cancelled).
        if cancelled {
            while let Some(joined) = set.join_next().await {
                match joined {
                    Ok((i, outcome)) => {
                        if results[i].is_none() {
                            results[i] = Some(outcome_to_result(&calls[i], outcome));
                        }
                    }
                    Err(e) => {
                        // Task was aborted (or panicked). Index is unknown;
                        // the post-loop fill catches the missing slot.
                        eprintln!(
                            "sprout-agent: agent: parallel tool task join error (drain): {e}"
                        );
                    }
                }
            }
        }

        // Fill any remaining unfilled runnable slots as cancelled. These
        // tasks never reached their own emit_failed path (they were aborted
        // before, during, or after acquiring a permit), so emit the
        // terminal "failed" wire update here — otherwise the client sees
        // "pending" forever.
        for &i in runnable {
            if results[i].is_none() {
                results[i] = Some(synthetic_tool_result(&calls[i], "cancelled".into()));
                emit_failed(self.wire, self.session_id, &calls[i], "cancelled").await;
            }
        }
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
            _ = tokio::time::sleep(self.cfg.tool_timeout) => PermissionOutcome::Deny,
            o = rx => o.unwrap_or(PermissionOutcome::Cancelled),
        };
        if outcome != PermissionOutcome::Allow {
            self.pending.lock().await.remove(&perm_id);
        }
        outcome
    }
}

/// Outcome of invoking a single tool. The wire notification is emitted by the
/// caller so the parallel and sequential paths share the same logic.
enum InvokeOutcome {
    Done(ToolResult),
    Failed(String),
    /// Tool timed out. `server_name` (when known) lets the parallel path
    /// mark the server dead without killing it (avoiding the kill-vs-restart
    /// race). The sequential path collapses this into a kill+mark_dead.
    Timeout {
        server_name: Option<String>,
        msg: String,
    },
    Cancelled,
}

/// Standalone tool invocation. Takes only owned/cloned handles so it can run
/// inside a spawned task. Performs the MCP call with timeout and returns an
/// outcome.
///
/// `kill_on_failure`: when true, the offending MCP server is killed and
/// marked dead on cancel/timeout (sequential path only — safe because at
/// most one call is in flight). The parallel path passes false: killing a
/// server while another concurrent call is using/restarting it races with
/// the registry's lifecycle. The MCP registry already lazily restarts dead
/// servers on the next call.
async fn invoke_tool_inner(
    mcp: &Arc<McpRegistry>,
    call: &ToolCall,
    tool_timeout: std::time::Duration,
    mut cancel: watch::Receiver<bool>,
    kill_on_failure: bool,
) -> InvokeOutcome {
    tokio::select! {
        biased;
        _ = cancel.changed() => {
            if kill_on_failure {
                if let Some(server) = mcp.server_of(&call.name).map(str::to_owned) {
                    mcp.kill_server(&server);
                    mcp.mark_dead(&server, "cancelled");
                }
            }
            InvokeOutcome::Cancelled
        }
        r = tokio::time::timeout(
            tool_timeout,
            mcp.call(&call.name, &call.provider_id, &call.arguments, MAX_TOOL_RESULT_BYTES),
        ) => match r {
            Ok(Ok(result)) => InvokeOutcome::Done(result),
            Ok(Err(e)) => InvokeOutcome::Failed(e.to_string()),
            Err(_) => {
                let server_name = mcp.server_of(&call.name).map(str::to_owned);
                let msg = format!(
                    "tool: timeout after {}s. The command took too long. Try a faster approach.",
                    tool_timeout.as_secs()
                );
                if kill_on_failure {
                    if let Some(server) = server_name.as_deref() {
                        mcp.kill_server(server);
                        mcp.mark_dead(server, "tool timeout");
                    }
                }
                InvokeOutcome::Timeout { server_name, msg }
            }
        },
    }
}

/// Convert an `InvokeOutcome` into a `ToolResult`. Errors, timeouts, and
/// cancellations become synthetic error results; success passes through.
fn outcome_to_result(call: &ToolCall, outcome: InvokeOutcome) -> ToolResult {
    match outcome {
        InvokeOutcome::Done(r) => r,
        InvokeOutcome::Failed(m) => synthetic_tool_result(call, m),
        InvokeOutcome::Timeout { msg, .. } => synthetic_tool_result(call, msg),
        InvokeOutcome::Cancelled => synthetic_tool_result(call, "cancelled".into()),
    }
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

fn synthetic_error(call: &ToolCall, msg: String) -> HistoryItem {
    HistoryItem::ToolResult(synthetic_tool_result(call, msg))
}

fn fill_cancelled(history: &mut Vec<HistoryItem>, remaining: &[ToolCall]) {
    for call in remaining {
        history.push(synthetic_error(call, "cancelled".into()));
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
