use std::sync::Arc;

use tracing::{error, info};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use sprout_audit::AuditService;
use sprout_auth::AuthService;
use sprout_db::{Db, DbConfig};
use sprout_pubsub::PubSubManager;
use sprout_search::{SearchConfig, SearchService};

use sprout_relay::{config::Config, router::build_router, state::AppState};
use sprout_workflow::WorkflowEngine;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env().add_directive("sprout_relay=info".parse()?))
        .init();

    info!("Starting sprout-relay");

    let config = Config::from_env().map_err(|e| {
        error!("Invalid configuration: {e}");
        anyhow::anyhow!("Configuration error: {e}")
    })?;
    info!(bind_addr = %config.bind_addr, relay_url = %config.relay_url, "Config loaded");

    let db_config = DbConfig {
        database_url: config.database_url.clone(),
        ..DbConfig::default()
    };
    let db = Db::new(&db_config).await.map_err(|e| {
        error!("Failed to connect to MySQL: {e}");
        anyhow::anyhow!("DB connection failed: {e}")
    })?;
    info!("MySQL connected");

    db.migrate().await.map_err(|e| {
        error!("Migration failed: {e}");
        anyhow::anyhow!("Migration failed: {e}")
    })?;
    info!("Migrations applied");

    if let Err(e) = db.ensure_future_partitions(3).await {
        error!("Failed to ensure partitions: {e}");
    }

    let audit_pool = sqlx::MySqlPool::connect(&config.database_url)
        .await
        .map_err(|e| anyhow::anyhow!("Audit DB connection failed: {e}"))?;
    let audit = AuditService::new(audit_pool);
    if let Err(e) = audit.ensure_schema().await {
        error!("Failed to ensure audit schema: {e}");
    }
    info!("Audit service ready");

    let redis_pool = {
        let cfg = deadpool_redis::Config::from_url(&config.redis_url);
        cfg.create_pool(Some(deadpool_redis::Runtime::Tokio1))
            .map_err(|e| anyhow::anyhow!("Redis pool creation failed: {e}"))?
    };
    let redis_health_pool = redis_pool.clone(); // cheap Arc clone — shared with readiness handler
    let pubsub = Arc::new(
        PubSubManager::new(&config.redis_url, redis_pool)
            .await
            .map_err(|e| anyhow::anyhow!("PubSub init failed: {e}"))?,
    );
    info!("Redis pub/sub connected");

    // Spawn Redis pub/sub subscriber for multi-node fan-out.
    // Events published by other relay instances are received here and
    // fanned out to local WebSocket subscribers.
    let pubsub_for_sub = Arc::clone(&pubsub);
    tokio::spawn(async move { pubsub_for_sub.run_subscriber().await });

    let auth = AuthService::new(config.auth.clone());

    let search_config = SearchConfig {
        url: config.typesense_url.clone(),
        api_key: config.typesense_key.clone(),
        collection: std::env::var("TYPESENSE_COLLECTION").unwrap_or_else(|_| "events".to_string()),
    };
    let search = SearchService::new(search_config);
    if let Err(e) = search.ensure_collection().await {
        error!("Typesense collection setup failed (non-fatal): {e}");
    }

    let workflow_config = sprout_workflow::WorkflowConfig::default();
    let workflow_engine = Arc::new(WorkflowEngine::new(db.clone(), workflow_config));

    let relay_keypair = if let Some(hex) = &config.relay_private_key {
        nostr::Keys::parse(hex)
            .map_err(|e| anyhow::anyhow!("invalid SPROUT_RELAY_PRIVATE_KEY: {e}"))?
    } else {
        let keys = nostr::Keys::generate();
        tracing::info!("Generated relay keypair: {}", keys.public_key().to_hex());
        keys
    };

    config.media.validate().map_err(|e| anyhow::anyhow!("invalid media config: {e}"))?;
    let media_storage = sprout_media::MediaStorage::new(&config.media)
        .map_err(|e| anyhow::anyhow!("failed to initialize media storage: {e}"))?;
    info!("Media storage connected");

    let state = Arc::new(AppState::new(
        config.clone(),
        db,
        redis_health_pool,
        audit,
        pubsub,
        auth,
        search,
        Arc::clone(&workflow_engine),
        relay_keypair,
        media_storage,
    ));

    // Wire the action sink — must happen after AppState (which creates
    // sub_registry, conn_manager) and before the cron loop starts.
    let action_sink = Arc::new(sprout_relay::workflow_sink::RelayActionSink::new(&state));
    workflow_engine.set_action_sink(action_sink);

    // Start the cron loop AFTER the action sink is wired.
    let wf_cron = Arc::clone(&workflow_engine);
    tokio::spawn(async move { wf_cron.run().await });

    // Multi-node fan-out consumer: receive events from Redis pub/sub
    // (published by other relay instances) and fan out to local WS subscribers.
    {
        let state_for_sub = Arc::clone(&state);
        let mut rx = state_for_sub.pubsub.subscribe_local();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(channel_event) => {
                        let stored = sprout_core::StoredEvent::new(
                            channel_event.event,
                            Some(channel_event.channel_id),
                        );

                        // Skip events that were already fanned out in-process (local echo).
                        // The cache has TTL-based eviction (60s) so entries are bounded
                        // regardless of subscriber health.
                        let event_id_bytes = stored.event.id.to_bytes();
                        if state_for_sub.local_event_ids.get(&event_id_bytes).is_some() {
                            state_for_sub.local_event_ids.invalidate(&event_id_bytes);
                            continue;
                        }

                        let matches = state_for_sub.sub_registry.fan_out(&stored);
                        if matches.is_empty() {
                            continue;
                        }

                        let event_json = match serde_json::to_string(&stored.event) {
                            Ok(json) => json,
                            Err(e) => {
                                tracing::error!(
                                    "Failed to serialize event for multi-node fan-out: {e}"
                                );
                                continue;
                            }
                        };
                        let mut drop_count = 0u32;
                        for (conn_id, sub_id) in &matches {
                            let msg = format!(r#"["EVENT","{}",{}]"#, sub_id, event_json);
                            if !state_for_sub.conn_manager.send_to(*conn_id, msg) {
                                drop_count += 1;
                            }
                        }
                        if drop_count > 0 {
                            tracing::warn!(
                                event_id = %stored.event.id.to_hex(),
                                drop_count,
                                "multi-node fan-out: {drop_count} connection(s) dropped"
                            );
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Multi-node fan-out lagged by {n} messages");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::error!("Multi-node fan-out broadcast channel closed");
                        break;
                    }
                }
            }
        });
    }

    let router = build_router(Arc::clone(&state));

    serve_dual(router, &config).await
}

/// Bind TCP (always) and optionally UDS, then run both concurrently.
/// If either listener exits, the function returns and the process exits.
async fn serve_dual(router: axum::Router, config: &Config) -> anyhow::Result<()> {
    let tcp_listener = tokio::net::TcpListener::bind(&config.bind_addr)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to bind {}: {e}", config.bind_addr))?;
    info!(addr = %config.bind_addr, "sprout-relay TCP listening");

    #[cfg(unix)]
    if let Some(ref uds_path) = config.uds_path {
        // Only remove stale socket files — refuse to delete non-socket paths.
        use std::os::unix::fs::FileTypeExt as _;
        match std::fs::symlink_metadata(uds_path) {
            Ok(meta) if meta.file_type().is_socket() => {
                let _ = std::fs::remove_file(uds_path);
            }
            Ok(_) => {
                return Err(anyhow::anyhow!(
                    "SPROUT_UDS_PATH {uds_path} exists but is not a socket — refusing to delete"
                ));
            }
            Err(_) => {} // Path doesn't exist — fine, we'll create it
        }
        let uds_listener = tokio::net::UnixListener::bind(uds_path)
            .map_err(|e| anyhow::anyhow!("Failed to bind UDS {uds_path}: {e}"))?;
        info!(path = %uds_path, "sprout-relay UDS listening");

        let router_uds = router.clone();
        tokio::select! {
            r = axum::serve(
                tcp_listener,
                router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            ) => r.map_err(|e| anyhow::anyhow!("TCP server error: {e}")),
            r = axum::serve(uds_listener, router_uds.into_make_service()) =>
                r.map_err(|e| anyhow::anyhow!("UDS server error: {e}")),
        }?;
        return Ok(());
    }

    #[cfg(not(unix))]
    if config.uds_path.is_some() {
        tracing::warn!(
            "SPROUT_UDS_PATH is set but UDS is not supported on this platform — \
             falling back to TCP only"
        );
    }

    // TCP-only path (non-unix, or unix without uds_path set).
    axum::serve(
        tcp_listener,
        router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
    .map_err(|e| anyhow::anyhow!("Server error: {e}"))?;

    Ok(())
}
