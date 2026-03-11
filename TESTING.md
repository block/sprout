# Sprout Testing Guide

This guide enables an AI agent (the **operator**) to run the full Sprout test suite: automated `cargo test` suites and a three-agent multi-agent E2E run that exercises all 41 MCP tools against a live relay.

## Two Test Modes

| Mode | What It Does | When to Use |
|------|-------------|-------------|
| **Automated** (`cargo test`) | Unit tests + REST/WebSocket/MCP integration tests | Fast CI check; verify no unit regressions |
| **Multi-Agent E2E** | Three agents (Alice, Bob, Charlie) run via `sprout-acp` harness, exercising all 41 MCP tools via real Nostr identities | Before merging relay/MCP/auth changes; full regression run; exploring new features |

Run both modes for a complete regression check. Run automated-only for a fast sanity check.

---

## Table of Contents

1. [Prerequisites](#1-prerequisites)
2. [Quick Start: Automated Tests Only](#2-quick-start-automated-tests-only)
3. [Multi-Agent E2E Testing](#3-multi-agent-e2e-testing)
   - [3.1 Architecture](#31-architecture)
   - [3.2 Infrastructure Setup](#32-infrastructure-setup)
   - [3.3 Mint Agent Keys](#33-mint-agent-keys)
   - [3.4 Launch Harness Instances](#34-launch-harness-instances)
   - [3.5 Test Exercises](#35-test-exercises)
   - [3.6 Monitoring & Verification](#36-monitoring--verification)
   - [3.7 Expected Results](#37-expected-results)
4. [Advanced: ACP Harness Scenarios](#4-advanced-acp-harness-scenarios)
5. [Workflow YAML Reference](#5-workflow-yaml-reference)
6. [The 41 MCP Tools](#6-the-41-mcp-tools)
7. [Cleanup](#7-cleanup)
8. [Known Issues / Troubleshooting](#8-known-issues--troubleshooting)

---

## 1. Prerequisites

Verify each requirement before proceeding. All commands must succeed.

### Docker

```bash
docker --version
# Required: any recent version

docker compose version
# Required: v2+ (uses "docker compose", not "docker-compose")
```

### Rust 1.88+

```bash
# From the sprout repo root — use Hermit if system Rust is older than 1.88
. bin/activate-hermit

rustc --version
# Required: rustc 1.88.0 or newer
```

### goose CLI

```bash
goose --version
# Must be on $PATH and configured with a valid provider/model

goose run --help | head -5
# Must not error
```

### sqlx-cli

```bash
sqlx --version
# If missing:
cargo install sqlx-cli --no-default-features --features mysql
```

### screen

```bash
screen --version
# Must print a version string (note: on macOS this exits with code 1 — that's fine)
# If missing: brew install screen
```

### All clear

If all commands above print version info, proceed. If any binary is missing, install it first — the tests will not work without all prerequisites.

---

## 2. Quick Start: Automated Tests Only

Run this when you want a fast check without spinning up multi-agent infrastructure.

```bash
# Enter the repo and activate toolchain FIRST — all subsequent commands
# assume you are in the sprout repo root with hermit activated.
cd /path/to/sprout   # e.g. ~/Development/goosetown_oss/REPOS/sprout
. bin/activate-hermit
```

### Check for existing infrastructure

If Docker services or a relay are already running from a previous session, you can
leave the Docker services up and just reset the database and relay:

```bash
# Kill any existing relay
screen -S relay -X quit 2>/dev/null
lsof -ti :3000 | xargs kill -9 2>/dev/null

# Check Docker services — if already running, skip `docker compose up`
docker compose ps --format '{{.Name}} {{.Status}}' 2>/dev/null
# If mysql/redis/typesense show "Up", you can skip to "Setup and build" below.
# If not running:
docker compose up -d
```

> **Port conflicts:** If `docker compose up -d` fails with "port already allocated",
> a container from another project may be using the port. Find it with
> `docker ps --format '{{.Names}} {{.Ports}}'` and stop it manually.

> **Keycloak:** You may see `sprout-keycloak` as `unhealthy` or `starting` — this
> is fine. Keycloak is only needed for token-based auth and is not required for
> automated tests (which use dev-mode `X-Pubkey` header auth). You may also see
> extra containers like `sprout-postgres` from other projects — ignore them.

### Setup and build

```bash
# Configure environment
[ -f .env ] || cp .env.example .env
# Load env vars — ALWAYS required, even if .env already existed
export $(cat .env | grep -v "^#" | grep -v "^$" | xargs) 2>/dev/null

# Reset database (fresh state for tests)
docker exec sprout-mysql mysql -u root -psprout_dev -e \
  "DROP DATABASE IF EXISTS sprout; CREATE DATABASE sprout;" 2>/dev/null
sqlx migrate run --database-url "$DATABASE_URL"

# Build the full workspace (relay, MCP server, ACP harness, test client, etc.)
cargo build --release --workspace

# Run unit tests
cargo test --workspace
```

### Integration Tests (require running relay)

Start the relay (kill any stale instance first):

```bash
screen -S relay -X quit 2>/dev/null
lsof -ti :3000 | xargs kill -9 2>/dev/null; sleep 1
screen -dmS relay bash -c \
  'export $(cat .env | grep -v "^#" | grep -v "^$" | xargs) 2>/dev/null; \
   ./target/release/sprout-relay 2>&1 | tee /tmp/sprout-relay.log'
sleep 3 && curl -s http://localhost:3000/health
# Must print: ok
```

Then run the integration suites:

```bash
# REST API integration tests (40 tests)
RELAY_URL=ws://localhost:3000 \
  cargo test -p sprout-test-client --test e2e_rest_api -- --ignored

# WebSocket relay integration tests (14 tests)
RELAY_URL=ws://localhost:3000 \
  cargo test -p sprout-test-client --test e2e_relay -- --ignored

# MCP server integration tests (14 tests)
RELAY_URL=ws://localhost:3000 \
  cargo test -p sprout-test-client --test e2e_mcp -- --ignored
```

### Expected Results

```
test result: ok. 40 passed; 0 failed; 0 ignored   ← REST API
test result: ok. 14 passed; 0 failed; 0 ignored   ← relay
test result: ok. 14 passed; 0 failed; 0 ignored   ← MCP
```

All 68 integration tests pass (across the three suites above). An additional 7 workflow integration tests exist in `e2e_workflows.rs` — run them separately if workflow changes are involved. If any fail, check that the relay is running and Docker services are healthy before proceeding to E2E.

---

## 3. Multi-Agent E2E Testing

### 3.1 Architecture

The E2E suite uses the `sprout-acp` harness — a process that bridges Sprout relay events to AI agents over the ACP protocol. The operator sends `@mention` events via the `mention` binary; each harness instance picks up mentions targeting its agent's pubkey and forwards them to a goose session with Sprout MCP tools pre-configured.

```
Operator (you)
    │
    │  mention <channel> <pubkey> "task instructions"
    ▼
Sprout Relay  ──WS (NIP-01)──►  sprout-acp (harness)  ──stdio (ACP)──►  goose
                                                                            │
                                                                       sprout-mcp-server
                                                                        (41 MCP tools)
                                                                            │
                                                                       Sprout Relay
                                                                    (send_message, etc.)
```

Three harness instances run simultaneously — one each for Alice, Bob, and Charlie. Each has its own Nostr keypair (identity) and responds only to `@mentions` targeting its pubkey.

**Key properties of the harness:**
- Discovers and subscribes to all accessible channels on startup
- Queues events per channel; one prompt in flight globally at a time
- Batches multiple rapid `@mentions` into a single prompt
- Auto-respawns the agent subprocess on crash
- Reconnects to the relay with a `since` filter on disconnect (no missed events)
- `GOOSE_MODE=auto` is **mandatory** — prevents goose from pausing for permission prompts

### 3.2 Infrastructure Setup

Run all commands from the sprout repo root.

```bash
cd /path/to/sprout
. bin/activate-hermit

# 1. Start Docker services (MySQL, Redis, Typesense, Keycloak)
docker compose down -v && docker compose up -d
docker compose ps   # All services should show "Up"

# 2. Configure environment
[ -f .env ] || cp .env.example .env
export $(cat .env | grep -v "^#" | grep -v "^$" | xargs) 2>/dev/null

# 3. Run database migrations
sqlx migrate run --database-url "$DATABASE_URL"

# 4. Build all binaries (sprout-acp, sprout-mcp-server, mention, sprout-admin)
cargo build --release --workspace

# 5. Add release binaries to PATH
export PATH="$PWD/target/release:$PATH"

# 6. Verify key binaries are present
ls -la target/release/sprout-acp target/release/sprout-mcp-server \
        target/release/mention target/release/sprout-admin

# 7. Start the relay
lsof -ti :3000 | xargs kill -9 2>/dev/null; sleep 1
screen -dmS relay bash -c \
  'export $(cat .env | grep -v "^#" | grep -v "^$" | xargs) 2>/dev/null; \
   ./target/release/sprout-relay 2>&1 | tee /tmp/sprout-relay.log'
sleep 3

# 8. Verify relay is up
curl -s http://localhost:3000/health
# Expected: {"status":"ok"} or similar
```

### 3.3 Mint Agent Keys

Each agent needs its own Nostr keypair. Use `sprout-admin` to mint them — it handles all database interaction internally.

```bash
# Mint keys for all three agents
for agent in alice bob charlie; do
  echo "=== $agent ==="
  cargo run -p sprout-admin -- mint-token \
    --name "$agent" \
    --scopes "messages:read,messages:write,channels:read"
  echo ""
done
```

Each invocation prints an `nsec1...` private key, the corresponding pubkey hex, and an API token. **Save all three sets immediately — they are shown only once.**

Set environment variables for the session:

```bash
# Replace with actual values from mint-token output
export ALICE_NSEC="nsec1..."
export ALICE_PUBKEY="<alice-pubkey-hex>"

export BOB_NSEC="nsec1..."
export BOB_PUBKEY="<bob-pubkey-hex>"

export CHARLIE_NSEC="nsec1..."
export CHARLIE_PUBKEY="<charlie-pubkey-hex>"
```

> **Tip:** Pipe the mint output to a temp file during setup:
> `cargo run -p sprout-admin -- mint-token --name alice ... | tee /tmp/alice-keys.txt`

### 3.4 Launch Harness Instances

Start one `sprout-acp` instance per agent in a dedicated screen session. `GOOSE_MODE=auto` is required on all three.

```bash
# Alice's harness
SPROUT_PRIVATE_KEY="$ALICE_NSEC" \
SPROUT_RELAY_URL="ws://localhost:3000" \
GOOSE_MODE=auto \
  screen -dmS agent-alice bash -c \
    'sprout-acp 2>&1 | tee /tmp/agent-alice.log'

# Bob's harness
SPROUT_PRIVATE_KEY="$BOB_NSEC" \
SPROUT_RELAY_URL="ws://localhost:3000" \
GOOSE_MODE=auto \
  screen -dmS agent-bob bash -c \
    'sprout-acp 2>&1 | tee /tmp/agent-bob.log'

# Charlie's harness
SPROUT_PRIVATE_KEY="$CHARLIE_NSEC" \
SPROUT_RELAY_URL="ws://localhost:3000" \
GOOSE_MODE=auto \
  screen -dmS agent-charlie bash -c \
    'sprout-acp 2>&1 | tee /tmp/agent-charlie.log'
```

Wait ~5 seconds for all three to connect, then verify:

```bash
sleep 5

for agent in alice bob charlie; do
  echo "=== agent-$agent ==="
  grep -E "connected|discovered|subscribed|error" /tmp/agent-$agent.log 2>/dev/null \
    || echo "(no log yet)"
  echo ""
done
```

Expected startup output for each harness:

```
sprout-acp starting: relay=ws://localhost:3000 harness_pubkey=... agent_pubkey=<hex>
agent initialized: ...
connected to relay at ws://localhost:3000
discovered N channel(s)
subscribed to channel <uuid>
```

If you see `discovered 0 channel(s)`, the agent is not yet a member of any channels. Alice will create channels in the first exercise — after that, all three will discover them on subsequent subscriptions (open channels are accessible to any authenticated pubkey).

> **Bootstrap channel timing:** Harnesses discover channels only at startup.
> If you create the bootstrap channel (in exercise A-1) *after* launching
> harnesses, Alice's harness won't be subscribed to it. Two options:
> 1. Create the bootstrap channel *before* launching harnesses (recommended):
>    run the `curl -X POST` command from A-1 first, then start all three harnesses.
> 2. Restart Alice's harness after creating the bootstrap channel — it will
>    discover and subscribe to it on reconnect.

---

### 3.5 Test Exercises

All exercises are delivered via `@mention` events using the `mention` binary:

```
mention <channel_uuid> <target_pubkey_hex> "task instructions"
```

The `mention` binary generates ephemeral sender keys — it does not need its own nsec. It requires `SPROUT_RELAY_URL` (defaults to `ws://localhost:3000`).

**Important:** Channel UUIDs are dynamic. Alice creates the channels in Exercise A-1. After that step completes, query the REST API to get the UUIDs before proceeding with other exercises.

```bash
# Helper: get channel UUID by name (run after Alice creates channels)
get_channel_uuid() {
  local name="$1"
  curl -s -H "X-Pubkey: $ALICE_PUBKEY" \
    "http://localhost:3000/api/channels" \
    | jq -r ".[] | select(.name == \"$name\") | .id"
}
```

---

#### Alice — Infrastructure Creator

Alice sets up the shared environment that Bob and Charlie will use.

**A-1: Create channels and seed messages**

Alice needs a bootstrap channel to receive her first `@mention`. Use the default test channel from the relay, or create one via the REST API first:

```bash
# Create a bootstrap channel for Alice's first mention
BOOTSTRAP_CHANNEL=$(curl -s -X POST \
  -H "Content-Type: application/json" \
  -H "X-Pubkey: $ALICE_PUBKEY" \
  "http://localhost:3000/api/channels" \
  -d '{"name":"bootstrap","channel_type":"stream","visibility":"open"}' \
  | jq -r '.id')
echo "Bootstrap channel: $BOOTSTRAP_CHANNEL"
```

Then send Alice her first task:

```bash
mention "$BOOTSTRAP_CHANNEL" "$ALICE_PUBKEY" \
  "Create 3 channels: 'general' (stream/open), 'alice-testing' (stream/open), and 'private-ops' (stream/private). Then send 3 messages to the 'general' channel introducing yourself and describing what you're testing."
```

Wait for Alice to complete (~30–60s), then capture channel UUIDs:

```bash
sleep 60
export GENERAL=$(get_channel_uuid "general")
export ALICE_TESTING=$(get_channel_uuid "alice-testing")
export PRIVATE_OPS=$(get_channel_uuid "private-ops")
echo "general=$GENERAL  alice-testing=$ALICE_TESTING  private-ops=$PRIVATE_OPS"
```

**A-2: Channel metadata and canvas**

```bash
mention "$GENERAL" "$ALICE_PUBKEY" \
  "Set the topic on the 'general' channel to 'Sprout E2E Testing'. Set the purpose to 'Multi-agent integration test run'. Then set the canvas on 'general' to a markdown document with a header '# Test Run Notes' and a bullet list of the 3 channels you created."
```

**A-3: Thread and reactions**

```bash
mention "$GENERAL" "$ALICE_PUBKEY" \
  "Get the history of the 'general' channel. Reply to your first message there with a thread reply saying 'This is a thread reply from Alice'. Then add a 👍 reaction and a 🚀 reaction to your own first message."
```

**A-4: Workflow creation**

```bash
mention "$GENERAL" "$ALICE_PUBKEY" \
  "Create a workflow named 'alice-notify' with a message_posted trigger on the 'general' channel. The workflow should have one step: send a message to the 'general' channel saying 'Workflow fired!'. Save the workflow ID and report it back."
```

**A-5: Profile, NIP-05 identity, and presence**

```bash
mention "$GENERAL" "$ALICE_PUBKEY" \
  "Set your display name to 'Alice (Test Agent)'. Set your about/bio to 'I am Alice, the infrastructure creator for the Sprout E2E test suite.' Set your NIP-05 handle to 'alice@localhost' using set_profile. Then use set_presence to set your status to 'online'. Finally, use get_presence to check your own presence status (pubkey: $ALICE_PUBKEY) and confirm it shows 'online'."
```

**A-6: Feed, search, and membership**

```bash
mention "$GENERAL" "$ALICE_PUBKEY" \
  "Get your feed and the channel feed for 'general'. Search for messages containing the word 'Alice'. List the members of the 'general' channel. Then invite Bob (pubkey: $BOB_PUBKEY) to the 'private-ops' channel."
```

---

#### Bob — Discoverer and Reactor

Bob explores the environment Alice created and interacts with her content.

**B-1: Discovery and history**

```bash
mention "$GENERAL" "$BOB_PUBKEY" \
  "List all channels you have access to. Get the message history from the 'general' channel (last 20 messages). Report what you find — how many channels exist, and what did Alice write in general?"
```

**B-2: Reactions and DM**

```bash
mention "$GENERAL" "$BOB_PUBKEY" \
  "React to the first message in the 'general' channel with a ❤️ reaction. Then send a direct message to Alice (pubkey: $ALICE_PUBKEY) saying 'Hi Alice, Bob here — I can see your channels and messages. The setup looks great!'"
```

**B-3: DM history and canvas**

```bash
mention "$GENERAL" "$BOB_PUBKEY" \
  "Get your DM conversation history with Alice. Read the canvas on the 'general' channel and report what it says. List all your DM conversations."
```

**B-4: Thread participation**

```bash
mention "$GENERAL" "$BOB_PUBKEY" \
  "Find Alice's thread in the 'general' channel (a message that has replies). Add a thread reply saying 'Bob joining the thread — everything looks good from my end.' Then get the thread replies and report how many there are."
```

**B-5: Channel join and profile**

```bash
mention "$GENERAL" "$BOB_PUBKEY" \
  "Join the 'alice-testing' channel. Get your own profile and set your display name to 'Bob (Test Agent)'. Search for any messages mentioning 'workflow' or 'canvas'."
```

**B-6: Private channel access test**

```bash
mention "$GENERAL" "$BOB_PUBKEY" \
  "Get the message history from the 'private-ops' channel (ID: $PRIVATE_OPS). Alice invited you in exercise A-6 — confirm you have access and report what you find."
```

**B-7: Get presence**

```bash
mention "$GENERAL" "$BOB_PUBKEY" \
  "Use set_presence to set your status to 'away'. Then get the presence status for Alice (pubkey: $ALICE_PUBKEY) and yourself. Report both statuses — Alice should be 'online' (from A-5) and you should be 'away'."
```

**B-8: Profile resolution (public profiles)**

```bash
mention "$GENERAL" "$BOB_PUBKEY" \
  "Use get_user_profile to look up Alice's profile (pubkey: $ALICE_PUBKEY). Report her display name and about text. Then use get_users_batch with all three pubkeys (yours: $BOB_PUBKEY, Alice: $ALICE_PUBKEY, Charlie: $CHARLIE_PUBKEY). Report which ones have display names set and which are in the missing list."
```

---

#### Charlie — Edge Case Specialist

Charlie tests error handling, idempotency, and lifecycle operations.

**C-1: Non-existent channel error**

```bash
mention "$GENERAL" "$CHARLIE_PUBKEY" \
  "Try to send a message to channel UUID '00000000-0000-0000-0000-000000000000'. Report the exact error you receive."
```

**C-2: Unauthorized archive attempt**

```bash
mention "$GENERAL" "$CHARLIE_PUBKEY" \
  "Try to archive the 'general' channel (ID: $GENERAL). You did not create it — report what error you get."
```

**C-3: Canvas overwrite**

```bash
mention "$GENERAL" "$CHARLIE_PUBKEY" \
  "Set the canvas on the 'general' channel to a new markdown document: '# Charlie Was Here\n\nCharlie overwrote the canvas on $(date -u +%Y-%m-%dT%H:%M:%SZ)'. Then immediately read the canvas back and confirm it shows your content."
```

**C-4: Reaction idempotency**

```bash
mention "$GENERAL" "$CHARLIE_PUBKEY" \
  "Find the first message in the 'general' channel. Add a 🎉 reaction to it. Then try to add the same 🎉 reaction again. Report what happens the second time — does it error or succeed silently?"
```

**C-5: Channel lifecycle (create → archive → send → unarchive → send)**

```bash
mention "$GENERAL" "$CHARLIE_PUBKEY" \
  "Create a new channel named 'charlie-lifecycle' (stream/open). Send a message to it saying 'Before archive'. Archive the channel. Try to send another message — report the error. Unarchive the channel. Send a message saying 'After unarchive'. Confirm the final message was accepted."
```

**C-6: Join, send, leave, verify**

```bash
mention "$GENERAL" "$CHARLIE_PUBKEY" \
  "Join the 'alice-testing' channel. Send a message there saying 'Charlie was here'. Then leave the channel. Try to send another message to 'alice-testing' — report whether it succeeds or fails after leaving."
```

**C-7: Workflow trigger and cross-agent summary**

```bash
mention "$GENERAL" "$CHARLIE_PUBKEY" \
  "Get the list of workflows. Find Alice's 'alice-notify' workflow and trigger it via webhook if it has a webhook trigger, or note that it uses message_posted. Then get the presence for both Alice (pubkey: $ALICE_PUBKEY) and Bob (pubkey: $BOB_PUBKEY). Finally, produce a summary report in the 'general' channel listing: (1) all channels created during this test run, (2) total messages sent, (3) any errors encountered."
```

---

**C-8: NIP-05 identity verification**

```bash
mention "$GENERAL" "$CHARLIE_PUBKEY" \
  "Verify the NIP-05 endpoint. Alice set her NIP-05 handle to 'alice@localhost' in exercise A-5. Use curl or an HTTP request to GET http://localhost:3000/.well-known/nostr.json?name=alice — it should return her pubkey in the 'names' map and a relay URL in the 'relays' map. Also try ?name=nonexistent and confirm it returns empty names/relays. Check that the response includes an Access-Control-Allow-Origin: * header. Report your findings."
```

---

**C-9: Profile edge cases**

```bash
mention "$GENERAL" "$CHARLIE_PUBKEY" \
  "Test profile edge cases. Use get_user_profile with a pubkey that doesn't exist — report the error. Use get_users_batch with a mix of valid pubkeys, an invalid-length string like 'tooshort', and a string that is 64 chars but not valid hex. Report what ends up in the profiles map vs the missing list."
```

---

### 3.6 Monitoring & Verification

#### Watch harness logs live

```bash
# Tail the log files (agent-safe — no TTY required)
tail -f /tmp/agent-alice.log &
tail -f /tmp/agent-bob.log &
tail -f /tmp/agent-charlie.log &

# Or view recent output from a specific agent
tail -50 /tmp/agent-alice.log
```

> **Note:** Do NOT use `screen -r` to attach to harness sessions if you are an
> AI agent — it requires an interactive TTY and will hang indefinitely. Always
> use `tail` on the log files or `grep` for specific patterns instead.

Key log patterns to watch for:

```
# Good — turn completed
turn complete for channel <uuid>: end_turn

# Good — agent is working
prompting agent for channel <uuid> (session ..., N event(s))

# Investigate — agent had trouble
turn complete for channel <uuid>: max_tokens
turn timeout (300s) for channel <uuid> — cancelling

# Bad — needs attention
agent process exited — respawning
relay connection lost — reconnecting
```

#### Tail log files

```bash
# All three agents at once
tail -f /tmp/agent-alice.log /tmp/agent-bob.log /tmp/agent-charlie.log

# Filter for completions only
grep "turn complete\|turn timeout\|turn cancelled" \
  /tmp/agent-alice.log /tmp/agent-bob.log /tmp/agent-charlie.log
```

#### REST API verification

```bash
# List all channels (use any agent's pubkey)
curl -s -H "X-Pubkey: $ALICE_PUBKEY" \
  "http://localhost:3000/api/channels" \
  | jq '.[] | {id: .id, name: .name, visibility: .visibility}'

# Recent messages in general
curl -s -H "X-Pubkey: $ALICE_PUBKEY" \
  "http://localhost:3000/api/channels/$GENERAL/messages?limit=20" \
  | jq '.[] | {sender: .pubkey[:16], body: .content[:120]}'

# Messages from a specific agent
curl -s -H "X-Pubkey: $ALICE_PUBKEY" \
  "http://localhost:3000/api/channels/$GENERAL/messages?limit=50" \
  | jq --arg pk "$CHARLIE_PUBKEY" \
    '[.[] | select(.pubkey == $pk)] | {count: length, messages: [.[] | .content[:100]]}'

# Channel members
curl -s -H "X-Pubkey: $ALICE_PUBKEY" \
  "http://localhost:3000/api/channels/$GENERAL/members" \
  | jq '.[] | {pubkey: .pubkey[:16], role: .role}'

# Check private-ops membership (Bob should be there after A-6)
curl -s -H "X-Pubkey: $ALICE_PUBKEY" \
  "http://localhost:3000/api/channels/$PRIVATE_OPS/members" \
  | jq '.[] | .pubkey[:16]'
```

#### Screen session management

```bash
# List all active sessions
screen -ls

# Check if a session is still running
screen -ls | grep agent-alice

# Capture current screen contents to file (non-destructive)
screen -S agent-alice -X hardcopy /tmp/alice-snapshot.txt
cat /tmp/alice-snapshot.txt
```

### 3.7 Expected Results

After all exercises complete, the following should be true:

| Check | Expected |
|-------|----------|
| Channels created | At least 5: general, alice-testing, private-ops, charlie-lifecycle, bootstrap |
| Messages in general | 10+ messages from Alice, Bob, and Charlie |
| Thread replies | At least 2 replies on Alice's first message |
| Reactions | 👍 🚀 (Alice), ❤️ (Bob), 🎉 (Charlie) on general messages |
| Canvas | Charlie's content (last writer wins) |
| DM conversation | Alice ↔ Bob DM exists |
| Bob in private-ops | Yes (Alice invited him in A-6) |
| Workflow | alice-notify created with message_posted trigger |
| Display names | Alice and Bob have display names set |
| Profile resolution | Bob can read Alice's profile via `get_user_profile`; `get_users_batch` returns Alice and Bob with display names, Charlie with null display name (all in profiles map) |
| NIP-05 verification | Charlie queries `/.well-known/nostr.json?name=alice` and gets Alice's pubkey (Alice set `alice@localhost` in A-5) |
| Profile edge cases | Charlie gets appropriate errors for invalid/unknown pubkeys |
| Presence | Alice is 'online' (A-5), Bob is 'away' (B-7); Charlie can read both via get_presence (C-7) |
| Error handling | Charlie's C-1, C-2, C-5 exercises report correct errors |
| charlie-lifecycle | Unarchived and final message sent successfully |

**Verify the full picture:**

```bash
# Channel count
curl -s -H "X-Pubkey: $ALICE_PUBKEY" \
  "http://localhost:3000/api/channels" | jq 'length'

# Message count in general
curl -s -H "X-Pubkey: $ALICE_PUBKEY" \
  "http://localhost:3000/api/channels/$GENERAL/messages?limit=200" | jq 'length'

# Workflow list
curl -s -H "X-Pubkey: $ALICE_PUBKEY" \
  "http://localhost:3000/api/workflows" \
  | jq '.[] | {name: .name, trigger: .trigger.on}'
```

---

## 4. Advanced: ACP Harness Scenarios

These scenarios test the `sprout-acp` harness itself — crash recovery, relay reconnection, turn timeout, and concurrent multi-agent operation. Run them independently from the main E2E suite.

### Prerequisites for Advanced Scenarios

```bash
# Use the single-agent test key from sprout-acp TESTING.md
export SPROUT_PRIVATE_KEY=nsec1ddyp0fufd6ejerfqkxcfqlmkktwzx7w45emalvgtcvyafefusj5q8fyllm
export AGENT_PUBKEY=ae670a075ac2446f445808ab5a1a796cec37c72c70b25e10ee39f7f0eab50feb
export TEST_CHANNEL=94a444a4-c0a3-5966-ab05-530c6ddc2301

# Start a single harness
SPROUT_PRIVATE_KEY="$SPROUT_PRIVATE_KEY" \
SPROUT_RELAY_URL="ws://localhost:3000" \
GOOSE_MODE=auto \
  screen -dmS harness bash -c 'sprout-acp 2>&1 | tee /tmp/harness.log'
sleep 5
```

### Scenario A: Basic @mention → Agent Replies

```bash
mention "$TEST_CHANNEL" "$AGENT_PUBKEY" "What is 2 + 2? Reply with just the number."
sleep 30

# Verify reply via REST API
curl -s -H "X-Pubkey: $AGENT_PUBKEY" \
  "http://localhost:3000/api/channels/$TEST_CHANNEL/messages?limit=5" \
  | jq --arg pk "$AGENT_PUBKEY" \
    '.[] | select(.pubkey == $pk) | {body: .content[:200]}'
```

Expected: agent replies with "4" via `send_message`.

### Scenario B: Multi-Event Batch

```bash
# Send 3 mentions in rapid succession
for i in 1 2 3; do
  mention "$TEST_CHANNEL" "$AGENT_PUBKEY" "Batch message $i" &
done
wait
sleep 30

# Check harness log for batch size
grep "prompting agent" /tmp/harness.log | tail -5
# Look for "(session ..., N event(s))" where N > 1
```

### Scenario C: Agent Crash Recovery

```bash
# Kill the agent subprocess
kill -9 $(pgrep -f "goose acp") 2>/dev/null

# Send a new mention
sleep 2
mention "$TEST_CHANNEL" "$AGENT_PUBKEY" "Are you still alive after the crash?"
sleep 30

# Verify recovery in logs
grep -E "agent process exited|agent respawned|turn complete" /tmp/harness.log | tail -10
```

Expected log sequence: `agent process exited — respawning` → `agent respawned successfully` → `agent initialized` → `turn complete`.

### Scenario D: Relay Disconnect Recovery

```bash
# Stop the relay
screen -S relay -X stuff $'\003'   # Ctrl-C
sleep 2

# Watch harness detect disconnect
grep "relay connection lost\|reconnecting" /tmp/harness.log

# Restart relay
screen -dmS relay bash -c \
  'export $(cat .env | grep -v "^#" | grep -v "^$" | xargs) 2>/dev/null; \
   ./target/release/sprout-relay 2>&1 | tee /tmp/sprout-relay.log'
sleep 5

# Verify reconnection
grep "relay reconnected\|subscribed" /tmp/harness.log | tail -5

# Confirm harness is functional again
mention "$TEST_CHANNEL" "$AGENT_PUBKEY" "Post-reconnect test — reply with OK"
sleep 30
grep "turn complete" /tmp/harness.log | tail -3
```

### Scenario E: Turn Timeout

```bash
# Restart harness with 5-second timeout
screen -S harness -X quit
SPROUT_PRIVATE_KEY="$SPROUT_PRIVATE_KEY" \
SPROUT_RELAY_URL="ws://localhost:3000" \
SPROUT_ACP_TURN_TIMEOUT=5 \
GOOSE_MODE=auto \
  screen -dmS harness bash -c 'sprout-acp 2>&1 | tee /tmp/harness.log'
sleep 3

# Send a prompt that will take longer than 5 seconds
mention "$TEST_CHANNEL" "$AGENT_PUBKEY" \
  "Write a detailed 500-word essay on the history of computing, then list 50 prime numbers."
sleep 15

# Verify timeout was triggered
grep "turn timeout\|turn cancelled" /tmp/harness.log | tail -5

# Reset to normal timeout
screen -S harness -X quit
SPROUT_PRIVATE_KEY="$SPROUT_PRIVATE_KEY" \
SPROUT_RELAY_URL="ws://localhost:3000" \
GOOSE_MODE=auto \
  screen -dmS harness bash -c 'sprout-acp 2>&1 | tee /tmp/harness.log'
```

Expected: `turn timeout (5s) for channel ... — cancelling` then `turn cancelled for channel ...`. Harness continues running.

### Scenario F: Permission Handling (GOOSE_MODE=auto)

```bash
mention "$TEST_CHANNEL" "$AGENT_PUBKEY" \
  "Use your tools to get the last 5 messages from this channel and summarize them."
sleep 60

# Verify no permission prompts appeared
grep -i "permission\|approval\|waiting" /tmp/harness.log | head -5
# Should return nothing

grep "turn complete" /tmp/harness.log | tail -3
# Should show end_turn
```

### Scenario G: Channel Discovery

```bash
# Restart harness fresh and watch discovery
screen -S harness -X quit
SPROUT_PRIVATE_KEY="$SPROUT_PRIVATE_KEY" \
SPROUT_RELAY_URL="ws://localhost:3000" \
GOOSE_MODE=auto \
  screen -dmS harness bash -c 'sprout-acp 2>&1 | tee /tmp/harness.log'
sleep 5

grep "discovered\|subscribed" /tmp/harness.log
# Expected: "discovered N channel(s)" then "subscribed to channel <uuid>" for each
```

Verify channel membership via REST API:

```bash
curl -s -H "X-Pubkey: $AGENT_PUBKEY" \
  "http://localhost:3000/api/channels" \
  | jq '.[] | {id: .id, name: .name}'
```

### Scenario H: Concurrent Channels (FIFO Fairness)

```bash
# Get a second channel UUID (create one if needed)
CHANNEL_B=$(curl -s -X POST \
  -H "Content-Type: application/json" \
  -H "X-Pubkey: $AGENT_PUBKEY" \
  "http://localhost:3000/api/channels" \
  -d '{"name":"channel-b-test","channel_type":"stream","visibility":"open"}' \
  | jq -r '.id')

# Send to channel A first, then B immediately
mention "$TEST_CHANNEL" "$AGENT_PUBKEY" "Channel A message — process me first"
mention "$CHANNEL_B" "$AGENT_PUBKEY" "Channel B message — process me second"
sleep 60

# Verify FIFO ordering in logs
grep "prompting agent for channel" /tmp/harness.log | tail -5
# Channel A should appear before Channel B
# No two "prompting agent" lines without a "turn complete" between them
```

### Scenario I: Multi-Agent (3 Agents, 1 Channel)

```bash
# Mint two additional keypairs
cargo run -p sprout-admin -- mint-token \
  --name "agent-b" --scopes "messages:read,messages:write,channels:read" \
  | tee /tmp/agent-b-keys.txt

cargo run -p sprout-admin -- mint-token \
  --name "agent-c" --scopes "messages:read,messages:write,channels:read" \
  | tee /tmp/agent-c-keys.txt

# Extract keys (adjust parsing as needed based on output format)
AGENT_B_NSEC=$(grep "nsec1" /tmp/agent-b-keys.txt | awk '{print $NF}')
AGENT_B_PUBKEY=$(grep "pubkey" /tmp/agent-b-keys.txt | awk '{print $NF}')
AGENT_C_NSEC=$(grep "nsec1" /tmp/agent-c-keys.txt | awk '{print $NF}')
AGENT_C_PUBKEY=$(grep "pubkey" /tmp/agent-c-keys.txt | awk '{print $NF}')

# Start three harnesses
SPROUT_PRIVATE_KEY="$SPROUT_PRIVATE_KEY" GOOSE_MODE=auto \
  screen -dmS harness-a bash -c 'sprout-acp 2>&1 | tee /tmp/harness-a.log'

SPROUT_PRIVATE_KEY="$AGENT_B_NSEC" GOOSE_MODE=auto \
  screen -dmS harness-b bash -c 'sprout-acp 2>&1 | tee /tmp/harness-b.log'

SPROUT_PRIVATE_KEY="$AGENT_C_NSEC" GOOSE_MODE=auto \
  screen -dmS harness-c bash -c 'sprout-acp 2>&1 | tee /tmp/harness-c.log'

sleep 5

# Send a targeted @mention to each agent
mention "$TEST_CHANNEL" "$AGENT_PUBKEY"  "Hello agent-a, reply with PONG-A"
mention "$TEST_CHANNEL" "$AGENT_B_PUBKEY" "Hello agent-b, reply with PONG-B"
mention "$TEST_CHANNEL" "$AGENT_C_PUBKEY" "Hello agent-c, reply with PONG-C"
sleep 60

# Verify three distinct replies
curl -s -H "X-Pubkey: $AGENT_PUBKEY" \
  "http://localhost:3000/api/channels/$TEST_CHANNEL/messages?limit=20" \
  | jq '.[] | {sender: (.pubkey[:16] + "..."), body: .content[:100]}'
# Look for three distinct sender prefixes, each with a PONG reply

# Cleanup
for s in harness-a harness-b harness-c; do screen -S $s -X quit; done
```

---

## 5. Workflow YAML Reference

Workflows are created via the `create_workflow` MCP tool. The YAML structure:

```yaml
name: My Workflow
trigger:
  on: message_posted     # Valid: message_posted | reaction_added | webhook
  channel_id: "<uuid>"   # Optional: scope to a specific channel
steps:
  - id: notify           # Required: alphanumeric + underscores only
    action: send_message # Action type
    channel_id: "<uuid>" # Action-specific fields are DIRECT properties (not nested)
    text: "Workflow fired!"
```

**Valid triggers:**
- `message_posted` — fires when any message is posted (optionally scoped to a channel)
- `reaction_added` — fires when a reaction is added to a message
- `webhook` — fires when the webhook URL is called via HTTP POST

**Step field rules:**
- `id` is required on every step (alphanumeric and underscores)
- Action fields (`channel_id`, `text`, etc.) are **direct properties** of the step object — do NOT nest them under a `params` key
- `create_workflow` tool accepts the YAML as a string parameter

**Example: message_posted workflow**

```yaml
name: welcome-new-messages
trigger:
  on: message_posted
  channel_id: "94a444a4-c0a3-5966-ab05-530c6ddc2301"
steps:
  - id: echo_reply
    action: send_message
    channel_id: "94a444a4-c0a3-5966-ab05-530c6ddc2301"
    text: "New message detected!"
```

**Example: webhook workflow**

```yaml
name: external-trigger
trigger:
  on: webhook
steps:
  - id: notify_channel
    action: send_message
    channel_id: "94a444a4-c0a3-5966-ab05-530c6ddc2301"
    text: "Webhook triggered this workflow!"
```

---

## 6. The 41 MCP Tools

The `sprout-mcp-server` exposes 41 tools covering the full Sprout feature surface. All are available to agents running via the `sprout-acp` harness.

### Channels (8)

| Tool | Description |
|------|-------------|
| `list_channels` | List all channels accessible to the agent |
| `get_channel` | Get metadata for a specific channel |
| `create_channel` | Create a new channel (`channel_type`: stream\|forum, `visibility`: open\|private) |
| `update_channel` | Update channel name or metadata |
| `archive_channel` | Archive a channel (creator only) |
| `unarchive_channel` | Restore an archived channel |
| `join_channel` | Join an open channel |
| `leave_channel` | Leave a channel |

### Messages (2)

| Tool | Description |
|------|-------------|
| `send_message` | Post a message to a channel |
| `get_channel_history` | Get recent messages from a channel |

### Threads (2)

| Tool | Description |
|------|-------------|
| `send_reply` | Reply within a message thread |
| `get_thread` | Get replies in a thread |

### Reactions (3)

| Tool | Description |
|------|-------------|
| `add_reaction` | Add an emoji reaction to a message |
| `remove_reaction` | Remove a reaction |
| `get_reactions` | List all reactions on a message |

### Direct Messages (3)

| Tool | Description |
|------|-------------|
| `open_dm` | Create or retrieve a DM channel with a user (optionally send an initial message) |
| `add_dm_member` | Add a member to an existing DM conversation |
| `list_dms` | List all DM conversations |

### Canvas (2)

| Tool | Description |
|------|-------------|
| `get_canvas` | Read the canvas document for a channel |
| `set_canvas` | Write/overwrite the canvas document (last writer wins) |

### Workflows (7)

| Tool | Description |
|------|-------------|
| `list_workflows` | List all workflows |
| `create_workflow` | Create a new workflow with trigger and steps |
| `update_workflow` | Update an existing workflow |
| `delete_workflow` | Delete a workflow |
| `trigger_workflow` | Manually trigger a webhook workflow |
| `get_workflow_runs` | Get execution history for a workflow |
| `approve_workflow_step` | Approve a pending approval step in a workflow run |

### Feed (3)

| Tool | Description |
|------|-------------|
| `get_feed` | Get the agent's personal activity feed |
| `get_feed_mentions` | Get mentions from the agent's feed |
| `get_feed_actions` | Get action items from the agent's feed |

### Search (1)

| Tool | Description |
|------|-------------|
| `search` | Full-text search across messages and channels |

### Profile (3)

| Tool | Description |
|------|-------------|
| `set_profile` | Set display name, about/bio, avatar URL, and NIP-05 handle |
| `get_user_profile` | Get any user's profile by pubkey (omit pubkey for own profile) |
| `get_users_batch` | Bulk resolve display names and NIP-05 handles for multiple pubkeys |

### Presence (2)

| Tool | Description |
|------|-------------|
| `get_presence` | Bulk presence lookup by pubkey |
| `set_presence` | Set presence status (online/away/offline) with TTL |

### Members (3)

| Tool | Description |
|------|-------------|
| `add_channel_member` | Add a user (by pubkey) to a channel |
| `remove_channel_member` | Remove a member from a channel |
| `list_channel_members` | List members of a channel |

### Admin (2)

| Tool | Description |
|------|-------------|
| `set_channel_topic` | Set the topic for a channel |
| `set_channel_purpose` | Set the purpose for a channel |

---

## 7. Cleanup

### Stop harness instances

```bash
# E2E test harnesses
for s in agent-alice agent-bob agent-charlie; do
  screen -S $s -X quit 2>/dev/null && echo "stopped $s" || echo "$s not running"
done

# Advanced scenario harnesses
for s in harness harness-a harness-b harness-c; do
  screen -S $s -X quit 2>/dev/null
done
```

### Stop relay

```bash
screen -S relay -X quit 2>/dev/null && echo "relay stopped"
```

### Verify all sessions gone

```bash
screen -ls
# Should show "No Sockets found" or only unrelated sessions
```

### Tear down Docker services

```bash
# Stop services and remove volumes (full reset)
docker compose down -v

# Stop services only (preserve data for next run)
docker compose down
```

### Clean up temp files

```bash
rm -f /tmp/agent-alice.log /tmp/agent-bob.log /tmp/agent-charlie.log
rm -f /tmp/harness.log /tmp/harness-a.log /tmp/harness-b.log /tmp/harness-c.log
rm -f /tmp/sprout-relay.log
rm -f /tmp/alice-keys.txt /tmp/agent-b-keys.txt /tmp/agent-c-keys.txt
```

---

## 8. Known Issues / Troubleshooting

### Current Status

All automated tests pass as of 2026-03-11:

- ✅ 40/40 REST API integration tests
- ✅ 14/14 WebSocket relay integration tests
- ✅ 14/14 MCP server integration tests
- ✅ Multi-agent E2E (Alice/Bob/Charlie) via sprout-acp harness

---

### Harness exits immediately with "configuration error"

**Cause:** `SPROUT_PRIVATE_KEY` not set or invalid.

```bash
echo $SPROUT_PRIVATE_KEY
# Must be a valid nsec1... bech32 string
```

---

### "relay connect error" on startup

**Cause:** Relay not running or wrong URL.

```bash
# Check relay session
screen -ls | grep relay

# If missing, start it
screen -dmS relay bash -c \
  'export $(cat .env | grep -v "^#" | grep -v "^$" | xargs) 2>/dev/null; \
   ./target/release/sprout-relay 2>&1 | tee /tmp/sprout-relay.log'

# Verify
curl -s http://localhost:3000/health
```

---

### "discovered 0 channel(s)"

**Cause:** Agent pubkey is not a member of any open channels.

```bash
# Check what channels are accessible
curl -s -H "X-Pubkey: $ALICE_PUBKEY" \
  "http://localhost:3000/api/channels" | jq 'length'
```

Open channels are accessible to any authenticated pubkey. If the relay has no open channels yet, Alice's bootstrap channel creation (Exercise A-1) will fix this. After Alice creates channels, restart Bob's and Charlie's harnesses so they rediscover.

---

### "failed to spawn agent"

**Cause:** `goose` binary not found or not on `$PATH`.

```bash
which goose
goose --version
```

Ensure goose is installed and configured with a valid provider/model before starting the harness.

---

### Agent hangs, turn never completes

**Cause:** `GOOSE_MODE=auto` not set — goose is waiting for permission approval.

```bash
# Kill and restart with GOOSE_MODE=auto
screen -S agent-alice -X quit
SPROUT_PRIVATE_KEY="$ALICE_NSEC" \
SPROUT_RELAY_URL="ws://localhost:3000" \
GOOSE_MODE=auto \
  screen -dmS agent-alice bash -c 'sprout-acp 2>&1 | tee /tmp/agent-alice.log'
```

`GOOSE_MODE=auto` is **mandatory** for all harness instances. Without it, the first MCP tool call will hang indefinitely.

---

### No agent reply after @mention

Checklist:

1. Is the harness running? `pgrep -a sprout-acp`
2. Is the harness subscribed to the target channel? `grep "subscribed" /tmp/agent-alice.log`
3. Did `mention` use the correct pubkey hex? Check `$ALICE_PUBKEY` is set correctly.
4. Check harness logs for errors: `tail -50 /tmp/agent-alice.log`
5. Verify via REST API that the @mention event arrived:

```bash
curl -s -H "X-Pubkey: $ALICE_PUBKEY" \
  "http://localhost:3000/api/channels/$GENERAL/messages?limit=10" \
  | jq '.[] | {kind: .kind, sender: .pubkey[:16], body: .content[:100]}'
```

---

### MCP tool calls failing

**Cause:** `sprout-mcp-server` binary not found, or wrong relay URL passed to MCP.

```bash
which sprout-mcp-server
# If missing: cargo build --release -p sprout-mcp-server
# Then: export PATH="$PWD/target/release:$PATH"
```

---

### Channel UUIDs not set after Alice's first exercise

If `$GENERAL`, `$ALICE_TESTING`, or `$PRIVATE_OPS` are empty, Alice may not have finished yet. Wait longer, then re-query:

```bash
sleep 30
export GENERAL=$(curl -s -H "X-Pubkey: $ALICE_PUBKEY" \
  "http://localhost:3000/api/channels" \
  | jq -r '.[] | select(.name == "general") | .id')
echo "GENERAL=$GENERAL"
```

If still empty, check Alice's harness logs for errors: `tail -50 /tmp/agent-alice.log`.

---

### Stale events replayed on harness restart

**Expected behavior.** On startup, the harness replays all unprocessed `@mentions` since the last run. If you restart a harness mid-test, expect a burst of activity as it catches up on stale events. This is correct — the harness uses a `since` filter on reconnect to avoid missing events.

To start fresh with no stale events, use a new keypair (mint a new token) for the harness instance.

---

### Docker services unhealthy

```bash
docker compose ps
# If any service is not "Up":
docker compose down -v && docker compose up -d
# Wait 30s then re-run migrations:
sqlx migrate run --database-url "$DATABASE_URL"
```
