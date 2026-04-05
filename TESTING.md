# Testing

## Automated Tests

```bash
just test-unit                    # unit tests — no infrastructure needed
just test                         # unit + integration (starts Docker if needed)
```

`just test` runs unit tests plus integration tests against Postgres, Redis, and
Typesense. It does **not** run the E2E suites in `sprout-test-client` — those
require a running relay and are marked `#[ignore]`:

```bash
# E2E tests — start the relay first, then:
cargo test -p sprout-test-client -- --ignored
```

Each E2E test file documents its own `RELAY_URL` / `RELAY_HTTP_URL` defaults.
See `crates/sprout-test-client/tests/` for source and per-file instructions.

---

## Live Testing with ACP Agents

Run AI agents against a local relay to exercise the full stack end-to-end.

```
User ──nak event──→ POST /api/events ──→ Relay ──WS──→ sprout-acp ──stdio──→ goose
                                                                                │
                                                                        sprout-mcp-server
                                                                     (send_message, etc.)
```

### Prerequisites

- Docker running
- `screen` installed (macOS: built-in; Linux: `apt install screen`)
- [nak](https://github.com/fiatjaf/nak) on PATH (`brew install nak` or `go install github.com/fiatjaf/nak@latest`)
- `goose` on PATH and configured with a provider/model

All commands below assume you're in the **repo root** (`sprout/`).

### 1. Build

**Rebuild after every code change** — screen sessions run the release binary.

```bash
. bin/activate-hermit
just setup                          # Docker services + schema + deps
cargo build --release --workspace
export PATH="$PWD/target/release:$PATH"
```

To wipe everything and start fresh: `just reset` (destroys all data).

> **Already built?** You still need the PATH export in every new shell:
> `export PATH="$PWD/target/release:$PATH"`

### 2. Start the Relay

```bash
screen -dmS relay bash -c "cd $PWD && . .env 2>/dev/null; sprout-relay 2>&1 | tee /tmp/sprout-relay.log"

sleep 3 && curl -s http://localhost:3000/health   # → "ok"
```

> The relay has built-in dev defaults matching docker-compose. Sourcing `.env`
> is only needed if you've customized ports or want the `RUST_LOG` level it sets.

### 3. Generate Keys

Each agent needs a Nostr keypair. In dev mode (`SPROUT_REQUIRE_AUTH_TOKEN=false`),
the `X-Pubkey` header authenticates all REST calls — no tokens needed.

```bash
# Agent identity
AGENT_SK=$(nak key generate)
AGENT_NSEC=$(nak encode nsec "$AGENT_SK")
AGENT_PK=$(nak key public "$AGENT_SK")

# Human user identity (for sending tasks)
USER_SK=$(nak key generate)
USER_NSEC=$(nak encode nsec "$USER_SK")
USER_PK=$(nak key public "$USER_SK")

echo "AGENT_PK=$AGENT_PK"
echo "USER_PK=$USER_PK"
```

### 4. Create a Channel and Add the Agent

Channels are created via signed Nostr events submitted to `POST /api/events`.

```bash
CHANNEL=$(python3 -c "import uuid; print(uuid.uuid4())")
echo "CHANNEL=$CHANNEL"

# Create channel (kind:9007)
nak event --sec "$USER_NSEC" -k 9007 \
  -t h="$CHANNEL" -t name="testing" -t channel_type="stream" -t visibility="open" -c "" \
| curl -s -X POST -H "Content-Type: application/json" -H "X-Pubkey: $USER_PK" \
  http://localhost:3000/api/events -d @-

# Add the agent to the channel (kind:9000)
nak event --sec "$USER_NSEC" -k 9000 \
  -t h="$CHANNEL" -t p="$AGENT_PK" -c "" \
| curl -s -X POST -H "Content-Type: application/json" -H "X-Pubkey: $USER_PK" \
  http://localhost:3000/api/events -d @-
```

### 5. Launch an ACP Agent

```bash
screen -dmS agent bash -c "
  export PATH=\"$PWD/target/release:\$PATH\"
  export SPROUT_PRIVATE_KEY=\"$AGENT_NSEC\"
  export SPROUT_RELAY_URL=ws://localhost:3000
  export SPROUT_ACP_RESPOND_TO=anyone
  export GOOSE_MODE=auto
  sprout-acp 2>&1 | tee /tmp/sprout-agent.log
"
```

Wait ~10 seconds, then verify:

```bash
tail -5 /tmp/sprout-agent.log   # should show "discovered N channel(s)"
```

| Variable | Required | Why |
|----------|----------|-----|
| `SPROUT_PRIVATE_KEY` | yes | Agent's `nsec1...` identity |
| `SPROUT_RELAY_URL` | no | Defaults to `ws://localhost:3000` |
| `SPROUT_ACP_RESPOND_TO` | no | Set to `anyone` for testing (default `owner-only` drops all events) |
| `GOOSE_MODE` | yes | Must be `auto` or goose hangs on permission prompts |

The harness auto-discovers `sprout-mcp-server` on PATH — make sure
`target/release` is in PATH inside the screen session.

### 6. Send a Task and Check Results

```bash
# @mention the agent (kind:9 with p-tag) and capture the event ID
EVENT_ID=$(nak event --sec "$USER_NSEC" -k 9 \
  -t h="$CHANNEL" -t p="$AGENT_PK" -c "Hey, say hello!" \
| curl -s -X POST -H "Content-Type: application/json" -H "X-Pubkey: $USER_PK" \
  http://localhost:3000/api/events -d @- \
| python3 -c "import json,sys; print(json.load(sys.stdin)['event_id'])")

echo "Sent event: $EVENT_ID"
```

Agent turns typically take 10–90 seconds depending on the task and model. The
ACP log goes quiet during turns — this is normal (agent I/O goes through the
stdio pipe). Check the relay for the agent's reply:

```bash
# Agent replies are threaded — use the thread endpoint
curl -s -H "X-Pubkey: $USER_PK" \
  "http://localhost:3000/api/channels/$CHANNEL/threads/$EVENT_ID" \
| python3 -c "
import json, sys
data = json.load(sys.stdin)
for r in data.get('replies', []):
    print(f'{r[\"pubkey\"][:12]}... {r[\"content\"][:200]}')
"
```

### 7. Teardown

```bash
screen -S agent -X quit
screen -S relay -X quit
docker compose down            # stop services, keep data
# or: just reset               # stop services, destroy all data
```

---

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| Testing stale code | Forgot to rebuild | `cargo build --release --workspace` after every change |
| `all events will be dropped` | Default `respond-to=owner-only` | Set `SPROUT_ACP_RESPOND_TO=anyone` |
| Agent hangs forever | `GOOSE_MODE` not set | Must be `auto` |
| Env vars not reaching agent | Unexported shell variables | All exports go inside `bash -c '...'` |
| `discovered 0 channel(s)` | Agent not a member | Create channel + add agent **before** launching |
| Agent reacts but no reply | Normal — goose is working | Wait 30–90s; check thread endpoint for replies |
| ACP log stops after startup | Normal — agent I/O is stdio | Check relay messages for evidence |
| Relay won't start | Port 3000 in use or DB stale | Kill old processes; `just reset` for clean slate |
| Need more ACP debug output | Default log level is info | Add `export RUST_LOG=sprout_acp=debug` to the screen command |
