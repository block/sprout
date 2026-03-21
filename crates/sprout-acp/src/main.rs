#![deny(unsafe_code)]

mod acp;
mod config;
mod filter;
mod pool;
mod queue;
mod relay;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use acp::{AcpClient, EnvVar, McpServer};
use anyhow::Result;
use clap::Parser;
use config::{Config, DedupMode, ModelsArgs, SubscribeMode};
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
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

// ── Subcommand dispatch ───────────────────────────────────────────────────────

/// Check if argv[1] matches a subcommand name, before any clap parsing.
///
/// This avoids clap rejecting harness flags (like `--private-key`) that aren't
/// declared on the subcommand's `Parser`. The `models` path has its own
/// `ModelsArgs` parser; the default path uses the existing `CliArgs`.
///
/// **Constraint**: subcommand must be argv[1] — flags before the subcommand
/// name (e.g., `sprout-acp --verbose models`) are not supported.
fn is_subcommand(name: &str) -> bool {
    std::env::args().nth(1).map(|a| a == name).unwrap_or(false)
}

/// Timeout for the `sprout-acp models` subcommand (spawn + init + session/new).
const MODELS_TIMEOUT: Duration = Duration::from_secs(10);

#[tokio::main]
async fn main() -> Result<()> {
    // Install the ring crypto provider for rustls (required for wss:// connections).
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install rustls crypto provider");
    // ── Subcommand dispatch — before Config::from_cli() or any harness setup ──
    if is_subcommand("models") {
        // Strip the "models" token so clap doesn't reject it as a positional.
        // Keeps argv[0] (binary name) and passes everything after "models".
        let filtered: Vec<String> = std::env::args()
            .enumerate()
            .filter(|(i, _)| *i != 1)
            .map(|(_, a)| a)
            .collect();
        let args = ModelsArgs::parse_from(&filtered);
        return run_models(args).await;
    }

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
            model_capabilities: None,
            desired_model: config.model.clone(),
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

    // ── Step 2c: Set initial presence ─────────────────────────────────────────
    let rest_client_for_presence = relay.rest_client();
    if config.presence_enabled {
        match rest_client_for_presence
            .put_json("/api/presence", &serde_json::json!({"status": "online"}))
            .await
        {
            Ok(_) => tracing::info!("presence set to online"),
            Err(e) => tracing::warn!("failed to set initial presence: {e}"),
        }
    }

    // ── Step 2d: Query agent owner (with retry) ─────────────────────────────
    // Owner lookup is critical for !shutdown — a transient failure here would
    // permanently disable remote shutdown for this process. Retry a few times
    // with backoff so a brief relay hiccup doesn't leave us uncontrollable.
    // Owner lookup: try at startup, but if it fails the shutdown handler will
    // retry lazily when a candidate !shutdown message arrives. This means a
    // relay outage during startup doesn't permanently disable remote shutdown.
    let mut owner_pubkey: Option<String> = {
        let profile_url = format!("/api/users/{pubkey_hex}/profile");
        match rest_client_for_presence.get_json(&profile_url).await {
            Ok(v) => v
                .get("agent_owner_pubkey")
                .and_then(|v| v.as_str())
                .map(String::from),
            Err(e) => {
                tracing::warn!("startup owner lookup failed (will retry lazily): {e}");
                None
            }
        }
    };
    if let Some(ref owner) = owner_pubkey {
        tracing::info!("agent owner: {owner}");
    } else {
        tracing::info!("no agent owner set at startup — will resolve lazily on !shutdown");
    }

    // ── Step 3: Discover channels and build subscription rules ────────────────
    let channel_info_map = relay
        .discover_channels()
        .await
        .map_err(|e| anyhow::anyhow!("channel discovery error: {e}"))?;

    tracing::info!("discovered {} channel(s)", channel_info_map.len());
    let channel_ids: Vec<Uuid> = channel_info_map.keys().copied().collect();

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
    let channel_filters = config::resolve_channel_filters(&config, &channel_ids, &rules);
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
        rest_client: relay.rest_client(),
        channel_info: channel_info_map,
        context_message_limit: config.context_message_limit,
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

    // ── Step 6b: Presence heartbeat timer (refreshes 90s TTL every 60s) ───────
    let mut presence_heartbeat = if config.presence_enabled {
        let interval = Duration::from_secs(60);
        Some(tokio::time::interval_at(
            tokio::time::Instant::now() + interval,
            interval,
        ))
    } else {
        None
    };

    // ── Step 6c: Typing refresh timer (re-publishes kind:20002 every 3s) ──────
    let mut typing_refresh = if config.typing_enabled {
        let interval = Duration::from_secs(3);
        Some(tokio::time::interval_at(
            tokio::time::Instant::now() + interval,
            interval,
        ))
    } else {
        None
    };
    let mut typing_channels: HashSet<Uuid> = HashSet::new();
    let mut presence_task: Option<tokio::task::JoinHandle<()>> = None;

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

    // Track the newest membership notification timestamp per channel.
    // On reconnect the relay replays events newest-first, so the first event
    // per channel is authoritative. Any later event with ts < newest is stale.
    // Exact duplicates (same event ID) are caught by seen_membership_ids.
    //
    // Uses strict `<` (not `<=`) so that legitimate live events at the same
    // second are both processed. The seen_membership_ids set handles exact
    // replays that share the same timestamp.
    let mut membership_newest_ts: HashMap<Uuid, u64> = HashMap::new();
    // Dedup set for exact membership event replays (bounded, cleared at 2000).
    let mut seen_membership_ids: HashSet<String> = HashSet::new();

    // Channels the agent has been removed from. When a checked-out agent is
    // returned to the pool, its sessions for these channels are stripped, and
    // failed/panicked batches for these channels are dropped instead of requeued.
    //
    // Cleared on re-add (KIND_MEMBER_ADDED_NOTIFICATION) so re-joined channels
    // regain session affinity.
    //
    // Known limitation: if a batch is in-flight when the channel is removed AND
    // re-added before the batch returns, the stale batch may be requeued. This
    // is acceptable because: (a) the agent is a member again and has access,
    // (b) the events are from the agent's authorized history, (c) the window
    // is extremely narrow (membership changes are rare, prompt turns are seconds),
    // and (d) fixing this would require per-channel epoch tracking on TaskMeta
    // and PromptResult — significant complexity for a benign edge case. If strict
    // causal invalidation is needed, add a monotonic epoch counter per channel
    // and capture it in TaskMeta at dispatch time.
    let mut removed_channels: HashSet<Uuid> = HashSet::new();

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
                                let eid = sprout_event.event.id.to_hex();

                                // Two-layer membership dedup:
                                //
                                // 1. Exact duplicate rejection (seen_membership_ids):
                                //    Catches the same event replayed on reconnect.
                                //
                                // 2. Timestamp watermark (membership_newest_ts):
                                //    Uses strict `<` so that older events from reconnect
                                //    replay are dropped, but legitimate live events at the
                                //    same second are both processed. This is safe because
                                //    exact duplicates are already caught by layer 1.
                                //
                                // Why not `<=`? That would suppress legitimate live
                                // add→remove (or remove→add) sequences in the same second,
                                // leaving the harness in the wrong membership state.
                                if !seen_membership_ids.insert(eid.clone()) {
                                    tracing::debug!(
                                        channel_id = %ch,
                                        kind = kind_u32,
                                        "skipping duplicate membership notification (same event_id)"
                                    );
                                    continue;
                                }
                                // Bound the dedup set to prevent unbounded growth.
                                // Re-insert the current ID after clearing so it stays
                                // protected against immediate replay (same pattern as
                                // relay.rs BgState::record_event).
                                if seen_membership_ids.len() > 2000 {
                                    seen_membership_ids.clear();
                                    seen_membership_ids.insert(eid);
                                }
                                if let Some(&newest) = membership_newest_ts.get(&ch) {
                                    if ts < newest {
                                        tracing::debug!(
                                            channel_id = %ch,
                                            kind = kind_u32,
                                            ts,
                                            newest,
                                            "skipping stale membership notification (older than newest)"
                                        );
                                        continue;
                                    }
                                }
                                membership_newest_ts.insert(ch, ts);

                                if kind_u32 == KIND_MEMBER_ADDED_NOTIFICATION {
                                    // Clear removal tracking so sessions are not
                                    // stripped for a legitimately re-added channel.
                                    removed_channels.remove(&ch);

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
                                    // Drain queued events and invalidate sessions for the
                                    // removed channel. Events already in-flight will
                                    // complete normally (the relay may reject actions if
                                    // the agent lost access).
                                    let drained_ids = queue.drain_channel(ch);
                                    let invalidated = pool.invalidate_channel_sessions(ch);
                                    // Track removed channels so checked-out agents get
                                    // their sessions stripped when they return to the pool.
                                    removed_channels.insert(ch);
                                    typing_channels.remove(&ch);
                                    // Best-effort: clean up 👀 on drained events.
                                    // Note: the relay revokes membership before
                                    // emitting the notification, so this DELETE may
                                    // 403 on non-open channels. Stale 👀 in that
                                    // case is a known limitation — fix belongs in
                                    // the relay (clean up bot reactions on removal).
                                    if !drained_ids.is_empty() {
                                        let rc = ctx.rest_client.clone();
                                        let ids = drained_ids.clone();
                                        tokio::spawn(async move {
                                            for eid in &ids {
                                                pool::reaction_remove(&rc, eid, "👀").await;
                                            }
                                        });
                                    }
                                    if !drained_ids.is_empty() || invalidated > 0 {
                                        tracing::info!(
                                            channel_id = %ch,
                                            drained = drained_ids.len(),
                                            invalidated,
                                            "cleaned up after membership removal"
                                        );
                                    }
                                }
                                continue;
                            }
                            // ── End membership notification handling ──────────

                            if config.ignore_self && sprout_event.event.pubkey.to_hex() == pubkey_hex {
                                tracing::debug!(channel_id = %sprout_event.channel_id, "dropping self-authored event");
                                continue;
                            }

                            // ── Shutdown command handling ─────────────────────
                            // Check: kind:9, content "!shutdown", from owner, mentions THIS agent.
                            let is_shutdown = kind_u32 == KIND_STREAM_MESSAGE
                                && sprout_event.event.content.trim() == "!shutdown"
                                && sprout_event.event.tags.iter().any(|t| {
                                    t.as_slice().first().map(|s| s.as_str()) == Some("p")
                                        && t.as_slice().get(1).map(|s| s.as_str()) == Some(pubkey_hex.as_str())
                                });
                            if is_shutdown {
                                // Lazy owner resolution: if we don't have the owner
                                // yet (startup lookup failed), try now. This ensures
                                // a relay outage during startup doesn't permanently
                                // disable remote shutdown.
                                if owner_pubkey.is_none() {
                                    let profile_url = format!("/api/users/{pubkey_hex}/profile");
                                    match rest_client_for_presence.get_json(&profile_url).await {
                                        Ok(v) => {
                                            owner_pubkey = v
                                                .get("agent_owner_pubkey")
                                                .and_then(|v| v.as_str())
                                                .map(String::from);
                                            if let Some(ref o) = owner_pubkey {
                                                tracing::info!("lazy owner resolution succeeded: {o}");
                                            }
                                        }
                                        Err(e) => {
                                            tracing::warn!("lazy owner lookup failed: {e}");
                                        }
                                    }
                                }
                                if let Some(ref owner) = owner_pubkey {
                                    if sprout_event.event.pubkey.to_hex() == *owner {
                                        tracing::info!(
                                            channel_id = %sprout_event.channel_id,
                                            sender = %sprout_event.event.pubkey.to_hex(),
                                            "shutdown command from owner — exiting gracefully"
                                        );
                                        let _ = shutdown_tx.send(());
                                        continue;
                                    }
                                }
                                // Not from owner — fall through to normal prompt handling.
                                // Don't drop it — it's a regular message that happens to
                                // contain "!shutdown" from a non-owner.
                            }
                            // ── End shutdown command handling ──────────────────

                            let matched = filter::match_event(&sprout_event.event, sprout_event.channel_id, &rules, &pubkey_hex).await;
                            let prompt_tag = match matched {
                                Some(m) => m.prompt_tag,
                                None => {
                                    tracing::debug!(channel_id = %sprout_event.channel_id, kind = sprout_event.event.kind.as_u16(), "event matched no rule — dropping");
                                    continue;
                                }
                            };
                            let event_id_hex = sprout_event.event.id.to_hex();
                            let accepted = queue.push(QueuedEvent {
                                channel_id: sprout_event.channel_id,
                                event: sprout_event.event,
                                received_at: std::time::Instant::now(),
                                prompt_tag,
                            });
                            // 👀 — immediate "seen" reaction, only if the event
                            // was actually queued (not dropped by DedupMode::Drop).
                            // Fire-and-forget: on rare fast-failure paths the
                            // guard's cleanup may race with this add, leaving a
                            // cosmetic stale 👀. Acceptable — see ReactionGuard docs.
                            if accepted {
                                let rc = ctx.rest_client.clone();
                                tokio::spawn(async move {
                                    pool::reaction_add(&rc, &event_id_hex, "👀").await;
                                });
                            }
                            typing_channels.extend(dispatch_pending(&mut pool, &mut queue, &ctx));
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
                        typing_channels.extend(dispatch_pending(&mut pool, &mut queue, &ctx));
                    } else if pool.any_idle() {
                        dispatch_heartbeat(&mut pool, &ctx, &mut heartbeat_in_flight);
                    } else {
                        tracing::debug!("heartbeat_skipped_busy");
                    }
                    None
                }
                _ = async {
                    match presence_heartbeat.as_mut() {
                        Some(t) => t.tick().await,
                        None => std::future::pending().await,
                    }
                } => {
                    let _ = result_rx;
                    // Abort previous heartbeat if still in flight (prevents race on shutdown).
                    if let Some(h) = presence_task.take() {
                        h.abort();
                    }
                    let rc = rest_client_for_presence.clone();
                    presence_task = Some(tokio::spawn(async move {
                        if let Err(e) = rc.put_json("/api/presence", &serde_json::json!({"status": "online"})).await {
                            tracing::warn!("presence heartbeat failed: {e}");
                        }
                    }));
                    None
                }
                _ = async {
                    match typing_refresh.as_mut() {
                        Some(t) => t.tick().await,
                        None => std::future::pending().await,
                    }
                } => {
                    let _ = result_rx;
                    for &ch in &typing_channels {
                        if let Ok(event) = relay.build_typing_event(ch) {
                            if let Err(e) = relay.publish_event(event).await {
                                tracing::debug!("typing indicator failed for {ch}: {e}");
                            }
                        }
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
                // Stop typing indicator for the completed channel.
                if let PromptSource::Channel(ch) = &result.source {
                    typing_channels.remove(ch);
                }
                if handle_prompt_result(
                    &mut pool,
                    &mut queue,
                    &config,
                    *result,
                    &mut heartbeat_in_flight,
                    &removed_channels,
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
                    &removed_channels,
                    &mut typing_channels,
                )
                .await
                    == LoopAction::Exit
                {
                    break;
                }
                typing_channels.extend(dispatch_pending(&mut pool, &mut queue, &ctx));
            }
            Some(PoolEvent::Panic(join_error)) => {
                tracing::error!("agent task panicked: {join_error}");
                recover_panicked_agent(
                    &mut pool,
                    &mut queue,
                    &config,
                    join_error,
                    &mut heartbeat_in_flight,
                    &removed_channels,
                    &mut typing_channels,
                )
                .await;
                if pool.live_count() == 0 {
                    tracing::error!("all agents dead — exiting");
                    break;
                }
                typing_channels.extend(dispatch_pending(&mut pool, &mut queue, &ctx));
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

    // Cancel any in-flight presence heartbeat before sending offline.
    if let Some(h) = presence_task.take() {
        h.abort();
    }

    // Best-effort: set presence to offline before exiting.
    if config.presence_enabled {
        match tokio::time::timeout(
            Duration::from_secs(2),
            rest_client_for_presence
                .put_json("/api/presence", &serde_json::json!({"status": "offline"})),
        )
        .await
        {
            Ok(Ok(_)) => tracing::info!("presence set to offline"),
            Ok(Err(e)) => tracing::warn!("failed to set offline presence: {e}"),
            Err(_) => tracing::warn!("offline presence timed out"),
        }
    }

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
fn dispatch_pending(
    pool: &mut AgentPool,
    queue: &mut EventQueue,
    ctx: &Arc<PromptContext>,
) -> Vec<Uuid> {
    let mut dispatched_channels = Vec::new();
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

        let recoverable_batch = match ctx.dedup_mode {
            DedupMode::Queue => Some(batch.clone()),
            DedupMode::Drop => None,
        };

        let result_tx = pool.result_tx();
        let ctx_clone = Arc::clone(ctx);
        let agent_index = agent.index;

        // Prompt text is now built inside run_prompt_task (needs async for
        // context fetching). Pass None for prompt_text; batch carries the data.
        let abort_handle = pool.join_set.spawn(async move {
            pool::run_prompt_task(agent, Some(batch), None, ctx_clone, result_tx).await;
        });

        pool.task_map_mut().insert(
            abort_handle.id(),
            pool::TaskMeta {
                agent_index,
                channel_id: Some(channel_id),
                recoverable_batch,
            },
        );
        dispatched_channels.push(channel_id);
    }
    tracing::debug!(
        dispatched = dispatched_channels.len(),
        queue_depth = queue.pending_channels(),
        "dispatch_pending"
    );
    dispatched_channels
}

// ── handle_prompt_result ──────────────────────────────────────────────────────

async fn handle_prompt_result(
    pool: &mut AgentPool,
    queue: &mut EventQueue,
    config: &Config,
    mut result: PromptResult,
    heartbeat_in_flight: &mut bool,
    removed_channels: &HashSet<Uuid>,
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
        // Don't requeue batches for channels the agent was removed from —
        // those events are stale and should be silently dropped.
        if !removed_channels.contains(&batch.channel_id) {
            queue.requeue(batch);
        } else {
            tracing::debug!(
                channel_id = %batch.channel_id,
                events = batch.events.len(),
                "dropping failed batch for removed channel"
            );
        }
    }

    // Strip sessions for channels the agent was removed from while this
    // agent was checked out. This covers the gap where invalidate_channel_sessions
    // only touches idle agents.
    if !removed_channels.is_empty() {
        result
            .agent
            .sessions
            .retain(|ch, _| !removed_channels.contains(ch));
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
    removed_channels: &HashSet<Uuid>,
    typing_channels: &mut HashSet<Uuid>,
) {
    let task_id = join_error.id();
    let Some(meta) = pool.task_map_mut().remove(&task_id) else {
        tracing::error!("panic for unknown task {task_id:?} — bug");
        return;
    };
    let i = meta.agent_index;

    if let Some(ch) = meta.channel_id {
        queue.mark_complete(ch);
        typing_channels.remove(&ch);
        tracing::warn!("cleared wedged in-flight channel {ch} from panicked agent {i}");
    } else {
        *heartbeat_in_flight = false;
        tracing::warn!("cleared wedged heartbeat_in_flight from panicked agent {i}");
    }

    if let Some(batch) = meta.recoverable_batch {
        // Don't requeue batches for removed channels.
        if !removed_channels.contains(&batch.channel_id) {
            queue.requeue(batch);
            tracing::warn!("requeued batch for panicked agent {i}");
        } else {
            tracing::debug!(
                channel_id = %batch.channel_id,
                "dropping panicked batch for removed channel"
            );
        }
    }

    match spawn_and_init(config).await {
        Ok(acp) => {
            pool.agents_mut()[i] = Some(OwnedAgent {
                index: i,
                acp,
                sessions: HashMap::new(),
                heartbeat_session: None,
                model_capabilities: None,
                desired_model: config.model.clone(),
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
    removed_channels: &HashSet<Uuid>,
    typing_channels: &mut HashSet<Uuid>,
) -> LoopAction {
    while let Some(Some(join_result)) = pool.join_set.join_next().now_or_never() {
        if let Err(join_error) = join_result {
            tracing::error!("agent task panicked: {join_error}");
            recover_panicked_agent(
                pool,
                queue,
                config,
                join_error,
                heartbeat_in_flight,
                removed_channels,
                typing_channels,
            )
            .await;
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
        pool::run_prompt_task(agent, None, Some(prompt_text), ctx_clone, result_tx).await;
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
         1. Call `get_feed(types='needs_action')` to check for pending workflow approvals or\n\
            high-priority requests addressed to you.\n\
         2. Call `get_feed(types='mentions')` to check for unanswered @mentions.\n\
         3. If you find actionable items, address them using the appropriate tools\n\
            (e.g., `approve_step`, `send_message`, `send_message(parent_event_id=...)`).\n\
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
        model_capabilities: None,
        desired_model: config.model.clone(),
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

// ── run_models ─────────────────────────────────────────────────────────────────

/// `sprout-acp models` — spawn an agent, query its available models, exit.
///
/// Flow: spawn → initialize → session/new → print models → shutdown.
/// No relay connection, no MCP servers, no subscriptions. ~2-5s total.
async fn run_models(args: ModelsArgs) -> Result<()> {
    use acp::{extract_model_config_options, extract_model_state};

    let agent_args = config::normalize_agent_args(&args.agent_command, args.agent_args);
    let cwd = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("/"))
        .to_string_lossy()
        .to_string();

    // Spawn outside the timeout so we always own the child for cleanup.
    let mut client = match AcpClient::spawn(&args.agent_command, &agent_args).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: failed to spawn agent: {e}");
            std::process::exit(1);
        }
    };

    // Initialize + session/new under a timeout. Client is owned above,
    // so shutdown() runs on all paths (success, error, timeout).
    let protocol_result = tokio::time::timeout(MODELS_TIMEOUT, async {
        let init = client.initialize().await?;
        let session = client.session_new_full(&cwd, vec![]).await?;
        Ok::<_, acp::AcpError>((init, session))
    })
    .await;

    let (init_result, session_resp) = match protocol_result {
        Ok(Ok(tuple)) => tuple,
        Ok(Err(e)) => {
            client.shutdown().await;
            eprintln!("error: agent communication failed: {e}");
            std::process::exit(1);
        }
        Err(_) => {
            client.shutdown().await;
            eprintln!("error: agent timed out ({MODELS_TIMEOUT:?})");
            std::process::exit(1);
        }
    };

    // Extract agent info from initialize response.
    // ACP spec uses "serverInfo" (MCP heritage); some agents may use "agentInfo".
    let info_obj = init_result
        .get("serverInfo")
        .or_else(|| init_result.get("agentInfo"));
    let agent_name = info_obj
        .and_then(|ai| ai.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let agent_version = info_obj
        .and_then(|ai| ai.get("version"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    // Extract model info from session/new response.
    let config_options = extract_model_config_options(&session_resp.raw);
    let model_state = extract_model_state(&session_resp.raw);

    if args.json {
        // Structured JSON output — consumed by Phase 3 `get_agent_models`.
        let output = serde_json::json!({
            "agent": {
                "name": agent_name,
                "version": agent_version,
            },
            "stable": {
                "configOptions": config_options,
            },
            "unstable": model_state.as_ref().map(|ms| serde_json::json!({
                "currentModelId": ms.get("currentModelId"),
                "availableModels": ms.get("availableModels"),
            })),
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        // Human-readable output.
        println!("Agent: {} v{}", agent_name, agent_version);
        println!();

        let mut has_models = false;

        if !config_options.is_empty() {
            println!("Models (stable configOptions):");
            for opt in &config_options {
                let config_id = opt.get("configId").and_then(|v| v.as_str()).unwrap_or("?");
                let display = opt
                    .get("displayName")
                    .and_then(|v| v.as_str())
                    .unwrap_or(config_id);
                println!("  {display} (configId: {config_id})");
                if let Some(options) = opt.get("options").and_then(|v| v.as_array()) {
                    for o in options {
                        let val = o.get("value").and_then(|v| v.as_str()).unwrap_or("?");
                        let name = o.get("displayName").and_then(|v| v.as_str()).unwrap_or(val);
                        println!("    - {name} (value: {val})");
                    }
                }
            }
            has_models = true;
        }

        if let Some(ref ms) = model_state {
            let current = ms
                .get("currentModelId")
                .and_then(|v| v.as_str())
                .unwrap_or("(none)");
            println!("Models (unstable SessionModelState):");
            println!("  Current: {current}");
            if let Some(available) = ms.get("availableModels").and_then(|v| v.as_array()) {
                println!("  Available:");
                for m in available {
                    let id = m.get("modelId").and_then(|v| v.as_str()).unwrap_or("?");
                    let name = m.get("name").and_then(|v| v.as_str()).unwrap_or(id);
                    let desc = m.get("description").and_then(|v| v.as_str()).unwrap_or("");
                    if desc.is_empty() {
                        println!("    - {name} (id: {id})");
                    } else {
                        println!("    - {name} (id: {id}) — {desc}");
                    }
                }
            }
            has_models = true;
        }

        if !has_models {
            println!("No model information available from this agent.");
        }
    }

    client.shutdown().await;
    Ok(())
}

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
