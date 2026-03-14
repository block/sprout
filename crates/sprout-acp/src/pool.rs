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
//!     next_result() → PromptResult
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
use crate::queue::FlushBatch;

// ── FlushBatch Clone note ─────────────────────────────────────────────────────
// FlushBatch and BatchEvent derive Clone (added in queue.rs) so we can store
// a recoverable copy in TaskMeta for panic recovery in Queue mode.

// ── Types ─────────────────────────────────────────────────────────────────────

/// Metadata stored per in-flight task for panic recovery.
struct TaskMeta {
    agent_index: usize,
    channel_id: Option<Uuid>,
    /// Clone of batch for Queue mode panic recovery.
    recoverable_batch: Option<FlushBatch>,
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
            let idx = self
                .agents
                .iter()
                .position(|slot| {
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
        self.agents[agent.index] = Some(agent);
    }

    /// Whether any agent is currently idle (sitting in its slot).
    pub fn any_idle(&self) -> bool {
        self.agents.iter().any(|slot| slot.is_some())
    }

    /// Count of agents that are alive: idle OR checked out (have a task_map entry).
    ///
    /// Used to detect when all agents have exited so the caller can respawn.
    pub fn live_count(&self) -> usize {
        let idle = self.agents.iter().filter(|s| s.is_some()).count();
        let checked_out = self.task_map.len();
        idle + checked_out
    }

    /// Wait for the next completed prompt result.
    ///
    /// Panics if the channel closes (impossible while pool is alive).
    pub async fn next_result(&mut self) -> PromptResult {
        self.result_rx
            .recv()
            .await
            .expect("result channel closed — pool invariant violated")
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

    pub fn agents_mut(&mut self) -> &mut Vec<Option<OwnedAgent>> {
        &mut self.agents
    }
}

// ── run_prompt_task ───────────────────────────────────────────────────────────

/// Core async function spawned for each prompt.
///
/// Lifecycle:
/// 1. Resolve or create a session (channel or heartbeat).
/// 2. Send `initial_message` on new channel sessions (if configured).
/// 3. Send the actual prompt with turn timeout.
/// 4. Handle all error paths, always returning the agent via `result_tx`.
///
/// The agent is ALWAYS returned — even on panic the `JoinSet` detects the
/// abort and the caller uses `task_map` to recover the agent index.
pub async fn run_prompt_task(
    mut agent: OwnedAgent,
    batch: Option<FlushBatch>,
    prompt_text: String,
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
                match agent.acp.session_new(&ctx.cwd, ctx.mcp_servers.clone()).await {
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
                match agent.acp.session_new(&ctx.cwd, ctx.mcp_servers.clone()).await {
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
        if let (PromptSource::Channel(cid), Some(ref initial_msg)) =
            (&source, &ctx.initial_message)
        {
            tracing::info!(
                target: "pool::session",
                "sending initial_message to session {session_id} for channel {cid}"
            );
            let init_result =
                timeout(ctx.turn_timeout, agent.acp.session_prompt(&session_id, initial_msg))
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

    // ── Send the actual prompt ────────────────────────────────────────────

    let prompt_result =
        timeout(ctx.turn_timeout, agent.acp.session_prompt(&session_id, &prompt_text)).await;

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
