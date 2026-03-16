//! Agent pool — owns N AcpClient instances and dispatches prompt tasks.
//!
//! # Mental model
//!
//! ```text
//!   AgentPool
//!   ├── agents: Vec<Option<OwnedAgent>>   ← idle agents sit here
//!   ├── join_set: JoinSet<()>             ← in-flight tasks
//!   ├── task_map: HashMap<Id, TaskMeta>   ← panic recovery metadata
//!   └── result_tx/rx: mpsc channel        ← tasks return agents here
//!
//!   Dispatch:
//!     try_claim() → OwnedAgent (removed from slot)
//!     spawn run_prompt_task(agent, ...) into join_set
//!     task sends PromptResult { agent, outcome } via result_tx
//!     rx_and_join_set() → poll result_rx for PromptResult
//!     return_agent(agent) → puts agent back in slot
//! ```
//!
//! `AcpClient` is NOT Clone — ownership moves out on claim and back on return.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio::time::timeout;
use uuid::Uuid;

use crate::acp::{AcpClient, AcpError, McpServer, StopReason};
use crate::config::DedupMode;
use crate::queue::{ContextMessage, ConversationContext, FlushBatch, PromptChannelInfo};
use crate::relay::{ChannelInfo, RestClient};

// ── FlushBatch Clone note ─────────────────────────────────────────────────────
// FlushBatch and BatchEvent derive Clone (added in queue.rs) so we can store
// a recoverable copy in TaskMeta for panic recovery in Queue mode.

// ── Types ─────────────────────────────────────────────────────────────────────

/// Metadata stored per in-flight task for panic recovery.
pub struct TaskMeta {
    pub agent_index: usize,
    pub channel_id: Option<Uuid>,
    /// Clone of batch for Queue mode panic recovery.
    pub recoverable_batch: Option<FlushBatch>,
}

/// An agent with its session state, owned by the pool or a running task.
pub struct OwnedAgent {
    pub index: usize,
    pub acp: AcpClient,
    /// channel_id → session_id
    pub sessions: HashMap<Uuid, String>,
    pub heartbeat_session: Option<String>,
}

/// Pool of agents with take-and-return ownership semantics.
///
/// Agents are either idle (sitting in `agents[i]`) or checked out
/// (running inside a spawned task). The `task_map` tracks in-flight
/// tasks for panic recovery.
pub struct AgentPool {
    agents: Vec<Option<OwnedAgent>>,
    result_tx: mpsc::UnboundedSender<PromptResult>,
    result_rx: mpsc::UnboundedReceiver<PromptResult>,
    pub join_set: JoinSet<()>,
    task_map: HashMap<tokio::task::Id, TaskMeta>,
}

/// Result returned by a completed prompt task.
pub struct PromptResult {
    pub agent: OwnedAgent,
    pub source: PromptSource,
    pub outcome: PromptOutcome,
    /// Present on failure in Queue mode, for requeue.
    pub batch: Option<FlushBatch>,
}

/// Whether the prompt came from a channel event or a heartbeat.
pub enum PromptSource {
    Channel(Uuid),
    Heartbeat,
}

/// Outcome of a prompt task.
#[allow(dead_code)]
pub enum PromptOutcome {
    Ok(StopReason),
    Error(AcpError),
    AgentExited,
    Timeout,
}

/// Immutable config subset shared (via `Arc`) by all spawned prompt tasks.
///
/// Built once from `Config` at startup. Avoids cloning the full config
/// into every task.
pub struct PromptContext {
    pub mcp_servers: Vec<McpServer>,
    pub initial_message: Option<String>,
    pub turn_timeout: Duration,
    pub dedup_mode: DedupMode,
    pub system_prompt: Option<String>,
    pub heartbeat_prompt: Option<String>,
    pub cwd: String,
    /// REST client for pre-prompt context fetches (thread/DM history).
    pub rest_client: RestClient,
    /// Channel metadata from discovery (name, type). Read-only after startup.
    pub channel_info: std::collections::HashMap<Uuid, ChannelInfo>,
    /// Max messages to include in thread/DM context. 0 = disabled.
    pub context_message_limit: u32,
}

// ── AgentPool impl ────────────────────────────────────────────────────────────

impl AgentPool {
    /// Create a new pool from a list of initialized agents.
    ///
    /// Agents are placed into indexed slots. The unbounded channel is created
    /// here; tasks send results back through `result_tx`.
    pub fn new(agents: Vec<OwnedAgent>) -> Self {
        let (result_tx, result_rx) = mpsc::unbounded_channel();
        let slots = agents.into_iter().map(Some).collect();
        Self {
            agents: slots,
            result_tx,
            result_rx,
            join_set: JoinSet::new(),
            task_map: HashMap::new(),
        }
    }

    /// Try to claim an idle agent for the given channel (or heartbeat if `None`).
    ///
    /// Pass 1: prefer an agent that already has a session for `channel_id`.
    /// Pass 2: any idle agent.
    ///
    /// Returns `None` if all agents are checked out.
    pub fn try_claim(&mut self, channel_id: Option<Uuid>) -> Option<OwnedAgent> {
        // Pass 1: prefer agent with existing session for this channel.
        if let Some(cid) = channel_id {
            let idx = self.agents.iter().position(|slot| {
                slot.as_ref()
                    .map(|a| a.sessions.contains_key(&cid))
                    .unwrap_or(false)
            });
            if let Some(i) = idx {
                return self.agents[i].take();
            }
        }

        // Pass 2: first idle agent.
        let idx = self.agents.iter().position(|slot| slot.is_some());
        idx.map(|i| self.agents[i].take().unwrap())
    }

    /// Return an agent to its slot after a task completes.
    pub fn return_agent(&mut self, agent: OwnedAgent) {
        let idx = agent.index;
        debug_assert!(
            self.agents[idx].is_none(),
            "return_agent: slot {idx} already occupied"
        );
        self.agents[idx] = Some(agent);
    }

    /// Whether any agent is currently idle (sitting in its slot).
    pub fn any_idle(&self) -> bool {
        self.agents.iter().any(|slot| slot.is_some())
    }

    /// Whether any idle agent already has a session for `channel_id`.
    /// Used to compute `affinity_hit` before calling `try_claim`.
    pub fn has_session_for(&self, channel_id: Uuid) -> bool {
        self.agents.iter().any(|slot| {
            slot.as_ref()
                .map(|a| a.sessions.contains_key(&channel_id))
                .unwrap_or(false)
        })
    }

    /// Count of agents that are alive: idle OR checked out (have a task_map entry).
    ///
    /// Used to detect when all agents have exited so the caller can respawn.
    pub fn live_count(&self) -> usize {
        let idle = self.agents.iter().filter(|s| s.is_some()).count();
        let checked_out = self.task_map.len();
        idle + checked_out
    }

    // ── Accessors ─────────────────────────────────────────────────────────

    pub fn task_map(&self) -> &HashMap<tokio::task::Id, TaskMeta> {
        &self.task_map
    }

    pub fn task_map_mut(&mut self) -> &mut HashMap<tokio::task::Id, TaskMeta> {
        &mut self.task_map
    }

    pub fn result_tx(&self) -> mpsc::UnboundedSender<PromptResult> {
        self.result_tx.clone()
    }

    /// Split-borrow: returns mutable refs to `result_rx` and `join_set`
    /// simultaneously. This lets callers poll both in a single `select!`
    /// without a double-borrow error on `&mut AgentPool`.
    pub fn rx_and_join_set(
        &mut self,
    ) -> (&mut mpsc::UnboundedReceiver<PromptResult>, &mut JoinSet<()>) {
        (&mut self.result_rx, &mut self.join_set)
    }

    pub fn agents_mut(&mut self) -> &mut Vec<Option<OwnedAgent>> {
        &mut self.agents
    }

    /// Remove the session for `channel_id` from all idle agents.
    ///
    /// Called when the agent is removed from a channel — stale sessions
    /// should not be reused. Checked-out agents (in-flight) are not
    /// modified; their sessions will fail naturally on the next prompt
    /// if the relay rejects the request.
    ///
    /// Returns the number of sessions invalidated.
    pub fn invalidate_channel_sessions(&mut self, channel_id: Uuid) -> usize {
        let mut count = 0;
        for slot in &mut self.agents {
            if let Some(agent) = slot.as_mut() {
                if agent.sessions.remove(&channel_id).is_some() {
                    count += 1;
                }
            }
        }
        count
    }
}

// ── run_prompt_task ───────────────────────────────────────────────────────────

/// Timeout for pre-prompt context fetches (thread/DM history).
const CONTEXT_FETCH_TIMEOUT: Duration = Duration::from_millis(500);

/// Core async function spawned for each prompt.
///
/// Lifecycle:
/// 1. Resolve or create a session (channel or heartbeat).
/// 2. Send `initial_message` on new channel sessions (if configured).
/// 3. Fetch conversation context if needed (thread reply or DM).
/// 4. Build the prompt text from batch + context.
/// 5. Send the actual prompt with turn timeout.
/// 6. Handle all error paths, always returning the agent via `result_tx`.
///
/// The agent is ALWAYS returned — even on panic the `JoinSet` detects the
/// abort and the caller uses `task_map` to recover the agent index.
pub async fn run_prompt_task(
    mut agent: OwnedAgent,
    batch: Option<FlushBatch>,
    prompt_text: Option<String>,
    ctx: Arc<PromptContext>,
    result_tx: mpsc::UnboundedSender<PromptResult>,
) {
    // ── Determine source and resolve/create session ───────────────────────

    // Is this a channel prompt or a heartbeat?
    let source = match &batch {
        Some(b) => PromptSource::Channel(b.channel_id),
        None => PromptSource::Heartbeat,
    };

    let (session_id, is_new_session) = match &source {
        PromptSource::Channel(cid) => {
            if let Some(sid) = agent.sessions.get(cid) {
                (sid.clone(), false)
            } else {
                // Create new session.
                match agent
                    .acp
                    .session_new(&ctx.cwd, ctx.mcp_servers.clone())
                    .await
                {
                    Ok(sid) => {
                        tracing::info!(
                            target: "pool::session",
                            "created session {sid} for channel {cid}"
                        );
                        agent.sessions.insert(*cid, sid.clone());
                        (sid, true)
                    }
                    Err(AcpError::AgentExited) => {
                        agent.sessions.clear();
                        agent.heartbeat_session = None;
                        let _ = result_tx.send(PromptResult {
                            agent,
                            source,
                            outcome: PromptOutcome::AgentExited,
                            batch: requeue_batch_if_queue(&ctx, batch),
                        });
                        return;
                    }
                    Err(e) => {
                        let _ = result_tx.send(PromptResult {
                            agent,
                            source,
                            outcome: PromptOutcome::Error(e),
                            batch: requeue_batch_if_queue(&ctx, batch),
                        });
                        return;
                    }
                }
            }
        }
        PromptSource::Heartbeat => {
            if let Some(sid) = &agent.heartbeat_session {
                (sid.clone(), false)
            } else {
                match agent
                    .acp
                    .session_new(&ctx.cwd, ctx.mcp_servers.clone())
                    .await
                {
                    Ok(sid) => {
                        tracing::info!(
                            target: "pool::session",
                            "created heartbeat session {sid} for agent {}",
                            agent.index
                        );
                        agent.heartbeat_session = Some(sid.clone());
                        (sid, true)
                    }
                    Err(AcpError::AgentExited) => {
                        agent.sessions.clear();
                        agent.heartbeat_session = None;
                        let _ = result_tx.send(PromptResult {
                            agent,
                            source,
                            outcome: PromptOutcome::AgentExited,
                            batch: None,
                        });
                        return;
                    }
                    Err(e) => {
                        let _ = result_tx.send(PromptResult {
                            agent,
                            source,
                            outcome: PromptOutcome::Error(e),
                            batch: None,
                        });
                        return;
                    }
                }
            }
        }
    };

    // ── Send initial_message on new channel sessions ──────────────────────

    if is_new_session {
        if let (PromptSource::Channel(cid), Some(ref initial_msg)) = (&source, &ctx.initial_message)
        {
            tracing::info!(
                target: "pool::session",
                "sending initial_message to session {session_id} for channel {cid}"
            );
            let init_result = timeout(
                ctx.turn_timeout,
                agent.acp.session_prompt(&session_id, initial_msg),
            )
            .await;

            match init_result {
                Ok(Ok(stop_reason)) => {
                    tracing::info!(
                        target: "pool::session",
                        "initial_message complete for channel {cid}: {stop_reason:?}"
                    );
                }
                Ok(Err(AcpError::AgentExited)) => {
                    agent.sessions.clear();
                    agent.heartbeat_session = None;
                    let _ = result_tx.send(PromptResult {
                        agent,
                        source,
                        outcome: PromptOutcome::AgentExited,
                        batch: requeue_batch_if_queue(&ctx, batch),
                    });
                    return;
                }
                Ok(Err(e)) => {
                    tracing::error!(
                        target: "pool::session",
                        "initial_message failed for channel {cid}: {e} — invalidating session"
                    );
                    agent.sessions.remove(cid);
                    let _ = result_tx.send(PromptResult {
                        agent,
                        source,
                        outcome: PromptOutcome::Error(e),
                        batch: requeue_batch_if_queue(&ctx, batch),
                    });
                    return;
                }
                Err(_elapsed) => {
                    tracing::warn!(
                        target: "pool::session",
                        "initial_message timed out for channel {cid} — cancelling"
                    );
                    match agent.acp.cancel_with_cleanup(&session_id).await {
                        Ok(_) => {
                            agent.sessions.remove(cid);
                        }
                        Err(AcpError::AgentExited) => {
                            agent.sessions.clear();
                            agent.heartbeat_session = None;
                            let _ = result_tx.send(PromptResult {
                                agent,
                                source,
                                outcome: PromptOutcome::AgentExited,
                                batch: requeue_batch_if_queue(&ctx, batch),
                            });
                            return;
                        }
                        Err(e) => {
                            tracing::error!(
                                target: "pool::session",
                                "cancel_with_cleanup failed during initial_message timeout: {e}"
                            );
                            agent.sessions.remove(cid);
                        }
                    }
                    let _ = result_tx.send(PromptResult {
                        agent,
                        source,
                        outcome: PromptOutcome::Timeout,
                        batch: requeue_batch_if_queue(&ctx, batch),
                    });
                    return;
                }
            }
        }
    }

    // ── Build prompt text (with optional context fetch) ──────────────────

    let prompt_text = if let Some(text) = prompt_text {
        // Pre-built prompt (heartbeat or legacy path).
        text
    } else if let Some(ref b) = batch {
        // Build prompt from batch with context enrichment.
        // Try startup cache first; lazy-fetch via REST for dynamic channels.
        let channel_info = match ctx.channel_info.get(&b.channel_id) {
            Some(ci) => Some(PromptChannelInfo {
                name: ci.name.clone(),
                channel_type: ci.channel_type.clone(),
            }),
            None => fetch_channel_info(b.channel_id, &ctx.rest_client).await,
        };

        let conversation_context = if ctx.context_message_limit > 0 {
            fetch_conversation_context(b, &channel_info, &ctx).await
        } else {
            None
        };

        crate::queue::format_prompt(
            b,
            ctx.system_prompt.as_deref(),
            channel_info.as_ref(),
            conversation_context.as_ref(),
        )
    } else {
        // Should not happen — batch is None only for heartbeats which have prompt_text.
        // Return the agent to the pool to prevent a permanent slot leak.
        tracing::error!("run_prompt_task: no batch and no prompt_text — returning agent");
        let _ = result_tx.send(PromptResult {
            agent,
            source,
            outcome: PromptOutcome::Error(AcpError::Protocol("no batch and no prompt_text".into())),
            batch: None,
        });
        return;
    };

    // ── Send the actual prompt ────────────────────────────────────────────

    let prompt_result = timeout(
        ctx.turn_timeout,
        agent.acp.session_prompt(&session_id, &prompt_text),
    )
    .await;

    match prompt_result {
        Ok(Ok(stop_reason)) => {
            log_stop_reason(&source, &stop_reason);
            let _ = result_tx.send(PromptResult {
                agent,
                source,
                outcome: PromptOutcome::Ok(stop_reason),
                batch: None,
            });
        }
        Ok(Err(AcpError::AgentExited)) => {
            tracing::error!(target: "pool::prompt", "agent {} exited during prompt", agent.index);
            agent.sessions.clear();
            agent.heartbeat_session = None;
            let _ = result_tx.send(PromptResult {
                agent,
                source,
                outcome: PromptOutcome::AgentExited,
                batch: requeue_batch_if_queue(&ctx, batch),
            });
        }
        Ok(Err(e)) => {
            tracing::error!(target: "pool::prompt", "session_prompt error: {e}");
            // Invalidate only the affected session.
            match &source {
                PromptSource::Channel(cid) => {
                    agent.sessions.remove(cid);
                }
                PromptSource::Heartbeat => {
                    agent.heartbeat_session = None;
                }
            }
            let _ = result_tx.send(PromptResult {
                agent,
                source,
                outcome: PromptOutcome::Error(e),
                batch: requeue_batch_if_queue(&ctx, batch),
            });
        }
        Err(_elapsed) => {
            tracing::warn!(
                target: "pool::prompt",
                "turn timeout ({}s) — cancelling session {session_id}",
                ctx.turn_timeout.as_secs()
            );
            match agent.acp.cancel_with_cleanup(&session_id).await {
                Ok(stop_reason) => {
                    log_stop_reason(&source, &stop_reason);
                    // Session is still valid after a clean cancel.
                    let _ = result_tx.send(PromptResult {
                        agent,
                        source,
                        outcome: PromptOutcome::Timeout,
                        batch: requeue_batch_if_queue(&ctx, batch),
                    });
                }
                Err(AcpError::AgentExited) => {
                    tracing::error!(
                        target: "pool::prompt",
                        "agent {} exited during cancel_with_cleanup",
                        agent.index
                    );
                    agent.sessions.clear();
                    agent.heartbeat_session = None;
                    let _ = result_tx.send(PromptResult {
                        agent,
                        source,
                        outcome: PromptOutcome::AgentExited,
                        batch: requeue_batch_if_queue(&ctx, batch),
                    });
                }
                Err(e) => {
                    tracing::error!(
                        target: "pool::prompt",
                        "cancel_with_cleanup error: {e} — invalidating session"
                    );
                    match &source {
                        PromptSource::Channel(cid) => {
                            agent.sessions.remove(cid);
                        }
                        PromptSource::Heartbeat => {
                            agent.heartbeat_session = None;
                        }
                    }
                    let _ = result_tx.send(PromptResult {
                        agent,
                        source,
                        outcome: PromptOutcome::Timeout,
                        batch: requeue_batch_if_queue(&ctx, batch),
                    });
                }
            }
        }
    }
}

// ── Context fetching ──────────────────────────────────────────────────────────

/// Lazy-fetch channel metadata for a channel not in the startup discovery cache.
///
/// Handles channels added dynamically via membership notifications after startup.
/// Uses the same 500ms timeout as context fetches. Returns `None` on failure
/// (graceful degradation — prompt will lack channel name and DM detection).
async fn fetch_channel_info(channel_id: Uuid, rest: &RestClient) -> Option<PromptChannelInfo> {
    let path = format!("/api/channels/{}", channel_id);
    let result = timeout(CONTEXT_FETCH_TIMEOUT, rest.get_json(&path)).await;

    match result {
        Ok(Ok(json)) => {
            let name = json
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let channel_type = json
                .get("channel_type")
                .and_then(|v| v.as_str())
                .unwrap_or("stream")
                .to_string();
            Some(PromptChannelInfo { name, channel_type })
        }
        Ok(Err(e)) => {
            tracing::debug!(
                channel_id = %channel_id,
                "channel info fetch failed: {e} — using defaults"
            );
            None
        }
        Err(_) => {
            tracing::debug!(
                channel_id = %channel_id,
                "channel info fetch timed out — using defaults"
            );
            None
        }
    }
}

/// Fetch conversation context (thread or DM) for a batch before prompting.
///
/// Returns `None` if:
/// - The event is a plain channel message (not a thread reply, not a DM)
/// - The REST fetch fails or times out (graceful degradation)
/// - `context_message_limit` is 0
///
/// For batches with multiple events, thread context is fetched for the **last**
/// reply event only (most recent = most likely to need a response).
async fn fetch_conversation_context(
    batch: &FlushBatch,
    channel_info: &Option<PromptChannelInfo>,
    ctx: &PromptContext,
) -> Option<ConversationContext> {
    let limit = ctx.context_message_limit;
    let is_dm = channel_info
        .as_ref()
        .map(|ci| ci.channel_type == "dm")
        .unwrap_or(false);

    // Check thread tags on the last event first — this applies to both
    // channels and DMs. A DM reply needs thread context (not channel history)
    // because /api/channels/{id}/messages excludes thread replies.
    let last_event = batch.events.last()?;
    let tags = crate::queue::parse_thread_tags(&last_event.event);
    if let Some(root_id) = tags.root_event_id {
        return fetch_thread_context(batch.channel_id, &root_id, limit, &ctx.rest_client).await;
    }

    // DM non-reply: fetch recent conversation history.
    if is_dm {
        return fetch_dm_context(batch.channel_id, limit, &ctx.rest_client).await;
    }

    None
}

/// Fetch thread context via REST: `GET /api/channels/{id}/threads/{event_id}?limit=N`
async fn fetch_thread_context(
    channel_id: Uuid,
    root_event_id: &str,
    limit: u32,
    rest: &RestClient,
) -> Option<ConversationContext> {
    // Defense-in-depth: validate hex before interpolating into URL path.
    // Nostr event IDs are 32-byte SHA-256 hashes = 64 hex chars.
    if root_event_id.is_empty()
        || root_event_id.len() != 64
        || !root_event_id.chars().all(|c| c.is_ascii_hexdigit())
    {
        tracing::warn!(
            channel_id = %channel_id,
            "invalid root_event_id (expected 64 hex chars) — skipping thread context fetch"
        );
        return None;
    }

    let path = format!(
        "/api/channels/{}/threads/{}?limit={}",
        channel_id, root_event_id, limit
    );

    let result = timeout(CONTEXT_FETCH_TIMEOUT, rest.get_json(&path)).await;

    match result {
        Ok(Ok(json)) => parse_thread_response(json),
        Ok(Err(e)) => {
            tracing::warn!(
                channel_id = %channel_id,
                root = root_event_id,
                "thread context fetch failed: {e} — falling back to hints-only"
            );
            None
        }
        Err(_) => {
            tracing::warn!(
                channel_id = %channel_id,
                root = root_event_id,
                "thread context fetch timed out — falling back to hints-only"
            );
            None
        }
    }
}

/// Fetch DM context via REST: `GET /api/channels/{id}/messages?limit=N`
async fn fetch_dm_context(
    channel_id: Uuid,
    limit: u32,
    rest: &RestClient,
) -> Option<ConversationContext> {
    let path = format!("/api/channels/{}/messages?limit={}", channel_id, limit);

    let result = timeout(CONTEXT_FETCH_TIMEOUT, rest.get_json(&path)).await;

    match result {
        Ok(Ok(json)) => parse_dm_response(json, limit),
        Ok(Err(e)) => {
            tracing::warn!(
                channel_id = %channel_id,
                "DM context fetch failed: {e} — falling back to hints-only"
            );
            None
        }
        Err(_) => {
            tracing::warn!(
                channel_id = %channel_id,
                "DM context fetch timed out — falling back to hints-only"
            );
            None
        }
    }
}

/// Parse the thread REST response into a `ConversationContext::Thread`.
///
/// Expected shape: `{ "root": {...}, "replies": [...], "total_replies": N }`
fn parse_thread_response(json: serde_json::Value) -> Option<ConversationContext> {
    let mut messages = Vec::new();

    // Root message.
    if let Some(root) = json.get("root") {
        if let Some(msg) = json_to_context_message(root) {
            messages.push(msg);
        }
    }

    // Replies.
    if let Some(replies) = json.get("replies").and_then(|v| v.as_array()) {
        for reply in replies {
            if let Some(msg) = json_to_context_message(reply) {
                messages.push(msg);
            }
        }
    }

    let total_replies = json
        .get("total_replies")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    let total = total_replies + 1; // +1 for root
    let truncated = total > messages.len();

    if messages.is_empty() {
        return None;
    }

    Some(ConversationContext::Thread {
        messages,
        total,
        truncated,
    })
}

/// Parse the DM messages REST response into a `ConversationContext::Dm`.
///
/// Expected shape: `{ "messages": [...], "next_cursor": ... }`
/// Messages arrive newest-first from the API; we reverse to chronological order.
fn parse_dm_response(json: serde_json::Value, limit: u32) -> Option<ConversationContext> {
    let arr = json.get("messages").and_then(|v| v.as_array())?;

    let mut messages: Vec<ContextMessage> =
        arr.iter().filter_map(json_to_context_message).collect();

    // API returns newest-first; reverse to chronological for the prompt.
    messages.reverse();

    // The relay's next_cursor is always set when the page is non-empty (not
    // just when more pages exist), so we can't use it for truncation detection.
    // Instead, compare returned count against the requested limit.
    let truncated = messages.len() >= limit as usize;
    let total = if truncated {
        messages.len() + 1 // indicate there are more
    } else {
        messages.len()
    };

    if messages.is_empty() {
        return None;
    }

    Some(ConversationContext::Dm {
        messages,
        total,
        truncated,
    })
}

/// Extract a `ContextMessage` from a JSON message object.
///
/// Works with both thread reply objects and channel message objects.
fn json_to_context_message(obj: &serde_json::Value) -> Option<ContextMessage> {
    let content = obj.get("content").and_then(|v| v.as_str())?;
    let pubkey = obj
        .get("pubkey")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let timestamp = obj
        .get("created_at")
        .and_then(|v| {
            // Handle both string timestamps and integer timestamps.
            v.as_str().map(|s| s.to_string()).or_else(|| {
                v.as_i64().map(|ts| {
                    chrono::DateTime::from_timestamp(ts, 0)
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_else(|| ts.to_string())
                })
            })
        })
        .unwrap_or_else(|| "unknown".to_string());

    Some(ContextMessage {
        pubkey: pubkey.to_string(),
        timestamp,
        content: content.to_string(),
    })
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Return the batch for requeue only in Queue mode; drop it in Drop mode.
#[inline]
fn requeue_batch_if_queue(ctx: &PromptContext, batch: Option<FlushBatch>) -> Option<FlushBatch> {
    match ctx.dedup_mode {
        DedupMode::Queue => batch,
        DedupMode::Drop => None,
    }
}

/// Log a stop reason at the appropriate tracing level.
fn log_stop_reason(source: &PromptSource, stop_reason: &StopReason) {
    let label = match source {
        PromptSource::Channel(cid) => format!("channel {cid}"),
        PromptSource::Heartbeat => "heartbeat".to_string(),
    };
    match stop_reason {
        StopReason::EndTurn => {
            tracing::info!(target: "pool::prompt", "turn complete for {label}: end_turn");
        }
        StopReason::Cancelled => {
            tracing::warn!(target: "pool::prompt", "turn cancelled for {label}");
        }
        StopReason::MaxTokens => {
            tracing::warn!(target: "pool::prompt", "turn hit max_tokens for {label}");
        }
        StopReason::MaxTurnRequests => {
            tracing::warn!(target: "pool::prompt", "turn hit max_turn_requests for {label}");
        }
        StopReason::Refusal => {
            tracing::warn!(target: "pool::prompt", "turn refused for {label}");
        }
    }
}

// ─── Unit Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── parse_thread_response tests ──────────────────────────────────────────

    #[test]
    fn test_parse_thread_response_basic() {
        let json = json!({
            "root": {
                "event_id": "abc123",
                "pubkey": "pub1",
                "content": "root message",
                "created_at": 1710518400
            },
            "replies": [
                {
                    "event_id": "def456",
                    "pubkey": "pub2",
                    "content": "first reply",
                    "created_at": 1710518460
                }
            ],
            "total_replies": 1
        });

        let ctx = parse_thread_response(json).expect("should parse");
        match ctx {
            ConversationContext::Thread {
                messages,
                total,
                truncated,
            } => {
                assert_eq!(messages.len(), 2); // root + 1 reply
                assert_eq!(total, 2); // 1 reply + 1 root
                assert!(!truncated);
                assert_eq!(messages[0].content, "root message");
                assert_eq!(messages[1].content, "first reply");
            }
            _ => panic!("expected Thread context"),
        }
    }

    #[test]
    fn test_parse_thread_response_truncated() {
        let json = json!({
            "root": {
                "event_id": "abc",
                "pubkey": "pub1",
                "content": "root",
                "created_at": 1710518400
            },
            "replies": [
                {
                    "event_id": "def",
                    "pubkey": "pub2",
                    "content": "reply1",
                    "created_at": 1710518460
                }
            ],
            "total_replies": 10
        });

        let ctx = parse_thread_response(json).expect("should parse");
        match ctx {
            ConversationContext::Thread {
                messages,
                total,
                truncated,
            } => {
                assert_eq!(messages.len(), 2);
                assert_eq!(total, 11); // 10 replies + 1 root
                assert!(truncated);
            }
            _ => panic!("expected Thread context"),
        }
    }

    #[test]
    fn test_parse_thread_response_empty() {
        let json = json!({
            "root": null,
            "replies": [],
            "total_replies": 0
        });
        assert!(parse_thread_response(json).is_none());
    }

    #[test]
    fn test_parse_thread_response_missing_fields() {
        // Malformed JSON — no root, no replies key.
        let json = json!({ "something": "else" });
        assert!(parse_thread_response(json).is_none());
    }

    // ── parse_dm_response tests ──────────────────────────────────────────────

    #[test]
    fn test_parse_dm_response_basic() {
        let json = json!({
            "messages": [
                {
                    "event_id": "msg2",
                    "pubkey": "pub2",
                    "content": "newer message",
                    "created_at": 1710518500
                },
                {
                    "event_id": "msg1",
                    "pubkey": "pub1",
                    "content": "older message",
                    "created_at": 1710518400
                }
            ],
            "next_cursor": null
        });

        // limit=12 > 2 messages → not truncated.
        let ctx = parse_dm_response(json, 12).expect("should parse");
        match ctx {
            ConversationContext::Dm {
                messages,
                total,
                truncated,
            } => {
                // Should be reversed to chronological order.
                assert_eq!(messages.len(), 2);
                assert_eq!(messages[0].content, "older message");
                assert_eq!(messages[1].content, "newer message");
                assert!(!truncated);
                assert_eq!(total, 2);
            }
            _ => panic!("expected Dm context"),
        }
    }

    #[test]
    fn test_parse_dm_response_truncated() {
        let json = json!({
            "messages": [
                {
                    "event_id": "msg1",
                    "pubkey": "pub1",
                    "content": "message",
                    "created_at": 1710518400
                }
            ],
            "next_cursor": "00000000660f5a80"
        });

        // limit=1 == 1 message → truncated.
        let ctx = parse_dm_response(json, 1).expect("should parse");
        match ctx {
            ConversationContext::Dm {
                truncated, total, ..
            } => {
                assert!(truncated);
                assert_eq!(total, 2); // 1 message + indicator
            }
            _ => panic!("expected Dm context"),
        }
    }

    #[test]
    fn test_parse_dm_response_not_truncated_despite_cursor() {
        // Relay always sets next_cursor when page is non-empty, but if
        // returned count < limit, the page is complete.
        let json = json!({
            "messages": [
                {
                    "event_id": "msg1",
                    "pubkey": "pub1",
                    "content": "only message",
                    "created_at": 1710518400
                }
            ],
            "next_cursor": "00000000660f5a80"
        });

        // limit=12 > 1 message → NOT truncated despite next_cursor being set.
        let ctx = parse_dm_response(json, 12).expect("should parse");
        match ctx {
            ConversationContext::Dm {
                truncated, total, ..
            } => {
                assert!(!truncated, "should not be truncated when count < limit");
                assert_eq!(total, 1);
            }
            _ => panic!("expected Dm context"),
        }
    }

    #[test]
    fn test_parse_dm_response_empty() {
        let json = json!({
            "messages": [],
            "next_cursor": null
        });
        assert!(parse_dm_response(json, 12).is_none());
    }

    #[test]
    fn test_parse_dm_response_missing_messages_key() {
        let json = json!({ "data": [] });
        assert!(parse_dm_response(json, 12).is_none());
    }

    // ── json_to_context_message tests ────────────────────────────────────────

    #[test]
    fn test_json_to_context_message_integer_timestamp() {
        let obj = json!({
            "pubkey": "abc",
            "content": "hello",
            "created_at": 1710518400
        });
        let msg = json_to_context_message(&obj).expect("should parse");
        assert_eq!(msg.pubkey, "abc");
        assert_eq!(msg.content, "hello");
        assert!(msg.timestamp.contains("2024")); // 1710518400 = 2024-03-15
    }

    #[test]
    fn test_json_to_context_message_string_timestamp() {
        let obj = json!({
            "pubkey": "abc",
            "content": "hello",
            "created_at": "2026-03-15T16:30:00+00:00"
        });
        let msg = json_to_context_message(&obj).expect("should parse");
        assert_eq!(msg.timestamp, "2026-03-15T16:30:00+00:00");
    }

    #[test]
    fn test_json_to_context_message_missing_content() {
        let obj = json!({ "pubkey": "abc" });
        assert!(json_to_context_message(&obj).is_none());
    }

    #[test]
    fn test_json_to_context_message_missing_pubkey_uses_default() {
        let obj = json!({ "content": "hello" });
        let msg = json_to_context_message(&obj).expect("should parse");
        assert_eq!(msg.pubkey, "unknown");
    }
}
