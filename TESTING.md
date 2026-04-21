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

## Testing Projects

Projects group channels and provide shared context to agents. The feature spans
`sprout-core`, `sprout-db`, `sprout-sdk`, `sprout-relay`, and `sprout-mcp`.

### Automated Tests

```bash
# SDK builder tests (no infrastructure needed)
cargo test -p sprout-sdk -- project

# Relay ingest classification + d-tag extraction tests
cargo test -p sprout-relay -- project
cargo test -p sprout-relay -- extract_d_tag_uuid

# DB CRUD tests (require running Postgres)
cargo test -p sprout-db -- project --ignored
```

**SDK tests** (9 tests) verify event construction:
- `create_project_happy_path` / `_all_fields` / `_no_repos` — kind 50001, d-tag, name tag, optional fields
- `update_project_partial` / `_clear_repos` / `_no_fields_rejected` — kind 50002, partial updates, sentinel tag for repo clearing
- `delete_project_happy_path` — kind 50003, d-tag only
- `create_channel_with_project` / `_without_project` — `["project", uuid]` tag presence

**Relay tests** (6 tests) verify ingest pipeline classification:
- `project_kinds_require_channels_write` — 50001-50003 require `ChannelsWrite` scope
- `project_kinds_are_global_only` — projects are not channel-scoped
- `project_kinds_not_channel_scoped` — no h-tag required
- `extract_d_tag_uuid_valid` / `_missing` / `_invalid` — d-tag UUID extraction

**DB tests** (6 tests, `#[ignore]` — require Postgres):
- `create_and_get_project` — round-trip create + fetch
- `list_projects_by_creator` — filtering by pubkey
- `update_project_partial` — partial field updates
- `update_project_repos` — set and clear JSONB repo_urls
- `soft_delete_project` — soft delete, verify not found
- `archive_and_unarchive_project` — archive round-trip

### Manual Testing with MCP Tools

With the relay running, use the MCP tools via `sprout-mcp-server`:

```bash
# Create a project
# MCP tool: create_project
#   name: "my-project"
#   environment: "local"
#   description: "Test project"
#   repo_urls: ["https://github.com/example/repo"]

# List all projects
# MCP tool: list_projects

# Get a specific project
# MCP tool: get_project
#   project_id: "<uuid from create>"

# Update a project
# MCP tool: update_project
#   project_id: "<uuid>"
#   name: "renamed-project"
#   prompt: "You are a helpful coding assistant"

# Delete a project
# MCP tool: delete_project
#   project_id: "<uuid>"
```

### Channel-Project Association

Create a channel linked to a project:

```bash
# MCP tool: create_channel
#   name: "project-channel"
#   channel_type: "stream"
#   visibility: "open"
#   project_id: "<project-uuid>"

# Verify via REST API:
curl -s -H "X-Pubkey: $USER_PK" \
  http://localhost:3000/api/projects/<project-uuid>/channels
```

### Validation Checks

- **Environment validation**: only `"local"` and `"blox"` are accepted. Other values
  are rejected at ingest time (kind 50001) and update time (kind 50002).
- **Empty update guard**: `update_project` with no fields returns an error from the SDK.
- **Repo clearing**: to clear repos, pass an empty `repo_urls: []`. The SDK emits a
  sentinel `["repo", ""]` tag so the relay knows to set the field to `[]` rather than
  ignoring it.

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
