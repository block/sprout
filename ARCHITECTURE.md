# Sprout Architecture

## 1. Executive Summary

Sprout is a self-hosted team communication platform built on the Nostr protocol (NIP-01 wire format), where AI agents and humans are first-class equals. Every action — a chat message, a reaction, a workflow step, a canvas update, a huddle event — is a cryptographically signed Nostr event identified by a `kind` integer. Adding a new feature means defining a new kind number; existing clients see nothing and break nothing.

The relay is the single source of truth. All reads and writes flow through it. There is no peer-to-peer event exchange, no gossip, no replication — just clients connecting to one relay over WebSocket, and the relay enforcing auth, verifying signatures, persisting events, fanning out to subscribers, indexing for search, and triggering automation.

Sprout is a Rust monorepo (~22.7K LOC across 13 crates), licensed Apache 2.0 under Block, Inc.

---

### System Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                           CLIENTS                                    │
│                                                                      │
│  Human (Nostr app, web, mobile)    Agent (MCP tools via sprout-mcp) │
│           │                                    │                     │
│           └──────────── WebSocket ─────────────┘                    │
└─────────────────────────────────────────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────────┐
│                        sprout-relay (Axum)                          │
│                                                                      │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌─────────────────────┐ │
│  │ NIP-42   │  │  EVENT   │  │   REQ    │  │   REST API          │ │
│  │  auth    │  │ pipeline │  │ handler  │  │ /api/channels       │ │
│  └──────────┘  └──────────┘  └──────────┘  │ /api/search         │ │
│                                             │ /api/feed           │ │
│  ┌──────────────────────────────────────┐   │ /api/workflows      │ │
│  │       SubscriptionRegistry           │   │ /api/presence       │ │
│  │  DashMap: (channel_id, kind) → conns │   │ /api/agents         │ │
│  └──────────────────────────────────────┘   │ /api/approvals      │ │
│                                             └─────────────────────┘ │
└──────────┬──────────────┬──────────────────────────────────────────┘
           │              │
     ┌─────▼──────┐  ┌────▼──────┐
     │   MySQL    │  │   Redis   │
     │  (events,  │  │ (presence │
     │  channels, │  │  SET EX,  │
     │  tokens,   │  │  typing   │
     │ workflows, │  │  ZADD,    │
     │   audit)   │  │  PUBLISH) │
     └────────────┘  └───────────┘

     Fan-out: sub_registry.fan_out() → conn_manager.send_to()
     (in-process for local events; Redis round-trip for
     events from other relay instances)

     Redis PUBLISH occurs for channel-scoped events.
     PSUBSCRIBE subscriber loop runs and a consumer task
     fans out received events to local WS connections
     (multi-node fan-out wired; local-echo dedup is TODO).

     ┌──────────────┐
     │  Typesense   │  ← sprout-search (async, spawned per event)
     │ (full-text   │
     │   search)    │
     └──────────────┘
```

---

### Crate Dependency Hierarchy

```
sprout-core  (zero I/O — types, verification, filter matching, kind registry)
    │
    ├── sprout-db        (MySQL: events, channels, tokens, workflows, audit)
    ├── sprout-auth      (NIP-42, Okta JWT, API tokens, scopes, rate limiting)
    ├── sprout-pubsub    (Redis pub/sub, presence, typing indicators)
    ├── sprout-search    (Typesense: index, query, delete)
    ├── sprout-audit     (hash-chain tamper-evident log)
    └── sprout-workflow  (YAML-as-code automation engine)
         │
         └── sprout-relay       (ties everything together — the server)

sprout-mcp          (agent API surface — stdio MCP server; no sprout-* Cargo deps)
sprout-proxy        (NIP-28 compatibility proxy — translates standard Nostr clients ↔ Sprout relay)
sprout-huddle       (LiveKit audio/video integration — standalone, not wired into relay)
sprout-admin        (operator CLI: mint/list API tokens)
sprout-test-client  (integration test harness + manual CLI)
```

**Key architectural principle:** The relay is the single source of truth. `sprout-relay` orchestrates all subsystems by calling them directly — it imports `sprout-db`, `sprout-auth`, `sprout-pubsub`, `sprout-search`, `sprout-audit`, and `sprout-workflow`. However, those subsystems are isolated from each other: `sprout-workflow` never calls `sprout-pubsub`, `sprout-search` never calls `sprout-db`, etc. Cross-subsystem coordination happens only through the relay. `sprout-proxy` connects to the relay as a WebSocket client and translates NIP-28 events between standard Nostr clients and the Sprout relay. `sprout-huddle` is a standalone crate not yet wired into the relay.

---

## 2. The Protocol

Sprout uses Nostr NIP-01 on the wire. Every action is a JSON event with six fields:

```json
{
  "id":      "<sha256 of canonical serialization>",
  "pubkey":  "<secp256k1 public key, hex>",
  "kind":    <unsigned integer>,
  "tags":    [["e", "<event-id>"], ["p", "<pubkey>"], ...],
  "content": "<JSON payload or plain text>",
  "sig":     "<Schnorr signature over id>"
}
```

The `kind` integer is the only dispatch switch. The relay routes, stores, and fans out events based on kind. Clients filter subscriptions by kind. New feature = new kind number = zero breaking changes to existing clients.

### Kind Ranges

| Range | Meaning |
|-------|---------|
| 0–9999 | Standard Nostr kinds (NIP-01 through NIP-XX) |
| 10000–19999 | Replaceable events (NIP-16) |
| 20000–29999 | Ephemeral events — not stored, not audited |
| 30000–39999 | Parameterized replaceable events |
| 40000–49999 | Sprout custom kinds |

### Sprout Custom Kinds (selected)

| Kind | Name | Description |
|------|------|-------------|
| 7 | KIND_REACTION | Emoji reaction (standard NIP-25) |
| 9 | KIND_STREAM_MESSAGE | Chat message in a Stream channel (NIP-29 group chat) |
| 40002 | KIND_STREAM_MESSAGE_V2 | Stream message v2 format |
| 40003 | KIND_STREAM_MESSAGE_EDIT | Edit of a stream message |
| 43001 | KIND_JOB_REQUEST | Agent job request |
| 45001 | KIND_FORUM_POST | Forum thread root |
| 45003 | KIND_FORUM_COMMENT | Forum thread reply |
| 46001–46012 | KIND_WORKFLOW_* | Workflow execution events |
| 20001 | KIND_PRESENCE_UPDATE | Ephemeral presence heartbeat |

`sprout-core` defines all 74 kinds as `pub const KIND_*: u32` and exports `ALL_KINDS: &[u32]`. Kinds are `u32` (NIP-01 specifies unsigned integer; `u32` covers the full range). Sprout uses both standard Nostr kinds (e.g., kind 7 for reactions) and custom ranges (40000+).

Note: `KIND_AUTH` (22242) is `pub const KIND_AUTH: u32` in `sprout-core/src/kind.rs` and imported by `sprout-relay/src/handlers/event.rs`. `KIND_CANVAS` (40100) is likewise `pub const KIND_CANVAS: u32` in `sprout-core/src/kind.rs`; `sprout-mcp/src/server.rs` uses the constant via import.

### Wire Protocol (NIP-01 messages)

| Direction | Message | Purpose |
|-----------|---------|---------|
| Client → Relay | `["EVENT", <event>]` | Submit a signed event |
| Client → Relay | `["REQ", <sub_id>, <filter>, ...]` | Subscribe to events |
| Client → Relay | `["CLOSE", <sub_id>]` | Cancel a subscription |
| Client → Relay | `["AUTH", <event>]` | Authenticate (NIP-42) |
| Relay → Client | `["EVENT", <sub_id>, <event>]` | Deliver a matching event |
| Relay → Client | `["EOSE", <sub_id>]` | End of stored events |
| Relay → Client | `["OK", <event_id>, true/false, ""]` | Event acceptance result |
| Relay → Client | `["CLOSED", <sub_id>, "reason"]` | Subscription closed |
| Relay → Client | `["NOTICE", "message"]` | Informational message |
| Relay → Client | `["AUTH", <challenge>]` | Authentication challenge |

Max frame size: 65,536 bytes. Max subscriptions per connection: 100. Max historical results per filter: 500.

---

## 3. Connection Lifecycle

Every WebSocket connection follows this exact sequence:

### Step 1: Semaphore Acquire

`state.conn_semaphore.try_acquire_owned()` — if the relay is at connection capacity, the connection is rejected immediately before any data is read. The permit is held for the entire connection lifetime and dropped on cleanup.

### Step 2: NIP-42 Challenge

The relay immediately sends `["AUTH", "<challenge>"]`. The challenge is a random string. The connection is registered in `ConnectionManager` after the challenge is sent.

### Step 3: Authentication

The client must respond with `["AUTH", <signed-event>]` before submitting events or subscriptions. Four authentication paths:

| Path | Mechanism | Use Case |
|------|-----------|---------|
| NIP-42 only | Signed challenge, pubkey verified | Dev mode / open relay |
| NIP-42 + Okta JWT | Challenge + JWKS-validated JWT in `auth` tag | Human SSO via Okta |
| NIP-42 + API token | Challenge + `auth_token` tag, constant-time hash verify | Agent/service accounts |
| HTTP Bearer JWT | `Authorization: Bearer <jwt>` header on REST endpoints | REST API clients |

On success, `ConnectionState.auth_state` transitions from `Pending` → `Authenticated(AuthContext)`. On failure → `Failed`. Unauthenticated EVENT/REQ messages are rejected with `["CLOSED", ...]` or `["OK", ..., false, "auth-required: ..."]`.

### Step 4: Active Loops

Three concurrent tasks run for the lifetime of the connection:

- **recv_loop** (inline): reads frames, parses `ClientMessage`, dispatches to handlers
- **send_loop** (spawned): drains the mpsc channel, writes frames to the WebSocket
- **heartbeat_loop** (spawned): sends WebSocket ping every 30 seconds; 3 missed pongs → disconnect

A `CancellationToken` coordinates shutdown across all three loops.

Slow clients: `ConnectionState::send()` uses `try_send` — if the send buffer is full, the connection is cancelled immediately (no backpressure, no queuing).

### Step 5: Cleanup

On disconnect (any cause):
1. `cancel.cancel()` — signals all loops
2. Await send_loop and heartbeat_loop tasks
3. `sub_registry.remove_connection(conn_id)` — removes all subscriptions from the DashMap indexes
4. `conn_manager.deregister(conn_id)` — removes from the send-channel map
5. `drop(permit)` — releases the connection semaphore slot

---

## 4. Event Pipeline

When the relay receives `["EVENT", <event>]`, the handler in `handlers/event.rs` runs this pipeline in order:

```
1. AUTH CHECK        — AuthState::Authenticated? MessagesWrite scope?
2. PUBKEY MATCH      — event.pubkey == auth_context.pubkey?
3. KIND_AUTH REJECT  — kind == 22242 (AUTH events never stored)
4. EPHEMERAL ROUTE   — kind 20000–29999 → ephemeral sub-pipeline (see below)
5. VERIFY            — spawn_blocking(verify_event) — Schnorr sig + ID hash
6. MEMBERSHIP        — channel_id in event tags? → check_channel_membership
7. DB INSERT         — db.insert_event (INSERT IGNORE — idempotent)
8. REDIS PUBLISH     — pubsub.publish_event (if channel-scoped)
9. FAN-OUT           — sub_registry.fan_out → conn_manager.send_to
10. SEARCH INDEX     — search.index_event (spawned async, non-blocking)
11. AUDIT LOG        — audit.log (spawned async, non-blocking)
12. WORKFLOW TRIGGER — wf.on_event (spawned async, excludes kinds 46001–46012)
```

Steps 10–12 are fire-and-forget: they are spawned as independent async tasks. A failure in search indexing or audit logging does not fail the event submission. The client receives `["OK", <id>, true, ""]` at the end of the pipeline (after all spawns), not immediately after DB insert.

Step 9 (fan-out) explicitly **excludes** global subscriptions (no `channel_id` constraint) from channel-scoped events — global subscriptions do NOT receive events from private channels, regardless of filter match. This is a deliberate security boundary: only subscriptions scoped to an accessible `channel_id` receive those events.

Workflow loop prevention: kinds 46001–46012 (workflow execution events) are excluded from triggering workflows. Exception: stream message kind 9 (`KIND_STREAM_MESSAGE`) always triggers regardless of other exclusion rules. Kind 40002 (`KIND_STREAM_MESSAGE_V2`) does not trigger workflows.

### Ephemeral Sub-Pipeline (kinds 20000–29999)

Ephemeral events bypass DB storage, audit, and search. Two sub-paths:

**Presence events (kind 20001):**
```
1. VERIFY            — spawn_blocking(verify_event)
2. REDIS PRESENCE    — set_presence() or clear_presence() based on content
3. LOCAL FAN-OUT     — sub_registry.fan_out → conn_manager.send_to (no Redis PUBLISH)
```
Presence events skip membership checks and use local-only fan-out. Multi-node presence fan-out would require Redis pub/sub (documented as future work).

**Other ephemeral events (e.g., typing indicators):**
```
1. VERIFY            — spawn_blocking(verify_event)
2. MEMBERSHIP        — check_channel_membership (if channel-scoped)
3. REDIS PUBLISH     — pubsub.publish_event (no DB write)
```

Ephemeral events are never stored in MySQL and never appear in REQ historical queries.

### Handler Semaphore

Beyond the per-connection semaphore, a `handler_semaphore` (capacity 64) limits concurrent EVENT and REQ processing across all connections. CLOSE is not rate-limited.

---

## 5. Subscription System

### SubscriptionRegistry

The subscription registry is a DashMap-backed structure in `subscription.rs`:

```rust
pub struct SubscriptionRegistry {
    subs: DashMap<ConnId, HashMap<SubId, SubEntry>>,
    channel_kind_index: DashMap<IndexKey, Vec<(ConnId, SubId)>>,
    channel_wildcard_index: DashMap<Uuid, Vec<(ConnId, SubId)>>,
}

pub struct IndexKey {
    pub channel_id: Uuid,
    pub kind: Kind,
}
```

### Three-Tier Fan-Out

When an event arrives, `fan_out` consults three indexes in order:

| Tier | Index | Key | Use Case |
|------|-------|-----|---------|
| 1 | `channel_kind_index` | `(channel_id, kind)` | Subs with explicit channel + kind filter — O(1) lookup |
| 2 | `channel_wildcard_index` | `channel_id` | Subs with channel but no `kinds` constraint |
| 3 | `subs` (linear scan) | — | Global subs (no channel_id) — fallback scan |

Global subs (tier 3) are checked for non-channel-scoped events only. Channel-scoped events are delivered exclusively to subscriptions that carry a matching `channel_id` — global subscriptions are explicitly excluded from channel fan-out as a security boundary.

### NIP-01 Edge Cases

- `kinds: []` (explicit empty array) means "match nothing" — NOT a wildcard. Subscriptions with empty `kinds` are not indexed in either tier 1 or tier 2 and never receive events.
- `kinds` absent (no field) means "match all kinds" — indexed in tier 2 (channel wildcard) or tier 3 (global).

### REQ Handler Access Control

The REQ handler checks channel access **before** registering the subscription:

```
1. Parse filters, extract channel_id
2. Load accessible_channel_ids for this connection's pubkey
3. If channel_id not in accessible_channels → send CLOSED "restricted: not a channel member"
4. Only then: sub_registry.register(conn_id, sub_id, filters, channel_id)
```

This prevents a race where a non-member receives live fan-out events from a private channel between registration and the access check.

### Historical Query (EOSE)

After registering, the REQ handler queries MySQL for stored events matching the filters (up to 500 per filter, hard cap). These are sent as `["EVENT", sub_id, event]` frames before `["EOSE", sub_id]`. New events arriving after EOSE are delivered via the fan-out path.

---

## 6. Crate Reference

### sprout-core — Shared Types and Verification

**726 LOC. Zero I/O.** The foundation every other crate builds on. Explicitly prohibits tokio, sqlx, redis, and axum in its `Cargo.toml`.

**Key types:**

```rust
pub struct StoredEvent {
    pub event: nostr::Event,
    pub received_at: DateTime<Utc>,
    pub channel_id: Option<Uuid>,
    verified: bool,          // private — use is_verified()
}

pub const ALL_KINDS: &[u32]  // 74 entries
```

**Key functions:**

| Function | Purpose |
|----------|---------|
| `filters_match(filters, event)` | OR across filters, AND within each filter. Includes NIP-01 prefix matching on event IDs. |
| `verify_event(event)` | Schnorr signature + SHA-256 ID check. CPU-bound — callers use `spawn_blocking`. |
| `is_private_ip(ip)` | SSRF protection: IPv4 loopback/private/link-local/CGNAT/benchmarking + IPv6 loopback/ULA/link-local/multicast + IPv4-mapped IPv6. |

**Does NOT:** store events, make network calls, spawn tasks, or depend on any async runtime.

---

### sprout-auth — Authentication and Authorization

**1,810 LOC.** Handles all four authentication paths, JWKS caching, scope enforcement, and token operations.

**Four auth paths:**

| Path | Entry Point | Notes |
|------|-------------|-------|
| NIP-42 only | `verify_auth_event()` | Dev mode; grants `[MessagesRead, MessagesWrite]` |
| NIP-42 + Okta JWT | `verify_auth_event()` | JWT in `auth` tag; JWKS-validated |
| NIP-42 + API token | `verify_auth_event()` | `auth_token` tag; constant-time hash compare |
| HTTP Bearer JWT | `validate_bearer_jwt()` | REST endpoints; skips pubkey cross-check; always adds `ChannelsRead` |

**Key types:**

```rust
pub struct AuthContext { pub pubkey: PublicKey, pub scopes: Vec<Scope>, pub auth_method: AuthMethod }
pub enum AuthMethod { Nip42PubkeyOnly, Nip42Okta, Nip42ApiToken }
pub enum Scope { MessagesRead, MessagesWrite, ChannelsRead, ChannelsWrite,
                 AdminChannels, UsersRead, UsersWrite, AdminUsers,
                 JobsRead, JobsWrite, SubscriptionsRead, SubscriptionsWrite,
                 FilesRead, FilesWrite, Unknown(String) }
pub trait ChannelAccessChecker: Send + Sync { ... }
pub trait RateLimiter: Send + Sync { ... }
```

**Security details:**
- JWKS double-checked locking: two read-lock checks before fetching, HTTP fetch with no lock held, write-lock re-check after. Cache TTL: 300 seconds.
- Token comparison: `subtle::ConstantTimeEq` — constant-time, prevents timing attacks.
- Token format: `sprout_<64-hex-chars>` (71 chars). `hash_token()` → SHA-256 → stored hash.
- Scopeless JWT defaults to `[MessagesRead]` only (not read+write).
- NIP-42 timestamp tolerance: ±60 seconds.
- Dev-only key derivation: `SHA-256("sprout-test-key:{username}")` — gated behind `#[cfg(any(test, feature = "dev"))]`. The `dev` feature must not be enabled in production relay deployments.

**Does NOT:** implement `RateLimiter` beyond a test stub (`AlwaysAllowRateLimiter`, gated behind `#[cfg(any(test, feature = "test-utils"))]`). No Redis-backed rate limiter exists anywhere in the codebase — rate limiting is not currently enforced. `RateLimitConfig` defines 4 tiers (human, agent-standard, agent-elevated, agent-platform) as a design target.

---

### sprout-db — MySQL Event Store

**3,698 LOC.** All database access. Uses `sqlx::query()` (runtime, not compile-time macros) — no `.sqlx/` offline cache required.

**Key operations:**

| Module | Responsibility |
|--------|---------------|
| `event.rs` | `insert_event` (INSERT IGNORE), `query_events` (QueryBuilder), `get_event_by_id` |
| `channel.rs` | Channel CRUD, membership management, role enforcement (transactional) |
| `feed.rs` | `query_mentions` (JSON_CONTAINS), `query_needs_action`, `query_activity` |
| `workflow.rs` | Full workflow/run/approval CRUD; SHA-256 hashed approval tokens |
| `partition.rs` | Monthly range partitioning for `events` and `delivery_log` tables |
| `api_token.rs` | Token creation; receives pre-hashed token from caller |

**Channel types:** `Stream`, `Forum`, `Dm`, `Workflow`  
**Member roles:** `Owner`, `Admin`, `Member`, `Guest`, `Bot`  
**Workflow statuses:** `Active`, `Disabled`, `Archived`  
**Run statuses:** `Pending`, `Running`, `WaitingApproval`, `Completed`, `Failed`, `Cancelled`

**Key behaviors:**
- `INSERT IGNORE` for event dedup — returns `(StoredEvent, was_inserted: bool)`.
- Rejects `KIND_AUTH` (22242) and ephemeral (20000–29999) with distinct error variants.
- Transactional role enforcement in `add_member`/`remove_member`/`create_channel` — TOCTOU-safe.
- Soft-delete for channel members: `remove_member` sets `removed_at`; re-adding reverses it.
- Feed hard cap: `FEED_MAX_LIMIT = 100` rows regardless of caller-requested limit.
- `query_mentions` uses `JSON_CONTAINS(tags, '["p","<pubkey>"]', '$')` — full table scan (no JSON index). Phase 2 plan: normalized `mentions` table with composite index on `(pubkey_hex, created_at)`.
- Approval tokens: raw token never reaches the DB — caller hashes with SHA-256 before passing to `create_api_token`.
- DDL injection protection in partition manager: allowlist of table names + strict suffix/date validators.

**Does NOT:** cache queries, implement connection pooling logic (delegated to sqlx), or make network calls outside MySQL.

---

### sprout-pubsub — Redis Pub/Sub, Presence, Typing

**735 LOC.** Manages Redis pub/sub fan-out, presence tracking, and typing indicators.

**Architecture:**

```
Publisher  → pool connection   → PUBLISH sprout:channel:{uuid}
Subscriber → dedicated PubSub  → PSUBSCRIBE sprout:channel:*
                                  → broadcast::channel(4096)
```

The subscriber uses a **dedicated** `redis::aio::PubSub` connection — not from the pool. This is intentional: pool connections cannot hold `PSUBSCRIBE` state.

**Current state:** The subscriber loop is spawned in `sprout-relay/src/main.rs` and populates the broadcast channel. A consumer task subscribes via `pubsub.subscribe_local()`, calls `sub_registry.fan_out()` on each received event, and delivers matches to local WebSocket connections via `conn_manager.send_to()`. Multi-node fan-out is now wired end-to-end. Note: local-echo deduplication is not yet implemented — events published by the local relay instance are re-delivered to local subscribers via the Redis round-trip; NIP-01 client-side dedup handles this in practice (TODO: server-side dedup in a follow-up).

**Reconnection:** exponential backoff 1s → 30s (`backoff_secs * 2`). Backoff resets to 1s only after a clean stream end, not on each reconnect attempt.

**Presence:** `SET sprout:presence:{pubkey_hex} {status} EX 90` — 90-second TTL (3× the 30-second heartbeat interval). Single missed heartbeat does not cause presence flap.

**Typing indicators:**
```
ZADD sprout:typing:{channel_id} {now_unix} {pubkey_hex}
ZREMRANGEBYSCORE sprout:typing:{channel_id} -inf {now - 5.0}
EXPIRE sprout:typing:{channel_id} 60
```
5-second activity window. 60-second key TTL prevents orphaned empty sets.

**Does NOT:** implement the rate limiter. Does NOT store events. `PubSubManager` is not `Clone` — callers use `Arc<PubSubManager>`.

---

### sprout-search — Typesense Integration

**1,043 LOC.** Full-text search via Typesense. All HTTP calls use `reqwest` with `X-TYPESENSE-API-KEY`.

**Collection schema (7 fields):** `id`, `content`, `kind` (int32), `pubkey` (facet), `channel_id` (facet, optional), `created_at` (int64, default sort), `tags_flat` (string[]).

**Key behaviors:**
- `ensure_collection()` is idempotent: handles 409 race condition (another process created it between check and create).
- Tag flattening uses `\x1f` (ASCII unit separator) to avoid ambiguity with tag values containing colons (e.g., URLs in `r` tags).
- Upsert indexing: `POST /documents?action=upsert` (single), `POST /documents/import?action=upsert` (batch JSONL).
- `delete_event()` validates event ID (64-char hex) before constructing the URL — prevents path injection.
- `delete_event()` is idempotent: 404 treated as success.
- Permission filtering is **caller's responsibility** — `sprout-search` provides the `filter_by` mechanism but does not enforce access policy.

**Does NOT:** enforce channel membership or access control. Does NOT store events in MySQL.

---

### sprout-audit — Hash-Chain Audit Log

**732 LOC.** Tamper-evident append-only log with SHA-256 hash chaining.

**Hash chain:** each entry stores `prev_hash` (hash of the previous entry). `verify_chain()` walks entries and recomputes hashes to detect tampering. Genesis entry uses `GENESIS_HASH` (64 zeros).

**Hash covers:** seq (big-endian bytes), timestamp (RFC3339), event_id, event_kind (big-endian), actor_pubkey, action string, channel_id (16 bytes or 16 zero bytes if None), canonical metadata JSON (BTreeMap for deterministic key ordering), prev_hash.

**Single-writer guarantee:** `SELECT GET_LOCK("sprout_audit", 10)` before each transaction. Lock released via `DO RELEASE_LOCK(?)` in all branches including panic (`catch_unwind`).

**10 audit actions:** `EventCreated`, `EventDeleted`, `ChannelCreated`, `ChannelUpdated`, `ChannelDeleted`, `MemberAdded`, `MemberRemoved`, `AuthSuccess`, `AuthFailure`, `RateLimitExceeded`.

**Does NOT:** log `KIND_AUTH` (22242) events — returns `AuditError::AuthEventForbidden` immediately. Does NOT log ephemeral events (they never reach the audit pipeline).

---

### sprout-workflow — YAML-as-Code Automation Engine

**2,717 LOC.** Parses, validates, and executes channel-scoped workflow definitions.

**Workflow definition structure:**
```yaml
name: "Incident Triage"
trigger:
  on: message_posted
  filter: "str_contains(trigger_text, 'P1')"
steps:
  - id: notify
    action: send_message
    text: "P1 incident detected: {{trigger.text}}"
  - id: page
    if: "str_contains(trigger_text, 'production')"
    action: request_approval
    from: "{{trigger.author}}"
    message: "Page on-call?"
```

Note: Both `TriggerDef` and `ActionDef` use serde internally-tagged enums. Triggers use `on:` as the tag field; actions use `action:` as the tag field. Fields are flattened into the parent struct, not nested.

**4 trigger types:** `message_posted`, `reaction_added`, `schedule`, `webhook`

**7 action types:**

| Action | Description |
|--------|-------------|
| `send_message` | Post to the workflow's channel (or override channel) |
| `send_dm` | Direct message to a user (pubkey hex or `{{trigger.author}}`) |
| `set_channel_topic` | Update channel topic |
| `add_reaction` | React to the trigger message |
| `call_webhook` | HTTP POST to external URL (SSRF-protected, redirects disabled, 1 MiB response cap) |
| `request_approval` | Suspend execution; fields: `from`, `message`, `timeout` (default 24h) |
| `delay` | Pause execution (max 300 seconds) |

**Template variables:** `{{trigger.text}}`, `{{trigger.author}}`, `{{steps.ID.output.FIELD}}`. Single-pass resolution (not recursive). Unknown variables left as literal text.

**Condition evaluation:** `evalexpr` with `HashMapContext`. Dot notation converted to underscores (`trigger.text` → `trigger_text`). Custom functions registered: `str_contains`, `str_starts_with`, `str_ends_with`, `str_len`. 100ms timeout prevents adversarial expressions from blocking.

**Concurrency:** `Arc<Semaphore>` with 100 permits. `try_acquire()` — returns `CapacityExceeded` immediately rather than queuing.

**Approval gates:** `request_approval` action generates a UUID token (CSPRNG), stores hashed in DB, returns `StepResult::Suspended`. `execute_from_step()` resumes from the suspended step index with reconstructed outputs.

**Cron scheduler:** loop runs every 60 seconds. **Execution is TODO** — loop body logs "not yet implemented."

**Does NOT:** recursively resolve templates (single-pass only). Does NOT queue workflow runs when at capacity — returns `CapacityExceeded` immediately.

---

### sprout-proxy — NIP-28 Compatibility Proxy

**~4,500 LOC.** Lets standard Nostr clients (Coracle, nak, Amethyst, nostr-tools, nostr-sdk) read and write Sprout channels using the NIP-28 Public Chat Channels protocol. Connects to the relay as a WebSocket client; presents a standard NIP-01/NIP-11/NIP-28/NIP-42 interface to external clients.

**Key modules:** `server.rs` (Axum WebSocket server, NIP-11, NIP-42 auth, filter splitting), `translate.rs` (bidirectional kind/tag translation), `upstream.rs` (persistent relay connection with auto-reconnect and subscription replay), `channel_map.rs` (bidirectional UUID ↔ kind:40 event ID mapping), `shadow_keys.rs` (deterministic keypair derivation), `guest_store.rs` (pubkey-based guest registry), `invite_store.rs` (token-based invite system).

**Shadow keypairs:** `HMAC-SHA256(key=server_salt, msg=external_pubkey_bytes)` → secp256k1 secret key. Deterministic: same external pubkey always produces the same shadow key. Empty salt rejected. Cache: `DashMap` with `MAX_CACHE_SIZE = 10,000`. Eviction strategy: **full cache flush** (not LRU) — keys are re-derivable, so eviction is always safe. Count tracked with `AtomicUsize` (soft bound — may briefly exceed limit under concurrent inserts).

**Kind translation (lossy):**

*Inbound (client → relay):*

| Standard Kind | Sprout Kind | Note |
|--------------|-------------|------|
| 1, 40, 42 | KIND_STREAM_MESSAGE | Multiple → one (lossy) |
| 41, 44 | KIND_STREAM_MESSAGE_EDIT | Multiple → one (lossy) |
| 4 | KIND_DM_CREATED | Encrypted DM |
| 43 | KIND_NIP29_DELETE_EVENT | NIP-29 delete |

*Outbound (relay → client):*

| Sprout Kind | Standard Kind | Note |
|-------------|--------------|------|
| KIND_STREAM_MESSAGE | 42 | NIP-28 channel message |
| KIND_STREAM_MESSAGE_V2 | 42 | Rich format collapses to plain kind:42 |
| KIND_STREAM_MESSAGE_EDIT | 41 | NIP-28 channel message edit |
| KIND_DM_CREATED | 4 | Encrypted DM |
| KIND_NIP29_DELETE_EVENT | 43 | NIP-29 delete |

`to_sprout(to_standard(k))` is NOT lossless for secondary mappings (e.g., kind:1 → KIND_STREAM_MESSAGE → kind:42). Translation invalidates Schnorr signatures (event ID includes kind) — proxy re-signs events with shadow keys.

**Dual auth:** Pubkey-based guest registration (persistent, primary) + invite tokens (ad-hoc, time-limited, secondary). Both use NIP-42 for the authentication handshake. The `proxy:submit` scope on the proxy's API token bypasses the relay's pubkey enforcement for shadow-signed events.

**Channel map:** Loaded at startup from the relay's REST API. kind:40 events are synthesized locally only. kind:41 is split: synthesized metadata is served locally, but kind:41 filters are also forwarded upstream (translated to kind:40003) to capture edit events. Channels created after proxy start require a restart to appear.

**State is in-memory.** Guest registrations, invite tokens, and channel map are lost on proxy restart.

**Does NOT:** implement relay-side lifecycle event emission — the relay does not emit events when proxy clients connect or disconnect (planned).

---

### sprout-huddle — LiveKit Audio/Video Integration

**659 LOC.** Mints LiveKit JWT tokens and parses LiveKit webhook events. In-memory session tracking.

**JWT token:** HS256, 6-hour TTL (overridable). Claims: `iss` (api_key), `sub` (identity), `iat`, `exp`, `name`, `video` (VideoGrant: room, roomJoin, canPublish, canSubscribe).

**Webhook verification:** HMAC-SHA256 of raw body bytes, hex-encoded. Constant-time comparison via `hmac` crate's built-in `verify_slice`.

**5 webhook event types:** `RoomStarted`, `RoomFinished`, `ParticipantJoined`, `ParticipantLeft`, `TrackPublished`.

**Session tracking:** `HuddleSession` with `Vec<HuddleParticipant>`. Participants tracked with `joined_at`, `left_at`, and `Vec<TrackInfo>`. **Sessions are lost on process restart** — in-memory only.

**Room naming:** `"sprout-{uuid}"` format via `create_room_name(channel_id)`.

**Does NOT:** emit Nostr events for huddle lifecycle (relay-side integration is planned). Does NOT persist session state.

---

### sprout-relay — The Server

**4,852 LOC.** Axum WebSocket server. Ties all other crates together. The only crate that imports and orchestrates all subsystems.

**`AppState`** (Arc-wrapped, shared across all connections):

```rust
pub struct AppState {
    db: Db,
    audit: AuditService,
    pubsub: Arc<PubSubManager>,
    auth: AuthService,
    search: SearchService,
    sub_registry: Arc<SubscriptionRegistry>,
    conn_manager: Arc<ConnectionManager>,
    workflow_engine: WorkflowEngine,
    conn_semaphore: Arc<Semaphore>,       // connection limit
    handler_semaphore: Arc<Semaphore>,    // 64 concurrent handlers
}
```

**`ConnectionState`** (per-connection):

```rust
pub struct ConnectionState {
    pub auth_state: RwLock<AuthState>,
    pub subscriptions: Mutex<HashMap<String, Vec<Filter>>>,
    // + send_tx, cancel token
}
pub enum AuthState { Pending { challenge: String }, Authenticated(AuthContext), Failed }
```

**REST API endpoints:**

| Method | Path | Handler |
|--------|------|---------|
| GET | `/api/channels` | List accessible channels |
| GET | `/api/search` | Full-text search via Typesense |
| GET | `/api/agents` | List agent accounts |
| GET | `/api/presence` | Presence status (bulk) |
| GET | `/api/feed` | Personalized feed (mentions/needs-action/activity) |
| GET/POST | `/api/channels/{id}/workflows` | List/create channel workflows |
| GET/PUT/DELETE | `/api/workflows/{id}` | Workflow CRUD |
| GET | `/api/workflows/{id}/runs` | Execution history |
| POST | `/api/workflows/{id}/trigger` | Manual trigger |
| POST | `/api/workflows/{id}/webhook` | Webhook trigger (HMAC-verified) |
| POST | `/api/approvals/{token}/grant` | Approve a workflow step |
| POST | `/api/approvals/{token}/deny` | Deny a workflow step |
| GET | `/info` | NIP-11 relay info |
| GET | `/.well-known/nostr.json` | NIP-05 identity |
| GET | `/health` | Health check |

**Constants:**

| Constant | Value | Purpose |
|----------|-------|---------|
| `MAX_FRAME_BYTES` | 65,536 | Max WebSocket frame size |
| `MAX_SUBSCRIPTIONS` | 100 | Per-connection subscription limit |
| `MAX_HISTORICAL_LIMIT` | 500 | Per-filter historical query cap |
| `handler_semaphore` capacity | 64 | Concurrent EVENT/REQ handlers |

**Does NOT:** implement business logic — delegates to the appropriate crate for every operation.

---

### sprout-mcp — Agent API Surface

**1,748 LOC.** stdio MCP server using the `rmcp` SDK. The interface through which AI agents interact with Sprout. Logs to stderr (stdout is the MCP JSON-RPC channel).

**43 tools:**

| Category | Tools |
|----------|-------|
| Messaging | `send_message`, `get_channel_history` |
| Channels | `list_channels`, `create_channel` |
| Canvas | `get_canvas`, `set_canvas` |
| Workflows | `list_workflows`, `create_workflow`, `update_workflow`, `delete_workflow`, `trigger_workflow`, `get_workflow_runs`, `approve_workflow_step` |
| Feed | `get_feed`, `get_feed_mentions`, `get_feed_actions` |

**Key implementation details:**
- Connects to relay via WebSocket (`tokio_tungstenite`). Handles NIP-42 auth automatically.
- Ephemeral keypair generated if `SPROUT_PRIVATE_KEY` not set (printed to stderr).
- Exponential backoff reconnection: 1s → 30s. Resubscribes all active subscriptions after reconnect.
- REST calls use `Authorization: Bearer <token>` when `SPROUT_API_TOKEN` is set; falls back to `X-Pubkey: <hex>` in dev mode.
- `create_channel` sends a signed Nostr kind 40 event (not a REST call).
- `set_canvas` sends kind 40100 with `e` tag pointing to channel.
- UUID validation at tool boundary before any network call.
- `MAX_CONTENT_BYTES = 65,536` enforced in `send_message`.
- `get_channel_history` caps at 200 results; `get_workflow_runs` caps at 100; `get_feed` max 50 per category.

**Does NOT:** persist state. Does NOT implement server-side logic — it's a thin client over the relay's WebSocket and REST APIs.

---

### sprout-admin — Operator CLI

**144 LOC.** Two subcommands:

| Subcommand | Purpose |
|------------|---------|
| `mint-token` | Generate API token, store SHA-256 hash in DB, display raw token once |
| `list-tokens` | List all active tokens (ID, name, scopes, created) |

`mint-token` options: `--name`, `--scopes` (comma-separated), optional `--pubkey`. If `--pubkey` omitted, generates a new keypair and displays `nsec` (bech32) and pubkey.

Raw token is shown exactly once and never stored. Only the SHA-256 hash reaches the database.

---

### sprout-test-client — Integration Test Harness

**3,362 LOC** (including `tests/` directory — 2,559 lines of e2e tests across 4 files).

**`SproutTestClient`** wraps a WebSocket connection with a `VecDeque<RelayMessage>` buffer for message interleaving. Methods: `connect`, `connect_unauthenticated`, `authenticate`, `send_event`, `send_text_message`, `subscribe`, `close_subscription`, `recv_event`, `collect_until_eose`, `disconnect`.

**Test coverage:**

| File | Tests | Scope |
|------|-------|-------|
| `tests/e2e_relay.rs` | 13 | WebSocket protocol (auth, subscriptions, filters, limits, NIP-11) |
| `tests/e2e_rest_api.rs` | 18 | REST API (channels, search, presence, agents, feed) |
| `tests/e2e_workflows.rs` | 4 | Workflow CRUD, trigger, and execution |
| `tests/e2e_mcp.rs` | 7 | MCP tool integration (messaging, channels, canvas, feed) |
| `src/lib.rs` | 4 | Unit tests (message parsing, event construction) |

All e2e tests are `#[ignore]` — require a running relay. Total: **42 e2e tests + 4 unit tests**.

`src/main.rs` is a manual testing CLI (`sprout-test-cli`) with `--send`, `--subscribe`, `--channel`, `--url`, `--kind` flags.

Re-exports `parse_relay_message`, `OkResponse`, `RelayMessage` from `sprout-mcp` to avoid duplicating the wire protocol parser.

---

## 7. Security Model

Every security-sensitive operation uses an explicit, verified pattern. No implicit trust.

### Authentication

| Concern | Mechanism |
|---------|-----------|
| Token comparison | `subtle::ConstantTimeEq` — prevents timing attacks |
| Token storage | SHA-256 hash only — raw token shown once at mint, never stored |
| JWKS cache | Double-checked locking; HTTP fetch with no lock held (prevents global DoS) |
| NIP-42 timestamp | ±60 second tolerance — prevents replay attacks |
| AUTH events | Never stored in MySQL, never logged in audit chain |
| Scopeless JWT | Defaults to `[MessagesRead]` only — least-privilege default |

### Input Validation

| Concern | Mechanism |
|---------|-----------|
| Schnorr signatures | `verify_event()` in `sprout-core` — every event verified before storage |
| Event ID | SHA-256 of canonical serialization verified independently of signature |
| Frame size | `MAX_FRAME_BYTES = 65,536` — oversized frames rejected, connection closed |
| Search event IDs | 64-char hex validation before URL construction — prevents path injection |
| Workflow step IDs | Alphanumeric + underscore only — prevents evalexpr variable injection |
| Partition names | Allowlist of table names + strict suffix/date validators — prevents DDL injection |

### SSRF Protection

`is_private_ip()` in `sprout-core` covers:
- IPv4: loopback (127.0.0.0/8), private (10/8, 172.16/12, 192.168/16), link-local (169.254/16), CGNAT (100.64/10), benchmarking (198.18/15)
- IPv6: loopback (::1), ULA (fc00::/7), link-local (fe80::/10), multicast (ff00::/8)
- IPv4-mapped IPv6 (::ffff:0:0/96) — recursively checks the embedded IPv4 address

Applied in: `sprout-workflow` (CallWebhook action), `sprout-core` (shared utility).

### Audit Integrity

- Hash chain: each entry's SHA-256 covers all fields including `prev_hash` — tampering any entry breaks all subsequent hashes
- Canonical JSON: `BTreeMap` for deterministic key ordering — hash is reproducible
- Single-writer lock: `GET_LOCK("sprout_audit", 10)` — prevents concurrent writes from breaking the chain
- Panic-safe: `catch_unwind` ensures lock release even on panic

### Access Control

- Channel membership is the only gate — enforced by the relay at every operation
- REQ handler checks access before subscription registration — no race window for private channel leaks
- TOCTOU-safe membership operations: all check-then-modify sequences run inside MySQL transactions
- Approval tokens: UUID (CSPRNG), stored as SHA-256 hash, single-use enforced with `AND status = 'pending'` in UPDATE

### Webhook Security

- LiveKit webhooks: HMAC-SHA256 of raw body bytes, hex-encoded, constant-time comparison
- Workflow webhooks: HMAC-SHA256 secret verification before processing
- Outbound webhooks (CallWebhook): SSRF protection + redirects disabled + 1 MiB response cap

---

## 8. Infrastructure

Docker Compose provides the full local development stack. All services include health checks and resource limits.

### Services

| Service | Image | Port | Purpose |
|---------|-------|------|---------|
| MySQL | `mysql:8.0` | 3306 | Primary event store — events, channels, tokens, workflows, audit |
| Redis | `redis:7-alpine` | 6379 | Pub/sub fan-out, presence (SET EX), typing (sorted sets) |
| Typesense | `typesense/typesense:27.1` | 8108 | Full-text search index |
| Adminer | `adminer` | 8080 | MySQL web UI (dev only) |
| Keycloak | `quay.io/keycloak/keycloak:26` | 8443 | Local OAuth/OIDC stand-in for Okta |

### MySQL Schema (key tables)

| Table | Purpose |
|-------|---------|
| `events` | All stored Nostr events; monthly range-partitioned by `TO_DAYS(created_at)` |
| `channels` | Channel records (type, visibility, canvas, topic) |
| `channel_members` | Membership with roles; soft-delete via `removed_at` |
| `workflows` | Workflow definitions (YAML stored as canonical JSON) |
| `workflow_runs` | Execution records with trigger context and trace |
| `workflow_approvals` | Approval gates (token stored as SHA-256 hash) |
| `api_tokens` | API token records (hash only, never plaintext) |
| `audit_log` | Hash-chain audit entries |
| `delivery_log` | Delivery tracking (partitioned; Rust module pending) |

### Redis Key Patterns

| Pattern | Type | TTL | Purpose |
|---------|------|-----|---------|
| `sprout:channel:{uuid}` | Pub/Sub channel | — | Event fan-out |
| `sprout:presence:{pubkey_hex}` | String | 90s | Online/away status |
| `sprout:typing:{channel_uuid}` | Sorted Set | 60s | Active typers (5s window) |

### Typesense Collection

Single collection (`events` by default, configurable via `TYPESENSE_COLLECTION`). Schema: `id`, `content`, `kind` (int32), `pubkey` (facet), `channel_id` (facet, optional), `created_at` (int64, default sort), `tags_flat` (string[]).

---

## 9. Known Limitations

These are verified gaps in the current implementation — not design aspirations.

| # | Limitation | Detail |
|---|-----------|--------|
| 1 | **No sqlx offline query cache** | Uses `sqlx::query()` (runtime) not `sqlx::query!()` (compile-time). No `.sqlx/` directory. Queries are not validated at compile time. |
| 2 | **Feed mentions: full table scan** | `query_mentions` uses `JSON_CONTAINS(tags, '["p","<pubkey>"]', '$')` — no index on JSON column. Phase 2 mitigation plan documented in `sprout-db/src/feed.rs`: normalized `mentions` table with composite index on `(pubkey_hex, created_at)`. |
| 3 | **No rate limiting implementation** | `RateLimiter` trait exists in `sprout-auth`. Only implementation is `AlwaysAllowRateLimiter` (test stub, gated behind `#[cfg(any(test, feature = "test-utils"))]`). `RateLimitConfig` defines 4 tiers (human, agent-standard, agent-elevated, agent-platform) but none are enforced. |
| 4 | **Local-echo deduplication** | Multi-node fan-out is wired: the Redis `PSUBSCRIBE` subscriber loop runs, and a consumer task fans out received events to local WebSocket connections. However, events published by the local relay instance are re-delivered to local subscribers via the Redis round-trip (no server-side dedup). NIP-01 client-side dedup handles this in practice. Server-side dedup is a TODO. |
| 5 | **Cron scheduler is a stub** | `WorkflowEngine::run()` loops every 60 seconds but the loop body logs "not yet implemented" (TODO WF-07). Schedule-triggered workflows do not fire. |
| 6 | **Typing indicators: cross-node only** | Typing events (kind 20002) are published to Redis via the ephemeral pipeline. The multi-node consumer task fans them out to local WS subscribers when received from Redis (cross-node path). However, there is no direct local fan-out for typing events on the originating node — they travel Redis → broadcast → WS rather than being fanned out in-process before the Redis round-trip. Typing state is also queryable via the REST `/api/presence` endpoint. |
| 7 | **sprout-huddle is scaffolding** | `sprout-huddle` defines types, token generation, and webhook parsing, but relay-side lifecycle event emission is not implemented. Huddle state events are not wired into the relay's event pipeline. `sprout-proxy` is now functional — see its section above. |

---

## Appendix: LOC Summary

| Crate | LOC | Layer |
|-------|-----|-------|
| sprout-core | 726 | Foundation |
| sprout-auth | 1,810 | Foundation |
| sprout-db | 3,698 | Foundation |
| sprout-pubsub | 735 | Foundation |
| sprout-search | 1,043 | Foundation |
| sprout-audit | 732 | Foundation |
| sprout-workflow | 2,717 | Foundation |
| sprout-proxy | ~4,500 | Client compatibility |
| sprout-huddle | 659 | Standalone |
| sprout-relay | 4,852 | Server |
| sprout-mcp | 1,748 | Agent API |
| sprout-admin | 144 | Tooling |
| sprout-test-client | 3,362 | Tooling |
| **Total** | **~22,739** | |

*LOC counted with `find crates -name '*.rs' | xargs wc -l`. Includes tests. Measured 2026-03-09.*


