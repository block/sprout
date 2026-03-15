# WebSocket Hardening: Fix 8 Reliability Bugs Across MCP, ACP, and Relay

## Summary

Agents disconnect during idle periods because no background reader responds to the relay's Pings. Events dropped under backpressure are never replayed because the dedup set still contains their IDs. This PR fixes 8 bugs across the three WebSocket components.

**8 commits · 6 files · +1,644 / −437 lines · 14 new tests**

---

## Problem

Sprout agents have two WebSocket paths to the relay:

```
Agent ──► sprout-mcp-server ──► relay    (read/write: send messages, subscribe)
Agent ──► sprout-acp harness ──► relay   (receive-only: @mention notifications)
```

1. **MCP client**: No background WebSocket reader. When no caller is actively reading (between `subscribe()` / `send_event()` calls), incoming Pings go unanswered. The relay's heartbeat (30s interval, 3 missed pongs) disconnects the client after ~90s of idle time. A `reconnect()` method existed but was never called — dead code.

2. **ACP harness**: When the event channel fills (256 capacity, slow consumer), events are dropped via `try_send` and never replayed. The dedup set (`seen_ids`) still contains their IDs, so even after reconnect the replayed events are rejected as duplicates. Reconnect is caller-driven only — there's a gap window between socket loss and the caller noticing.

3. **Relay server**: Pong frames use `try_send` on the same data channel as EVENT messages. Under backpressure, Pong is silently dropped. A single buffer-full event immediately cancels the connection with no grace for transient stalls.

---

## The 8 Bugs

| # | Severity | Component | Bug | Fix |
|---|----------|-----------|-----|-----|
| 1 | Critical | MCP | No background reader — Pings unanswered when idle | Background task with `select!` loop responds to Pings inline |
| 2 | Critical | MCP | `reconnect()` never called — dead code | Auto-reconnect with exponential backoff (1s→2s→…→30s) |
| 3 | Critical | MCP | Mid-session AUTH challenges silently ignored | AUTH handled inline, response sent immediately |
| 4 | High | MCP | `Arc<Mutex<Inner>>` serializes all ops | mpsc command channel, single-owner background task |
| 5 | High | ACP | Events dropped permanently under backpressure | `channel_dropped_since` tracking + dedup removal on drop |
| 6 | High | ACP | Reconnect is caller-driven only | Autonomous reconnect (3 attempts, 1s→2s→4s) before fallback |
| 7 | Medium | Relay | Pong uses `try_send` on data channel | Separate `ctrl_tx/ctrl_rx` with priority drain |
| 8 | Medium | Relay | Single buffer-full → immediate disconnect | Shared grace counter (3 consecutive) before cancellation |

---

## Architecture

### MCP: Background Task Model

**Before**: `Arc<Mutex<Inner>>` wrapping the WebSocket. Individual methods (`wait_for_ok`, `collect_until_eose`) did handle Pings while actively reading, but between calls no reader was running — Pings arrived with nobody listening.

**After**: Single-owner background task with `tokio::select!`:

```
RelayClient (Clone-able)
  └── cmd_tx ──► run_background_task()
                   ├── ws.next()  → Ping→Pong, AUTH→respond, OK→resolve, EVENT→collect
                   ├── cmd_rx     → SendEvent, Subscribe, CloseSubscription, Shutdown
                   └── tick(1s)   → expire timed-out pending ops (10s default)
```

Key details:
- `BgTaskHandle` wraps `cmd_tx` + `JoinHandle`. `Drop` calls `try_send(Shutdown)` then `abort()` immediately — abort is a safety net, not a graceful shutdown path. Use `close()` for graceful shutdown.
- `close()` now enqueues a `Shutdown` command (previously it performed a WebSocket close handshake). This is a behavioral change — see Breaking Changes.
- `do_reconnect()` uses `tokio::select!` during backoff sleeps so Shutdown is processed promptly.
- Active subscriptions replayed after reconnect. EOSE removes one-shot subscriptions from the replay set. CLOSED removes subscriptions to prevent stale replay.
- 10s `CONNECT_TIMEOUT` wraps the TCP + WebSocket handshake in `do_connect()`.

### ACP: Event Replay + Autonomous Reconnect

**Before**: `try_send` returns `Full` → log warning → event gone forever. Dedup set retains the ID, blocking replay.

**After**:
- On `Full`: remove event ID from `seen_ids` (so replay passes dedup), record `channel_dropped_since[channel_id] = min(existing, event.created_at)`
- On reconnect: `since = min(last_seen, channel_dropped_since)` per channel
- `send_subscribe()` returns `bool` — drop tracker cleared only on successful REQ send
- `try_autonomous_reconnect()`: 3 attempts with 1s→2s→4s backoff. If all fail, falls back to caller-driven `wait_for_reconnect`
- Stale `Reconnect` commands drained after autonomous reconnect; other commands (Subscribe, Unsubscribe, SubscribeMembership) processed inline
- 10s `CONNECT_TIMEOUT` added to `do_connect()`
- Event channel capacity now configurable via `SPROUT_ACP_EVENT_BUFFER` env var (default 256)

### Relay: Control-Frame Priority + Shared Grace Counter

**Before**: Pong competes with EVENT on same `mpsc` channel. One `Full` → cancel.

**After**:
- Separate `ctrl_tx/ctrl_rx` (capacity 8) for Pong and Ping
- `send_loop` drains control channel before data with `biased` select
- Heartbeat Ping sent via `ctrl_tx.try_send()` (not data channel)
- `backpressure_count: Arc<AtomicU8>` shared between `ConnectionState::send()` and `ConnectionManager::send_to()` via the same `Arc` — both direct sends and fan-out broadcasts track one counter. 3 consecutive full events before cancel.

---

## Files Changed

| File | +/− | What |
|------|-----|------|
| `crates/sprout-mcp/src/relay_client.rs` | +1,097/−323 | Rewrite to background task + 8 in-module tests with mini relay servers |
| `crates/sprout-mcp/src/lib.rs` | +20/−4 | Updated module docs + ASCII architecture diagram |
| `crates/sprout-mcp/Cargo.toml` | +4 | `dev-dependencies` for test-util + tokio-tungstenite |
| `crates/sprout-acp/src/relay.rs` | +301/−78 | Bug 5 + 6 fixes, autonomous reconnect, 2 unit tests |
| `crates/sprout-relay/src/connection.rs` | +84/−20 | Control channel, grace counter, priority send loop |
| `crates/sprout-relay/src/state.rs` | +138/−12 | `ConnEntry` struct, shared backpressure counter, 4 unit tests |

---

## Tests

| Suite | Total | New | Result |
|-------|-------|-----|--------|
| sprout-mcp | 43 | +8 | ✅ |
| sprout-acp | 118 | +2 | ✅ |
| sprout-relay | 92 | +4 | ✅ |

### New MCP Tests (in-module, with mini relay servers)
- `bg_responds_to_ping_without_caller_activity` — Ping→Pong while idle
- `bg_handles_mid_session_auth_challenge` — re-auth on mid-session challenge
- `send_event_receives_ok_response` — EVENT→OK round-trip
- `subscribe_collects_events_until_eose` — REQ→EVENT×3→EOSE
- `send_event_times_out_when_no_ok` — 10s timeout with `tokio::time::pause`
- `close_subscription_sends_close_message` — CLOSE frame delivery
- `bg_reconnects_on_transport_close` — reconnect + send_event on new connection
- `shutdown_during_reconnect_exits_promptly` — graceful exit via Shutdown, not abort

### New ACP Unit Tests
- `acp_records_channel_dropped_since_on_backpressure` — min() semantics
- `acp_reconnect_uses_dropped_since_for_replay` — replay filter calculation

### New Relay Unit Tests
- `send_to_resets_grace_counter_on_success` — reset on success
- `send_to_increments_grace_counter_on_full` — increment on Full
- `send_to_cancels_after_grace_limit` — cancellation at limit
- `shared_counter_between_direct_and_fanout` — shared `Arc<AtomicU8>` accounting

### Manual E2E Testing (not in diff)
Tested locally with live agents against a freshly built relay:
- Idle survival past 65s (2+ heartbeat periods)
- Relay restart + new connections work
- No panics in relay log

---

## Breaking Changes

### `ConnectionState` (sprout-relay)
New public fields added to the struct:
- `pub ctrl_tx: mpsc::Sender<WsMessage>` — control-frame channel sender
- `pub backpressure_count: Arc<AtomicU8>` — shared grace counter

Any code constructing `ConnectionState` directly will need to provide these fields. Internal relay code already does. External consumers of this struct (if any) will need updating.

### `ConnectionManager::register()` (sprout-relay)
New parameter: `backpressure_count: Arc<AtomicU8>`.

### `RelayClient::close()` (sprout-mcp)
Previously performed a WebSocket close handshake directly. Now enqueues a `Shutdown` command to the background task. The connection is still closed, but asynchronously. Callers that depended on the WebSocket being closed by the time `close()` returns may need adjustment.

### New environment variable
`SPROUT_ACP_EVENT_BUFFER` — overrides the default event channel capacity (256) for the ACP harness. Optional, no action needed if unset.

---

## Commits

```
98af77f refactor(sprout-mcp): rewrite relay_client to background task architecture
1d4fb1b test(sprout-mcp): add integration tests for relay_client background task
d5ae55b fix(sprout-mcp): crossfire fixes — non-blocking reconnect, Drop safety, send-failure reconnect
56497bc fix(sprout-mcp): re-crossfire fixes — CLOSED cleanup, CloseSubscription reconnect, remove expect()
9cca058 fix(sprout-acp): harden ACP harness — Bug 5 (channel drop replay) + Bug 6 (autonomous reconnect)
a29d179 fix(sprout-acp): crossfire fixes — dedup safety, send_subscribe success tracking, connect timeout
bf8b867 fix(sprout-relay): control frame priority lane + slow-client grace period (Bugs 7-8)
356d34c fix: MCP connect timeout + relay grace counter tests + shutdown test hardening
```
