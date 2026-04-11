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
use config::{Config, DedupMode, ModelsArgs, MultipleEventHandling, RespondTo, SubscribeMode};
use filter::SubscriptionRule;
use futures_util::FutureExt;
use nostr::ToBech32;
use pool::{
    AgentPool, OwnedAgent, PromptContext, PromptOutcome, PromptResult, PromptSource, SessionState,
};
use queue::{EventQueue, QueuedEvent};
use relay::HarnessRelay;
use sprout_core::kind::{
    KIND_MEMBER_ADDED_NOTIFICATION, KIND_MEMBER_REMOVED_NOTIFICATION, KIND_STREAM_MESSAGE,
    KIND_STREAM_REMINDER, KIND_WORKFLOW_APPROVAL_REQUESTED,
};
use tokio::sync::{mpsc, watch};
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

// ── Owner cache ───────────────────────────────────────────────────────────────

/// Lazy-resolving cache for the agent's owner pubkey.
///
/// Replaces the bare `Option<String>` that previously lived in the event loop.
/// On success, the owner pubkey is cached for the process lifetime (owner
/// changes require a harness restart). On failure/miss, retries after 60s
/// to avoid hammering the API.
struct OwnerCache {
    pubkey: Option<String>,
    last_attempt: Option<std::time::Instant>,
}

/// How long to cache a failed owner lookup before retrying.
const OWNER_CACHE_TTL: Duration = Duration::from_secs(60);

impl OwnerCache {
    fn new(initial: Option<String>) -> Self {
        let last_attempt = if initial.is_some() {
            Some(std::time::Instant::now())
        } else {
            None
        };
        Self {
            pubkey: initial,
            last_attempt,
        }
    }

    /// Return the cached owner pubkey, or attempt a lazy resolution if stale.
    async fn get_or_resolve(
        &mut self,
        rest_client: &relay::RestClient,
        agent_pubkey_hex: &str,
    ) -> Option<&str> {
        if self.pubkey.is_some() {
            return self.pubkey.as_deref();
        }
        let stale = self
            .last_attempt
            .map(|t| t.elapsed() >= OWNER_CACHE_TTL)
            .unwrap_or(true);
        if !stale {
            return None;
        }
        self.last_attempt = Some(std::time::Instant::now());
        let profile_url = format!("/api/users/{agent_pubkey_hex}/profile");
        match rest_client.get_json(&profile_url).await {
            Ok(v) => {
                // Normalize to lowercase hex for consistent comparison with
                // nostr PublicKey::to_hex() and validated allowlist entries.
                self.pubkey = v
                    .get("agent_owner_pubkey")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_ascii_lowercase());
                if let Some(ref o) = self.pubkey {
                    tracing::info!("lazy owner resolution succeeded: {o}");
                }
            }
            Err(e) => {
                tracing::warn!("lazy owner lookup failed: {e}");
            }
        }
        self.pubkey.as_deref()
    }
}

// ── Sibling cache ─────────────────────────────────────────────────────────────

/// Result of looking up an author's owner via the REST API.
#[derive(Debug, Clone)]
enum SiblingLookup {
    /// Profile resolved; contains the author's `agent_owner_pubkey` (if any),
    /// normalized to lowercase hex.
    Resolved(Option<String>),
    /// REST call failed — treat as "not a sibling" (fail-closed).
    Failed,
}

/// Cache of author → owner lookups for the sibling author gate.
///
/// When `--respond-to=owner-only`, the harness accepts events from the owner
/// AND from any pubkey whose `agent_owner_pubkey` matches the owner (siblings).
/// This cache avoids hitting the REST API on every event from a known author.
///
/// TTL is derived at **read time**: a cached `Resolved(Some(owner))` that
/// matches the expected owner uses `SIBLING_CACHE_HIT_TTL` (5 min); all other
/// results use `SIBLING_CACHE_MISS_TTL` (1 min). This is correct even if the
/// agent owner changes (it doesn't — `OwnerCache` is process-stable — but the
/// design doesn't depend on that).
struct SiblingCache {
    /// author_hex → (lookup_result, resolved_at)
    entries: HashMap<String, (SiblingLookup, std::time::Instant)>,
}

/// TTL for a cached sibling match (Resolved(Some(owner)) where owner == expected).
const SIBLING_CACHE_HIT_TTL: Duration = Duration::from_secs(300);
/// TTL for a cached miss/different-owner/failure.
const SIBLING_CACHE_MISS_TTL: Duration = Duration::from_secs(60);
/// Maximum entries before oldest-eviction.
const SIBLING_CACHE_MAX_ENTRIES: usize = 256;

impl SiblingCache {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Record a lookup result for `author_hex`. Pure cache mutation — no I/O.
    ///
    /// Normalizes any owner pubkey inside `Resolved(Some(_))` to lowercase hex
    /// so callers of `check()` get consistent comparisons regardless of API
    /// casing. Evicts the oldest entry when at capacity.
    fn record(&mut self, author_hex: String, result: SiblingLookup) {
        // Normalize before caching.
        let normalized = match result {
            SiblingLookup::Resolved(Some(owner)) => {
                SiblingLookup::Resolved(Some(owner.to_ascii_lowercase()))
            }
            other => other,
        };

        if self.entries.len() >= SIBLING_CACHE_MAX_ENTRIES
            && !self.entries.contains_key(&author_hex)
        {
            // Evict oldest entry by resolved_at.
            if let Some(oldest_key) = self
                .entries
                .iter()
                .min_by_key(|(_, (_, ts))| *ts)
                .map(|(k, _)| k.clone())
            {
                self.entries.remove(&oldest_key);
            }
        }

        self.entries
            .insert(author_hex, (normalized, std::time::Instant::now()));
    }

    /// Check if a cached entry exists and is fresh for the given expected owner.
    ///
    /// Returns `Some(true)` if the author is a confirmed sibling (same owner),
    /// `Some(false)` if confirmed non-sibling, or `None` if the cache entry is
    /// missing or stale (caller should fetch).
    fn check(&self, author_hex: &str, expected_owner_hex: &str) -> Option<bool> {
        let (lookup, resolved_at) = self.entries.get(author_hex)?;

        let is_match = matches!(
            lookup,
            SiblingLookup::Resolved(Some(ref o)) if o == expected_owner_hex
        );

        let ttl = if is_match {
            SIBLING_CACHE_HIT_TTL
        } else {
            SIBLING_CACHE_MISS_TTL
        };

        if resolved_at.elapsed() >= ttl {
            return None; // stale
        }

        Some(is_match)
    }

    /// Full lookup: check cache, fetch if needed, record result.
    async fn is_sibling(
        &mut self,
        rest_client: &relay::RestClient,
        author_hex: &str,
        expected_owner_hex: &str,
    ) -> bool {
        if let Some(result) = self.check(author_hex, expected_owner_hex) {
            return result;
        }

        let lookup = Self::fetch_owner(rest_client, author_hex).await;
        // Note: fetch_owner() already lowercases, and record() normalizes too,
        // so no additional to_ascii_lowercase() needed here.
        let is_match = matches!(
            &lookup,
            SiblingLookup::Resolved(Some(ref o)) if o == expected_owner_hex
        );
        self.record(author_hex.to_owned(), lookup);
        is_match
    }

    /// Fetch an author's owner from the REST API.
    async fn fetch_owner(rest_client: &relay::RestClient, author_hex: &str) -> SiblingLookup {
        let url = format!("/api/users/{author_hex}/profile");
        match rest_client.get_json(&url).await {
            Ok(v) => {
                let owner = v
                    .get("agent_owner_pubkey")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_ascii_lowercase());
                tracing::debug!(
                    author = author_hex,
                    owner = ?owner,
                    "sibling cache: resolved author owner"
                );
                SiblingLookup::Resolved(owner)
            }
            Err(e) => {
                tracing::warn!(
                    author = author_hex,
                    error = %e,
                    "sibling cache: REST lookup failed — treating as non-sibling"
                );
                SiblingLookup::Failed
            }
        }
    }
}

/// Check if `author` is the owner or a sibling (shares the same owner).
///
/// Used by the `OwnerOnly` author gate mode. The owner is the direct match;
/// siblings are other pubkeys whose `agent_owner_pubkey` equals the owner.
async fn is_owner_or_sibling(
    author: &str,
    owner_cache: &mut OwnerCache,
    sibling_cache: &mut SiblingCache,
    rest_client: &relay::RestClient,
    agent_pubkey_hex: &str,
) -> bool {
    let owner = owner_cache
        .get_or_resolve(rest_client, agent_pubkey_hex)
        .await;
    match owner {
        Some(o) if author == o => true, // direct owner match
        Some(o) => {
            // Check if author is a sibling — another agent with the same owner.
            // Need to copy `o` because `owner_cache` borrows are released.
            let o = o.to_owned();
            sibling_cache.is_sibling(rest_client, author, &o).await
        }
        None => false, // no owner resolved — fail closed
    }
}

/// Maximum crashes in a 60-second window before a slot's circuit opens.
const CIRCUIT_BREAKER_THRESHOLD: usize = 3;
/// Window for circuit-breaker crash counting.
const CIRCUIT_BREAKER_WINDOW: Duration = Duration::from_secs(60);
/// Cooldown before a tripped circuit breaker allows a probe respawn.
const CIRCUIT_BREAKER_COOLDOWN: Duration = Duration::from_secs(300); // 5 minutes
/// Base backoff delay for respawn (doubles per recent crash, capped at 30s).
const RESPAWN_BASE_DELAY: Duration = Duration::from_secs(1);
/// Maximum respawn backoff delay.
const RESPAWN_MAX_DELAY: Duration = Duration::from_secs(30);

/// Per-slot circuit breaker state.
///
/// `crash_times` holds timestamps of recent crashes within `CIRCUIT_BREAKER_WINDOW`.
/// `open_until` is set when the threshold is hit; the circuit stays open until that
/// instant, then allows one probe respawn (half-open). If the probe crashes, the
/// circuit re-opens for another `CIRCUIT_BREAKER_COOLDOWN` period.
///
/// All state transitions go through methods on this struct — callers never
/// manipulate `crash_times` or `open_until` directly.
struct SlotCircuit {
    crash_times: Vec<std::time::Instant>,
    open_until: Option<std::time::Instant>,
    /// True while a background respawn/refill task is in flight for this slot.
    /// Prevents duplicate spawns from maintenance ticks that fire before the
    /// previous spawn_and_init completes.
    respawn_in_flight: bool,
}

/// Result of [`SlotCircuit::record_crash`].
enum CrashVerdict {
    /// Respawn is allowed after sleeping for this duration (jittered backoff).
    Respawn(Duration),
    /// Circuit is open — do not respawn.
    CircuitOpen,
    /// Circuit was open but cooldown has elapsed — one probe respawn is allowed
    /// (no backoff sleep). If the probe crashes, the next `record_crash` will
    /// immediately re-open the circuit.
    HalfOpenProbe,
}

impl SlotCircuit {
    /// Record a crash and decide whether to respawn.
    ///
    /// This is the **single canonical path** for all crash → respawn decisions.
    /// Called by `respawn_agent_into`, `recover_panicked_agent`, and slot refill.
    fn record_crash(&mut self) -> CrashVerdict {
        let now = std::time::Instant::now();

        // Half-open: cooldown elapsed → allow one probe.
        if let Some(open_until) = self.open_until {
            if now >= open_until {
                // Pre-seed crash_times to threshold-1 so that if the probe
                // itself crashes on the *next* call, the threshold is hit
                // immediately and the circuit re-opens. This implements a
                // "prove stability for one full window" policy.
                self.crash_times.clear();
                for _ in 0..(CIRCUIT_BREAKER_THRESHOLD - 1) {
                    self.crash_times.push(now);
                }
                self.open_until = None;
                return CrashVerdict::HalfOpenProbe;
            } else {
                return CrashVerdict::CircuitOpen;
            }
        }

        // Record this crash and prune old entries.
        self.crash_times.push(now);
        self.crash_times
            .retain(|&t| now.duration_since(t) < CIRCUIT_BREAKER_WINDOW);

        let recent = self.crash_times.len();

        if recent >= CIRCUIT_BREAKER_THRESHOLD {
            self.open_until = Some(now + CIRCUIT_BREAKER_COOLDOWN);
            return CrashVerdict::CircuitOpen;
        }

        // Exponential backoff: 1s * 2^(recent-1), capped at 30s, with ±20% jitter.
        let base = RESPAWN_BASE_DELAY.saturating_mul(1u32 << (recent - 1).min(5));
        let capped = base.min(RESPAWN_MAX_DELAY);
        let jitter = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos() as f64)
            / 1_000_000_000.0; // 0.0..1.0
        let factor = 0.8 + jitter * 0.4; // 0.8..1.2
        CrashVerdict::Respawn(capped.mul_f64(factor))
    }

    /// Mark a spawn failure — opens the circuit so the slot isn't retried
    /// on every heartbeat tick. Uses fresh `Instant::now()` so spawn latency
    /// doesn't shorten the effective cooldown.
    fn mark_spawn_failed(&mut self) {
        self.open_until = Some(std::time::Instant::now() + CIRCUIT_BREAKER_COOLDOWN);
    }

    /// Check if an empty slot can be refilled. Unlike `record_crash`, this
    /// does NOT record a new crash — it only checks whether the circuit
    /// allows a respawn attempt.
    ///
    /// Returns `true` if respawn is allowed. For half-open probes, pre-seeds
    /// crash_times so the next crash re-opens immediately. For normal refills
    /// (no circuit was ever opened), crash history is preserved so the breaker
    /// can still trip if the refilled agent crashes quickly.
    fn can_refill(&mut self) -> bool {
        let now = std::time::Instant::now();
        match self.open_until {
            Some(open_until) => {
                if now >= open_until {
                    // Half-open probe: pre-seed crash_times.
                    self.crash_times.clear();
                    for _ in 0..(CIRCUIT_BREAKER_THRESHOLD - 1) {
                        self.crash_times.push(now);
                    }
                    self.open_until = None;
                    true
                } else {
                    false // cooldown not elapsed
                }
            }
            None => true, // no circuit open — normal refill, preserve crash history
        }
    }
}

/// True if any slot has a respawn task in flight. Used to prevent premature
/// "all agents dead" exits — a respawning agent may succeed in seconds.
fn any_respawn_in_flight(crash_history: &[SlotCircuit]) -> bool {
    crash_history.iter().any(|s| s.respawn_in_flight)
}

/// Result of a background respawn task.
struct RespawnResult {
    index: usize,
    result: Result<AcpClient>,
}

/// RAII guard that ensures a `RespawnResult` is sent even if the task panics.
/// Without this, a panicked respawn task would leave `respawn_in_flight = true`
/// permanently, silently losing the slot forever.
struct RespawnGuard {
    index: usize,
    tx: mpsc::Sender<RespawnResult>,
    sent: bool,
}

impl RespawnGuard {
    fn new(index: usize, tx: mpsc::Sender<RespawnResult>) -> Self {
        Self {
            index,
            tx,
            sent: false,
        }
    }

    /// Send the result and disarm the guard. Uses `try_send` (sync) so there
    /// is no await boundary between marking `sent` and actually enqueueing —
    /// cancellation cannot slip between the two.
    fn send(mut self, result: Result<AcpClient>) {
        // Invariant: try_send succeeds because the channel capacity equals the
        // slot count, and respawn_in_flight guarantees at most one outstanding
        // result per slot. If this ever fails, the channel sizing or the
        // respawn_in_flight guard has drifted — that's a bug, not a transient.
        match self.tx.try_send(RespawnResult {
            index: self.index,
            result,
        }) {
            Ok(()) => self.sent = true,
            Err(e) => {
                tracing::error!(
                    agent = self.index,
                    "respawn result channel full or closed: {e}"
                );
                // Drop will fire and send a failure result as fallback.
            }
        }
    }
}

impl Drop for RespawnGuard {
    fn drop(&mut self) {
        if !self.sent {
            tracing::error!(
                agent = self.index,
                "respawn task exited without sending result — sending failure"
            );
            // Best-effort: try_send in Drop (can't await).
            let _ = self.tx.try_send(RespawnResult {
                index: self.index,
                result: Err(anyhow::anyhow!("respawn task panicked or was cancelled")),
            });
        }
    }
}

// ── Finding #16: propagate_legacy_env_vars before tokio runtime ───────────────
//
// Sync env-var propagation must run before the tokio runtime starts so that
// any child processes inherit the correct environment. This must happen in the
// sync entry point — `std::env::set_var` is only safe before tokio spawns
// worker threads (Rust 2024 edition safety requirement).

fn main() -> Result<()> {
    config::propagate_legacy_env_vars();
    tokio_main()
}

#[tokio::main]
async fn tokio_main() -> Result<()> {
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
    //
    // Finding #10: one agent failing to start must not kill the whole pool.
    // We attempt each spawn under a 60-second timeout; failures are logged and
    // skipped. If ALL agents fail we return an error. A partial pool is valid —
    // the harness continues with reduced capacity and logs a warning.
    let mut agent_slots: Vec<Option<OwnedAgent>> = Vec::with_capacity(config.agents as usize);
    for i in 0..config.agents as usize {
        // Spawn OUTSIDE the timeout so we always own the child for cleanup.
        // This matches the run_models pattern and prevents zombie leaks on
        // init timeout (the cancelled future would drop the AcpClient via
        // Drop which is best-effort only).
        let spawn_result = AcpClient::spawn(
            &config.agent_command,
            &config.agent_args,
            &config.persona_env_vars,
        )
        .await;
        match spawn_result {
            Ok(mut acp) => {
                match tokio::time::timeout(Duration::from_secs(60), acp.initialize()).await {
                    Ok(Ok(init_result)) => {
                        tracing::info!(agent = i, "agent initialized: {init_result}");
                        agent_slots.push(Some(OwnedAgent {
                            index: i,
                            acp,
                            state: SessionState::default(),
                            model_capabilities: None,
                            desired_model: config.model.clone(),
                        }));
                    }
                    Ok(Err(e)) => {
                        tracing::error!(agent = i, "agent initialize failed: {e}");
                        acp.shutdown().await;
                        agent_slots.push(None);
                    }
                    Err(_) => {
                        tracing::error!(agent = i, "agent timed out during init (60s)");
                        acp.shutdown().await;
                        agent_slots.push(None);
                    }
                }
            }
            Err(e) => {
                tracing::error!(agent = i, "agent failed to spawn: {e}");
                agent_slots.push(None);
            }
        }
    }
    let live_count = agent_slots.iter().filter(|s| s.is_some()).count();
    if live_count == 0 {
        return Err(anyhow::anyhow!(
            "all {} agents failed to start — cannot continue",
            config.agents
        ));
    }
    if live_count < config.agents as usize {
        tracing::warn!(
            "started {}/{} agents — continuing with reduced pool",
            live_count,
            config.agents
        );
    }
    tracing::info!("agent_pool_ready agents={}", live_count);
    let mut pool = AgentPool::from_slots(agent_slots);

    // ── Step 2: Connect to Sprout relay ──────────────────────────────────────
    //
    // Finding #22: capture a startup watermark BEFORE connecting to the relay.
    // This timestamp is used for membership notification replay (via
    // startup_watermark) and as the initial subscribe_since for channels
    // discovered at startup. The Subscribe handler falls back to
    // subscribe_since when last_seen is None, closing the blind spot
    // between "agents ready" and "first REQ sent".
    let startup_watermark: u64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let pubkey_hex = config.keys.public_key().to_hex();
    let mut relay = HarnessRelay::connect(
        &config.relay_url,
        &config.keys,
        config.api_token.as_deref(),
        &pubkey_hex,
    )
    .await
    .map_err(|e| anyhow::anyhow!("relay connect error: {e}"))?;

    // Finding #22: tell the relay background task the watermark so it can use
    // `since = watermark - 5s` on the first REQ instead of `since=now`.
    // Best-effort: a failure here is non-fatal (we just lose the startup window
    // protection, which is the same as the pre-fix behaviour).
    if let Err(e) = relay.set_startup_watermark(startup_watermark).await {
        tracing::warn!("failed to set startup watermark: {e}");
    }

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

    // ── Step 2d: Query agent owner ──────────────────────────────────────────
    // Owner lookup is used by !shutdown and the inbound author gate.
    // Try at startup; OwnerCache retries lazily on cache miss.
    let startup_owner: Option<String> = {
        let profile_url = format!("/api/users/{pubkey_hex}/profile");
        match rest_client_for_presence.get_json(&profile_url).await {
            Ok(v) => v
                .get("agent_owner_pubkey")
                .and_then(|v| v.as_str())
                .map(|s| s.to_ascii_lowercase()),
            Err(e) => {
                tracing::warn!("startup owner lookup failed (will retry lazily): {e}");
                None
            }
        }
    };
    if let Some(ref owner) = startup_owner {
        tracing::info!("agent owner: {owner}");
    } else {
        tracing::info!("no agent owner set at startup — will resolve lazily");
    }
    // Warn if owner-dependent mode but no owner resolved yet.
    if startup_owner.is_none() {
        match &config.respond_to {
            RespondTo::OwnerOnly => {
                tracing::warn!(
                    "respond-to=owner-only but no owner is set — all events will be \
                     dropped until owner is resolved. Set --respond-to=anyone to override."
                );
            }
            RespondTo::Allowlist => {
                tracing::warn!(
                    "respond-to=allowlist but no owner is set — allowlisted pubkeys \
                     will still be accepted, but owner-based matching is unavailable \
                     until owner is resolved."
                );
            }
            _ => {} // anyone/nobody don't depend on owner
        }
    }
    let mut owner_cache = OwnerCache::new(startup_owner);
    let mut sibling_cache = SiblingCache::new();

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
                compiled_filter: None,
                consecutive_timeouts: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
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
                compiled_filter: None,
                consecutive_timeouts: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
                prompt_tag: Some("all".into()),
            }]
        }
        SubscribeMode::Config => {
            // load_rules() already warns if the config file has zero rules.
            config::load_rules(&config.config_path)?
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
        idle_timeout: Duration::from_secs(config.idle_timeout_secs),
        max_turn_duration: Duration::from_secs(config.max_turn_duration_secs),
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
        max_turns_per_session: config.max_turns_per_session,
        permission_mode: config.permission_mode,
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

    // ── Step 6d: Maintenance (slot refill + queue compaction) ────────────────
    // Runs at the TOP of every loop iteration via Instant check — cannot be
    // starved by the biased select. Slot refill spawns background tasks so
    // spawn_and_init never blocks the main loop.
    let maintenance_interval = Duration::from_secs(30);
    let mut last_maintenance = std::time::Instant::now();

    // Channel for background respawn tasks to return completed agents.
    // Bounded to agent count — at most one respawn per slot in flight.
    let (respawn_tx, mut respawn_rx) = mpsc::channel::<RespawnResult>(config.agents as usize);
    // JoinSet for respawn tasks so shutdown can abort them.
    let mut respawn_tasks: tokio::task::JoinSet<()> = tokio::task::JoinSet::new();

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
    // Two-generation dedup for membership event replays (bounded, no amnesia).
    // Rotates at 1000 entries instead of clearing the entire set at 2000.
    let mut seen_membership_current: HashSet<String> = HashSet::new();
    let mut seen_membership_previous: HashSet<String> = HashSet::new();

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

    // ── Finding #14: Per-slot crash history for circuit breaker ───────────────
    //
    // One SlotCircuit per agent slot. crash_times entries are pruned to the last
    // CIRCUIT_BREAKER_WINDOW on each respawn attempt. The Vec is indexed by
    // agent slot index, so it must be sized to the configured pool capacity
    // (not the live count, which may be smaller after partial startup).
    let mut crash_history: Vec<SlotCircuit> = (0..config.agents as usize)
        .map(|_| SlotCircuit {
            crash_times: Vec::new(),
            open_until: None,
            respawn_in_flight: false,
        })
        .collect();

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
        // ── Maintenance (runs at loop top — cannot be starved by biased select) ──
        if last_maintenance.elapsed() >= maintenance_interval {
            last_maintenance = std::time::Instant::now();
            queue.compact_expired_state();

            // Slot refill: spawn background tasks for empty slots whose
            // circuit breaker allows it. spawn_and_init runs off the main
            // loop so it never blocks event processing.
            for (idx, slot) in crash_history.iter_mut().enumerate() {
                if pool.slot_alive(idx) || slot.respawn_in_flight {
                    continue;
                }
                if !slot.can_refill() {
                    continue;
                }
                slot.respawn_in_flight = true;
                tracing::info!(agent = idx, "slot refill: spawning background respawn");
                let cmd = config.agent_command.clone();
                let args = config.agent_args.clone();
                let env = config.persona_env_vars.clone();
                let guard = RespawnGuard::new(idx, respawn_tx.clone());
                respawn_tasks.spawn(async move {
                    let result = spawn_and_init(&cmd, &args, &env).await;
                    guard.send(result);
                });
            }

            // Flush requeued batches whose retry_after has expired. Without
            // this, a batch requeued during crash recovery can sit idle
            // indefinitely on quiet channels — dispatch_pending is only
            // called on relay events or pool results, neither of which
            // arrive when the channel is silent.
            if queue.has_flushable_work() {
                typing_channels.extend(dispatch_pending(&mut pool, &mut queue, &ctx));
            }
        }

        // ── Collect completed background respawns (non-blocking) ─────────────
        let mut respawn_collected = false;
        while let Ok(rr) = respawn_rx.try_recv() {
            crash_history[rr.index].respawn_in_flight = false;
            match rr.result {
                Ok(acp) => {
                    let agent = OwnedAgent {
                        index: rr.index,
                        acp,
                        state: SessionState::default(),
                        model_capabilities: None,
                        desired_model: config.model.clone(),
                    };
                    pool.return_agent(agent);
                    tracing::info!(agent = rr.index, "respawn complete");
                    respawn_collected = true;
                }
                Err(e) => {
                    crash_history[rr.index].mark_spawn_failed();
                    tracing::warn!(agent = rr.index, "respawn failed: {e} — circuit re-opened");
                }
            }
        }
        // Flush requeued events that were waiting for a live agent. Without
        // this, batches requeued during crash recovery sit idle until the
        // next relay event arrives — which can be minutes on quiet channels.
        if respawn_collected {
            typing_channels.extend(dispatch_pending(&mut pool, &mut queue, &ctx));
        }

        // Borrow result_rx and join_set simultaneously via split-borrow helper.
        let pool_event: Option<PoolEvent> = {
            let (result_rx, join_set) = pool.rx_and_join_set();
            tokio::select! {
                biased;
                // Finding #24: recv() returning None means all senders dropped
                // (pool was torn down). Break cleanly instead of panicking.
                r = result_rx.recv() => match r {
                    Some(result) => Some(PoolEvent::Result(Box::new(result))),
                    None => {
                        tracing::info!("result channel closed — exiting main loop");
                        break;
                    }
                },
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
                                // Two-generation dedup: check both sets before inserting.
                                if seen_membership_current.contains(&eid)
                                    || seen_membership_previous.contains(&eid)
                                {
                                    tracing::debug!(
                                        channel_id = %ch,
                                        kind = kind_u32,
                                        "skipping duplicate membership notification (same event_id)"
                                    );
                                    continue;
                                }
                                seen_membership_current.insert(eid);
                                // Rotate at 1000: current → previous, no amnesia window.
                                if seen_membership_current.len() >= 1000 {
                                    seen_membership_previous =
                                        std::mem::take(&mut seen_membership_current);
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
                                // Lazy owner resolution via OwnerCache — a relay
                                // outage during startup doesn't permanently
                                // disable remote shutdown.
                                let owner = owner_cache
                                    .get_or_resolve(&rest_client_for_presence, &pubkey_hex)
                                    .await;
                                if let Some(owner) = owner {
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

                            // ── Cancel command handling ──────────────────────
                            // Mirrors !shutdown: kind:9, content "!cancel", from
                            // owner, mentions THIS agent. Must be BEFORE
                            // queue.push() — the event content is moved by push.
                            //
                            // Mode-independent: !cancel fires regardless of
                            // --multiple-event-handling. It is explicit user
                            // intent, not an automatic policy decision.
                            let is_cancel = kind_u32 == KIND_STREAM_MESSAGE
                                && sprout_event.event.content.trim() == "!cancel"
                                && sprout_event.event.tags.iter().any(|t| {
                                    t.as_slice().first().map(|s| s.as_str()) == Some("p")
                                        && t.as_slice().get(1).map(|s| s.as_str()) == Some(pubkey_hex.as_str())
                                });
                            if is_cancel {
                                let owner = owner_cache
                                    .get_or_resolve(&rest_client_for_presence, &pubkey_hex)
                                    .await;
                                if let Some(owner) = owner {
                                    if sprout_event.event.pubkey.to_hex() == *owner {
                                        let fired = cancel_in_flight_task(&mut pool, sprout_event.channel_id);
                                        if !fired {
                                            tracing::warn!(
                                                channel_id = %sprout_event.channel_id,
                                                "!cancel received but no in-flight task — no-op"
                                            );
                                        }
                                        continue; // consume event — do NOT push to queue
                                    }
                                }
                                // Not from owner — fall through to normal prompt handling.
                            }
                            // ── End cancel command handling ───────────────────

                            // ── Inbound author gate ──────────────────────────
                            // Coarse security policy: drop events from disallowed
                            // authors before they reach subscription rules or the
                            // agent. Must be AFTER !shutdown (owner can always
                            // shut down regardless of gate mode).
                            //
                            // OwnerOnly also accepts events from "siblings" —
                            // pubkeys whose agent_owner_pubkey matches this
                            // agent's owner (e.g. other bots launched by the
                            // same human). Allowlist is unchanged: owner +
                            // explicit pubkey list only.
                            {
                                let author = sprout_event.event.pubkey.to_hex();
                                let allowed = match &config.respond_to {
                                    RespondTo::Anyone => true,
                                    RespondTo::Nobody => false,
                                    RespondTo::OwnerOnly => {
                                        is_owner_or_sibling(
                                            &author,
                                            &mut owner_cache,
                                            &mut sibling_cache,
                                            &rest_client_for_presence,
                                            &pubkey_hex,
                                        )
                                        .await
                                    }
                                    RespondTo::Allowlist => {
                                        let owner = owner_cache
                                            .get_or_resolve(
                                                &rest_client_for_presence,
                                                &pubkey_hex,
                                            )
                                            .await;
                                        config.respond_to_allowlist.contains(&author)
                                            || owner == Some(author.as_str())
                                    }
                                };
                                if !allowed {
                                    tracing::debug!(
                                        channel_id = %sprout_event.channel_id,
                                        author = %sprout_event.event.pubkey.to_hex(),
                                        mode = %config.respond_to,
                                        "inbound author gate — dropping event"
                                    );
                                    continue;
                                }
                            }
                            // ── End inbound author gate ──────────────────────

                            let matched = filter::match_event(&sprout_event.event, sprout_event.channel_id, &rules, &pubkey_hex).await;
                            let prompt_tag = match matched {
                                Some(m) => m.prompt_tag,
                                None => {
                                    tracing::debug!(channel_id = %sprout_event.channel_id, kind = sprout_event.event.kind.as_u16(), "event matched no rule — dropping");
                                    continue;
                                }
                            };
                            // Capture author pubkey before queue.push() moves
                            // sprout_event.event (needed for mode gate below).
                            let author_hex = sprout_event.event.pubkey.to_hex();
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
                            // ── Multiple-event-handling mode gate ─────────────
                            // Event is already queued. If mode requires it AND
                            // the channel has an in-flight task, fire cancel.
                            if accepted && queue.is_channel_in_flight(sprout_event.channel_id) {
                                let should_cancel = match config.multiple_event_handling {
                                    MultipleEventHandling::Queue => false,
                                    MultipleEventHandling::Interrupt => true,
                                    MultipleEventHandling::OwnerInterrupt => {
                                        let owner = owner_cache
                                            .get_or_resolve(&rest_client_for_presence, &pubkey_hex)
                                            .await;
                                        match owner {
                                            Some(o) => author_hex == *o,
                                            None => false,
                                        }
                                    }
                                };
                                if should_cancel {
                                    cancel_in_flight_task(&mut pool, sprout_event.channel_id);
                                }
                            }
                            // ── End mode gate ────────────────────────────────
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
                    // Use try_publish (non-blocking) for typing indicators —
                    // they're ephemeral and must not block the main loop during
                    // relay reconnection (#35).
                    for &ch in &typing_channels {
                        if let Ok(event) = relay.build_typing_event(ch) {
                            if let Err(e) = relay.try_publish_event(event) {
                                tracing::debug!("typing indicator dropped for {ch}: {e}");
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
                    &mut crash_history,
                    &respawn_tx,
                    &mut respawn_tasks,
                ) == LoopAction::Exit
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
                    &mut crash_history,
                    &respawn_tx,
                    &mut respawn_tasks,
                ) == LoopAction::Exit
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
                    &mut crash_history,
                    &respawn_tx,
                    &mut respawn_tasks,
                );
                if pool.live_count() == 0 && !any_respawn_in_flight(&crash_history) {
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
    // 30 s is generous for in-flight prompts to be cancelled; using
    // max_turn_duration here would cause Ctrl+C to hang for up to an hour.
    let grace = Duration::from_secs(30);
    // Best-effort drain of both join_set and result_rx during the grace period.
    // Tasks that finish normally send their OwnedAgent through result_rx — we
    // explicitly shut them down here to reap child processes. If the grace
    // period expires, remaining tasks are aborted and fall back to
    // AcpClient::Drop (start_kill + try_wait — best-effort, not guaranteed).
    let (rx_ref, js_ref) = pool.rx_and_join_set();
    let shutdown_result = tokio::time::timeout(grace, async {
        loop {
            tokio::select! {
                result = js_ref.join_next() => {
                    match result {
                        Some(Err(e)) => tracing::warn!("task error during shutdown: {e}"),
                        Some(Ok(())) => {}
                        None => break, // join_set empty
                    }
                }
                maybe_result = rx_ref.recv() => {
                    if let Some(mut pr) = maybe_result {
                        let idx = pr.agent.index;
                        pr.agent.acp.shutdown().await;
                        tracing::debug!(agent = idx, "reaped checked-out agent on shutdown");
                    }
                    // If None, channel closed — tasks are done.
                }
            }
        }
    })
    .await;
    if shutdown_result.is_err() {
        tracing::warn!("grace period expired, aborting remaining tasks");
        pool.join_set.shutdown().await;
    }
    // Drain any remaining results that arrived after join_set drained but
    // before tasks were aborted.
    while let Ok(mut pr) = pool.result_rx_try_recv() {
        let idx = pr.agent.index;
        pr.agent.acp.shutdown().await;
        tracing::debug!(agent = idx, "reaped late-arriving agent on shutdown");
    }
    // Explicitly shut down idle agents still sitting in their slots.
    for slot in pool.agents_mut().iter_mut() {
        if let Some(agent) = slot.take() {
            let idx = agent.index;
            let mut acp = agent.acp;
            acp.shutdown().await;
            tracing::debug!(agent = idx, "reaped idle agent on shutdown");
        }
    }
    drop(pool);

    // Abort any in-flight respawn tasks. They may be sleeping in backoff or
    // running spawn_and_init — either way, we don't want them spawning new
    // children after the main loop has exited. RespawnGuard::Drop sends a
    // failure result for aborted tasks, so respawn_in_flight is cleared.
    respawn_tasks.shutdown().await;

    // Drain any respawn results that completed before the abort. Explicitly
    // shut down returned agents instead of relying on AcpClient::Drop.
    while let Ok(rr) = respawn_rx.try_recv() {
        if let Ok(mut acp) = rr.result {
            acp.shutdown().await;
            tracing::debug!(agent = rr.index, "reaped respawned agent on shutdown");
        }
    }

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

    // Graceful relay shutdown — sends WebSocket close frame and waits up to 5s
    // for the background task to finish, rather than aborting immediately (#40).
    relay.shutdown().await;

    tracing::info!("sprout-acp stopped");
    Ok(())
}

// ── Loop control ──────────────────────────────────────────────────────────────

#[derive(PartialEq)]
enum LoopAction {
    Continue,
    Exit,
}

// ── cancel_in_flight_task ─────────────────────────────────────────────────────

/// Send a cancel signal to the in-flight task for `channel_id`.
/// Returns `true` if a signal was sent, `false` if no in-flight task was found.
fn cancel_in_flight_task(pool: &mut AgentPool, channel_id: uuid::Uuid) -> bool {
    let entry = pool
        .task_map_mut()
        .values_mut()
        .find(|m| m.channel_id == Some(channel_id));

    if let Some(meta) = entry {
        if let Some(tx) = meta.cancel_tx.take() {
            let _ = tx.send(());
            tracing::info!(channel = %channel_id, "cancel signal sent to in-flight task");
            return true;
        }
    }
    false
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
        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();

        let abort_handle = pool.join_set.spawn(async move {
            pool::run_prompt_task(
                agent,
                Some(batch),
                None,
                ctx_clone,
                result_tx,
                Some(cancel_rx),
            )
            .await;
        });

        pool.task_map_mut().insert(
            abort_handle.id(),
            pool::TaskMeta {
                agent_index,
                channel_id: Some(channel_id),
                recoverable_batch,
                cancel_tx: Some(cancel_tx),
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

#[allow(clippy::too_many_arguments)]
fn handle_prompt_result(
    pool: &mut AgentPool,
    queue: &mut EventQueue,
    config: &Config,
    mut result: PromptResult,
    heartbeat_in_flight: &mut bool,
    removed_channels: &HashSet<Uuid>,
    crash_history: &mut [SlotCircuit],
    respawn_tx: &mpsc::Sender<RespawnResult>,
    respawn_tasks: &mut tokio::task::JoinSet<()>,
) -> LoopAction {
    let before = pool.task_map().len();
    let agent_index = result.agent.index;
    pool.task_map_mut()
        .retain(|_, meta| meta.agent_index != agent_index);
    debug_assert_eq!(before, pool.task_map().len() + 1);

    // Requeue BEFORE mark_complete: requeue() sets retry_after with a future
    // deadline, and mark_complete() checks for it to decide whether to preserve
    // retry_counts. If mark_complete runs first, retry_counts is cleared and
    // every retry starts at attempt 1 — defeating exponential backoff and
    // dead-letter protection.
    if let Some(batch) = result.batch {
        // Don't requeue batches for channels the agent was removed from —
        // those events are stale and should be silently dropped.
        if !removed_channels.contains(&batch.channel_id) {
            if matches!(result.outcome, PromptOutcome::Cancelled) {
                // Cancel re-prompt: store as cancelled events so flush_next()
                // merges them into the next FlushBatch.cancelled_events,
                // enabling the annotated merged-prompt format.
                queue.requeue_as_cancelled(batch);
            } else {
                queue.requeue(batch);
            }
        } else {
            tracing::debug!(
                channel_id = %batch.channel_id,
                events = batch.events.len(),
                "dropping failed batch for removed channel"
            );
        }
    }

    match &result.source {
        PromptSource::Channel(ch) => queue.mark_complete(*ch),
        PromptSource::Heartbeat => *heartbeat_in_flight = false,
    }

    // Strip sessions for channels the agent was removed from while this
    // agent was checked out. This covers the gap where invalidate_channel_sessions
    // only touches idle agents.
    for ch in removed_channels {
        result.agent.state.invalidate_channel(ch);
    }

    let outcome_label = match &result.outcome {
        PromptOutcome::Ok(_) => "ok",
        PromptOutcome::Error(_) => "error",
        PromptOutcome::Timeout => "timeout",
        PromptOutcome::AgentExited => "exited",
        PromptOutcome::Cancelled => "cancelled",
    };
    let agent_index = result.agent.index;

    match result.outcome {
        // Successful prompt — return agent to pool.
        PromptOutcome::Ok(_) => {
            tracing::debug!(
                agent = agent_index,
                outcome = outcome_label,
                "agent_returned"
            );
            pool.return_agent(result.agent);
        }
        // Fatal outcomes: the agent subprocess is dead or poisoned — respawn it.
        PromptOutcome::AgentExited | PromptOutcome::Timeout => {
            tracing::debug!(
                agent = agent_index,
                outcome = outcome_label,
                "agent_returned — respawning"
            );
            let index = result.agent.index;
            let slot_history = &mut crash_history[index];
            if !spawn_respawn_task(
                result.agent,
                config,
                slot_history,
                respawn_tx,
                respawn_tasks,
            ) {
                // Circuit open — slot stays empty until maintenance refill.
                if pool.live_count() == 0 && !any_respawn_in_flight(crash_history) {
                    tracing::error!("all agents dead — exiting");
                    return LoopAction::Exit;
                }
            }
        }
        // Errors fall into two categories:
        //
        // 1. Transport-class (Io, WriteTimeout, Timeout, Protocol): the stdio
        //    pipe may be corrupted or the agent desynchronized. These are fatal
        //    to the agent regardless of whether they occurred during session
        //    creation or an active prompt — respawn unconditionally.
        //
        // 2. Application-class (IdleTimeout, HardTimeout, Json): the pipe is
        //    intact but the prompt failed. Return the agent to the pool so it
        //    can be reused for the next event.

        // Intentional cancel — agent is healthy, return it to the pool.
        // No respawn, no retry penalty. The cancelled batch was already stored
        // via requeue_as_cancelled() above and will be merged into the next
        // FlushBatch by flush_next().
        PromptOutcome::Cancelled => {
            tracing::debug!(
                agent = agent_index,
                outcome = outcome_label,
                "agent_returned (cancelled)"
            );
            pool.return_agent(result.agent);
        }
        PromptOutcome::Error(ref e) => {
            let is_transport_error = matches!(
                e,
                acp::AcpError::Io(_)
                    | acp::AcpError::WriteTimeout(_)
                    | acp::AcpError::Timeout(_)
                    | acp::AcpError::Protocol(_)
            );
            if is_transport_error {
                tracing::warn!(
                    agent = agent_index,
                    outcome = outcome_label,
                    "transport/protocol error — respawning agent"
                );
                let index = result.agent.index;
                let slot_history = &mut crash_history[index];
                if !spawn_respawn_task(
                    result.agent,
                    config,
                    slot_history,
                    respawn_tx,
                    respawn_tasks,
                ) && pool.live_count() == 0
                    && !any_respawn_in_flight(crash_history)
                {
                    tracing::error!("all agents dead — exiting");
                    return LoopAction::Exit;
                }
            } else {
                tracing::debug!(
                    agent = agent_index,
                    outcome = outcome_label,
                    "agent_returned (application error — pipe intact)"
                );
                pool.return_agent(result.agent);
            }
        }
    }
    LoopAction::Continue
}

// ── recover_panicked_agent ────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn recover_panicked_agent(
    pool: &mut AgentPool,
    queue: &mut EventQueue,
    config: &Config,
    join_error: tokio::task::JoinError,
    heartbeat_in_flight: &mut bool,
    removed_channels: &HashSet<Uuid>,
    typing_channels: &mut HashSet<Uuid>,
    crash_history: &mut [SlotCircuit],
    respawn_tx: &mpsc::Sender<RespawnResult>,
    respawn_tasks: &mut tokio::task::JoinSet<()>,
) {
    let task_id = join_error.id();
    let Some(meta) = pool.task_map_mut().remove(&task_id) else {
        tracing::error!("panic for unknown task {task_id:?} — bug");
        return;
    };
    let i = meta.agent_index;

    // Requeue BEFORE mark_complete (same rationale as handle_prompt_result).
    if let Some(batch) = meta.recoverable_batch {
        if let Some(ch) = meta.channel_id {
            if !removed_channels.contains(&ch) {
                queue.requeue(batch);
                tracing::warn!("requeued batch for panicked agent {i}");
            } else {
                tracing::debug!(
                    channel_id = %ch,
                    "dropping panicked batch for removed channel"
                );
            }
        }
    }

    if let Some(ch) = meta.channel_id {
        queue.mark_complete(ch);
        typing_channels.remove(&ch);
        tracing::warn!("cleared wedged in-flight channel {ch} from panicked agent {i}");
    } else {
        *heartbeat_in_flight = false;
        tracing::warn!("cleared wedged heartbeat_in_flight from panicked agent {i}");
    }

    // Panics count as crashes for the circuit breaker.
    // The panicked task already dropped the AcpClient, so we just need to
    // check the circuit and spawn a fresh agent in the background.
    let slot = &mut crash_history[i];

    let delay = match slot.record_crash() {
        CrashVerdict::CircuitOpen => {
            tracing::error!(agent = i, "circuit open after panic — not respawning");
            return;
        }
        CrashVerdict::HalfOpenProbe => {
            tracing::info!(agent = i, "circuit half-open — probe respawn after panic");
            Duration::ZERO
        }
        CrashVerdict::Respawn(d) => {
            tracing::info!(
                agent = i,
                delay_ms = d.as_millis(),
                "respawn backoff after panic"
            );
            d
        }
    };

    // Spawn respawn work off the main loop.
    slot.respawn_in_flight = true;
    let cmd = config.agent_command.clone();
    let args = config.agent_args.clone();
    let env = config.persona_env_vars.clone();
    let guard = RespawnGuard::new(i, respawn_tx.clone());
    respawn_tasks.spawn(async move {
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        let result = spawn_and_init(&cmd, &args, &env).await;
        guard.send(result);
    });
}

// ── drain_ready_join_results ──────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn drain_ready_join_results(
    pool: &mut AgentPool,
    queue: &mut EventQueue,
    config: &Config,
    heartbeat_in_flight: &mut bool,
    removed_channels: &HashSet<Uuid>,
    typing_channels: &mut HashSet<Uuid>,
    crash_history: &mut [SlotCircuit],
    respawn_tx: &mpsc::Sender<RespawnResult>,
    respawn_tasks: &mut tokio::task::JoinSet<()>,
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
                crash_history,
                respawn_tx,
                respawn_tasks,
            );
            if pool.live_count() == 0 && !any_respawn_in_flight(crash_history) {
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
        pool::run_prompt_task(agent, None, Some(prompt_text), ctx_clone, result_tx, None).await;
    });

    pool.task_map_mut().insert(
        abort_handle.id(),
        pool::TaskMeta {
            agent_index,
            channel_id: None,
            recoverable_batch: None,
            cancel_tx: None,
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

/// Spawn a background respawn task for a crashed agent slot.
///
/// Does the circuit breaker check synchronously (non-blocking), then spawns
/// the actual shutdown + backoff + spawn_and_init work into a background task.
/// The result comes back through `respawn_tx` so the main loop stays responsive.
///
/// Returns `true` if a respawn task was spawned, `false` if the circuit is open.
fn spawn_respawn_task(
    old_agent: OwnedAgent,
    config: &Config,
    slot: &mut SlotCircuit,
    respawn_tx: &mpsc::Sender<RespawnResult>,
    respawn_tasks: &mut tokio::task::JoinSet<()>,
) -> bool {
    let index = old_agent.index;

    // Circuit breaker: record crash, decide whether to respawn.
    let delay = match slot.record_crash() {
        CrashVerdict::CircuitOpen => {
            tracing::error!(agent = index, "circuit open — not respawning");
            return false;
        }
        CrashVerdict::HalfOpenProbe => {
            tracing::info!(agent = index, "circuit half-open — probe respawn");
            Duration::ZERO
        }
        CrashVerdict::Respawn(d) => {
            tracing::info!(agent = index, delay_ms = d.as_millis(), "respawn backoff");
            d
        }
    };

    slot.respawn_in_flight = true;

    // Spawn the actual work (shutdown + sleep + spawn + init) off the main loop.
    let cmd = config.agent_command.clone();
    let args = config.agent_args.clone();
    let env = config.persona_env_vars.clone();
    let guard = RespawnGuard::new(index, respawn_tx.clone());
    respawn_tasks.spawn(async move {
        // Shutdown old agent (reap child, prevent zombie).
        let mut agent = old_agent;
        agent.acp.shutdown().await;
        drop(agent);

        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }

        let result = spawn_and_init(&cmd, &args, &env).await;
        guard.send(result);
    });

    true
}

// ── spawn_and_init ────────────────────────────────────────────────────────────

/// Spawn an agent subprocess and run the MCP `initialize` handshake.
///
/// Takes owned args so it can run in a background `tokio::spawn` task without
/// borrowing `Config`. All respawn/refill paths use this.
async fn spawn_and_init(
    command: &str,
    args: &[String],
    extra_env: &[(String, String)],
) -> Result<AcpClient> {
    let mut acp = AcpClient::spawn(command, args, extra_env)
        .await
        .map_err(|e| anyhow::anyhow!("failed to spawn agent: {e}"))?;

    match acp.initialize().await {
        Ok(init_result) => {
            tracing::info!("agent initialized: {init_result}");
            Ok(acp)
        }
        Err(e) => {
            // Explicitly shut down the spawned child to prevent zombie/leak.
            // Drop only does start_kill + try_wait (best-effort); shutdown()
            // does start_kill + bounded wait (guaranteed reap).
            acp.shutdown().await;
            Err(anyhow::anyhow!("agent initialize failed: {e}"))
        }
    }
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
    // `models` subcommand doesn't use persona packs — no extra env.
    let mut client = match AcpClient::spawn(&args.agent_command, &agent_args, &[]).await {
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
                    // bech32 encoding of a valid secret key is infallible.
                    // Panic here is correct: injecting a bogus secret would cause
                    // delayed, hard-to-diagnose agent failures downstream.
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
            // Forward SPROUT_TOOLSETS so the MCP server enables the
            // same toolsets the operator configured for this harness.
            if let Ok(ts) = std::env::var("SPROUT_TOOLSETS") {
                if !ts.is_empty() {
                    env.push(EnvVar {
                        name: "SPROUT_TOOLSETS".into(),
                        value: ts,
                    });
                }
            }
            env
        },
    }]
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod owner_cache_tests {
    use super::*;

    #[test]
    fn new_with_some_caches_immediately() {
        let cache = OwnerCache::new(Some("abcd".into()));
        assert_eq!(cache.pubkey.as_deref(), Some("abcd"));
        assert!(cache.last_attempt.is_some());
    }

    #[test]
    fn new_with_none_has_no_attempt() {
        let cache = OwnerCache::new(None);
        assert!(cache.pubkey.is_none());
        assert!(cache.last_attempt.is_none());
    }

    #[test]
    fn get_or_resolve_returns_cached_immediately() {
        // When pubkey is already cached, get_or_resolve should return it
        // without any API call. We can verify this by checking the return
        // value without providing a real RestClient (the method short-circuits
        // before using it).
        let cache = OwnerCache::new(Some("ab".repeat(32)));
        // We can't call get_or_resolve without a RestClient in a sync test,
        // but we can verify the cache state directly.
        assert_eq!(cache.pubkey.as_deref(), Some("ab".repeat(32)).as_deref());
    }

    #[test]
    fn success_cached_for_process_lifetime() {
        // After a successful resolution, pubkey stays cached even if
        // last_attempt is old. The early return `if self.pubkey.is_some()`
        // means the TTL check is never reached.
        let mut cache = OwnerCache::new(Some("ab".repeat(32)));
        // Simulate time passing by backdating last_attempt
        cache.last_attempt = Some(std::time::Instant::now() - Duration::from_secs(3600));
        // pubkey is still cached — success is permanent
        assert!(cache.pubkey.is_some());
    }

    #[test]
    fn failure_respects_ttl() {
        // After a failed lookup, last_attempt is set. A subsequent call
        // within the TTL window should NOT retry (stale == false).
        let mut cache = OwnerCache::new(None);
        cache.last_attempt = Some(std::time::Instant::now());
        // Within TTL: stale check returns false, so no retry
        let stale = cache
            .last_attempt
            .map(|t| t.elapsed() >= OWNER_CACHE_TTL)
            .unwrap_or(true);
        assert!(!stale, "should not be stale within TTL");
    }

    #[test]
    fn failure_retries_after_ttl() {
        // After TTL expires, the cache should consider itself stale.
        let mut cache = OwnerCache::new(None);
        cache.last_attempt = Some(std::time::Instant::now() - Duration::from_secs(61));
        let stale = cache
            .last_attempt
            .map(|t| t.elapsed() >= OWNER_CACHE_TTL)
            .unwrap_or(true);
        assert!(stale, "should be stale after TTL");
    }

    #[test]
    fn no_attempt_is_always_stale() {
        let cache = OwnerCache::new(None);
        let stale = cache
            .last_attempt
            .map(|t| t.elapsed() >= OWNER_CACHE_TTL)
            .unwrap_or(true);
        assert!(stale, "no prior attempt should be considered stale");
    }
}

#[cfg(test)]
mod sibling_cache_tests {
    use super::*;

    const OWNER_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const OWNER_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const AUTHOR_1: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    const AUTHOR_2: &str = "2222222222222222222222222222222222222222222222222222222222222222";

    #[test]
    fn sibling_with_matching_owner_returns_true() {
        let mut cache = SiblingCache::new();
        cache.record(
            AUTHOR_1.into(),
            SiblingLookup::Resolved(Some(OWNER_A.into())),
        );
        assert_eq!(cache.check(AUTHOR_1, OWNER_A), Some(true));
    }

    #[test]
    fn different_owner_returns_false() {
        let mut cache = SiblingCache::new();
        cache.record(
            AUTHOR_1.into(),
            SiblingLookup::Resolved(Some(OWNER_B.into())),
        );
        assert_eq!(cache.check(AUTHOR_1, OWNER_A), Some(false));
    }

    #[test]
    fn no_owner_on_profile_returns_false() {
        let mut cache = SiblingCache::new();
        cache.record(AUTHOR_1.into(), SiblingLookup::Resolved(None));
        assert_eq!(cache.check(AUTHOR_1, OWNER_A), Some(false));
    }

    #[test]
    fn lookup_failure_returns_false() {
        let mut cache = SiblingCache::new();
        cache.record(AUTHOR_1.into(), SiblingLookup::Failed);
        assert_eq!(cache.check(AUTHOR_1, OWNER_A), Some(false));
    }

    #[test]
    fn unknown_author_returns_none() {
        let cache = SiblingCache::new();
        assert_eq!(cache.check(AUTHOR_1, OWNER_A), None);
    }

    #[test]
    fn record_normalizes_to_lowercase() {
        let mut cache = SiblingCache::new();
        let mixed_case = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        cache.record(
            AUTHOR_1.into(),
            SiblingLookup::Resolved(Some(mixed_case.into())),
        );
        // Should match lowercase expected owner.
        assert_eq!(cache.check(AUTHOR_1, OWNER_A), Some(true));
    }

    #[test]
    fn positive_ttl_holds_within_window() {
        let mut cache = SiblingCache::new();
        cache.record(
            AUTHOR_1.into(),
            SiblingLookup::Resolved(Some(OWNER_A.into())),
        );
        // Freshly inserted — should be within 5-minute TTL.
        assert_eq!(cache.check(AUTHOR_1, OWNER_A), Some(true));
    }

    #[test]
    fn positive_ttl_expires() {
        let mut cache = SiblingCache::new();
        cache.record(
            AUTHOR_1.into(),
            SiblingLookup::Resolved(Some(OWNER_A.into())),
        );
        // Backdate the entry past the hit TTL.
        if let Some((_, ts)) = cache.entries.get_mut(AUTHOR_1) {
            *ts = std::time::Instant::now() - SIBLING_CACHE_HIT_TTL - Duration::from_secs(1);
        }
        assert_eq!(
            cache.check(AUTHOR_1, OWNER_A),
            None,
            "should be stale after hit TTL"
        );
    }

    #[test]
    fn negative_ttl_expires() {
        let mut cache = SiblingCache::new();
        cache.record(
            AUTHOR_1.into(),
            SiblingLookup::Resolved(Some(OWNER_B.into())),
        );
        // Backdate past the miss TTL.
        if let Some((_, ts)) = cache.entries.get_mut(AUTHOR_1) {
            *ts = std::time::Instant::now() - SIBLING_CACHE_MISS_TTL - Duration::from_secs(1);
        }
        assert_eq!(
            cache.check(AUTHOR_1, OWNER_A),
            None,
            "should be stale after miss TTL"
        );
    }

    #[test]
    fn negative_ttl_holds_within_window() {
        let mut cache = SiblingCache::new();
        cache.record(AUTHOR_1.into(), SiblingLookup::Failed);
        // Freshly inserted — should be within 1-minute TTL.
        assert_eq!(cache.check(AUTHOR_1, OWNER_A), Some(false));
    }

    #[test]
    fn eviction_when_at_capacity() {
        let mut cache = SiblingCache::new();
        // Fill to capacity with unique authors.
        for i in 0..SIBLING_CACHE_MAX_ENTRIES {
            let author = format!("{:064x}", i);
            cache.record(author, SiblingLookup::Resolved(Some(OWNER_A.into())));
        }
        assert_eq!(cache.entries.len(), SIBLING_CACHE_MAX_ENTRIES);

        // Insert one more — should evict the oldest and stay at capacity.
        cache.record(
            AUTHOR_1.into(),
            SiblingLookup::Resolved(Some(OWNER_A.into())),
        );
        assert_eq!(cache.entries.len(), SIBLING_CACHE_MAX_ENTRIES);

        // The new entry should be present.
        assert_eq!(cache.check(AUTHOR_1, OWNER_A), Some(true));
    }

    #[test]
    fn update_existing_entry_refreshes_timestamp() {
        let mut cache = SiblingCache::new();
        cache.record(
            AUTHOR_1.into(),
            SiblingLookup::Resolved(Some(OWNER_B.into())),
        );
        assert_eq!(cache.check(AUTHOR_1, OWNER_A), Some(false));

        // Update with new owner — should overwrite.
        cache.record(
            AUTHOR_1.into(),
            SiblingLookup::Resolved(Some(OWNER_A.into())),
        );
        assert_eq!(cache.check(AUTHOR_1, OWNER_A), Some(true));
    }

    #[test]
    fn multiple_authors_independent() {
        let mut cache = SiblingCache::new();
        cache.record(
            AUTHOR_1.into(),
            SiblingLookup::Resolved(Some(OWNER_A.into())),
        );
        cache.record(
            AUTHOR_2.into(),
            SiblingLookup::Resolved(Some(OWNER_B.into())),
        );

        assert_eq!(cache.check(AUTHOR_1, OWNER_A), Some(true));
        assert_eq!(cache.check(AUTHOR_2, OWNER_A), Some(false));
        assert_eq!(cache.check(AUTHOR_2, OWNER_B), Some(true));
    }
}
