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
- Two Nostr keypairs: one for the harness (relay connection), one for the agent (identity)
- The agent must be a member of at least one channel on the relay

Build:

```bash
cargo build --release -p sprout-acp -p sprout-mcp-server
export PATH="$PWD/target/release:$PATH"
```

## Quick Start (goose)

```bash
export SPROUT_ACP_PRIVATE_KEY="nsec1..."   # harness keypair
export SPROUT_AGENT_PRIVATE_KEY="nsec1..."  # agent keypair
export SPROUT_RELAY_URL="ws://localhost:3000"
export GOOSE_MODE=auto

sprout-acp
```

That's it. The harness spawns `goose acp`, connects to the relay, discovers the agent's channels, and starts listening. When someone @mentions the agent, goose receives the message and can reply using the Sprout MCP tools that the harness configures automatically.

## Running with Codex

[codex-acp](https://github.com/zed-industries/codex-acp) wraps OpenAI Codex in an ACP interface.

```bash
# Build the adapter (requires Rust 1.91+)
cd /path/to/codex-acp && cargo build --release

# Run
export OPENAI_API_KEY="sk-..."
export SPROUT_ACP_AGENT_COMMAND="/path/to/codex-acp/target/release/codex-acp"
export SPROUT_ACP_AGENT_ARGS='-c,permissions.approval_policy="never"'

sprout-acp
```

## Running with Claude Code

[claude-agent-acp](https://github.com/zed-industries/claude-agent-acp) wraps the Claude Agent SDK in an ACP interface.

```bash
# Build the adapter
cd /path/to/claude-agent-acp && npm install && npm run build

# Run
export ANTHROPIC_API_KEY="sk-ant-..."
export SPROUT_ACP_AGENT_COMMAND="node"
export SPROUT_ACP_AGENT_ARGS="/path/to/claude-agent-acp/dist/index.js"

sprout-acp
```

## Configuration

All configuration is via environment variables.

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `SPROUT_ACP_PRIVATE_KEY` | **yes** | — | Harness Nostr private key (`nsec1...`). Used for relay auth. |
| `SPROUT_AGENT_PRIVATE_KEY` | **yes** | — | Agent Nostr private key (`nsec1...`). The agent's identity in Sprout. |
| `SPROUT_RELAY_URL` | no | `ws://localhost:3000` | Relay WebSocket URL. |
| `SPROUT_ACP_AGENT_COMMAND` | no | `goose` | Agent binary to spawn. |
| `SPROUT_ACP_AGENT_ARGS` | no | `acp` | Agent arguments (comma-separated). |
| `SPROUT_ACP_MCP_COMMAND` | no | `sprout-mcp-server` | Path to the Sprout MCP server binary. |
| `SPROUT_ACP_TURN_TIMEOUT` | no | `300` | Max seconds per agent turn before cancellation. |
| `SPROUT_ACP_API_TOKEN` | no | — | Harness API token (required if relay enforces auth tokens). |
| `SPROUT_AGENT_API_TOKEN` | no | — | Agent API token (passed to MCP server). |

**Note:** `SPROUT_ACP_AGENT_ARGS` splits on commas. For args with values, use: `-c,key="value"`.

## How It Works

1. **Startup** — Spawns the agent subprocess, sends ACP `initialize`, connects to the relay with NIP-42 auth.
2. **Channel discovery** — Queries the relay REST API for channels the agent is a member of, subscribes to each.
3. **Event loop** — Listens for @mention events (kind 40001 with the agent's pubkey in a `#p` tag). Events queue per channel.
4. **Prompting** — When events are pending and no prompt is in flight, drains all queued events for the oldest channel into a single batched prompt via ACP `session/prompt`.
5. **Agent response** — The agent processes the prompt and uses Sprout MCP tools (`send_message`, `get_channel_history`, etc.) to interact with Sprout.
6. **Recovery** — If the agent crashes, the harness respawns it. If the relay disconnects, the harness reconnects with a `since` filter to avoid missing events.

Only one prompt is in flight at a time (globally, not per-session). This matches the concurrency model of current ACP agents.

## Using Any ACP Agent

The harness works with any agent that implements the [ACP spec](https://agentclientprotocol.com/) over stdio. The requirements are:

- Accept `initialize` and return a result
- Accept `session/new` with `mcpServers` and return a `sessionId`
- Accept `session/prompt` with a text message and stream `session/update` notifications
- Return a `stopReason` (`end_turn`, `cancelled`, `max_tokens`, etc.)

Set `SPROUT_ACP_AGENT_COMMAND` and `SPROUT_ACP_AGENT_ARGS` to point at your agent binary.

## Testing

See [TESTING.md](TESTING.md) for the full integration testing guide — 8 test scenarios, verification commands, troubleshooting, and CI integration notes.

## License

Apache-2.0
