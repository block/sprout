#![deny(unsafe_code)]

mod acp;
mod config;
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
use config::Config;
use queue::{EventQueue, QueuedEvent};
use relay::HarnessRelay;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("sprout_acp=info")),
        )
        .compact()
        .init();

    let config = Config::from_env().map_err(|e| anyhow::anyhow!("configuration error: {e}"))?;
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

    // ── Step 3: Discover channels and subscribe ───────────────────────────────
    let channels = relay
        .discover_channels()
        .await
        .map_err(|e| anyhow::anyhow!("channel discovery error: {e}"))?;

    tracing::info!("discovered {} channel(s)", channels.len());

    for channel_id in &channels {
        if let Err(e) = relay.subscribe_channel(*channel_id).await {
            tracing::warn!("failed to subscribe to channel {channel_id}: {e}");
        } else {
            tracing::info!("subscribed to channel {channel_id}");
        }
    }

    // ── Step 4: Main orchestration loop ──────────────────────────────────────
    let mut sessions: HashMap<Uuid, String> = HashMap::new();
    let mut queue = EventQueue::new();
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
                // Push event into the queue.
                queue.push(QueuedEvent {
                    channel_id: sprout_event.channel_id,
                    event: sprout_event.event,
                    received_at: std::time::Instant::now(),
                });

                // Try to flush and process batches.
                loop {
                    let batch = match queue.flush_next() {
                        Some(b) => b,
                        None => break,
                    };

                    let channel_id = batch.channel_id;
                    // Format prompt before potentially requeuing (borrows batch by ref).
                    let prompt_text = queue::format_prompt(&batch);

                    // Get or create session for this channel.
                    let session_id = match get_or_create_session(
                        &mut sessions,
                        channel_id,
                        &mut acp,
                        &mcp_servers,
                    )
                    .await
                    {
                        Ok(id) => id,
                        Err(e) => {
                            tracing::error!(
                                "failed to create session for channel {channel_id}: {e}"
                            );
                            queue.requeue(batch);
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
                            queue.requeue(batch);
                            queue.mark_complete();
                            acp = respawn_agent(&config).await?;
                            break;
                        }
                        Ok(Err(e)) => {
                            tracing::error!("session_prompt error for channel {channel_id}: {e}");
                            queue.requeue(batch);
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
                                    tracing::error!("cancel_with_cleanup error: {e}");
                                }
                            }
                            queue.mark_complete();
                            break;
                        }
                    }
                }
            }
            None => {
                // Relay connection lost — reconnect.
                tracing::warn!("relay connection lost — reconnecting");
                loop {
                    match relay.reconnect().await {
                        Ok(()) => {
                            tracing::info!("relay reconnected");
                            break;
                        }
                        Err(e) => {
                            tracing::error!("relay reconnect failed: {e} — retrying in 5s");
                            tokio::time::sleep(Duration::from_secs(5)).await;
                        }
                    }
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

async fn get_or_create_session(
    sessions: &mut HashMap<Uuid, String>,
    channel_id: Uuid,
    acp: &mut AcpClient,
    mcp_servers: &[McpServer],
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
                    value: config.keys.secret_key().to_bech32().unwrap_or_default(),
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
