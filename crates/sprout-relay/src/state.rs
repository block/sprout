//! Shared application state — Arc-wrapped, shared across all connections.

use std::sync::Arc;

use axum::extract::ws::Message as WsMessage;
use dashmap::DashMap;
use tokio::sync::{mpsc, Semaphore};
use uuid::Uuid;

use sprout_audit::AuditService;
use sprout_auth::AuthService;
use sprout_db::Db;
use sprout_pubsub::PubSubManager;
use sprout_search::SearchService;
use sprout_workflow::WorkflowEngine;

use crate::config::Config;
use crate::subscription::SubscriptionRegistry;

/// Tracks active WebSocket connections and provides message routing by connection ID.
pub struct ConnectionManager {
    /// Map from connection ID to the sender half of the connection's outbound channel.
    connections: DashMap<Uuid, mpsc::Sender<WsMessage>>,
}

impl ConnectionManager {
    /// Creates a new, empty connection manager.
    pub fn new() -> Self {
        Self {
            connections: DashMap::new(),
        }
    }

    /// Registers a connection with its outbound sender.
    pub fn register(&self, conn_id: Uuid, tx: mpsc::Sender<WsMessage>) {
        self.connections.insert(conn_id, tx);
    }

    /// Removes a connection from the registry.
    pub fn deregister(&self, conn_id: Uuid) {
        self.connections.remove(&conn_id);
    }

    /// Sends a text message to the given connection. Returns `false` if the connection is gone or the buffer is full.
    pub fn send_to(&self, conn_id: Uuid, msg: String) -> bool {
        if let Some(tx) = self.connections.get(&conn_id) {
            tx.try_send(WsMessage::Text(msg.into())).is_ok()
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
        }
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
