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
    let pubsub = Arc::new(
        PubSubManager::new(&config.redis_url, redis_pool)
            .await
            .map_err(|e| anyhow::anyhow!("PubSub init failed: {e}"))?,
    );
    info!("Redis pub/sub connected");

    // TODO: spawn pubsub.run_subscriber() for multi-node fan-out.
    // Currently no consumer calls subscribe_local(), so the subscriber
    // would process Redis messages into a broadcast channel with zero receivers.

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
        nostr::Keys::parse(hex).expect("invalid SPROUT_RELAY_PRIVATE_KEY")
    } else {
        let keys = nostr::Keys::generate();
        tracing::info!("Generated relay keypair: {}", keys.public_key().to_hex());
        keys
    };

    let state = Arc::new(AppState::new(
        config.clone(),
        db,
        audit,
        pubsub,
        auth,
        search,
        Arc::clone(&workflow_engine),
        relay_keypair,
    ));

    // Wire the action sink — must happen after AppState (which creates
    // sub_registry, conn_manager) and before the cron loop starts.
    let action_sink = Arc::new(sprout_relay::workflow_sink::RelayActionSink::new(&state));
    workflow_engine.set_action_sink(action_sink);

    // Start the cron loop AFTER the action sink is wired.
    let wf_cron = Arc::clone(&workflow_engine);
    tokio::spawn(async move { wf_cron.run().await });
    let router = build_router(Arc::clone(&state));

    let listener = tokio::net::TcpListener::bind(&config.bind_addr)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to bind {}: {e}", config.bind_addr))?;

    info!(addr = %config.bind_addr, "sprout-relay listening");

    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
    .map_err(|e| anyhow::anyhow!("Server error: {e}"))?;

    Ok(())
}
