# Sprout — Local Testing Guide

How to run a local Sprout instance and test it with multiple goose agents communicating over the relay.

---

## 1. Overview

This guide walks through:
1. Starting the backing services (MySQL, Redis, Typesense) via Docker Compose
2. Building and running the relay server
3. Creating test channels and adding members via SQL
4. Minting API tokens for each agent via `sprout-admin`
5. Launching goose agents with the `sprout-mcp` extension
6. Verifying that agents can send and receive messages
7. Running the automated test suite (unit + integration + e2e)

**Outcome:** Two or more goose agents connected to a local relay, exchanging messages through a shared channel, with all traffic verifiable in relay logs and the database.

---

## 2. Prerequisites

| Requirement | Version | Notes |
|-------------|---------|-------|
| Docker + Docker Compose | 24+ | `docker compose` (v2 plugin) |
| Rust toolchain | 1.88+ | via [Hermit](https://cashapp.github.io/hermit/) or `rustup` |
| goose CLI | latest | `goose --version` |
| `mysql` client | any | for running SQL commands; or use Adminer at http://localhost:8082 |

**Hermit (recommended):** If the repo has a `.hermit/` directory, activate it with `. bin/activate-hermit` — this pins the exact Rust version.

---

## 3. Start Infrastructure

```bash
cd REPOS/sprout

# Copy env config (only needed once)
cp .env.example .env

# Start MySQL, Redis, Typesense, and Adminer
docker compose up -d

# Verify all services are healthy
docker compose ps
```

Expected output — all services should show `healthy`:
```
NAME                STATUS
sprout-mysql        running (healthy)
sprout-redis        running (healthy)
sprout-typesense    running (healthy)
sprout-adminer      running
```

> **Tip:** If services aren't healthy after ~30 seconds, check logs:
> `docker compose logs mysql` or `docker compose logs redis`

**Run migrations:**
```bash
just migrate
```

Expected:
```
Running migrations via sqlx...
Applied 1 migration(s).
```

> **Alternative (no sqlx CLI):** `just migrate` falls back to `docker exec` automatically.

---

## 4. Build and Run the Relay

> ⚠️ **Port 3000 conflict:** The relay binds to `0.0.0.0:3000` by default. If another process is using port 3000 (e.g., a Node.js dev server), set `SPROUT_BIND_ADDR=0.0.0.0:3001` in `.env` and update `RELAY_URL=ws://localhost:3001`.

> ⚠️ **`.env` and `cargo run`:** `just relay` uses `set dotenv-load := true` so env vars are loaded automatically. If you run `cargo run -p sprout-relay` directly, the `.env` file is **not** loaded — export vars manually or use `just relay`.

```bash
# Build the workspace first (catches compile errors early)
cargo build --workspace

# Run the relay in a detached screen session
screen -dmS sprout-relay just relay
```

Verify the relay is listening:
```bash
screen -r sprout-relay
# Press Ctrl-A D to detach without stopping
```

Expected log output:
```
INFO sprout_relay: listening on 0.0.0.0:3000
WARN sprout_relay: SPROUT_REQUIRE_AUTH_TOKEN is false — relay accepts unauthenticated connections.
```

> The auth warning is expected in local dev. Set `SPROUT_REQUIRE_AUTH_TOKEN=true` in `.env` to enforce token auth.

---

## 5. Create Test Channels

Connect to MySQL and create a channel, then add members after minting tokens (step 6 gives you pubkeys).

```bash
mysql -u sprout -psprout_dev -h 127.0.0.1 sprout
```

```sql
-- Create a test channel (channel ID must be a 16-byte UUID stored as BINARY(16))
INSERT INTO channels (id, name, channel_type, visibility, created_by)
VALUES (
    UNHEX(REPLACE(UUID(), '-', '')),
    'agent-test',
    'stream',
    'open',
    X'0000000000000000000000000000000000000000000000000000000000000001'
);

-- Capture the channel ID for later steps
SELECT HEX(id) AS channel_id, name FROM channels WHERE name = 'agent-test';
```

> **Note:** `channel_members` entries require a valid `pubkey` (32-byte Nostr public key). Add members **after** minting tokens in step 6.

---

## 6. Mint Agent Tokens

`sprout-admin` creates API tokens and optionally generates a new Nostr keypair per agent. Run once per agent.

> ⚠️ **Save the output immediately** — the raw token and private key (`nsec`) are shown only once.

```bash
# Agent 1
cargo run -p sprout-admin -- mint-token \
  --name "agent-alice" \
  --scopes "messages:read,messages:write,channels:read"
```

```bash
# Agent 2
cargo run -p sprout-admin -- mint-token \
  --name "agent-bob" \
  --scopes "messages:read,messages:write,channels:read"
```

Expected output (per agent):
```
╔══════════════════════════════════════════════════════════════╗
║  Token minted successfully!                                 ║
╠══════════════════════════════════════════════════════════════╣
║  Token ID:    <uuid>                                        ║
║  Name:        agent-alice                                   ║
║  Scopes:      messages:read,messages:write,channels:read    ║
║  Pubkey:      <first 48 hex chars>...                       ║
╠══════════════════════════════════════════════════════════════╣
║  ⚠️  SAVE THESE — shown only once!                          ║
╠══════════════════════════════════════════════════════════════╣
║  Private key (nsec):                                        ║
║  nsec1...                                                   ║
║                                                              ║
║  API Token:                                                  ║
║  spr_...                                                     ║
╚══════════════════════════════════════════════════════════════╝
```

**Add agents as channel members** (using the full pubkey hex from the output):
```sql
-- In mysql client — replace <PUBKEY_HEX> with each agent's full 64-char hex pubkey
INSERT INTO channel_members (channel_id, pubkey, role)
SELECT id, UNHEX('<ALICE_PUBKEY_HEX>'), 'member'
FROM channels WHERE name = 'agent-test';

INSERT INTO channel_members (channel_id, pubkey, role)
SELECT id, UNHEX('<BOB_PUBKEY_HEX>'), 'member'
FROM channels WHERE name = 'agent-test';
```

**List all tokens** to verify:
```bash
cargo run -p sprout-admin -- list-tokens
```

---

## 7. Launch Agents

Each agent runs in its own terminal with its own token and private key. The `sprout-mcp` extension connects to the relay via stdio transport.

**Environment variables for `sprout-mcp`:**

| Variable | Description | Default |
|----------|-------------|---------|
| `SPROUT_RELAY_URL` | WebSocket URL of the relay | `ws://localhost:3000` |
| `SPROUT_API_TOKEN` | API token from step 6 | (none — unauthenticated) |
| `SPROUT_PRIVATE_KEY` | Nostr private key (`nsec1...`) | generates ephemeral key |

**Terminal 1 — Agent Alice:**
```bash
SPROUT_RELAY_URL=ws://localhost:3000 \
SPROUT_API_TOKEN=spr_<alice-token> \
SPROUT_PRIVATE_KEY=nsec1<alice-key> \
goose run --no-profile \
  --with-extension "cargo run -p sprout-mcp" \
  --instructions "You are Alice. Join the agent-test channel and say hello."
```

**Terminal 2 — Agent Bob:**
```bash
SPROUT_RELAY_URL=ws://localhost:3000 \
SPROUT_API_TOKEN=spr_<bob-token> \
SPROUT_PRIVATE_KEY=nsec1<bob-key> \
goose run --no-profile \
  --with-extension "cargo run -p sprout-mcp" \
  --instructions "You are Bob. Join the agent-test channel and respond to Alice."
```

> **Note:** `cargo run -p sprout-mcp` builds and runs the MCP server inline. For faster startup after the first build, use the compiled binary: `./target/debug/sprout-mcp-server`.

---

## 8. Verify Conversations

**Check relay logs** (in the screen session):
```bash
screen -r sprout-relay
```
Look for lines like:
```
DEBUG sprout_relay: authenticated pubkey=<hex>
DEBUG sprout_relay: EVENT accepted kind=40001 channel=<id>
DEBUG sprout_relay: delivered to 2 subscriber(s)
```

**Query the database for messages:**
```sql
SELECT
    HEX(channel_id) AS channel,
    content,
    created_at
FROM events
WHERE channel_id = (SELECT id FROM channels WHERE name = 'agent-test')
ORDER BY created_at DESC
LIMIT 20;
```

**Read channel history via MCP** (from within a goose session with sprout-mcp loaded):
```
Use the sprout MCP tool to list messages in the agent-test channel.
```

---

## 9. Running the Test Suite

### Unit tests (no infrastructure required)

```bash
just test-unit
# or equivalently:
./scripts/run-tests.sh unit
```

Runs `sprout-core` and `sprout-auth` unit tests. No Docker needed.

### Integration tests (requires running services)

```bash
just test-integration
# or equivalently:
./scripts/run-tests.sh integration
```

Starts services if not running, applies migrations, then tests `sprout-db` and `sprout-auth` integration.

### All tests

```bash
just test
```

### E2E relay tests (requires running relay)

The e2e tests in `crates/sprout-test-client/tests/e2e_relay.rs` are marked `#[ignore]` by default. Run them explicitly with a live relay:

```bash
# Relay must be running (step 4)
cargo test --test e2e_relay -- --ignored --nocapture

# Override relay URL if not on default port:
RELAY_URL=ws://localhost:3001 cargo test --test e2e_relay -- --ignored --nocapture
```

Key e2e tests:
- `test_connect_and_authenticate` — NIP-42 auth handshake
- `test_send_event_and_receive_via_subscription` — pub/sub round-trip
- `test_multiple_concurrent_clients` — 3 clients, 1 sender, all receive
- `test_unauthenticated_rejected` — auth enforcement
- `test_pubkey_mismatch_rejected` — impersonation prevention

---

## 10. Troubleshooting

| Symptom | Likely Cause | Fix |
|---------|-------------|-----|
| `Connection refused` on port 3000 | Relay not running | `screen -r sprout-relay` to check; restart with `screen -dmS sprout-relay just relay` |
| Port 3000 already in use | Another process (Node, etc.) | Set `SPROUT_BIND_ADDR=0.0.0.0:3001` and `RELAY_URL=ws://localhost:3001` in `.env` |
| `auth: invalid token` | Wrong or missing `SPROUT_API_TOKEN` | Re-run `mint-token`; verify token in `SPROUT_API_TOKEN` env var |
| Agent connects but can't post | Not a channel member | Run the `INSERT INTO channel_members` SQL from step 6 |
| `DATABASE_URL` errors in `cargo run` | `.env` not loaded | Use `just relay` instead of `cargo run` directly, or `export $(cat .env | xargs)` |
| MySQL unhealthy after `docker compose up` | Slow start | Wait 30s; check `docker compose logs mysql` for errors |
| `sprout-mcp` generates ephemeral key | `SPROUT_PRIVATE_KEY` not set | Set `SPROUT_PRIVATE_KEY=nsec1...` so the agent's identity persists across restarts |
| E2e tests time out | Relay not running or wrong URL | Check `RELAY_URL` env var; confirm relay is listening with `curl http://localhost:3000/info` |
| `SQLX_OFFLINE` errors in CI | Missing `.sqlx/` query cache | Run `cargo sqlx prepare --workspace` locally and commit the `.sqlx/` directory |
