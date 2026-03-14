//! Shared application state — Arc-wrapped, shared across all connections.

use std::sync::Arc;
use std::time::Instant;

use axum::extract::ws::Message as WsMessage;
use dashmap::DashMap;
use tokio::sync::{mpsc, Semaphore};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use sprout_audit::AuditService;
use sprout_auth::AuthService;
use sprout_db::Db;
use sprout_pubsub::PubSubManager;
use sprout_search::SearchService;
use sprout_workflow::WorkflowEngine;

use crate::api::tokens::MintRateLimiter;
use crate::config::Config;
use crate::subscription::SubscriptionRegistry;

/// Tracks active WebSocket connections and provides message routing by connection ID.
pub struct ConnectionManager {
    /// Map from connection ID to the sender half of the connection's outbound channel
    /// and the cancellation token for the connection.
    connections: DashMap<Uuid, (mpsc::Sender<WsMessage>, CancellationToken)>,
}

impl ConnectionManager {
    /// Creates a new, empty connection manager.
    pub fn new() -> Self {
        Self {
            connections: DashMap::new(),
        }
    }

    /// Registers a connection with its outbound sender and cancellation token.
    pub fn register(&self, conn_id: Uuid, tx: mpsc::Sender<WsMessage>, cancel: CancellationToken) {
        self.connections.insert(conn_id, (tx, cancel));
    }

    /// Removes a connection from the registry.
    pub fn deregister(&self, conn_id: Uuid) {
        self.connections.remove(&conn_id);
    }

    /// Sends a text message to the given connection.
    ///
    /// Returns `false` if the connection is gone or the buffer is full.
    /// On a full buffer, cancels the slow client's connection to prevent silent drops.
    pub fn send_to(&self, conn_id: Uuid, msg: String) -> bool {
        if let Some(entry) = self.connections.get(&conn_id) {
            let (tx, cancel) = entry.value();
            match tx.try_send(WsMessage::Text(msg.into())) {
                Ok(_) => true,
                Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                    tracing::warn!(conn_id = %conn_id, "fan-out: send buffer full — cancelling slow client");
                    cancel.cancel();
                    false
                }
                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                    tracing::debug!(conn_id = %conn_id, "fan-out: send channel closed");
                    false
                }
            }
        } else {
            false
        }
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared application state, cloned cheaply via inner `Arc` fields.
#[derive(Clone)]
pub struct AppState {
    /// Relay configuration.
    pub config: Arc<Config>,
    /// Database connection pool.
    pub db: Db,
    /// Audit event service.
    pub audit: Arc<AuditService>,
    /// Pub/sub manager for broadcasting events to subscribers.
    pub pubsub: Arc<PubSubManager>,
    /// Authentication service.
    pub auth: Arc<AuthService>,
    /// Full-text search service.
    pub search: Arc<SearchService>,
    /// Registry of active client subscriptions.
    pub sub_registry: Arc<SubscriptionRegistry>,
    /// Registry of active WebSocket connections.
    pub conn_manager: Arc<ConnectionManager>,
    /// Semaphore limiting total concurrent connections.
    pub conn_semaphore: Arc<Semaphore>,
    /// Semaphore limiting concurrent message handler tasks.
    pub handler_semaphore: Arc<Semaphore>,
    /// Workflow engine for background processing.
    pub workflow_engine: Arc<WorkflowEngine>,
    /// Relay signing keypair — used to sign system messages (kind 40099).
    pub relay_keypair: nostr::Keys,
    /// Rate limiter for `POST /api/tokens` — 5 mints per pubkey per hour.
    pub mint_rate_limiter: Arc<MintRateLimiter>,
    /// Debounce cache for `last_used_at` token updates — avoids a DB write on every request.
    /// Entries map token UUID → last time we wrote `last_used_at` to the DB.
    /// Resets on restart (acceptable — `last_used_at` is informational, not security-critical).
    pub last_used_cache: Arc<DashMap<Uuid, Instant>>,
    /// Recently-published event IDs for local-echo deduplication.
    /// Events fanned out in-process are added here; the Redis subscriber
    /// consumer skips them to avoid double delivery. Entries expire after
    /// 60 seconds via moka's TTL eviction — bounded regardless of subscriber health.
    pub local_event_ids: Arc<moka::sync::Cache<[u8; 32], ()>>,
}

impl AppState {
    /// Constructs `AppState` from its component services.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: Config,
        db: Db,
        audit: AuditService,
        pubsub: Arc<PubSubManager>,
        auth: AuthService,
        search: SearchService,
        workflow_engine: Arc<WorkflowEngine>,
        relay_keypair: nostr::Keys,
    ) -> Self {
        let max_connections = config.max_connections;
        let max_concurrent_handlers = config.max_concurrent_handlers;
        Self {
            config: Arc::new(config),
            db,
            audit: Arc::new(audit),
            pubsub,
            auth: Arc::new(auth),
            search: Arc::new(search),
            sub_registry: Arc::new(SubscriptionRegistry::new()),
            conn_manager: Arc::new(ConnectionManager::new()),
            conn_semaphore: Arc::new(Semaphore::new(max_connections)),
            handler_semaphore: Arc::new(Semaphore::new(max_concurrent_handlers)),
            workflow_engine,
            relay_keypair,
            mint_rate_limiter: Arc::new(MintRateLimiter::new()),
            last_used_cache: Arc::new(DashMap::new()),
            local_event_ids: Arc::new(
                moka::sync::Cache::builder()
                    .max_capacity(10_000)
                    .time_to_live(std::time::Duration::from_secs(60))
                    .build(),
            ),
        }
    }

    /// Record an event ID as locally-published for dedup.
    /// Called before Redis publish so the multi-node consumer can skip the echo.
    pub fn mark_local_event(&self, event_id: &nostr::EventId) {
        self.local_event_ids.insert(event_id.to_bytes(), ());
    }
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("relay_url", &self.config.relay_url)
            .field("max_connections", &self.config.max_connections)
            .finish()
    }
}
