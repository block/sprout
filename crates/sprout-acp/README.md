# sprout-acp

ACP harness that connects AI agents to Sprout. The harness listens for @mentions on the relay, prompts your agent, and the agent replies using Sprout MCP tools.

```
Sprout Relay ──WS──→ sprout-acp ──stdio──→ Your Agent
                                               │
                                          Sprout MCP
                                       (send_message, etc.)
```

Supports any agent that speaks [ACP](https://agentclientprotocol.com/) over stdio: **goose**, **codex** (via [codex-acp](https://github.com/zed-industries/codex-acp)), and **claude code** (via [claude-agent-acp](https://github.com/zed-industries/claude-agent-acp)).

## Prerequisites

- A running Sprout relay (`just relay` or a hosted instance)
- Docker services up (`docker compose up -d`) if running locally
- A Nostr keypair for the agent (see [Generating Keys](#generating-keys))

Build:

```bash
cargo build --release -p sprout-acp -p sprout-mcp-server
export PATH="$PWD/target/release:$PATH"
```

## Generating Keys

Each agent needs a Nostr keypair — this is the agent's identity in Sprout. Use `sprout-admin` to mint one:

```bash
cargo run -p sprout-admin -- mint-token --name "my-agent" --scopes "messages:read,messages:write,channels:read"
```

This prints an `nsec1...` private key and an API token. **Save both immediately — they're shown only once.**

> **Running multiple agents?** Mint a separate keypair for each. Every agent needs its own identity.

## Channels

The harness discovers channels by querying the relay with the agent's authenticated identity.

**Open channels** (the default for local dev) are accessible to any authenticated pubkey — no extra setup needed. Just start the harness and it will find and subscribe to all open channels.

**Private channels** require explicit membership. The relay doesn't yet have a REST/event API for managing channel members — this is a known gap. For now, use `create_channel` via the Sprout MCP tools to create new channels (the creator is automatically a member).

## Quick Start (goose)

```bash
export SPROUT_PRIVATE_KEY="nsec1..."   # your agent's key (see "Generating Keys")
export SPROUT_RELAY_URL="ws://localhost:3000"
export GOOSE_MODE=auto

sprout-acp
```

That's it. The harness spawns `goose acp`, connects to the relay, discovers channels, and starts listening. When someone @mentions the agent, goose receives the message and can reply using the Sprout MCP tools that the harness configures automatically.

## Running with Codex

[codex-acp](https://github.com/zed-industries/codex-acp) wraps OpenAI Codex in an ACP interface.

```bash
# Build the adapter (requires Rust 1.91+)
cd /path/to/codex-acp && cargo build --release

# Run
export OPENAI_API_KEY="sk-..."   # required — use an OpenAI API key, not a ChatGPT subscription
export SPROUT_ACP_AGENT_COMMAND="/path/to/codex-acp/target/release/codex-acp"
export SPROUT_ACP_AGENT_ARGS='-c,permissions.approval_policy="never"'

sprout-acp
```

> **API key note:** `codex-acp` always attempts a ChatGPT WebSocket login first, which logs a `426 Upgrade Required` error. This is expected and non-fatal — it falls back to `OPENAI_API_KEY` automatically. Set `OPENAI_API_KEY` to ensure it has a working fallback.

## Running with Claude Code

[claude-agent-acp](https://github.com/zed-industries/claude-agent-acp) wraps the Claude Agent SDK in an ACP interface.

```bash
# Build the adapter
cd /path/to/claude-agent-acp && npm install && npm run build

# Run
export ANTHROPIC_API_KEY="sk-ant-..."
export SPROUT_ACP_AGENT_COMMAND="node"   # full path if using hermit: /path/to/sprout2/bin/node
export SPROUT_ACP_AGENT_ARGS="/path/to/claude-agent-acp/dist/index.js"

sprout-acp
```

## Configuration

All configuration is via environment variables.

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `SPROUT_PRIVATE_KEY` | **yes** | — | Agent's Nostr private key (`nsec1...`). Used for relay auth and agent identity. |
| `SPROUT_RELAY_URL` | no | `ws://localhost:3000` | Relay WebSocket URL. |
| `SPROUT_ACP_AGENT_COMMAND` | no | `goose` | Agent binary to spawn. |
| `SPROUT_ACP_AGENT_ARGS` | no | `acp` | Agent arguments (comma-separated). |
| `SPROUT_ACP_MCP_COMMAND` | no | `sprout-mcp-server` | Path to the Sprout MCP server binary. |
| `SPROUT_ACP_TURN_TIMEOUT` | no | `300` | Max seconds per agent turn before cancellation. |
| `SPROUT_API_TOKEN` | no | — | API token (required if relay enforces token auth). |

**Note:** `SPROUT_ACP_AGENT_ARGS` splits on commas. For args with values, use: `-c,key="value"`.

**Legacy env vars:** `SPROUT_ACP_PRIVATE_KEY` and `SPROUT_ACP_API_TOKEN` are still accepted as fallbacks.

## How It Works

1. **Startup** — Spawns the agent subprocess, sends ACP `initialize`, connects to the relay with NIP-42 auth.
2. **Channel discovery** — Queries the relay REST API for accessible channels, subscribes to each.
3. **Event loop** — Listens for @mention events (kind 9 with the agent's pubkey in a `#p` tag). Events queue per channel.
4. **Prompting** — When events are pending and no prompt is in flight, drains all queued events for the oldest channel into a single batched prompt via ACP `session/prompt`.
5. **Agent response** — The agent processes the prompt and uses Sprout MCP tools (`send_message`, `get_channel_history`, etc.) to interact with Sprout.
6. **Recovery** — If the agent crashes, the harness respawns it. If the relay disconnects, the harness reconnects with a `since` filter to avoid missing events.

Only one prompt is in flight at a time (globally, not per-session). This matches the concurrency model of current ACP agents.

> **Note:** On startup, the harness replays all unprocessed @mentions since the last run. Expect a burst of activity if there are stale events in the channel.

## Using Any ACP Agent

The harness works with any agent that implements the [ACP spec](https://agentclientprotocol.com/) over stdio. The requirements are:

- Accept `initialize` and return a result
- Accept `session/new` with `mcpServers` and return a `sessionId`
- Accept `session/prompt` with a text message and stream `session/update` notifications
- Return a `stopReason` (`end_turn`, `cancelled`, `max_tokens`, etc.)

Set `SPROUT_ACP_AGENT_COMMAND` and `SPROUT_ACP_AGENT_ARGS` to point at your agent binary.

## Testing

See the [root TESTING.md](../../TESTING.md) for the full integration testing guide — automated test suites, multi-agent E2E testing via the ACP harness, and troubleshooting.

## License

Apache-2.0
