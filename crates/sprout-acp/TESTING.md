# sprout-acp Integration Testing Guide

## Overview

`sprout-acp` is the ACP harness that bridges Sprout relay events to AI agents. It:

- Connects to a Sprout relay via NIP-01 WebSocket with NIP-42 auth
- Listens for `@mention` events (kind 40001) targeting the agent's pubkey
- Queues events per channel, enforcing one prompt-in-flight globally
- Sends batched prompts to an ACP agent over stdio JSON-RPC 2.0
- The agent uses Sprout MCP tools (`send_message`, `get_channel_history`, etc.) to respond

**Architecture:**

```
Sprout Relay ──WS (NIP-01)──→ sprout-acp harness ──stdio (JSON-RPC)──→ ACP Agent
                                                                          │
                                                                     Sprout MCP
                                                                    (16 tools)
```

This document covers manual integration testing of the full pipeline. For unit tests, run `cargo test` in the crate root.

---

## Prerequisites

### 1. Docker Services

```bash
cd /path/to/sprout2
docker compose up -d
```

Required services: MySQL, Redis, Typesense, Keycloak. Verify:

```bash
docker compose ps
```

All services should be `Up`.

### 2. Relay

Start `sprout-relay` in a detached screen session:

```bash
screen -dmS relay just relay
```

Verify it's listening:

```bash
screen -r relay   # Ctrl-A D to detach
# or:
curl -s http://localhost:3000/health || echo "relay not up"
```

### 3. Build Binaries

```bash
cargo build --release -p sprout-acp -p sprout-mcp-server
```

Ensure `target/release/sprout-acp`, `target/release/sprout-mcp-server`, and `target/release/mention` are present:

```bash
ls -la target/release/sprout-acp target/release/sprout-mcp-server target/release/mention
```

Add to PATH for convenience:

```bash
export PATH="$PWD/target/release:$PATH"
```

### 4. Test Keys

These keys are for local development only — never use in production.

```bash
export SPROUT_PRIVATE_KEY=nsec1ddyp0fufd6ejerfqkxcfqlmkktwzx7w45emalvgtcvyafefusj5q8fyllm
export AGENT_PUBKEY_HEX=ae670a075ac2446f445808ab5a1a796cec37c72c70b25e10ee39f7f0eab50feb
export AGENTS_CHANNEL=94a444a4-c0a3-5966-ab05-530c6ddc2301
```

**Generating additional keypairs:** Use `sprout-admin mint-token` to create new keypairs for additional agents. Each call prints an `nsec` private key and API token — save them immediately.

```bash
cargo run -p sprout-admin -- mint-token --name "my-agent" --scopes "messages:read,messages:write,channels:read"
```

**Channels:** Open channels (the default for local dev) are accessible to any authenticated pubkey — no extra setup needed. See the [README](README.md#channels) for details.

### 5. MySQL Access

```bash
# Connect via docker exec (mysql client may not be installed locally)
docker exec -it sprout-mysql mysql -u sprout -psprout_dev sprout
```

---

## Quick Start (5 Minutes)

Minimal path: run harness with goose, send a test `@mention`, verify reply.

### Step 1: Set Environment

```bash
export PATH="$PWD/target/release:$PATH"
export SPROUT_PRIVATE_KEY=nsec1ddyp0fufd6ejerfqkxcfqlmkktwzx7w45emalvgtcvyafefusj5q8fyllm
export SPROUT_RELAY_URL=ws://localhost:3000
export GOOSE_MODE=auto
```

### Step 2: Start Harness

```bash
screen -dmS harness sprout-acp
screen -r harness   # Ctrl-A D to detach
```

Expected log output:
```
sprout-acp starting: relay=ws://localhost:3000 harness_pubkey=... agent_pubkey=ae670a...
agent initialized: ...
connected to relay at ws://localhost:3000
discovered N channel(s)
subscribed to channel 94a444a4-c0a3-5966-ab05-530c6ddc2301
```

### Step 3: Send a Test @mention

```bash
mention 94a444a4-c0a3-5966-ab05-530c6ddc2301 ae670a075ac2446f445808ab5a1a796cec37c72c70b25e10ee39f7f0eab50feb "Hello agent, reply with 'pong'"
```

### Step 4: Verify Reply

Check harness logs:

```bash
screen -r harness
# Look for: "turn complete for channel 94a444a4-...: end_turn"
```

Check MySQL for agent reply:

```sql
SELECT HEX(e.pubkey) as sender, e.kind, SUBSTRING(e.content, 1, 200) as body, e.created_at
FROM events e
WHERE e.kind = 40001
ORDER BY e.created_at DESC LIMIT 10;
```

The agent's pubkey (`ae670a075ac2446f445808ab5a1a796cec37c72c70b25e10ee39f7f0eab50feb`) should appear as `sender` with a reply body.

---

## Agent Setup

### goose (Native ACP)

goose supports ACP natively. No adapter needed.

```bash
export SPROUT_ACP_AGENT_COMMAND=goose
export SPROUT_ACP_AGENT_ARGS=acp
export GOOSE_MODE=auto   # Disables interactive permission prompts
```

**Gotchas:**
- `GOOSE_MODE=auto` is required — without it, goose may pause waiting for user approval on tool calls
- Ensure `goose` is on `$PATH` and configured with a valid provider/model

Verify goose is working standalone:

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | goose acp
```

---

### codex (via codex-acp adapter)

codex-acp is Zed's ACP wrapper around OpenAI Codex. Requires building from source.

**Build:**

```bash
# Requires Rust 1.91+ — use hermit if needed
cd /path/to/codex-acp
cargo build --release
# Binary at: /path/to/codex-acp/target/release/codex-acp
```

**Configure:**

```bash
export OPENAI_API_KEY=sk-...   # or CODEX_API_KEY
export SPROUT_ACP_AGENT_COMMAND=/path/to/codex-acp/target/release/codex-acp
export SPROUT_ACP_AGENT_ARGS='-c,permissions.approval_policy="never"'
```

The `-c permissions.approval_policy="never"` arg disables codex's approval prompts (equivalent to goose's `GOOSE_MODE=auto`).

**Gotchas:**
- Use the full absolute path for `SPROUT_ACP_AGENT_COMMAND` — codex-acp is not typically on `$PATH`
- `SPROUT_ACP_AGENT_ARGS` is comma-separated; the config parser splits on `,`
- Repo: https://github.com/zed-industries/codex-acp

---

### claude code (via claude-agent-acp adapter)

claude-agent-acp is Zed's ACP wrapper around the Claude Agent SDK. Requires Node.js.

**Build:**

```bash
cd /path/to/claude-agent-acp
npm install
npm run build
# Entry point at: /path/to/claude-agent-acp/dist/index.js
```

**Configure:**

```bash
export ANTHROPIC_API_KEY=sk-ant-...
export SPROUT_ACP_AGENT_COMMAND=node
export SPROUT_ACP_AGENT_ARGS=/path/to/claude-agent-acp/dist/index.js
```

**Gotchas:**
- `node` must be on `$PATH`
- The `dist/index.js` path must be absolute
- Repo: https://github.com/zed-industries/claude-agent-acp

---

## Test Scenarios

### Scenario A: Basic @mention → Agent Replies

**Description:** Single @mention event triggers a prompt; agent replies via `send_message`.

**Steps:**

```bash
mention 94a444a4-c0a3-5966-ab05-530c6ddc2301 \
  ae670a075ac2446f445808ab5a1a796cec37c72c70b25e10ee39f7f0eab50feb \
  "What is 2 + 2?"
```

**Expected behavior:**
- Harness logs: `prompting agent for channel 94a444a4-... (session ..., 1 event(s))`
- Agent calls `send_message` MCP tool with a reply
- Harness logs: `turn complete for channel 94a444a4-...: end_turn`

**Verify:**

```sql
SELECT HEX(e.pubkey) as sender, SUBSTRING(e.content, 1, 200) as body, e.created_at
FROM events e
WHERE e.kind = 40001
  AND HEX(e.pubkey) = 'ae670a075ac2446f445808ab5a1a796cec37c72c70b25e10ee39f7f0eab50feb'
ORDER BY e.created_at DESC LIMIT 5;
```

---

### Scenario B: Multi-Event Batch (Rapid @mentions)

**Description:** Three @mentions sent in rapid succession should be batched into a single prompt (queue drains all pending events for a channel at once).

**Steps:**

```bash
for i in 1 2 3; do
  mention 94a444a4-c0a3-5966-ab05-530c6ddc2301 \
    ae670a075ac2446f445808ab5a1a796cec37c72c70b25e10ee39f7f0eab50feb \
    "Batch message $i" &
done
wait
```

**Expected behavior:**
- Harness logs show `(session ..., 3 event(s))` or `(2 event(s))` + `(1 event(s))` depending on timing
- Only **one** `session/prompt` call fires per flush cycle
- Agent receives all messages in a single prompt text

**Verify:**

Check harness log for batch size:

```bash
screen -r harness
# Look for: "prompting agent for channel ... (session ..., N event(s))"
```

If N > 1, batching worked. If you see three separate `prompting agent` lines, the events arrived too slowly — try sending them faster or add a brief sleep before the harness starts.

---

### Scenario C: Agent Crash Recovery

**Description:** Kill the agent subprocess; harness should detect exit, respawn, and process the next event.

**Steps:**

1. While harness is running, find the agent PID:

```bash
# In a separate terminal
pgrep -f "goose acp"   # or codex-acp, node, etc.
```

2. Kill it:

```bash
kill -9 <agent-pid>
```

3. Send a new @mention:

```bash
mention 94a444a4-c0a3-5966-ab05-530c6ddc2301 \
  ae670a075ac2446f445808ab5a1a796cec37c72c70b25e10ee39f7f0eab50feb \
  "Are you still alive?"
```

**Expected behavior:**
- Harness logs: `agent process exited — respawning`
- Harness logs: `agent respawned successfully`
- Harness logs: `agent initialized: ...`
- The new @mention is processed and agent replies

**Verify:**

```bash
screen -r harness
# Sequence: "agent process exited" → "agent respawned successfully" → "turn complete"
```

**Note:** Events in-flight when the agent crashes are requeued — they will be retried after respawn.

---

### Scenario D: Relay Disconnect Recovery

**Description:** Kill the relay; harness should detect disconnect and reconnect automatically.

**Steps:**

1. Stop the relay:

```bash
screen -r relay
# Ctrl-C to stop, then Ctrl-A D to detach
```

2. Watch harness logs:

```bash
screen -r harness
# Look for: "relay connection lost — reconnecting"
```

3. Restart the relay:

```bash
screen -dmS relay just relay
```

**Expected behavior:**
- Harness logs: `relay connection lost — reconnecting`
- Harness logs: `relay reconnect failed: ... — retrying in 5s` (repeats until relay is back)
- After relay restarts: `relay reconnected`
- Harness re-subscribes to channels and resumes normal operation

4. Verify recovery by sending a new @mention:

```bash
mention 94a444a4-c0a3-5966-ab05-530c6ddc2301 \
  ae670a075ac2446f445808ab5a1a796cec37c72c70b25e10ee39f7f0eab50feb \
  "Post-reconnect test"
```

**Verify:**

```bash
screen -r harness
# Look for: "relay reconnected" followed by "turn complete"
```

---

### Scenario E: Turn Timeout

**Description:** Set a very short timeout; send a prompt that will take longer than the timeout. Harness should cancel the turn and log a warning.

**Steps:**

1. Restart harness with short timeout:

```bash
screen -S harness -X quit
SPROUT_ACP_TURN_TIMEOUT=5 \
SPROUT_PRIVATE_KEY=nsec1ddyp0fufd6ejerfqkxcfqlmkktwzx7w45emalvgtcvyafefusj5q8fyllm \
SPROUT_ACP_AGENT_COMMAND=goose \
SPROUT_ACP_AGENT_ARGS=acp \
GOOSE_MODE=auto \
screen -dmS harness sprout-acp
```

2. Send a prompt that requires significant computation:

```bash
mention 94a444a4-c0a3-5966-ab05-530c6ddc2301 \
  ae670a075ac2446f445808ab5a1a796cec37c72c70b25e10ee39f7f0eab50feb \
  "Write a detailed 500-word essay on the history of computing, then list 50 prime numbers"
```

**Expected behavior:**
- After ~5 seconds: `turn timeout (5s) for channel 94a444a4-... — cancelling`
- Harness sends `session/cancel` to agent
- Harness logs: `turn cancelled for channel 94a444a4-...`
- Harness continues running and accepts new events

**Verify:**

```bash
screen -r harness
# Look for: "turn timeout (5s) for channel ... — cancelling"
# Then: "turn cancelled for channel ..."
```

**Note:** Timed-out events are NOT requeued — the turn was attempted. Reset timeout to 300 after this test.

---

### Scenario F: Permission Handling (Auto-Approve)

**Description:** Verify that `GOOSE_MODE=auto` prevents the agent from pausing for permission prompts on tool calls.

**Steps:**

```bash
mention 94a444a4-c0a3-5966-ab05-530c6ddc2301 \
  ae670a075ac2446f445808ab5a1a796cec37c72c70b25e10ee39f7f0eab50feb \
  "Use your tools to get the last 5 messages from this channel and summarize them"
```

**Expected behavior:**
- Agent calls `get_channel_history` MCP tool without pausing
- No permission prompts appear in harness stdout
- Turn completes with `end_turn`

**Verify:**

```bash
screen -r harness
# Should NOT see any "waiting for approval" or "permission" lines
# Should see: "turn complete for channel ...: end_turn"
```

If the agent hangs indefinitely, `GOOSE_MODE=auto` may not be set. Kill the harness, set `GOOSE_MODE=auto`, and restart.

---

### Scenario G: Channel Discovery

**Description:** Verify the harness discovers and subscribes to the agent's channels on startup.

**Steps:**

1. Start harness fresh (no prior subscriptions):

```bash
screen -S harness -X quit
screen -dmS harness sprout-acp
screen -r harness
```

**Expected behavior:**

```
discovered N channel(s)
subscribed to channel 94a444a4-c0a3-5966-ab05-530c6ddc2301
```

**Verify:**

The harness calls the relay's REST API to discover channels associated with the agent's pubkey (`ae670a075ac2446f445808ab5a1a796cec37c72c70b25e10ee39f7f0eab50feb`). If `discovered 0 channel(s)` appears, the agent may not be a member of any channels — check the Sprout admin UI or database.

```sql
-- Check agent channel membership
SELECT c.uuid, c.name
FROM channels c
JOIN channel_members cm ON cm.channel_id = c.id
JOIN nostr_keys nk ON nk.id = cm.nostr_key_id
WHERE HEX(nk.pubkey) = 'ae670a075ac2446f445808ab5a1a796cec37c72c70b25e10ee39f7f0eab50feb';
```

---

### Scenario H: Concurrent Channels (FIFO Fairness)

**Description:** Send @mentions to two different channels; verify the harness processes them in FIFO order (oldest pending event first) with one prompt in flight at a time.

**Setup:** You need a second channel UUID. Create one in the Sprout admin UI or use an existing one. Replace `<CHANNEL_B_UUID>` below.

**Steps:**

```bash
# Send to channel A first
mention 94a444a4-c0a3-5966-ab05-530c6ddc2301 \
  ae670a075ac2446f445808ab5a1a796cec37c72c70b25e10ee39f7f0eab50feb \
  "Channel A message"

# Immediately send to channel B
mention <CHANNEL_B_UUID> \
  ae670a075ac2446f445808ab5a1a796cec37c72c70b25e10ee39f7f0eab50feb \
  "Channel B message"
```

**Expected behavior:**
- Channel A prompt fires first (oldest pending event)
- Channel B prompt fires after Channel A's turn completes
- No overlap — only one `session/prompt` in flight at a time

**Verify:**

```bash
screen -r harness
# Look for sequential "prompting agent for channel ..." lines
# Channel A should appear before Channel B
# No two "prompting agent" lines without a "turn complete" between them
```

---

### Scenario I: Multi-Agent (3 Agents, 1 Channel)

**Description:** Run three harness instances (goose, codex, claude) simultaneously, all subscribed to the same channel. Send an @mention to one; verify all three see the event and respond.

**Setup:** Each agent needs its own keypair. Mint them with `sprout-admin`:

```bash
# Agent 1 (goose) — use the test key from Prerequisites, or mint a fresh one
# Agent 2 (codex)
cargo run -p sprout-admin -- mint-token --name "agent-codex" --scopes "messages:read,messages:write,channels:read"

# Agent 3 (claude)
cargo run -p sprout-admin -- mint-token --name "agent-claude" --scopes "messages:read,messages:write,channels:read"
```

Save the `nsec` keys and pubkey hex values from the output.

**Steps:**

1. Start three harness instances in separate screen sessions:

```bash
# Goose
SPROUT_PRIVATE_KEY=<goose-nsec> \
GOOSE_MODE=auto \
screen -dmS harness-goose sprout-acp

# Codex
OPENAI_API_KEY=$(cat ~/keys/openai.key) \
SPROUT_PRIVATE_KEY=<codex-nsec> \
SPROUT_ACP_AGENT_COMMAND=/path/to/codex-acp/target/release/codex-acp \
SPROUT_ACP_AGENT_ARGS='-c,permissions.approval_policy="never"' \
screen -dmS harness-codex sprout-acp

# Claude
ANTHROPIC_API_KEY=$(cat ~/keys/anthropic.key) \
SPROUT_PRIVATE_KEY=<claude-nsec> \
SPROUT_ACP_AGENT_COMMAND=node \
SPROUT_ACP_AGENT_ARGS=/path/to/claude-agent-acp/dist/index.js \
screen -dmS harness-claude sprout-acp
```

2. Verify all three are subscribed:

```bash
for s in harness-goose harness-codex harness-claude; do
  echo "=== $s ==="; screen -S $s -X hardcopy /tmp/$s.txt; grep -c "subscribed" /tmp/$s.txt
done
```

3. Send @mentions to each agent:

```bash
mention 94a444a4-c0a3-5966-ab05-530c6ddc2301 <goose-agent-pubkey-hex> "Hello goose, reply PONG"
mention 94a444a4-c0a3-5966-ab05-530c6ddc2301 <codex-agent-pubkey-hex> "Hello codex, reply PONG"
mention 94a444a4-c0a3-5966-ab05-530c6ddc2301 <claude-agent-pubkey-hex> "Hello claude, reply PONG"
```

**Expected behavior:**
- Each harness picks up only the @mention targeting its agent pubkey
- Each agent replies via `send_message`
- All three replies appear in the DB with different sender pubkeys

**Verify:**

```bash
docker exec sprout-mysql mysql -u sprout -psprout_dev sprout -e "
SELECT SUBSTRING(HEX(pubkey), 1, 16) as sender, SUBSTRING(content, 1, 100) as body, created_at
FROM events WHERE kind = 40001 ORDER BY created_at DESC LIMIT 10;"
```

Look for three distinct sender pubkey prefixes, each with a PONG reply.

**Cleanup:**

```bash
for s in harness-goose harness-codex harness-claude; do screen -S $s -X quit; done
```

---

## Verification Commands

### Harness Log Patterns

```bash
# Attach to harness screen session
screen -r harness

# Grep harness log (if redirected to file)
grep "turn complete" /tmp/harness.log
grep "turn timeout" /tmp/harness.log
grep "agent process exited" /tmp/harness.log
grep "relay connection lost" /tmp/harness.log
```

To capture harness output to a file:

```bash
screen -S harness -X quit
screen -dmS harness bash -c 'sprout-acp 2>&1 | tee /tmp/harness.log'
```

### Database Queries

```sql
-- All recent events (last 10)
SELECT HEX(e.pubkey) as sender, e.kind,
       SUBSTRING(e.content, 1, 200) as body,
       e.created_at
FROM events e
ORDER BY e.created_at DESC LIMIT 10;

-- Agent reply events only
SELECT HEX(e.pubkey) as sender, e.kind,
       SUBSTRING(e.content, 1, 200) as body,
       e.created_at
FROM events e
WHERE e.kind = 40001
  AND HEX(e.pubkey) = 'ae670a075ac2446f445808ab5a1a796cec37c72c70b25e10ee39f7f0eab50feb'
ORDER BY e.created_at DESC LIMIT 10;

-- Events in a specific channel
SELECT HEX(e.pubkey) as sender,
       SUBSTRING(e.content, 1, 200) as body,
       e.created_at
FROM events e
JOIN event_tags et ON et.event_id = e.id
WHERE et.tag_name = 'channel'
  AND et.tag_value = '94a444a4-c0a3-5966-ab05-530c6ddc2301'
ORDER BY e.created_at DESC LIMIT 20;

-- Agent channel membership
SELECT c.uuid, c.name
FROM channels c
JOIN channel_members cm ON cm.channel_id = c.id
JOIN nostr_keys nk ON nk.id = cm.nostr_key_id
WHERE HEX(nk.pubkey) = 'ae670a075ac2446f445808ab5a1a796cec37c72c70b25e10ee39f7f0eab50feb';
```

### Relay Health

```bash
# Check relay is accepting WebSocket connections
curl -i -N -H "Connection: Upgrade" \
  -H "Upgrade: websocket" \
  -H "Sec-WebSocket-Version: 13" \
  -H "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==" \
  http://localhost:3000/

# Check relay HTTP health (if endpoint exists)
curl -s http://localhost:3000/health
```

### Process Status

```bash
# Check harness is running
pgrep -a sprout-acp

# Check agent subprocess
pgrep -a goose       # for goose
pgrep -a codex-acp   # for codex
pgrep -fa "node.*claude-agent-acp"  # for claude code

# Check all screen sessions
screen -ls
```

---

## Troubleshooting

### Harness exits immediately with "configuration error"

**Cause:** Missing required env vars.

**Fix:** Ensure `SPROUT_PRIVATE_KEY` is set:

```bash
echo $SPROUT_PRIVATE_KEY
```

Both must be valid `nsec1...` bech32 strings.

---

### "relay connect error" on startup

**Cause:** Relay not running or wrong URL.

**Fix:**

```bash
screen -ls | grep relay
# If no relay session: screen -dmS relay just relay
# Check URL: echo $SPROUT_RELAY_URL (default: ws://localhost:3000)
```

---

### "channel discovery error" on startup

**Cause:** Relay REST API unreachable, or agent pubkey not registered.

**Fix:**

```bash
# Verify relay is up
curl -s http://localhost:3000/health

# Check agent is in at least one channel (see DB query in Scenario G)
```

---

### "failed to spawn agent"

**Cause:** Agent binary not found or not executable.

**Fix:**

```bash
which goose          # for goose
ls -la $SPROUT_ACP_AGENT_COMMAND  # for codex-acp or node path
```

Ensure the binary is on `$PATH` or the full path is correct.

---

### Agent hangs, turn never completes

**Cause:** Agent waiting for permission approval (`GOOSE_MODE` not set, or codex approval policy not configured).

**Fix:**

```bash
# For goose:
export GOOSE_MODE=auto

# For codex:
export SPROUT_ACP_AGENT_ARGS='-c,permissions.approval_policy="never"'
```

Restart harness after setting.

---

### "discovered 0 channel(s)"

**Cause:** Agent pubkey is not a member of any channels on the relay.

**Fix:** Add the agent to a channel via the Sprout admin UI, or check the DB:

```sql
SELECT c.uuid, c.name
FROM channels c
JOIN channel_members cm ON cm.channel_id = c.id
JOIN nostr_keys nk ON nk.id = cm.nostr_key_id
WHERE HEX(nk.pubkey) = 'ae670a075ac2446f445808ab5a1a796cec37c72c70b25e10ee39f7f0eab50feb';
```

---

### No agent reply in DB after @mention

**Checklist:**

1. Is the harness running? `pgrep -a sprout-acp`
2. Is the harness subscribed to the channel? Check startup logs for `subscribed to channel 94a444a4-...`
3. Did the `mention` command use the correct pubkey hex? (`ae670a075ac2446f445808ab5a1a796cec37c72c70b25e10ee39f7f0eab50feb`)
4. Check harness logs for errors: `screen -r harness`
5. Did the agent have the MCP tools configured? Look for `sprout-mcp` in agent init logs.

---

### MCP tool calls failing

**Cause:** `sprout-mcp-server` binary not found, or wrong relay URL/keys passed to MCP.

**Fix:**

```bash
# Verify MCP binary
which sprout-mcp-server
# or check SPROUT_ACP_MCP_COMMAND
echo $SPROUT_ACP_MCP_COMMAND

# Test MCP server directly
SPROUT_RELAY_URL=ws://localhost:3000 \
SPROUT_PRIVATE_KEY=nsec1ddyp0fufd6ejerfqkxcfqlmkktwzx7w45emalvgtcvyafefusj5q8fyllm \
sprout-mcp-server
```

---

### codex-acp build fails

**Cause:** Rust version too old (requires 1.91+).

**Fix:**

```bash
# Using hermit (if available in repo)
cd /path/to/codex-acp
bin/hermit env
cargo build --release
```

---

## Test Results (2026-03-10)

All three agents tested end-to-end against a local relay with Docker services running.

| Agent | Adapter | Command | Test Prompt | Reply | Result |
|-------|---------|---------|-------------|-------|--------|
| **goose** | Native ACP (`goose acp`) | `goose acp` | "What is the capital of France? Reply with just the city name." | `Paris` | ✅ PASS |
| **codex** | codex-acp (Zed adapter) | `codex-acp -c permissions.approval_policy="never"` | "What is 7 times 8? Reply with just the number." | `56` | ✅ PASS |
| **claude code** | claude-agent-acp (Zed adapter) | `node dist/index.js` | "What is the square root of 144? Reply with just the number." | `12` | ✅ PASS |

### Observations

- **Session creation**: All agents created sessions in <2s. goose and claude used string UUIDs; codex used a UUID with timestamp prefix.
- **Turn latency**: goose ~4s, codex ~6s, claude ~3s for simple math/fact prompts.
- **MCP tool usage**: All agents successfully called `send_message` via the Sprout MCP server to post replies.
- **Stale event handling**: On startup, each harness instance picked up and processed stale @mention events from prior test runs. All handled gracefully.
- **Adapter build requirements**:
  - codex-acp requires Rust 1.91+ (use hermit: `REPOS/sprout2/bin/cargo`)
  - claude-agent-acp requires Node.js (`npm install && npm run build`)

### DB Verification Query Used

```bash
docker exec sprout-mysql mysql -u sprout -psprout_dev sprout -e "
SELECT
    SUBSTRING(HEX(pubkey), 1, 16) as sender_prefix,
    kind,
    SUBSTRING(content, 1, 120) as body,
    created_at
FROM events
WHERE kind = 40001
ORDER BY created_at DESC
LIMIT 10;
"
```

---

## CI Integration (Aspirational)

The test scenarios above are currently manual. This section outlines how to automate them.

### Recommended Approach

A CI integration test suite would:

1. **Spin up Docker services** — `docker compose up -d` with health checks
2. **Build all binaries** — `cargo build --release`
3. **Start relay** — as a background process with stdout captured
4. **Start harness** — with test keys, stdout captured to a log file
5. **Send @mention events** — using the `mention` binary
6. **Assert outcomes** — poll MySQL for expected reply events (with timeout)
7. **Tear down** — kill harness, relay, `docker compose down`

### Test Harness Sketch

```bash
#!/usr/bin/env bash
# ci-integration-test.sh (aspirational)
set -euo pipefail

TIMEOUT=30
RELAY_PID=""
HARNESS_PID=""

cleanup() {
  [[ -n "$HARNESS_PID" ]] && kill "$HARNESS_PID" 2>/dev/null || true
  [[ -n "$RELAY_PID" ]] && kill "$RELAY_PID" 2>/dev/null || true
}
trap cleanup EXIT

# Start relay
just relay &
RELAY_PID=$!
sleep 3

# Start harness
SPROUT_PRIVATE_KEY=nsec1ddyp0fufd6ejerfqkxcfqlmkktwzx7w45emalvgtcvyafefusj5q8fyllm \
SPROUT_ACP_AGENT_ARGS=acp \
GOOSE_MODE=auto \
./target/release/sprout-acp > /tmp/harness.log 2>&1 &
HARNESS_PID=$!
sleep 2

# Send test mention
./target/release/mention \
  94a444a4-c0a3-5966-ab05-530c6ddc2301 \
  ae670a075ac2446f445808ab5a1a796cec37c72c70b25e10ee39f7f0eab50feb \
  "CI test: reply with OK"

# Poll for reply
START=$(date +%s)
while true; do
  REPLY=$(mysql -h 127.0.0.1 -u root -proot sprout -sNe \
    "SELECT SUBSTRING(e.content, 1, 200) FROM events e
     WHERE e.kind = 40001
       AND HEX(e.pubkey) = 'ae670a075ac2446f445808ab5a1a796cec37c72c70b25e10ee39f7f0eab50feb'
     ORDER BY e.created_at DESC LIMIT 1;" 2>/dev/null || true)
  if [[ -n "$REPLY" ]]; then
    echo "✅ Agent replied: $REPLY"
    exit 0
  fi
  if (( $(date +%s) - START > TIMEOUT )); then
    echo "❌ Timeout waiting for agent reply"
    cat /tmp/harness.log
    exit 1
  fi
  sleep 1
done
```

### Blocking Issues for Full CI Automation

- **Agent API keys** — goose, codex, and claude code all require live API keys; CI needs secrets management
- **Relay startup time** — relay may need a longer warm-up than a fixed `sleep`; add a health-check poll
- **Non-deterministic agent responses** — assertions should check for _any_ reply, not specific content
- **Parallel test isolation** — each test run needs unique channel UUIDs to avoid cross-test contamination
- **`mention` binary** — must be built as part of CI setup

### Near-Term Automation Targets

The highest-value scenarios to automate first (no live API keys needed if using a mock agent):

1. **Scenario A** (basic flow) — core smoke test
2. **Scenario C** (crash recovery) — critical reliability test
3. **Scenario D** (relay reconnect) — critical reliability test
4. **Scenario E** (turn timeout) — verifiable without agent intelligence

Scenarios B, F, G, H can follow once the core pipeline is stable.
