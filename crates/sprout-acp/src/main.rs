#![deny(unsafe_code)]

mod acp;
mod config;
mod filter;
mod queue;
mod relay;

use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use nostr::ToBech32;
use tokio::time::timeout;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

use acp::{AcpClient, AcpError, EnvVar, McpServer, StopReason};
use config::{Config, DedupMode, SubscribeMode};
use filter::SubscriptionRule;
use queue::{EventQueue, QueuedEvent};
use relay::HarnessRelay;
use sprout_core::kind::{
    KIND_STREAM_MESSAGE, KIND_STREAM_REMINDER, KIND_WORKFLOW_APPROVAL_REQUESTED,
};

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

    // ── Step 1: Spawn ACP agent subprocess and initialize ─────────────────────
    let mut acp = spawn_and_init(&config).await?;

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

    // ── Step 3: Discover channels and build subscription rules ────────────────
    let channels = relay
        .discover_channels()
        .await
        .map_err(|e| anyhow::anyhow!("channel discovery error: {e}"))?;

    tracing::info!("discovered {} channel(s)", channels.len());

    // Build subscription rules from the configured mode.
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

    // ── Step 5: Main orchestration loop ──────────────────────────────────────
    let mut sessions: HashMap<Uuid, String> = HashMap::new();
    let dedup_mode = config.dedup_mode;
    let mut queue = EventQueue::new(dedup_mode);
    let mcp_servers = build_mcp_servers(&config);
    let turn_timeout = Duration::from_secs(config.turn_timeout_secs);

    loop {
        // Wait for the next relay event or shutdown signal.
        let sprout_event = tokio::select! {
            event = relay.next_event() => event,
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("shutting down (SIGINT/SIGTERM)");
                break;
            }
        };

        match sprout_event {
            Some(sprout_event) => {
                // Self-event filter: drop events authored by this agent.
                if config.ignore_self && sprout_event.event.pubkey.to_hex() == pubkey_hex {
                    tracing::debug!(
                        channel_id = %sprout_event.channel_id,
                        "dropping self-authored event"
                    );
                    continue;
                }

                // Rule match: find first matching rule.
                let matched = filter::match_event(
                    &sprout_event.event,
                    sprout_event.channel_id,
                    &rules,
                    &pubkey_hex,
                )
                .await;

                let prompt_tag = match matched {
                    Some(m) => m.prompt_tag,
                    None => {
                        tracing::debug!(
                            channel_id = %sprout_event.channel_id,
                            kind = sprout_event.event.kind.as_u16(),
                            "event matched no rule — dropping"
                        );
                        continue;
                    }
                };

                // Push event into the queue.
                queue.push(QueuedEvent {
                    channel_id: sprout_event.channel_id,
                    event: sprout_event.event,
                    received_at: std::time::Instant::now(),
                    prompt_tag,
                });

                // Try to flush and process batches.
                loop {
                    let batch = match queue.flush_next() {
                        Some(b) => b,
                        None => break,
                    };

                    let channel_id = batch.channel_id;
                    // Format prompt before potentially requeuing (borrows batch by ref).
                    let prompt_text = queue::format_prompt(&batch, config.system_prompt.as_deref());

                    // Get or create session for this channel.
                    let session_id = match get_or_create_session(
                        &mut sessions,
                        channel_id,
                        &mut acp,
                        &mcp_servers,
                        &config,
                    )
                    .await
                    {
                        Ok(id) => id,
                        Err(AcpError::AgentExited) => {
                            tracing::error!("agent exited during session setup — respawning");
                            sessions.clear();
                            match dedup_mode {
                                DedupMode::Queue => queue.requeue(batch),
                                DedupMode::Drop => { /* discard */ }
                            }
                            queue.mark_complete();
                            acp = respawn_agent(&config).await?;
                            break;
                        }
                        Err(e) => {
                            tracing::error!(
                                "failed to create session for channel {channel_id}: {e}"
                            );
                            match dedup_mode {
                                DedupMode::Queue => queue.requeue(batch),
                                DedupMode::Drop => { /* discard */ }
                            }
                            queue.mark_complete();
                            break;
                        }
                    };

                    tracing::info!(
                        "prompting agent for channel {channel_id} (session {session_id}, {} event(s))",
                        batch.events.len()
                    );

                    // Send prompt with turn timeout.
                    let prompt_result =
                        timeout(turn_timeout, acp.session_prompt(&session_id, &prompt_text)).await;

                    match prompt_result {
                        Ok(Ok(stop_reason)) => {
                            log_stop_reason(channel_id, &stop_reason);
                            queue.mark_complete();
                        }
                        Ok(Err(AcpError::AgentExited)) => {
                            tracing::error!("agent process exited — respawning");
                            sessions.clear();
                            match dedup_mode {
                                DedupMode::Queue => queue.requeue(batch),
                                DedupMode::Drop => { /* discard */ }
                            }
                            queue.mark_complete();
                            acp = respawn_agent(&config).await?;
                            break;
                        }
                        Ok(Err(e)) => {
                            tracing::error!("session_prompt error for channel {channel_id}: {e}");
                            match dedup_mode {
                                DedupMode::Queue => queue.requeue(batch),
                                DedupMode::Drop => { /* discard */ }
                            }
                            queue.mark_complete();
                            sessions.remove(&channel_id);
                            break;
                        }
                        Err(_elapsed) => {
                            tracing::warn!(
                                "turn timeout ({}s) for channel {channel_id} — cancelling",
                                config.turn_timeout_secs
                            );
                            match acp.cancel_with_cleanup(&session_id).await {
                                Ok(stop_reason) => {
                                    log_stop_reason(channel_id, &stop_reason);
                                }
                                Err(AcpError::AgentExited) => {
                                    tracing::error!("agent exited during cancel — respawning");
                                    sessions.clear();
                                    acp = respawn_agent(&config).await?;
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "cancel_with_cleanup error for channel {channel_id}: {e} — invalidating session"
                                    );
                                    sessions.remove(&channel_id);
                                }
                            }
                            queue.mark_complete();
                            break;
                        }
                    }
                }
            }
            None => {
                // Relay event stream ended — request background reconnect.
                // The background task handles the actual reconnection and
                // resubscription asynchronously; we just resume waiting for events.
                tracing::warn!("relay event stream ended — requesting reconnect");
                if let Err(e) = relay.reconnect().await {
                    tracing::error!("failed to send reconnect command: {e}");
                }
            }
        }
    }

    tracing::info!("sprout-acp stopped");
    Ok(())
}

// ── Helper: respawn agent after exit ─────────────────────────────────────────

async fn respawn_agent(config: &Config) -> Result<AcpClient> {
    match spawn_and_init(config).await {
        Ok(new_acp) => {
            tracing::info!("agent respawned successfully");
            Ok(new_acp)
        }
        Err(e) => {
            tracing::error!("failed to respawn agent: {e}");
            Err(e)
        }
    }
}

// ── Helper: spawn agent and initialize ───────────────────────────────────────

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

// ── Helper: get or create session for a channel ───────────────────────────────

/// Get or create an ACP session for the given channel.
///
/// If a session already exists, returns it immediately. Otherwise creates a new
/// session and — if `config.initial_message` is set — sends it as the first
/// prompt before returning. The initial-message turn counts against
/// `in_flight_channel`, so in `drop` mode events for this channel are dropped
/// while it runs.
async fn get_or_create_session(
    sessions: &mut HashMap<Uuid, String>,
    channel_id: Uuid,
    acp: &mut AcpClient,
    mcp_servers: &[McpServer],
    config: &Config,
) -> Result<String, AcpError> {
    if let Some(session_id) = sessions.get(&channel_id) {
        return Ok(session_id.clone());
    }

    let cwd = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("/"))
        .to_string_lossy()
        .to_string();

    let session_id = acp.session_new(&cwd, mcp_servers.to_vec()).await?;
    tracing::info!("created session {session_id} for channel {channel_id}");
    sessions.insert(channel_id, session_id.clone());

    // Send initial message if configured.
    if let Some(ref initial_message) = config.initial_message {
        tracing::info!("sending initial message to session {session_id} for channel {channel_id}");
        let turn_timeout = Duration::from_secs(config.turn_timeout_secs);
        let result = timeout(
            turn_timeout,
            acp.session_prompt(&session_id, initial_message),
        )
        .await;
        match result {
            Ok(Ok(stop_reason)) => {
                tracing::info!(
                    "initial message complete for channel {channel_id}: {stop_reason:?}"
                );
            }
            Ok(Err(e)) => {
                tracing::error!(
                    "initial message failed for channel {channel_id}: {e} — invalidating session"
                );
                sessions.remove(&channel_id);
                return Err(e);
            }
            Err(_elapsed) => {
                tracing::warn!(
                    "initial message timed out for channel {channel_id} — cancelling and invalidating session"
                );
                // Cancel the in-flight prompt to keep the NDJSON stream in sync,
                // matching the cleanup path used by the main event loop.
                match acp.cancel_with_cleanup(&session_id).await {
                    Ok(_) => {
                        // Agent is still alive — just this session timed out.
                        sessions.remove(&channel_id);
                        return Err(AcpError::Timeout);
                    }
                    Err(AcpError::AgentExited) => {
                        // Agent actually died during cancel.
                        sessions.remove(&channel_id);
                        return Err(AcpError::AgentExited);
                    }
                    Err(cancel_err) => {
                        tracing::error!("cancel_with_cleanup failed during initial message timeout: {cancel_err}");
                        sessions.remove(&channel_id);
                        return Err(AcpError::Timeout);
                    }
                }
            }
        }
    }

    Ok(session_id)
}

// ── Helper: build MCP server config from Config ───────────────────────────────

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

// ── Helper: log stop reason at appropriate level ──────────────────────────────

fn log_stop_reason(channel_id: Uuid, stop_reason: &StopReason) {
    match stop_reason {
        StopReason::EndTurn => {
            tracing::info!("turn complete for channel {channel_id}: end_turn");
        }
        StopReason::Cancelled => {
            tracing::warn!("turn cancelled for channel {channel_id}");
        }
        StopReason::MaxTokens => {
            tracing::warn!("turn hit max_tokens for channel {channel_id}");
        }
        StopReason::MaxTurnRequests => {
            tracing::warn!("turn hit max_turn_requests for channel {channel_id}");
        }
        StopReason::Refusal => {
            tracing::warn!("turn refused for channel {channel_id}");
        }
    }
}
