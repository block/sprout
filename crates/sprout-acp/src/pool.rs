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

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio::time::timeout;
use uuid::Uuid;

use crate::acp::{
    extract_model_config_options, extract_model_state, resolve_model_switch_method, AcpClient,
    AcpError, McpServer, ModelSwitchMethod, StopReason,
};
use crate::config::{DedupMode, PermissionMode};
use crate::queue::{
    ContextMessage, ConversationContext, FlushBatch, PromptChannelInfo, PromptProfile,
    PromptProfileLookup,
};
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
    /// Cancel signal for the in-flight prompt task.
    /// `None` for heartbeat tasks (not cancellable) and after signal is consumed.
    pub cancel_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

/// Agent-level model capabilities. Populated on first session creation.
/// The catalog is the same across all sessions for a given agent process.
/// Fields are read by the desktop's `get_agent_models` Tauri command (Phase 3).
#[allow(dead_code)] // Scaffolding for desktop integration — fields read via serde.
pub struct AgentModelCapabilities {
    /// Stable: configOptions with category "model" from session/new.
    pub config_options_raw: Vec<serde_json::Value>,
    /// Unstable: SessionModelState from session/new.
    pub available_models_raw: Option<serde_json::Value>,
}

/// Per-channel session IDs and turn counters.
///
/// Separated from `OwnedAgent` so the state machine is testable without
/// spawning a real agent subprocess.
#[derive(Default)]
pub struct SessionState {
    /// channel_id → session_id
    pub sessions: HashMap<Uuid, String>,
    pub heartbeat_session: Option<String>,
    /// Per-channel turn counters for proactive session rotation.
    /// Incremented on each successful prompt; reset when the session is rotated.
    pub turn_counts: HashMap<Uuid, u32>,
    /// Turn counter for the heartbeat session.
    pub heartbeat_turn_count: u32,
}

impl SessionState {
    /// Invalidate the session (and turn counter) for a specific prompt source.
    pub fn invalidate(&mut self, source: &PromptSource) {
        match source {
            PromptSource::Channel(cid) => {
                self.invalidate_channel(cid);
            }
            PromptSource::Heartbeat => {
                self.heartbeat_session = None;
                self.heartbeat_turn_count = 0;
            }
        }
    }

    /// Invalidate a single channel's session and turn counter.
    /// Returns `true` if the channel had an active session.
    pub fn invalidate_channel(&mut self, channel_id: &Uuid) -> bool {
        self.turn_counts.remove(channel_id);
        self.sessions.remove(channel_id).is_some()
    }

    /// Invalidate all sessions and turn counters (e.g. after agent exit).
    pub fn invalidate_all(&mut self) {
        self.sessions.clear();
        self.turn_counts.clear();
        self.heartbeat_session = None;
        self.heartbeat_turn_count = 0;
    }
}

/// An agent with its session state, owned by the pool or a running task.
pub struct OwnedAgent {
    pub index: usize,
    pub acp: AcpClient,
    pub state: SessionState,
    /// Model catalog from first session/new. None until first session created.
    pub model_capabilities: Option<AgentModelCapabilities>,
    /// Desired model ID (from `Config.model`). Applied after every `session_new_full()`.
    pub desired_model: Option<String>,
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
#[derive(Debug)]
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
    /// Intentional cancel via `!cancel` command or interrupt mode.
    /// Agent is healthy — no respawn, no retry penalty.
    Cancelled,
}

/// Immutable config subset shared (via `Arc`) by all spawned prompt tasks.
///
/// Built once from `Config` at startup. Avoids cloning the full config
/// into every task.
pub struct PromptContext {
    pub mcp_servers: Vec<McpServer>,
    pub initial_message: Option<String>,
    pub idle_timeout: Duration,
    pub max_turn_duration: Duration,
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
    /// Max turns per session before proactive rotation. 0 = disabled.
    pub max_turns_per_session: u32,
    /// Permission mode to apply after session creation. `Default` = skip.
    pub permission_mode: PermissionMode,
}

// ── AgentPool impl ────────────────────────────────────────────────────────────

impl AgentPool {
    /// Create a new pool from a list of initialized agents.
    ///
    /// Agents are placed into indexed slots. The unbounded channel is created
    /// here; tasks send results back through `result_tx`.
    ///
    /// Prefer [`AgentPool::from_slots`] for startup paths where some agents may
    /// have failed — `new()` packs agents densely and will break the
    /// `agent.index` invariant if any slot was skipped.
    #[allow(dead_code)]
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

    /// Create a pool from pre-indexed slots (may contain None for failed startups).
    ///
    /// Slot positions are preserved so that `agent.index` always matches the
    /// index into `self.agents`. Use this instead of `new()` when the startup
    /// loop skips failed agents — `new()` would pack agents densely and break
    /// the index invariant.
    pub fn from_slots(slots: Vec<Option<OwnedAgent>>) -> Self {
        let (result_tx, result_rx) = mpsc::unbounded_channel();
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
                    .map(|a| a.state.sessions.contains_key(&cid))
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
        if self.agents[idx].is_some() {
            // This is a bug: two tasks returned the same agent index. Log it
            // loudly so it shows up in production logs, then overwrite — the
            // alternative (dropping the incoming agent) would permanently leak
            // the slot.
            tracing::error!(
                idx,
                "BUG: return_agent called for slot {idx} which is already occupied — overwriting"
            );
        }
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
                .map(|a| a.state.sessions.contains_key(&channel_id))
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

    /// Non-blocking drain of the result channel. Used during shutdown to
    /// collect agents that completed while join_set was being drained.
    pub fn result_rx_try_recv(&mut self) -> Result<PromptResult, mpsc::error::TryRecvError> {
        self.result_rx.try_recv()
    }

    /// Check whether a slot is alive: either idle in the pool or checked out
    /// for an in-flight task. Returns `false` only when the slot is truly
    /// empty and available for refill.
    pub fn slot_alive(&self, index: usize) -> bool {
        let idle = self.agents.get(index).is_some_and(|s| s.is_some());
        if idle {
            return true;
        }
        // Check if the agent is checked out (in-flight on a task).
        self.task_map.values().any(|m| m.agent_index == index)
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
                if agent.state.invalidate_channel(&channel_id) {
                    count += 1;
                }
            }
        }
        count
    }
}

// ── run_prompt_task ───────────────────────────────────────────────────────────

/// Timeout for a single pre-prompt context fetch attempt (thread/DM history).
/// Each call gets this budget; with one retry the total worst-case is
/// 2 × CONTEXT_FETCH_TIMEOUT + CONTEXT_FETCH_RETRY_DELAY ≈ 6.5 s.
const CONTEXT_FETCH_TIMEOUT: Duration = Duration::from_millis(3_000);

/// Delay between the first failed context fetch and the single retry.
const CONTEXT_FETCH_RETRY_DELAY: Duration = Duration::from_millis(500);

/// Timeout for model-switch requests (`session/set_config_option`, `session/set_model`).
const MODEL_SWITCH_TIMEOUT: Duration = Duration::from_secs(5);

/// Timeout for permission-mode requests (`session/set_config_option` with `configId: "mode"`).
const PERMISSION_MODE_TIMEOUT: Duration = Duration::from_secs(5);

/// Create a new ACP session via `session_new_full()`, populate model capabilities
/// on the agent (first session only), and apply `desired_model` if set.
///
/// On error from `session_new_full()`, returns the `AcpError` — caller handles
/// error reporting. Model-switch failures are logged and gracefully ignored
/// (the agent proceeds with its default model).
async fn create_session_and_apply_model(
    agent: &mut OwnedAgent,
    ctx: &PromptContext,
) -> Result<String, AcpError> {
    let resp = agent
        .acp
        .session_new_full(&ctx.cwd, ctx.mcp_servers.clone())
        .await?;

    // Populate model capabilities on first session creation.
    if agent.model_capabilities.is_none() {
        agent.model_capabilities = Some(AgentModelCapabilities {
            config_options_raw: extract_model_config_options(&resp.raw),
            available_models_raw: extract_model_state(&resp.raw),
        });
    }

    // Apply desired_model if set, matching against the fresh session/new response.
    if let Some(ref desired) = agent.desired_model {
        match resolve_model_switch_method(&resp.raw, desired) {
            Some(method) => {
                apply_model_switch(&mut agent.acp, &resp.session_id, desired, &method).await?;
            }
            None => {
                tracing::warn!(
                    target: "pool::model",
                    "desired model {desired} not found in agent's available models — proceeding with agent default"
                );
            }
        }
    }

    // Apply permission mode if not the agent's built-in default AND the agent
    // advertises a compatible mode in session/new. Agents that don't support
    // the mode (e.g., goose crashes on unrecognized set_config_option values)
    // are safely skipped — the harness auto-approves via handle_permission_request.
    if !ctx.permission_mode.is_default() {
        if let Some(mode_id) = resolve_mode_id(&resp.raw, &ctx.permission_mode) {
            apply_permission_mode(&mut agent.acp, &resp.session_id, &mode_id).await?;
        }
    }

    Ok(resp.session_id)
}

/// Send the appropriate ACP model-switch request with a timeout.
///
/// On timeout or error, logs a warning and returns — the caller proceeds
/// with the agent's default model. This is intentionally non-fatal: a stale
/// response from a timed-out request is safely ignored by `read_until_response`
/// (non-matching JSON-RPC IDs are skipped).
async fn apply_model_switch(
    acp: &mut AcpClient,
    session_id: &str,
    desired: &str,
    method: &ModelSwitchMethod,
) -> Result<(), AcpError> {
    let method_label = match method {
        ModelSwitchMethod::ConfigOption { config_id, .. } => {
            format!("configOption (configId={config_id})")
        }
        ModelSwitchMethod::SetModel { .. } => "set_model".to_string(),
    };

    let result = tokio::time::timeout(MODEL_SWITCH_TIMEOUT, async {
        match method {
            ModelSwitchMethod::ConfigOption {
                config_id,
                option_value,
            } => {
                acp.session_set_config_option(session_id, config_id, option_value)
                    .await
            }
            ModelSwitchMethod::SetModel { model_id } => {
                acp.session_set_model(session_id, model_id).await
            }
        }
    })
    .await;

    match result {
        Ok(Ok(_)) => {
            tracing::info!(
                target: "pool::model",
                "applied model {desired} via {method_label} on session {session_id}"
            );
        }
        // Transport-class errors may have corrupted the stdio stream — propagate
        // so the caller can respawn the agent instead of reusing a poisoned one.
        Ok(Err(e @ AcpError::Io(_)))
        | Ok(Err(e @ AcpError::WriteTimeout(_)))
        | Ok(Err(e @ AcpError::Timeout(_)))
        | Ok(Err(e @ AcpError::Protocol(_)))
        | Ok(Err(e @ AcpError::AgentExited)) => {
            tracing::error!(
                target: "pool::model",
                "fatal error setting model {desired} via {method_label}: {e}"
            );
            return Err(e);
        }
        // Application-level errors (Json, etc.) — agent is fine, just uses default model.
        Ok(Err(e)) => {
            tracing::warn!(
                target: "pool::model",
                "failed to set model {desired} via {method_label}: {e} — proceeding with agent default"
            );
        }
        Err(_) => {
            // Outer timeout fired — the inner send_request may have left the
            // stream in an unknown state. Treat as transport error.
            tracing::error!(
                target: "pool::model",
                "model set via {method_label} timed out ({MODEL_SWITCH_TIMEOUT:?}) — treating as fatal"
            );
            return Err(AcpError::Timeout(MODEL_SWITCH_TIMEOUT));
        }
    }
    Ok(())
}

/// Set the session permission mode via `session/set_config_option`.
///
/// Non-fatal for most errors: logs and proceeds. The agent falls back
/// to its default permission mode (`"default"`), which still works via
/// Find the agent's advertised mode ID that matches the requested permission
/// mode.  Tries the canonical ACP wire string first (e.g. `"bypassPermissions"`),
/// then falls back to known aliases used by other ACP adapters (e.g. amp-acp
/// advertises `"bypass"` instead of `"bypassPermissions"`).
///
/// Returns `None` if the agent doesn't advertise a compatible mode.
fn resolve_mode_id(
    session_new_result: &serde_json::Value,
    mode: &PermissionMode,
) -> Option<String> {
    let available = session_new_result
        .get("modes")?
        .get("availableModes")?
        .as_array()?;

    let ids: Vec<&str> = available
        .iter()
        .filter_map(|m| m.get("id")?.as_str())
        .collect();

    // Exact match on the canonical wire string.
    let wire = mode.as_wire_str();
    if ids.contains(&wire) {
        return Some(wire.to_string());
    }

    // Fallback aliases for known variations across ACP adapters.
    let aliases: &[&str] = match mode {
        PermissionMode::BypassPermissions => &["bypass"],
        PermissionMode::AcceptEdits => &["accept-edits", "accept_edits"],
        PermissionMode::DontAsk => &["dont-ask", "dont_ask"],
        _ => &[],
    };

    aliases
        .iter()
        .find(|a| ids.contains(a))
        .map(|a| a.to_string())
}

/// per-tool auto-approval in `handle_permission_request`.
///
/// **Fatal exception:** if the agent process exits (e.g., goose crashes on
/// unrecognized methods), returns `Err(AgentExited)` so the caller can respawn.
async fn apply_permission_mode(
    acp: &mut AcpClient,
    session_id: &str,
    mode_id: &str,
) -> Result<(), AcpError> {
    let result = tokio::time::timeout(PERMISSION_MODE_TIMEOUT, async {
        acp.session_set_config_option(session_id, "mode", mode_id)
            .await
    })
    .await;

    match result {
        Ok(Ok(_)) => {
            tracing::info!(
                target: "pool::permission",
                "applied permission mode {mode_id:?} on session {session_id}"
            );
        }
        // Transport-class errors may have corrupted the stdio stream — propagate
        // so the caller can respawn the agent.
        Ok(Err(e @ AcpError::Io(_)))
        | Ok(Err(e @ AcpError::WriteTimeout(_)))
        | Ok(Err(e @ AcpError::Timeout(_)))
        | Ok(Err(e @ AcpError::AgentExited)) => {
            tracing::error!(
                target: "pool::permission",
                "fatal error setting permission mode {mode_id:?}: {e}"
            );
            return Err(e);
        }
        // Application-level errors (including JSON-RPC error responses like
        // "method not found" which surface as AcpError::Protocol) — agent is
        // fine, just doesn't support this config method. Fall back to
        // per-tool auto-approval.
        Ok(Err(e)) => {
            tracing::warn!(
                target: "pool::permission",
                "failed to set permission mode {mode_id:?}: {e} — falling back to per-tool auto-approval"
            );
        }
        Err(_) => {
            // Outer timeout fired — stream may be in unknown state.
            tracing::error!(
                target: "pool::permission",
                "permission mode set timed out ({PERMISSION_MODE_TIMEOUT:?}) — treating as fatal"
            );
            return Err(AcpError::Timeout(PERMISSION_MODE_TIMEOUT));
        }
    }
    Ok(())
}

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
    cancel_rx: Option<tokio::sync::oneshot::Receiver<()>>,
) {
    // ── Determine source and resolve/create session ───────────────────────

    // Is this a channel prompt or a heartbeat?
    let source = match &batch {
        Some(b) => PromptSource::Channel(b.channel_id),
        None => PromptSource::Heartbeat,
    };

    // ── Reaction cleanup guard ────────────────────────────────────────────
    // Collects event IDs up front. On drop (any exit path — normal, early
    // return, or panic), spawns best-effort cleanup of both 👀 and 💬.
    // See `ReactionGuard` docs for ordering guarantees and known edge cases.
    let reaction_ids: Vec<String> = batch
        .as_ref()
        .map(|b| b.events.iter().map(|be| be.event.id.to_hex()).collect())
        .unwrap_or_default();
    let _reaction_guard = ReactionGuard::new(ctx.rest_client.clone(), reaction_ids.clone());

    let (session_id, is_new_session) = match &source {
        PromptSource::Channel(cid) => {
            if let Some(sid) = agent.state.sessions.get(cid) {
                (sid.clone(), false)
            } else {
                // Create new session with model application.
                match create_session_and_apply_model(&mut agent, &ctx).await {
                    Ok(sid) => {
                        tracing::info!(
                            target: "pool::session",
                            "created session {sid} for channel {cid}"
                        );
                        agent.state.sessions.insert(*cid, sid.clone());
                        (sid, true)
                    }
                    Err(AcpError::AgentExited) => {
                        agent.state.invalidate_all();
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
            if let Some(sid) = &agent.state.heartbeat_session {
                (sid.clone(), false)
            } else {
                match create_session_and_apply_model(&mut agent, &ctx).await {
                    Ok(sid) => {
                        tracing::info!(
                            target: "pool::session",
                            "created heartbeat session {sid} for agent {}",
                            agent.index
                        );
                        agent.state.heartbeat_session = Some(sid.clone());
                        (sid, true)
                    }
                    Err(AcpError::AgentExited) => {
                        agent.state.invalidate_all();
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
            let init_result = agent
                .acp
                .session_prompt_with_idle_timeout(
                    &session_id,
                    initial_msg,
                    ctx.idle_timeout,
                    ctx.max_turn_duration,
                )
                .await;

            match init_result {
                Ok(stop_reason) => {
                    tracing::info!(
                        target: "pool::session",
                        "initial_message complete for channel {cid}: {stop_reason:?}"
                    );
                }
                Err(AcpError::AgentExited) => {
                    agent.state.invalidate_all();
                    let _ = result_tx.send(PromptResult {
                        agent,
                        source,
                        outcome: PromptOutcome::AgentExited,
                        batch: requeue_batch_if_queue(&ctx, batch),
                    });
                    return;
                }
                Err(AcpError::IdleTimeout(_)) => {
                    tracing::warn!(
                        target: "pool::session",
                        "initial_message idle timeout ({}s) for channel {cid} — cancelling",
                        ctx.idle_timeout.as_secs()
                    );
                    match agent
                        .acp
                        .cancel_with_cleanup(&session_id, ctx.idle_timeout)
                        .await
                    {
                        Ok(_) => {
                            agent.state.invalidate(&source);
                        }
                        Err(AcpError::AgentExited) => {
                            agent.state.invalidate_all();
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
                            agent.state.invalidate(&source);
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
                Err(AcpError::HardTimeout) => {
                    tracing::error!(
                        target: "pool::session",
                        "hard timeout ({}s cap) during initial_message for channel {cid} — agent process is unrecoverable",
                        ctx.max_turn_duration.as_secs()
                    );
                    agent.state.invalidate_all();
                    let _ = result_tx.send(PromptResult {
                        agent,
                        source,
                        outcome: PromptOutcome::Timeout,
                        batch: requeue_batch_if_queue(&ctx, batch),
                    });
                    return;
                }
                Err(e) => {
                    tracing::error!(
                        target: "pool::session",
                        "initial_message failed for channel {cid}: {e} — invalidating session"
                    );
                    agent.state.invalidate(&source);
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

        let profile_lookup =
            fetch_prompt_profile_lookup(b, conversation_context.as_ref(), &ctx.rest_client).await;

        crate::queue::format_prompt(
            b,
            ctx.system_prompt.as_deref(),
            channel_info.as_ref(),
            conversation_context.as_ref(),
            profile_lookup.as_ref(),
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

    // 💬 — fire-and-forget so the prompt fires immediately.
    // The guard's cleanup (spawned on drop) removes 💬 after the turn completes.
    // A brief race where 💬 appears slightly after the agent starts is acceptable.
    if !reaction_ids.is_empty() {
        let rest = ctx.rest_client.clone();
        let ids = reaction_ids.clone();
        tokio::spawn(async move {
            react_working(&rest, &ids).await;
        });
    }

    // ── Send the actual prompt ────────────────────────────────────────────

    // ── Cancel-aware prompt dispatch ──────────────────────────────────────
    // When cancel_rx is Some (channel tasks), wrap the prompt in select! so
    // the main loop can interrupt it. Heartbeats (cancel_rx=None) take the
    // simple await path — they are not cancellable.
    let prompt_result = match cancel_rx {
        None => {
            // Heartbeat / non-cancellable path.
            agent
                .acp
                .session_prompt_with_idle_timeout(
                    &session_id,
                    &prompt_text,
                    ctx.idle_timeout,
                    ctx.max_turn_duration,
                )
                .await
        }
        Some(rx) => {
            tokio::select! {
                biased;
                result = agent.acp.session_prompt_with_idle_timeout(
                    &session_id,
                    &prompt_text,
                    ctx.idle_timeout,
                    ctx.max_turn_duration,
                ) => result,
                _ = rx => {
                    // Cancel signal received. Guard against Race 1: the turn may
                    // have completed naturally just as cancel fired.
                    if agent.acp.has_in_flight_prompt() {
                        // Prompt is genuinely in-flight — cancel it.
                        match agent.acp.cancel_with_cleanup(&session_id, ctx.idle_timeout).await {
                            Ok(stop_reason) => {
                                log_stop_reason(&source, &stop_reason);
                                agent.state.invalidate(&source);
                                let _ = result_tx.send(PromptResult {
                                    agent,
                                    source,
                                    outcome: PromptOutcome::Cancelled,
                                    batch: requeue_batch_if_queue(&ctx, batch),
                                });
                                return;
                            }
                            Err(AcpError::AgentExited) => {
                                agent.state.invalidate_all();
                                let _ = result_tx.send(PromptResult {
                                    agent,
                                    source,
                                    outcome: PromptOutcome::AgentExited,
                                    batch: requeue_batch_if_queue(&ctx, batch),
                                });
                                return;
                            }
                            Err(AcpError::IdleTimeout(_) | AcpError::HardTimeout) => {
                                // Cancel drain timed out — agent state uncertain.
                                agent.state.invalidate(&source);
                                let _ = result_tx.send(PromptResult {
                                    agent,
                                    source,
                                    outcome: PromptOutcome::Timeout,
                                    batch: requeue_batch_if_queue(&ctx, batch),
                                });
                                return;
                            }
                            Err(e) => {
                                agent.state.invalidate(&source);
                                let _ = result_tx.send(PromptResult {
                                    agent,
                                    source,
                                    outcome: PromptOutcome::Error(e),
                                    batch: requeue_batch_if_queue(&ctx, batch),
                                });
                                return;
                            }
                        }
                    } else {
                        // Race 1 resolution: turn completed naturally before cancel
                        // could fire. last_prompt_id is None — cleared by
                        // session_prompt_with_idle_timeout() on success. The prompt
                        // future was dropped by select! — its Ok result is gone.
                        //
                        // Note: this `else` branch (last_prompt_id is None) cannot
                        // fire during the pre-prompt phase because `biased` select!
                        // polls the prompt arm first. That arm sets last_prompt_id
                        // synchronously before its first yield point, so by the time
                        // the cancel arm can win, last_prompt_id is already Some.
                        // This branch only fires when the turn genuinely completed
                        // and last_prompt_id was cleared by the success path.
                        //
                        // MUST send a PromptResult or the main loop deadlocks.
                        tracing::debug!(
                            target: "pool::prompt",
                            "cancel signal arrived but turn already completed — treating as success"
                        );
                        let _ = result_tx.send(PromptResult {
                            agent,
                            source,
                            outcome: PromptOutcome::Ok(StopReason::EndTurn),
                            batch: None, // turn succeeded — batch was processed, no requeue
                        });
                        return;
                    }
                }
            }
        }
    };

    match prompt_result {
        Ok(stop_reason) => {
            log_stop_reason(&source, &stop_reason);

            // ── Session rotation on context exhaustion ────────────────
            let should_rotate = matches!(
                stop_reason,
                StopReason::MaxTokens | StopReason::MaxTurnRequests
            );

            // ── Proactive turn-based rotation ─────────────────────────
            let should_rotate = should_rotate || {
                let limit = ctx.max_turns_per_session;
                if limit > 0 {
                    match &source {
                        PromptSource::Channel(cid) => {
                            let count = agent.state.turn_counts.entry(*cid).or_insert(0);
                            *count += 1;
                            *count >= limit
                        }
                        PromptSource::Heartbeat => {
                            agent.state.heartbeat_turn_count += 1;
                            agent.state.heartbeat_turn_count >= limit
                        }
                    }
                } else {
                    false
                }
            };

            if should_rotate {
                tracing::info!(
                    target: "pool::session",
                    "rotating session for {source:?} after {stop_reason:?}",
                );
                agent.state.invalidate(&source);
            }

            let _ = result_tx.send(PromptResult {
                agent,
                source,
                outcome: PromptOutcome::Ok(stop_reason),
                batch: None,
            });
        }
        Err(AcpError::AgentExited) => {
            tracing::error!(target: "pool::prompt", "agent {} exited during prompt", agent.index);
            agent.state.invalidate_all();
            let _ = result_tx.send(PromptResult {
                agent,
                source,
                outcome: PromptOutcome::AgentExited,
                batch: requeue_batch_if_queue(&ctx, batch),
            });
        }
        Err(AcpError::IdleTimeout(_)) => {
            tracing::warn!(
                target: "pool::prompt",
                "idle timeout ({}s) — cancelling session {session_id}",
                ctx.idle_timeout.as_secs()
            );
            match agent
                .acp
                .cancel_with_cleanup(&session_id, ctx.idle_timeout)
                .await
            {
                Ok(stop_reason) => {
                    log_stop_reason(&source, &stop_reason);
                    // Timeout triggers respawn in handle_prompt_result —
                    // session state will be discarded with the old agent.
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
                    agent.state.invalidate_all();
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
                    agent.state.invalidate(&source);
                    let _ = result_tx.send(PromptResult {
                        agent,
                        source,
                        outcome: PromptOutcome::Timeout,
                        batch: requeue_batch_if_queue(&ctx, batch),
                    });
                }
            }
        }
        Err(AcpError::HardTimeout) => {
            tracing::error!(
                target: "pool::prompt",
                "hard timeout ({}s cap) — agent process is unrecoverable, invalidating all sessions",
                ctx.max_turn_duration.as_secs()
            );
            agent.state.invalidate_all();
            let _ = result_tx.send(PromptResult {
                agent,
                source,
                outcome: PromptOutcome::Timeout,
                batch: requeue_batch_if_queue(&ctx, batch),
            });
        }
        Err(e) => {
            tracing::error!(target: "pool::prompt", "session_prompt error: {e}");
            // Invalidate only the affected session.
            agent.state.invalidate(&source);
            let _ = result_tx.send(PromptResult {
                agent,
                source,
                outcome: PromptOutcome::Error(e),
                batch: requeue_batch_if_queue(&ctx, batch),
            });
        }
    }
    // _reaction_guard drops here → spawns clear_reactions for all exit paths.
}

// ── Context fetching ──────────────────────────────────────────────────────────

/// Retry wrapper for context fetches: one retry with `CONTEXT_FETCH_RETRY_DELAY`
/// on any `None` result. The closure is called twice at most.
///
/// Using a closure (not a `Future`) so the retry can construct a fresh `Future`
/// each attempt without requiring `Clone` or re-boxing.
async fn fetch_with_retry<F, Fut, T>(f: F) -> Option<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Option<T>>,
{
    if let Some(result) = f().await {
        return Some(result);
    }
    tokio::time::sleep(CONTEXT_FETCH_RETRY_DELAY).await;
    f().await
}

/// Lazy-fetch channel metadata for a channel not in the startup discovery cache.
///
/// Handles channels added dynamically via membership notifications after startup.
/// Uses `CONTEXT_FETCH_TIMEOUT` with one retry on failure. Returns `None` on
/// persistent failure (graceful degradation — prompt will lack channel name and
/// DM detection).
async fn fetch_channel_info(channel_id: Uuid, rest: &RestClient) -> Option<PromptChannelInfo> {
    let path = format!("/api/channels/{}", channel_id);
    fetch_with_retry(|| async {
        match timeout(CONTEXT_FETCH_TIMEOUT, rest.get_json(&path)).await {
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
                    "channel info fetch failed: {e} — will retry"
                );
                None
            }
            Err(_) => {
                tracing::debug!(
                    channel_id = %channel_id,
                    "channel info fetch timed out — will retry"
                );
                None
            }
        }
    })
    .await
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

/// Normalize AND validate a pubkey for the batch profile API request.
/// Returns `None` for malformed input — only valid 64-char hex passes.
/// See also: `normalize_lookup_key` in queue.rs (normalize-only, no validation).
fn normalize_prompt_pubkey(pubkey: &str) -> Option<String> {
    let normalized = pubkey.trim().to_ascii_lowercase();
    if normalized.len() == 64 && normalized.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(normalized)
    } else {
        None
    }
}

fn collect_prompt_pubkeys(
    batch: &FlushBatch,
    conversation_context: Option<&ConversationContext>,
) -> Vec<String> {
    let mut pubkeys = HashSet::new();

    for event in &batch.events {
        pubkeys.insert(event.event.pubkey.to_hex().to_ascii_lowercase());

        for mentioned in crate::queue::parse_thread_tags(&event.event).mentioned_pubkeys {
            if let Some(normalized) = normalize_prompt_pubkey(&mentioned) {
                pubkeys.insert(normalized);
            }
        }
    }

    let context_messages = match conversation_context {
        Some(ConversationContext::Thread { messages, .. })
        | Some(ConversationContext::Dm { messages, .. }) => Some(messages),
        None => None,
    };

    if let Some(messages) = context_messages {
        for message in messages {
            if let Some(normalized) = normalize_prompt_pubkey(&message.pubkey) {
                pubkeys.insert(normalized);
            }
        }
    }

    let mut pubkeys: Vec<String> = pubkeys.into_iter().collect();
    pubkeys.sort();
    pubkeys
}

fn parse_profile_lookup_response(json: serde_json::Value) -> Option<PromptProfileLookup> {
    let profiles = json.get("profiles")?.as_object()?;
    let mut lookup = PromptProfileLookup::new();

    for (pubkey, profile) in profiles {
        lookup.insert(
            pubkey.to_ascii_lowercase(),
            PromptProfile {
                display_name: profile
                    .get("display_name")
                    .and_then(|value| value.as_str())
                    .map(str::to_string),
                nip05_handle: profile
                    .get("nip05_handle")
                    .and_then(|value| value.as_str())
                    .map(str::to_string),
            },
        );
    }

    if lookup.is_empty() {
        None
    } else {
        Some(lookup)
    }
}

async fn fetch_prompt_profile_lookup(
    batch: &FlushBatch,
    conversation_context: Option<&ConversationContext>,
    rest: &RestClient,
) -> Option<PromptProfileLookup> {
    let pubkeys = collect_prompt_pubkeys(batch, conversation_context);
    if pubkeys.is_empty() {
        return None;
    }

    let body = serde_json::json!({ "pubkeys": pubkeys });
    fetch_with_retry(|| async {
        match timeout(
            CONTEXT_FETCH_TIMEOUT,
            rest.post_json("/api/users/batch", &body),
        )
        .await
        {
            Ok(Ok(json)) => parse_profile_lookup_response(json),
            Ok(Err(e)) => {
                tracing::debug!("prompt profile lookup failed: {e} — will retry");
                None
            }
            Err(_) => {
                tracing::debug!("prompt profile lookup timed out — will retry");
                None
            }
        }
    })
    .await
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

    fetch_with_retry(|| async {
        match timeout(CONTEXT_FETCH_TIMEOUT, rest.get_json(&path)).await {
            Ok(Ok(json)) => parse_thread_response(json),
            Ok(Err(e)) => {
                tracing::warn!(
                    channel_id = %channel_id,
                    root = root_event_id,
                    "thread context fetch failed: {e} — will retry"
                );
                None
            }
            Err(_) => {
                tracing::warn!(
                    channel_id = %channel_id,
                    root = root_event_id,
                    "thread context fetch timed out — will retry"
                );
                None
            }
        }
    })
    .await
}

/// Fetch DM context via REST: `GET /api/channels/{id}/messages?limit=N`
async fn fetch_dm_context(
    channel_id: Uuid,
    limit: u32,
    rest: &RestClient,
) -> Option<ConversationContext> {
    let path = format!("/api/channels/{}/messages?limit={}", channel_id, limit);

    fetch_with_retry(|| async {
        match timeout(CONTEXT_FETCH_TIMEOUT, rest.get_json(&path)).await {
            Ok(Ok(json)) => parse_dm_response(json, limit),
            Ok(Err(e)) => {
                tracing::warn!(
                    channel_id = %channel_id,
                    "DM context fetch failed: {e} — will retry"
                );
                None
            }
            Err(_) => {
                tracing::warn!(
                    channel_id = %channel_id,
                    "DM context fetch timed out — will retry"
                );
                None
            }
        }
    })
    .await
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
            tracing::warn!(target: "pool::prompt", "turn hit max_tokens for {label} — session will be rotated");
        }
        StopReason::MaxTurnRequests => {
            tracing::warn!(target: "pool::prompt", "turn hit max_turn_requests for {label} — session will be rotated");
        }
        StopReason::Refusal => {
            tracing::warn!(target: "pool::prompt", "turn refused for {label}");
        }
    }
}

// ── Reaction indicators ───────────────────────────────────────────────────────
//
// Two-phase lifecycle visible to users:
//   👀  "seen"    — event was queued and an agent will handle it
//   💬  "working" — agent is actively prompting
//
// 💬 is awaited inline in `run_prompt_task` before the prompt fires, so
// add-before-remove ordering is structural. 👀 is fire-and-forget from
// `main.rs` at queue-push time for immediate responsiveness; on rare
// fast-failure paths the guard's cleanup may race with the 👀 add,
// leaving a cosmetic stale 👀 (see `ReactionGuard` docs).
//
// Cleanup is fire-and-forget via `ReactionGuard` (spawned on drop).
// Failures are debug-logged and ignored — reactions are cosmetic.

/// Drop guard that spawns reaction cleanup on any exit path.
///
/// Created at the top of `run_prompt_task`. On drop — normal return, early
/// return, or panic — spawns fire-and-forget removal of both 👀 and 💬.
///
/// ## Ordering
///
/// 💬 (`react_working`) is fire-and-forget (spawned before the prompt fires).
/// A brief race where 💬 appears slightly after the agent starts is acceptable.
///
/// 👀 (`react_seen`) is fire-and-forget from `main.rs` at queue-push time.
/// On rare fast-failure paths (e.g., `session_new` error on an idle agent),
/// the cleanup spawn may race with the 👀 add, leaving a stale 👀. This is
/// accepted as a cosmetic edge case — the message will be retried and the
/// stale 👀 is harmless.
struct ReactionGuard {
    rest: Option<crate::relay::RestClient>,
    ids: Vec<String>,
}

impl ReactionGuard {
    fn new(rest: crate::relay::RestClient, ids: Vec<String>) -> Self {
        Self {
            rest: if ids.is_empty() { None } else { Some(rest) },
            ids,
        }
    }
}

impl Drop for ReactionGuard {
    fn drop(&mut self) {
        // Guard against drop outside a tokio runtime (e.g., in unit tests or
        // during process teardown before the runtime is fully initialized).
        // `run_prompt_task` is always spawned via `JoinSet::spawn`, so a
        // runtime handle is normally available; `try_current` is the safe
        // fallback for the rare cases it isn't.
        if let Some(rest) = self.rest.take() {
            let ids = std::mem::take(&mut self.ids);
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(clear_reactions(rest, ids));
            }
            // If no runtime is available, reactions are left as-is — they are
            // cosmetic indicators and the stale state is harmless.
        }
    }
}

const REACTION_SEEN: &str = "👀";
const REACTION_WORKING: &str = "💬";

/// Best-effort timeout for a single reaction REST call.
const REACTION_TIMEOUT: Duration = Duration::from_millis(500);

/// Percent-encode a string for use in a URL path segment.
/// Emoji bytes are not URL-safe; event IDs (hex) pass through unchanged.
fn pct_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                use std::fmt::Write;
                let _ = write!(out, "%{byte:02X}");
            }
        }
    }
    out
}

/// Best-effort: add a reaction via a signed Nostr kind-7 event (NIP-25).
///
/// Builds a reaction event with `sprout_sdk::build_reaction`, signs it with
/// the keys already stored in `RestClient`, and submits via POST /api/events.
/// Returns immediately on timeout or any error — reactions are cosmetic.
pub(crate) async fn reaction_add(rest: &crate::relay::RestClient, event_id: &str, emoji: &str) {
    let target_id = match nostr::EventId::from_hex(event_id) {
        Ok(id) => id,
        Err(e) => {
            tracing::debug!(event_id, emoji, "reaction add: invalid event ID: {e}");
            return;
        }
    };
    let builder = match sprout_sdk::build_reaction(target_id, emoji) {
        Ok(b) => b,
        Err(e) => {
            tracing::debug!(event_id, emoji, "reaction add: build failed: {e}");
            return;
        }
    };
    let event = match builder.sign_with_keys(&rest.keys) {
        Ok(e) => e,
        Err(e) => {
            tracing::debug!(event_id, emoji, "reaction add: sign failed: {e}");
            return;
        }
    };
    let body = match serde_json::to_value(&event) {
        Ok(v) => v,
        Err(e) => {
            tracing::debug!(event_id, emoji, "reaction add: serialize failed: {e}");
            return;
        }
    };
    match tokio::time::timeout(REACTION_TIMEOUT, rest.post_json("/api/events", &body)).await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => tracing::debug!(event_id, emoji, "reaction add failed: {e}"),
        Err(_) => tracing::debug!(event_id, emoji, "reaction add timed out"),
    }
}

/// Best-effort: remove a reaction via a signed kind:5 (NIP-09) deletion event.
///
/// Looks up our kind:7 reaction event ID via GET /api/messages/{event_id}/reactions,
/// then submits a signed kind:5 deletion via POST /api/events.
/// Returns immediately on timeout or any error — reactions are cosmetic.
pub(crate) async fn reaction_remove(rest: &crate::relay::RestClient, event_id: &str, emoji: &str) {
    // Step 1: look up the reaction event ID we own for this emoji.
    let path = format!("/api/messages/{}/reactions", pct_encode(event_id));
    let resp = match tokio::time::timeout(Duration::from_millis(1_000), rest.get_json(&path)).await
    {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            tracing::debug!(event_id, emoji, "reaction remove: fetch failed: {e}");
            return;
        }
        Err(_) => {
            tracing::debug!(event_id, emoji, "reaction remove: fetch timed out");
            return;
        }
    };

    let my_pubkey = rest.keys.public_key().to_hex();
    let reid = resp
        .get("reactions")
        .and_then(|r| r.as_array())
        .and_then(|groups| {
            groups.iter().find_map(|group| {
                if group.get("emoji")?.as_str()? != emoji {
                    return None;
                }
                group.get("users")?.as_array()?.iter().find_map(|user| {
                    if user.get("pubkey")?.as_str()? != my_pubkey {
                        return None;
                    }
                    user.get("reaction_event_id")?
                        .as_str()
                        .map(|s| s.to_string())
                })
            })
        });

    let reid = match reid {
        Some(id) => id,
        None => {
            tracing::debug!(event_id, emoji, "reaction remove: no reaction event found");
            return;
        }
    };

    // Step 2: build and submit a signed kind:5 deletion for the reaction event.
    let target_id = match nostr::EventId::from_hex(&reid) {
        Ok(id) => id,
        Err(e) => {
            tracing::debug!(
                event_id,
                emoji,
                "reaction remove: invalid reaction event ID: {e}"
            );
            return;
        }
    };
    let builder = match sprout_sdk::build_remove_reaction(target_id) {
        Ok(b) => b,
        Err(e) => {
            tracing::debug!(event_id, emoji, "reaction remove: build failed: {e}");
            return;
        }
    };
    let event = match builder.sign_with_keys(&rest.keys) {
        Ok(e) => e,
        Err(e) => {
            tracing::debug!(event_id, emoji, "reaction remove: sign failed: {e}");
            return;
        }
    };
    let body = match serde_json::to_value(&event) {
        Ok(v) => v,
        Err(e) => {
            tracing::debug!(event_id, emoji, "reaction remove: serialize failed: {e}");
            return;
        }
    };
    match tokio::time::timeout(
        Duration::from_millis(1_000),
        rest.post_json("/api/events", &body),
    )
    .await
    {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => tracing::debug!(event_id, emoji, "reaction remove failed: {e}"),
        Err(_) => tracing::debug!(event_id, emoji, "reaction remove timed out"),
    }
}

/// Maximum concurrent reaction HTTP requests per fan-out call.
/// Prevents unbounded parallelism when a large batch of events arrives.
const REACTION_CONCURRENCY: usize = 10;

/// Add 💬 to all events, capped at `REACTION_CONCURRENCY` concurrent requests.
/// Awaited inline before the prompt fires.
async fn react_working(rest: &crate::relay::RestClient, event_ids: &[String]) {
    for chunk in event_ids.chunks(REACTION_CONCURRENCY) {
        futures_util::future::join_all(
            chunk
                .iter()
                .map(|eid| reaction_add(rest, eid, REACTION_WORKING)),
        )
        .await;
    }
}

/// Fire-and-forget: remove both 👀 and 💬 from all events. Spawned on turn complete.
/// Capped at `REACTION_CONCURRENCY` concurrent requests per chunk to avoid
/// unbounded HTTP fan-out on large batches.
async fn clear_reactions(rest: crate::relay::RestClient, event_ids: Vec<String>) {
    // Each event needs two removals (👀 and 💬); pair them and chunk by
    // REACTION_CONCURRENCY pairs so the total concurrent requests stay bounded.
    for chunk in event_ids.chunks(REACTION_CONCURRENCY) {
        futures_util::future::join_all(chunk.iter().flat_map(|eid| {
            [
                reaction_remove(&rest, eid, REACTION_SEEN),
                reaction_remove(&rest, eid, REACTION_WORKING),
            ]
        }))
        .await;
    }
}

// ─── Unit Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind, Tag};
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
    fn test_collect_prompt_pubkeys_includes_authors_mentions_and_context() {
        let keys = Keys::generate();
        let p_tag = Tag::parse(&[
            "p",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ])
        .unwrap();
        let event = EventBuilder::new(Kind::Custom(9), "hello", [p_tag])
            .sign_with_keys(&keys)
            .unwrap();
        let author_hex = event.pubkey.to_hex();
        let batch = FlushBatch {
            channel_id: Uuid::new_v4(),
            events: vec![crate::queue::BatchEvent {
                event,
                prompt_tag: "@mention".into(),
                received_at: std::time::Instant::now(),
            }],
            cancelled_events: vec![],
        };
        let context = ConversationContext::Thread {
            messages: vec![ContextMessage {
                pubkey: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
                timestamp: "2026-03-25T05:51:25Z".into(),
                content: "follow up".into(),
            }],
            total: 1,
            truncated: false,
        };

        let pubkeys = collect_prompt_pubkeys(&batch, Some(&context));

        let mut expected = vec![
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            author_hex,
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
        ];
        expected.sort();

        assert_eq!(pubkeys, expected);
    }

    #[test]
    fn test_parse_profile_lookup_response_extracts_display_name_and_nip05() {
        let lookup = parse_profile_lookup_response(json!({
            "profiles": {
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa": {
                    "display_name": "Wes",
                    "avatar_url": null,
                    "nip05_handle": "wes@example.com"
                }
            },
            "missing": []
        }))
        .expect("lookup should parse");

        assert_eq!(
            lookup.get("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            Some(&PromptProfile {
                display_name: Some("Wes".into()),
                nip05_handle: Some("wes@example.com".into()),
            })
        );
    }

    #[test]
    fn test_parse_profile_lookup_response_returns_none_for_empty() {
        assert!(parse_profile_lookup_response(json!({"profiles": {}})).is_none());
        assert!(parse_profile_lookup_response(json!({})).is_none());
    }

    #[test]
    fn test_json_to_context_message_missing_pubkey_uses_default() {
        let obj = json!({ "content": "hello" });
        let msg = json_to_context_message(&obj).expect("should parse");
        assert_eq!(msg.pubkey, "unknown");
    }

    // ── pct_encode tests ─────────────────────────────────────────────────

    #[test]
    fn test_pct_encode_hex_passthrough() {
        let hex = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
        assert_eq!(pct_encode(hex), hex);
    }

    #[test]
    fn test_pct_encode_emoji() {
        // 👀 = U+1F440 = F0 9F 91 80 in UTF-8
        assert_eq!(pct_encode("👀"), "%F0%9F%91%80");
    }

    #[test]
    fn test_pct_encode_emoji_speech_balloon() {
        // 💬 = U+1F4AC = F0 9F 92 AC in UTF-8
        assert_eq!(pct_encode("💬"), "%F0%9F%92%AC");
    }

    #[test]
    fn test_pct_encode_empty() {
        assert_eq!(pct_encode(""), "");
    }

    #[test]
    fn test_pct_encode_unreserved_passthrough() {
        assert_eq!(pct_encode("AZaz09-_.~"), "AZaz09-_.~");
    }

    #[test]
    fn test_pct_encode_reserved_chars() {
        assert_eq!(pct_encode("/"), "%2F");
        assert_eq!(pct_encode("+"), "%2B");
        assert_eq!(pct_encode(" "), "%20");
    }

    // ── SessionState tests ───────────────────────────────────────────────

    fn make_state() -> (SessionState, Uuid, Uuid) {
        let ch_a = Uuid::new_v4();
        let ch_b = Uuid::new_v4();
        let mut s = SessionState::default();
        s.sessions.insert(ch_a, "sess-a".into());
        s.sessions.insert(ch_b, "sess-b".into());
        s.turn_counts.insert(ch_a, 5);
        s.turn_counts.insert(ch_b, 3);
        s.heartbeat_session = Some("sess-hb".into());
        s.heartbeat_turn_count = 7;
        (s, ch_a, ch_b)
    }

    #[test]
    fn test_invalidate_channel_clears_session_and_turn_count() {
        let (mut s, ch_a, ch_b) = make_state();
        s.invalidate(&PromptSource::Channel(ch_a));

        assert!(!s.sessions.contains_key(&ch_a));
        assert!(!s.turn_counts.contains_key(&ch_a));
        // ch_b untouched
        assert_eq!(s.sessions.get(&ch_b).unwrap(), "sess-b");
        assert_eq!(*s.turn_counts.get(&ch_b).unwrap(), 3);
        // heartbeat untouched
        assert_eq!(s.heartbeat_session.as_deref(), Some("sess-hb"));
        assert_eq!(s.heartbeat_turn_count, 7);
    }

    #[test]
    fn test_invalidate_heartbeat_clears_session_and_turn_count() {
        let (mut s, ch_a, ch_b) = make_state();
        s.invalidate(&PromptSource::Heartbeat);

        assert!(s.heartbeat_session.is_none());
        assert_eq!(s.heartbeat_turn_count, 0);
        // channels untouched
        assert_eq!(s.sessions.len(), 2);
        assert_eq!(*s.turn_counts.get(&ch_a).unwrap(), 5);
        assert_eq!(*s.turn_counts.get(&ch_b).unwrap(), 3);
    }

    #[test]
    fn test_invalidate_all_clears_everything() {
        let (mut s, _ch_a, _ch_b) = make_state();
        s.invalidate_all();

        assert!(s.sessions.is_empty());
        assert!(s.turn_counts.is_empty());
        assert!(s.heartbeat_session.is_none());
        assert_eq!(s.heartbeat_turn_count, 0);
    }

    #[test]
    fn test_invalidate_nonexistent_channel_is_noop() {
        let (mut s, ch_a, ch_b) = make_state();
        let ghost = Uuid::new_v4();
        s.invalidate(&PromptSource::Channel(ghost));

        // Everything still intact.
        assert_eq!(s.sessions.len(), 2);
        assert_eq!(s.turn_counts.len(), 2);
        assert_eq!(*s.turn_counts.get(&ch_a).unwrap(), 5);
        assert_eq!(*s.turn_counts.get(&ch_b).unwrap(), 3);
    }

    #[test]
    fn test_invalidate_all_on_empty_state_is_noop() {
        let mut s = SessionState::default();
        s.invalidate_all(); // should not panic
        assert!(s.sessions.is_empty());
        assert!(s.turn_counts.is_empty());
    }

    #[test]
    fn test_invalidate_channel_returns_true_when_session_existed() {
        let (mut s, ch_a, ch_b) = make_state();
        assert!(s.invalidate_channel(&ch_a));
        assert!(!s.sessions.contains_key(&ch_a));
        assert!(!s.turn_counts.contains_key(&ch_a));
        // ch_b untouched
        assert_eq!(s.sessions.get(&ch_b).unwrap(), "sess-b");
        assert_eq!(*s.turn_counts.get(&ch_b).unwrap(), 3);
        // heartbeat untouched
        assert_eq!(s.heartbeat_session.as_deref(), Some("sess-hb"));
        assert_eq!(s.heartbeat_turn_count, 7);
    }

    #[test]
    fn test_invalidate_channel_returns_false_when_no_session() {
        let (mut s, _ch_a, _ch_b) = make_state();
        let ghost = Uuid::new_v4();
        assert!(!s.invalidate_channel(&ghost));
        // Nothing changed.
        assert_eq!(s.sessions.len(), 2);
        assert_eq!(s.turn_counts.len(), 2);
    }

    #[test]
    fn test_removed_channels_cleaned_via_invalidate_channel() {
        // Simulates handle_prompt_result: channels removed while agent
        // was checked out should have both sessions and turn_counts stripped.
        let (mut s, ch_a, ch_b) = make_state();
        let removed = vec![ch_a];
        for ch in &removed {
            s.invalidate_channel(ch);
        }
        assert!(!s.sessions.contains_key(&ch_a));
        assert!(!s.turn_counts.contains_key(&ch_a));
        assert_eq!(s.sessions.get(&ch_b).unwrap(), "sess-b");
        assert_eq!(*s.turn_counts.get(&ch_b).unwrap(), 3);
    }
}
