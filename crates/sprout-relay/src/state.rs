//! Shared application state — Arc-wrapped, shared across all connections.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Instant;

use axum::extract::ws::Message as WsMessage;
use dashmap::DashMap;
use tokio::sync::{mpsc, Semaphore};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use deadpool_redis;
use sprout_audit::AuditService;
use sprout_auth::AuthService;
use sprout_core::event::StoredEvent;
use sprout_db::Db;
use sprout_media::MediaStorage;
use sprout_pubsub::PubSubManager;
use sprout_search::SearchService;
use sprout_workflow::WorkflowEngine;

use crate::api::tokens::MintRateLimiter;
use crate::config::Config;
use crate::connection::SLOW_CLIENT_GRACE_LIMIT;
use crate::subscription::SubscriptionRegistry;

/// Per-connection entry in the connection manager.
struct ConnEntry {
    tx: mpsc::Sender<WsMessage>,
    cancel: CancellationToken,
    /// Shared with `ConnectionState` — both direct sends and fan-out
    /// broadcasts track the same consecutive-full counter.
    backpressure_count: Arc<AtomicU8>,
}

/// Tracks active WebSocket connections and provides message routing by connection ID.
pub struct ConnectionManager {
    connections: DashMap<Uuid, ConnEntry>,
}

impl ConnectionManager {
    /// Creates a new, empty connection manager.
    pub fn new() -> Self {
        Self {
            connections: DashMap::new(),
        }
    }

    /// Registers a connection with its outbound sender, cancellation token,
    /// and shared backpressure counter (same `Arc` as `ConnectionState`).
    pub fn register(
        &self,
        conn_id: Uuid,
        tx: mpsc::Sender<WsMessage>,
        cancel: CancellationToken,
        backpressure_count: Arc<AtomicU8>,
    ) {
        self.connections.insert(
            conn_id,
            ConnEntry {
                tx,
                cancel,
                backpressure_count,
            },
        );
    }

    /// Removes a connection from the registry.
    pub fn deregister(&self, conn_id: Uuid) {
        self.connections.remove(&conn_id);
    }

    /// Sends a text message to the given connection.
    ///
    /// Returns `false` if the connection is gone or the buffer is full.
    /// On sustained backpressure (>[`SLOW_CLIENT_GRACE_LIMIT`] consecutive full
    /// buffers), cancels the connection. Transient stalls get a warning only.
    pub fn send_to(&self, conn_id: Uuid, msg: String) -> bool {
        if let Some(entry) = self.connections.get(&conn_id) {
            let conn = entry.value();
            match conn.tx.try_send(WsMessage::Text(msg.into())) {
                Ok(_) => {
                    conn.backpressure_count.store(0, Ordering::Relaxed);
                    true
                }
                Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                    let count = conn.backpressure_count.fetch_add(1, Ordering::Relaxed) + 1;
                    if count >= SLOW_CLIENT_GRACE_LIMIT {
                        tracing::warn!(conn_id = %conn_id, count, "fan-out: sustained backpressure — cancelling slow client");
                        metrics::counter!("sprout_ws_backpressure_disconnects_total").increment(1);
                        conn.cancel.cancel();
                    } else {
                        tracing::warn!(conn_id = %conn_id, count, grace = SLOW_CLIENT_GRACE_LIMIT, "fan-out: send buffer full — grace {count}/{SLOW_CLIENT_GRACE_LIMIT}");
                    }
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
    /// Redis pool for readiness health checks.
    pub redis_pool: deadpool_redis::Pool,
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
    /// Membership cache: (channel_id, pubkey_bytes) → is_member.
    /// Short TTL (10s) — membership changes are rare but must propagate.
    pub membership_cache: Arc<moka::sync::Cache<(Uuid, Vec<u8>), bool>>,
    /// Accessible channel IDs cache: pubkey_bytes → Vec<Uuid>.
    /// 5s TTL — must be short because E2E tests create and join channels rapidly.
    pub accessible_channels_cache: Arc<moka::sync::Cache<Vec<u8>, Vec<Uuid>>>,
    /// Bounded channel for search indexing — prevents OOM if Typesense is slow/down.
    /// Capacity 1000: at ~1KB/event that's ~1MB of backlog before we start dropping.
    pub search_index_tx: mpsc::Sender<StoredEvent>,
    /// Media storage client (S3/MinIO).
    pub media_storage: Arc<MediaStorage>,
    /// Set to `true` on SIGTERM — readiness probe returns 503.
    pub shutting_down: Arc<AtomicBool>,
    /// Process start time — used by `/_status` endpoint.
    pub started_at: Instant,
}

impl AppState {
    /// Constructs `AppState` from its component services.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: Config,
        db: Db,
        redis_pool: deadpool_redis::Pool,
        audit: AuditService,
        pubsub: Arc<PubSubManager>,
        auth: AuthService,
        search: SearchService,
        workflow_engine: Arc<WorkflowEngine>,
        relay_keypair: nostr::Keys,
        media_storage: MediaStorage,
    ) -> Self {
        let max_connections = config.max_connections;
        let max_concurrent_handlers = config.max_concurrent_handlers;
        let search_arc = Arc::new(search);

        let (search_index_tx, mut search_index_rx) = mpsc::channel::<StoredEvent>(1000);
        let search_for_worker = Arc::clone(&search_arc);
        tokio::spawn(async move {
            while let Some(stored_event) = search_index_rx.recv().await {
                let t = std::time::Instant::now();
                match search_for_worker.index_event(&stored_event).await {
                    Ok(()) => {
                        metrics::histogram!("sprout_search_index_seconds")
                            .record(t.elapsed().as_secs_f64());
                    }
                    Err(e) => {
                        metrics::counter!("sprout_search_index_errors_total").increment(1);
                        tracing::error!(
                            event_id = %stored_event.event.id.to_hex(),
                            "Search index failed: {e}"
                        );
                    }
                }
            }
            tracing::warn!("search index worker exited (expected on shutdown)");
        });

        Self {
            config: Arc::new(config),
            db,
            redis_pool,
            audit: Arc::new(audit),
            pubsub,
            auth: Arc::new(auth),
            search: search_arc,
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
            membership_cache: Arc::new(
                moka::sync::Cache::builder()
                    .max_capacity(10_000)
                    .time_to_live(std::time::Duration::from_secs(10))
                    .build(),
            ),
            accessible_channels_cache: Arc::new(
                moka::sync::Cache::builder()
                    .max_capacity(5_000)
                    .time_to_live(std::time::Duration::from_secs(5))
                    .build(),
            ),
            search_index_tx,
            media_storage: Arc::new(media_storage),
            shutting_down: Arc::new(AtomicBool::new(false)),
            started_at: Instant::now(),
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

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::{AuthState, ConnectionState};
    use std::collections::HashMap;
    use tokio::sync::{Mutex, RwLock};

    /// Helper: create a ConnectionManager with one registered connection.
    /// Returns (manager, conn_id, receiver, cancel, shared_backpressure_count).
    fn setup_conn(
        buffer_size: usize,
    ) -> (
        ConnectionManager,
        Uuid,
        mpsc::Receiver<WsMessage>,
        CancellationToken,
        Arc<AtomicU8>,
    ) {
        let mgr = ConnectionManager::new();
        let conn_id = Uuid::new_v4();
        let (tx, rx) = mpsc::channel(buffer_size);
        let cancel = CancellationToken::new();
        let bp = Arc::new(AtomicU8::new(0));
        mgr.register(conn_id, tx, cancel.clone(), Arc::clone(&bp));
        (mgr, conn_id, rx, cancel, bp)
    }

    #[test]
    fn send_to_resets_grace_counter_on_success() {
        let (mgr, id, _rx, _cancel, bp) = setup_conn(16);
        // Simulate prior backpressure.
        bp.store(2, Ordering::Relaxed);
        assert!(mgr.send_to(id, "hello".into()));
        assert_eq!(
            bp.load(Ordering::Relaxed),
            0,
            "successful send should reset counter"
        );
    }

    #[test]
    fn send_to_increments_grace_counter_on_full() {
        // Buffer size 1 — fill it, then the next send is Full.
        let (mgr, id, _rx, cancel, bp) = setup_conn(1);
        assert!(mgr.send_to(id, "fill".into()));
        // Buffer is now full.
        assert!(!mgr.send_to(id, "overflow-1".into()));
        assert_eq!(bp.load(Ordering::Relaxed), 1, "first overflow → count=1");
        assert!(
            !cancel.is_cancelled(),
            "should not cancel on first overflow"
        );

        assert!(!mgr.send_to(id, "overflow-2".into()));
        assert_eq!(bp.load(Ordering::Relaxed), 2);
        assert!(
            !cancel.is_cancelled(),
            "should not cancel on second overflow"
        );
    }

    #[test]
    fn send_to_cancels_after_grace_limit() {
        let (mgr, id, _rx, cancel, _bp) = setup_conn(1);
        assert!(mgr.send_to(id, "fill".into()));
        // Exhaust grace: 3 consecutive Full events.
        for _ in 0..SLOW_CLIENT_GRACE_LIMIT {
            mgr.send_to(id, "overflow".into());
        }
        assert!(
            cancel.is_cancelled(),
            "should cancel after SLOW_CLIENT_GRACE_LIMIT overflows"
        );
    }

    #[test]
    fn shared_counter_between_direct_and_fanout() {
        // Verify that ConnectionState::send() and ConnectionManager::send_to()
        // share the same backpressure counter via Arc<AtomicU8>.
        let conn_id = Uuid::new_v4();
        let (tx, _rx) = mpsc::channel(1);
        let (ctrl_tx, _ctrl_rx) = mpsc::channel(8);
        let cancel = CancellationToken::new();
        let bp = Arc::new(AtomicU8::new(0));

        let conn = ConnectionState {
            conn_id,
            remote_addr: "127.0.0.1:1234".parse().unwrap(),
            auth_state: RwLock::new(AuthState::Failed),
            subscriptions: Mutex::new(HashMap::new()),
            send_tx: tx.clone(),
            ctrl_tx,
            cancel: cancel.clone(),
            backpressure_count: Arc::clone(&bp),
        };

        let mgr = ConnectionManager::new();
        mgr.register(conn_id, tx, cancel.clone(), Arc::clone(&bp));

        // Fill the buffer via direct send.
        assert!(conn.send("fill".into()));
        // Overflow via fan-out.
        assert!(!mgr.send_to(conn_id, "overflow-fanout".into()));
        assert_eq!(
            bp.load(Ordering::Relaxed),
            1,
            "fan-out overflow increments shared counter"
        );
        // Overflow via direct send.
        assert!(!conn.send("overflow-direct".into()));
        assert_eq!(
            bp.load(Ordering::Relaxed),
            2,
            "direct overflow increments same counter"
        );
        // One more fan-out overflow → should cancel (3 consecutive).
        mgr.send_to(conn_id, "overflow-final".into());
        assert!(
            cancel.is_cancelled(),
            "shared counter reached limit via mixed path"
        );
    }
}
