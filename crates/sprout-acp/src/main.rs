#![deny(unsafe_code)]

mod acp;
mod config;
mod filter;
mod pool;
mod queue;
mod relay;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use acp::{AcpClient, EnvVar, McpServer};
use anyhow::Result;
use config::{Config, DedupMode, SubscribeMode};
use filter::SubscriptionRule;
use futures_util::FutureExt;
use nostr::ToBech32;
use pool::{AgentPool, OwnedAgent, PromptContext, PromptOutcome, PromptResult, PromptSource};
use queue::{EventQueue, QueuedEvent};
use relay::HarnessRelay;
use sprout_core::kind::{
    KIND_MEMBER_ADDED_NOTIFICATION, KIND_MEMBER_REMOVED_NOTIFICATION, KIND_STREAM_MESSAGE,
    KIND_STREAM_REMINDER, KIND_WORKFLOW_APPROVAL_REQUESTED,
};
use tokio::sync::watch;
use uuid::Uuid;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("sprout_acp=info")),
        )
        .compact()
        .init();

    let config = Config::from_cli().map_err(|e| anyhow::anyhow!("configuration error: {e}"))?;
    tracing::info!("sprout-acp starting: {}", config.summary());

    // ── Step 1: Spawn N ACP agent subprocesses and initialize ─────────────────
    let mut agents = Vec::with_capacity(config.agents as usize);
    for i in 0..config.agents as usize {
        let acp = spawn_and_init(&config).await?;
        agents.push(OwnedAgent {
            index: i,
            acp,
            sessions: HashMap::new(),
            heartbeat_session: None,
        });
    }
    tracing::info!("agent_pool_ready agents={}", agents.len());
    let mut pool = AgentPool::new(agents);

    // ── Step 2: Connect to Sprout relay ──────────────────────────────────────
    let pubkey_hex = config.keys.public_key().to_hex();
    let mut relay = HarnessRelay::connect(
        &config.relay_url,
        &config.keys,
        config.api_token.as_deref(),
        &pubkey_hex,
    )
    .await
    .map_err(|e| anyhow::anyhow!("relay connect error: {e}"))?;

    tracing::info!("connected to relay at {}", config.relay_url);

    // ── Step 2b: Subscribe to membership notifications ────────────────────────
    relay
        .subscribe_membership_notifications()
        .await
        .map_err(|e| anyhow::anyhow!("membership notification subscribe error: {e}"))?;
    tracing::info!("subscribed to membership notifications");

    // ── Step 3: Discover channels and build subscription rules ────────────────
    let channels = relay
        .discover_channels()
        .await
        .map_err(|e| anyhow::anyhow!("channel discovery error: {e}"))?;

    tracing::info!("discovered {} channel(s)", channels.len());

    let rules: Vec<SubscriptionRule> = match config.subscribe_mode {
        SubscribeMode::Mentions => {
            vec![SubscriptionRule {
                name: "mentions".into(),
                channels: filter::ChannelScope::All("all".into()),
                kinds: config.kinds_override.clone().unwrap_or_else(|| {
                    vec![
                        KIND_STREAM_MESSAGE,
                        KIND_WORKFLOW_APPROVAL_REQUESTED,
                        KIND_STREAM_REMINDER,
                    ]
                }),
                require_mention: !config.no_mention_filter,
                filter: None,
                prompt_tag: Some("@mention".into()),
            }]
        }
        SubscribeMode::All => {
            vec![SubscriptionRule {
                name: "all".into(),
                channels: filter::ChannelScope::All("all".into()),
                kinds: config.kinds_override.clone().unwrap_or_default(),
                require_mention: false,
                filter: None,
                prompt_tag: Some("all".into()),
            }]
        }
        SubscribeMode::Config => {
            let loaded = config::load_rules(&config.config_path)?;
            if loaded.is_empty() {
                tracing::warn!(
                    "config file {} contains zero rules — agent will receive no events",
                    config.config_path.display()
                );
            }
            loaded
        }
    };

    // ── Step 4: Subscribe to channels ────────────────────────────────────────
    let channel_filters = config::resolve_channel_filters(&config, &channels, &rules);
    if channel_filters.is_empty() {
        tracing::warn!("no channel subscriptions resolved — agent will sit idle");
    }
    for (channel_id, filter) in &channel_filters {
        if let Err(e) = relay.subscribe_channel(*channel_id, filter.clone()).await {
            tracing::warn!("failed to subscribe to channel {channel_id}: {e}");
        } else {
            tracing::info!("subscribed to channel {channel_id}");
        }
    }

    // ── Step 5: Build shared prompt context ──────────────────────────────────
    let dedup_mode = config.dedup_mode;
    let mut queue = EventQueue::new(dedup_mode);

    let ctx = Arc::new(PromptContext {
        mcp_servers: build_mcp_servers(&config),
        initial_message: config.initial_message.clone(),
        turn_timeout: Duration::from_secs(config.turn_timeout_secs),
        dedup_mode: config.dedup_mode,
        system_prompt: config.system_prompt.clone(),
        heartbeat_prompt: config.heartbeat_prompt.clone(),
        cwd: std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("/"))
            .to_string_lossy()
            .to_string(),
    });

    // ── Step 6: Heartbeat timer ───────────────────────────────────────────────
    let mut heartbeat = if config.heartbeat_interval_secs > 0 {
        let interval = Duration::from_secs(config.heartbeat_interval_secs);
        Some(tokio::time::interval_at(
            tokio::time::Instant::now() + interval,
            interval,
        ))
    } else {
        None
    };
    let mut heartbeat_in_flight = false;

    // ── Step 7: Shutdown signal ───────────────────────────────────────────────
    let (shutdown_tx, mut shutdown_rx) = watch::channel(());

    let tx = shutdown_tx.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        let _ = tx.send(());
    });

    #[cfg(unix)]
    {
        let tx = shutdown_tx.clone();
        tokio::spawn(async move {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigterm = signal(SignalKind::terminate()).expect("SIGTERM handler");
            sigterm.recv().await;
            let _ = tx.send(());
        });
    }

    // Track the newest membership notification timestamp per channel so that
    // replayed events (returned in DESC order on reconnect) don't override the
    // correct final state. The first event seen per channel is the newest; any
    // older duplicate for the same channel is skipped.
    let mut membership_newest_ts: HashMap<Uuid, u64> = HashMap::new();

    // ── Step 8: Main orchestration loop ──────────────────────────────────────
    //
    // Branches 1 & 2 both need to borrow `pool`, but they access different
    // fields (result_rx vs join_set). We use `rx_and_join_set()` to split the
    // borrow, yielding a typed enum so the outer code can dispatch cleanly.
    enum PoolEvent {
        Result(Box<PromptResult>),
        Panic(tokio::task::JoinError),
    }

    loop {
        // Borrow result_rx and join_set simultaneously via split-borrow helper.
        let pool_event: Option<PoolEvent> = {
            let (result_rx, join_set) = pool.rx_and_join_set();
            tokio::select! {
                biased;
                r = result_rx.recv() => Some(PoolEvent::Result(Box::new(r.expect("result channel closed")))),
                // Guard: join_next() returns None immediately when JoinSet is
                // empty, which would cause a tight spin. Only poll when there
                // are in-flight tasks.
                Some(Err(e)) = join_set.join_next(), if !join_set.is_empty() => {
                    Some(PoolEvent::Panic(e))
                }
                // Remaining branches don't touch pool — evaluated when pool is idle.
                sprout_event = relay.next_event() => {
                    let _ = result_rx; // end split borrow before relay handling
                    match sprout_event {
                        Some(sprout_event) => {
                            let kind_u32 = sprout_event.event.kind.as_u16() as u32;

                            // ── Membership notification handling ──────────────
                            if kind_u32 == KIND_MEMBER_ADDED_NOTIFICATION
                                || kind_u32 == KIND_MEMBER_REMOVED_NOTIFICATION
                            {
                                let ch = sprout_event.channel_id;
                                let ts = sprout_event.event.created_at.as_u64();

                                // Skip stale membership events: on reconnect the relay
                                // replays events newest-first, so the first event per
                                // channel is authoritative. Any later (older) event for
                                // the same channel is outdated and must be ignored.
                                let dominated = membership_newest_ts
                                    .get(&ch)
                                    .map_or(false, |&newest| ts < newest);
                                if dominated {
                                    tracing::debug!(
                                        channel_id = %ch,
                                        kind = kind_u32,
                                        ts,
                                        "skipping stale membership notification (newer already processed)"
                                    );
                                    continue;
                                }
                                membership_newest_ts
                                    .entry(ch)
                                    .and_modify(|v| *v = (*v).max(ts))
                                    .or_insert(ts);

                                if kind_u32 == KIND_MEMBER_ADDED_NOTIFICATION {
                                    if let Some(filter) = config::resolve_dynamic_channel_filter(&config, ch, &rules) {
                                        tracing::info!(channel_id = %ch, "membership notification: subscribing to new channel");
                                        if let Err(e) = relay.subscribe_channel(ch, filter).await {
                                            tracing::warn!("failed to subscribe to new channel {ch}: {e}");
                                        }
                                    } else {
                                        tracing::debug!(channel_id = %ch, "membership notification: no matching rules — skipping");
                                    }
                                } else {
                                    tracing::info!(channel_id = %ch, "membership notification: unsubscribing from channel");
                                    if let Err(e) = relay.unsubscribe_channel(ch).await {
                                        tracing::warn!("failed to unsubscribe from channel {ch}: {e}");
                                    }
                                }
                                continue;
                            }
                            // ── End membership notification handling ──────────

                            if config.ignore_self && sprout_event.event.pubkey.to_hex() == pubkey_hex {
                                tracing::debug!(channel_id = %sprout_event.channel_id, "dropping self-authored event");
                                continue;
                            }
                            let matched = filter::match_event(&sprout_event.event, sprout_event.channel_id, &rules, &pubkey_hex).await;
                            let prompt_tag = match matched {
                                Some(m) => m.prompt_tag,
                                None => {
                                    tracing::debug!(channel_id = %sprout_event.channel_id, kind = sprout_event.event.kind.as_u16(), "event matched no rule — dropping");
                                    continue;
                                }
                            };
                            queue.push(QueuedEvent {
                                channel_id: sprout_event.channel_id,
                                event: sprout_event.event,
                                received_at: std::time::Instant::now(),
                                prompt_tag,
                            });
                            dispatch_pending(&mut pool, &mut queue, &ctx);
                        }
                        None => {
                            tracing::warn!("relay event stream ended — requesting reconnect");
                            if let Err(e) = relay.reconnect().await {
                                tracing::error!("relay background task is gone: {e} — exiting");
                                tokio::time::sleep(Duration::from_secs(1)).await;
                                break;
                            }
                        }
                    }
                    None
                }
                _ = async {
                    match heartbeat.as_mut() {
                        Some(hb) => hb.tick().await,
                        None => std::future::pending().await,
                    }
                } => {
                    let _ = result_rx;
                    if queue.has_flushable_work() {
                        tracing::debug!("heartbeat_skipped_events");
                        dispatch_pending(&mut pool, &mut queue, &ctx);
                    } else if pool.any_idle() {
                        dispatch_heartbeat(&mut pool, &ctx, &mut heartbeat_in_flight);
                    } else {
                        tracing::debug!("heartbeat_skipped_busy");
                    }
                    None
                }
                _ = shutdown_rx.changed() => {
                    tracing::info!("shutting down");
                    break;
                }
            }
        };

        match pool_event {
            Some(PoolEvent::Result(result)) => {
                if handle_prompt_result(
                    &mut pool,
                    &mut queue,
                    &config,
                    *result,
                    &mut heartbeat_in_flight,
                )
                .await
                    == LoopAction::Exit
                {
                    break;
                }
                if drain_ready_join_results(
                    &mut pool,
                    &mut queue,
                    &config,
                    &mut heartbeat_in_flight,
                )
                .await
                    == LoopAction::Exit
                {
                    break;
                }
                dispatch_pending(&mut pool, &mut queue, &ctx);
            }
            Some(PoolEvent::Panic(join_error)) => {
                tracing::error!("agent task panicked: {join_error}");
                recover_panicked_agent(
                    &mut pool,
                    &mut queue,
                    &config,
                    join_error,
                    &mut heartbeat_in_flight,
                )
                .await;
                if pool.live_count() == 0 {
                    tracing::error!("all agents dead — exiting");
                    break;
                }
                dispatch_pending(&mut pool, &mut queue, &ctx);
            }
            None => {} // relay/heartbeat/shutdown branches handled inline above
        }
    }

    // ── Shutdown sequence ─────────────────────────────────────────────────────
    tracing::info!("shutdown: waiting for in-flight prompts");
    let grace = Duration::from_secs(config.turn_timeout_secs + 5);
    let shutdown_result = tokio::time::timeout(grace, async {
        while let Some(result) = pool.join_set.join_next().await {
            if let Err(e) = result {
                tracing::warn!("task finished with error during shutdown: {e}");
            }
        }
    })
    .await;
    if shutdown_result.is_err() {
        tracing::warn!("grace period expired, aborting remaining tasks");
        pool.join_set.shutdown().await;
    }
    drop(pool);
    tracing::info!("sprout-acp stopped");
    Ok(())
}

// ── Loop control ──────────────────────────────────────────────────────────────

#[derive(PartialEq)]
enum LoopAction {
    Continue,
    Exit,
}

// ── dispatch_pending ──────────────────────────────────────────────────────────

/// Flush queued work to available agents.
fn dispatch_pending(pool: &mut AgentPool, queue: &mut EventQueue, ctx: &Arc<PromptContext>) {
    let mut dispatched: usize = 0;
    loop {
        let batch = match queue.flush_next() {
            Some(b) => b,
            None => break,
        };
        let channel_id = batch.channel_id;
        let affinity_hit = pool.has_session_for(channel_id);
        let agent = match pool.try_claim(Some(channel_id)) {
            Some(a) => a,
            None => {
                let pending = queue.pending_channels();
                tracing::debug!(pending_channels = pending, "pool_exhausted");
                queue.requeue_preserve_timestamps(batch);
                queue.mark_complete(channel_id);
                break;
            }
        };
        tracing::debug!(agent = agent.index, channel = %channel_id, affinity_hit, "agent_claimed");

        let prompt_text = queue::format_prompt(&batch, ctx.system_prompt.as_deref());
        let recoverable_batch = match ctx.dedup_mode {
            DedupMode::Queue => Some(batch.clone()),
            DedupMode::Drop => None,
        };

        let result_tx = pool.result_tx();
        let ctx_clone = Arc::clone(ctx);
        let agent_index = agent.index;

        let abort_handle = pool.join_set.spawn(async move {
            pool::run_prompt_task(agent, Some(batch), prompt_text, ctx_clone, result_tx).await;
        });

        pool.task_map_mut().insert(
            abort_handle.id(),
            pool::TaskMeta {
                agent_index,
                channel_id: Some(channel_id),
                recoverable_batch,
            },
        );
        dispatched += 1;
    }
    tracing::debug!(
        dispatched,
        queue_depth = queue.pending_channels(),
        "dispatch_pending"
    );
}

// ── handle_prompt_result ──────────────────────────────────────────────────────

async fn handle_prompt_result(
    pool: &mut AgentPool,
    queue: &mut EventQueue,
    config: &Config,
    result: PromptResult,
    heartbeat_in_flight: &mut bool,
) -> LoopAction {
    let before = pool.task_map().len();
    let agent_index = result.agent.index;
    pool.task_map_mut()
        .retain(|_, meta| meta.agent_index != agent_index);
    debug_assert_eq!(before, pool.task_map().len() + 1);

    match &result.source {
        PromptSource::Channel(ch) => queue.mark_complete(*ch),
        PromptSource::Heartbeat => *heartbeat_in_flight = false,
    }

    if let Some(batch) = result.batch {
        queue.requeue(batch);
    }

    let outcome_label = match &result.outcome {
        PromptOutcome::Ok(_) => "ok",
        PromptOutcome::Error(_) => "error",
        PromptOutcome::Timeout => "timeout",
        PromptOutcome::AgentExited => "exited",
    };
    let agent_index = result.agent.index;

    match result.outcome {
        PromptOutcome::AgentExited => {
            tracing::debug!(
                agent = agent_index,
                outcome = outcome_label,
                "agent_returned"
            );
            let index = result.agent.index;
            match respawn_agent_into(result.agent, config).await {
                Ok(agent) => pool.return_agent(agent),
                Err(e) => {
                    tracing::error!("failed to respawn agent {index}: {e}");
                    if pool.live_count() == 0 {
                        tracing::error!("all agents dead — exiting");
                        return LoopAction::Exit;
                    }
                }
            }
        }
        _ => {
            tracing::debug!(
                agent = agent_index,
                outcome = outcome_label,
                "agent_returned"
            );
            pool.return_agent(result.agent);
        }
    }
    LoopAction::Continue
}

// ── recover_panicked_agent ────────────────────────────────────────────────────

async fn recover_panicked_agent(
    pool: &mut AgentPool,
    queue: &mut EventQueue,
    config: &Config,
    join_error: tokio::task::JoinError,
    heartbeat_in_flight: &mut bool,
) {
    let task_id = join_error.id();
    let Some(meta) = pool.task_map_mut().remove(&task_id) else {
        tracing::error!("panic for unknown task {task_id:?} — bug");
        return;
    };
    let i = meta.agent_index;

    if let Some(ch) = meta.channel_id {
        queue.mark_complete(ch);
        tracing::warn!("cleared wedged in-flight channel {ch} from panicked agent {i}");
    } else {
        *heartbeat_in_flight = false;
        tracing::warn!("cleared wedged heartbeat_in_flight from panicked agent {i}");
    }

    if let Some(batch) = meta.recoverable_batch {
        queue.requeue(batch);
        tracing::warn!("requeued batch for panicked agent {i}");
    }

    match spawn_and_init(config).await {
        Ok(acp) => {
            pool.agents_mut()[i] = Some(OwnedAgent {
                index: i,
                acp,
                sessions: HashMap::new(),
                heartbeat_session: None,
            });
            tracing::info!("respawned agent {i} after panic");
        }
        Err(e) => {
            tracing::error!("failed to respawn agent {i} after panic: {e}");
        }
    }
}

// ── drain_ready_join_results ──────────────────────────────────────────────────

async fn drain_ready_join_results(
    pool: &mut AgentPool,
    queue: &mut EventQueue,
    config: &Config,
    heartbeat_in_flight: &mut bool,
) -> LoopAction {
    while let Some(Some(join_result)) = pool.join_set.join_next().now_or_never() {
        if let Err(join_error) = join_result {
            tracing::error!("agent task panicked: {join_error}");
            recover_panicked_agent(pool, queue, config, join_error, heartbeat_in_flight).await;
            if pool.live_count() == 0 {
                return LoopAction::Exit;
            }
        }
    }
    LoopAction::Continue
}

// ── dispatch_heartbeat ────────────────────────────────────────────────────────

fn dispatch_heartbeat(
    pool: &mut AgentPool,
    ctx: &Arc<PromptContext>,
    heartbeat_in_flight: &mut bool,
) {
    if *heartbeat_in_flight {
        return;
    }
    let agent = match pool.try_claim(None) {
        Some(a) => a,
        None => return,
    };

    let prompt_text = ctx
        .heartbeat_prompt
        .clone()
        .unwrap_or_else(default_heartbeat_prompt);
    let result_tx = pool.result_tx();
    let ctx_clone = Arc::clone(ctx);
    let agent_index = agent.index;

    let abort_handle = pool.join_set.spawn(async move {
        pool::run_prompt_task(agent, None, prompt_text, ctx_clone, result_tx).await;
    });

    pool.task_map_mut().insert(
        abort_handle.id(),
        pool::TaskMeta {
            agent_index,
            channel_id: None,
            recoverable_batch: None,
        },
    );
    *heartbeat_in_flight = true;
    tracing::info!(agent = agent_index, "heartbeat_fired");
}

// ── default_heartbeat_prompt ──────────────────────────────────────────────────

fn default_heartbeat_prompt() -> String {
    let now = chrono::Utc::now().to_rfc3339();
    format!(
        "[System: Heartbeat]\nTime: {now}\n\n\
         You have been awakened for a routine heartbeat. You have NO incoming messages or\n\
         active channel context for this turn.\n\n\
         Your tasks:\n\
         1. Call `get_feed_actions()` to check for pending workflow approvals or\n\
            high-priority requests addressed to you.\n\
         2. Call `get_feed_mentions()` to check for unanswered @mentions.\n\
         3. If you find actionable items, address them using the appropriate tools\n\
            (e.g., `approve_workflow_step`, `send_message`, `send_reply`).\n\
         4. If there are no pending actions or mentions, end your turn immediately.\n\n\
         Do not call `list_channels()` or `search()` unless you have a specific reason.\n\
         Do not invent work — only act on items surfaced by the feed tools."
    )
}

// ── respawn_agent_into ────────────────────────────────────────────────────────

async fn respawn_agent_into(old_agent: OwnedAgent, config: &Config) -> Result<OwnedAgent> {
    let index = old_agent.index;
    drop(old_agent); // kill the old process via AcpClient Drop
    let acp = spawn_and_init(config).await?;
    Ok(OwnedAgent {
        index,
        acp,
        sessions: HashMap::new(),
        heartbeat_session: None,
    })
}

// ── spawn_and_init ────────────────────────────────────────────────────────────

async fn spawn_and_init(config: &Config) -> Result<AcpClient> {
    let mut acp = AcpClient::spawn(&config.agent_command, &config.agent_args)
        .await
        .map_err(|e| anyhow::anyhow!("failed to spawn agent: {e}"))?;

    let init_result = acp
        .initialize()
        .await
        .map_err(|e| anyhow::anyhow!("agent initialize failed: {e}"))?;

    tracing::info!("agent initialized: {init_result}");
    Ok(acp)
}

// ── build_mcp_servers ─────────────────────────────────────────────────────────

fn build_mcp_servers(config: &Config) -> Vec<McpServer> {
    vec![McpServer {
        name: "sprout-mcp".to_string(),
        command: config.mcp_command.clone(),
        args: vec![],
        env: {
            let mut env = vec![
                EnvVar {
                    name: "SPROUT_RELAY_URL".into(),
                    value: config.relay_url.clone(),
                },
                EnvVar {
                    name: "SPROUT_PRIVATE_KEY".into(),
                    value: config
                        .keys
                        .secret_key()
                        .to_bech32()
                        .expect("secret key bech32 encoding should never fail"),
                },
            ];
            if let Some(ref token) = config.api_token {
                env.push(EnvVar {
                    name: "SPROUT_API_TOKEN".into(),
                    value: token.clone(),
                });
            }
            env
        },
    }]
}
