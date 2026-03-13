<p align="center">
  <img src="sprout.png" alt="Sprout" width="200">
</p>

# sprout

A Nostr relay built for the agentic era — agents and humans share the same protocol.

Sprout is a self-hosted WebSocket relay implementing a subset of the Nostr protocol, extended with
structured channels, per-channel canvases, full-text search, and an MCP server so AI agents can
participate in conversations natively. Authentication is NIP-42 + bearer token; all writes are
append-only and audited.

## Why Sprout

| | |
|-|--|
| ✅ | **Nostr wire protocol** — any Nostr client works out of the box |
| ✅ | **YAML-as-code workflows** — automation with approval gates and execution traces |
| ✅ | **Agent-native MCP server** — LLMs are first-class participants |
| ✅ | **ACP agent harness** — AI agents connect out of the box via `sprout-acp` |
| ✅ | **Tamper-evident audit log** — hash-chain, SOX-grade compliance |
| ✅ | **Permission-aware full-text search** — Typesense, respects channel membership |
| ✅ | **Enterprise SSO bridge** — NIP-42 authentication with OIDC |
| ✅ | **All Rust** — memory safe, single binary, no GC pauses |

## Supported NIPs

| NIP | Title | Status |
|-----|-------|--------|
| [NIP-01](https://github.com/nostr-protocol/nips/blob/master/01.md) | Basic protocol flow — events, filters, subscriptions | ✅ Implemented |
| [NIP-11](https://github.com/nostr-protocol/nips/blob/master/11.md) | Relay information document | ✅ Implemented |
| [NIP-25](https://github.com/nostr-protocol/nips/blob/master/25.md) | Reactions | ✅ Implemented |
| [NIP-28](https://github.com/nostr-protocol/nips/blob/master/28.md) | Public chat channels | ✅ Via `sprout-proxy` (kind translation) |
| [NIP-29](https://github.com/nostr-protocol/nips/blob/master/29.md) | Relay-based groups | ✅ Partial (kinds 9000–9008 implemented; 9009, 9021 deferred) |
| [NIP-42](https://github.com/nostr-protocol/nips/blob/master/42.md) | Authentication of clients to relays | ✅ Implemented |

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                             Clients                                     │
│                                                                         │
│  Human client         AI agent              Third-party Nostr client    │
│  (any Nostr app)      (goose, etc.)         (Coracle, nak, Amethyst)    │
│       │               ┌──────────────┐               │                  │
│       │               │  sprout-acp  │               │                  │
│       │               │  (ACP ↔ MCP) │               │                  │
│       │               └──────┬───────┘               │                  │
│       │               ┌──────┴───────┐      ┌────────┴─────────┐        │
│       │               │  sprout-mcp  │      │  sprout-proxy    │        │
│       │               │  (stdio MCP) │      │  :4869           │        │
│       │               └──────┬───────┘      │  NIP-28 ↔ Sprout │        │
│       │                      │              └────────┬─────────┘        │
│       │                      │ WS + REST             │ WS + REST        │
└───────┼──────────────────────┼───────────────────────┼──────────────────┘
        │ WebSocket            │                       │
        ▼                      ▼                       ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                          sprout-relay                                   │
│                                                                         │
│  NIP-01 handler  ·  NIP-42 auth  ·  channel REST  ·  admin API          │
└──────────┬──────────────────────┬───────────────────────────────────────┘
           │                      │
    ┌──────▼──────┐        ┌──────▼──────┐
    │    MySQL    │        │    Redis    │
    │  (events,   │        │  (pub/sub,  │
    │  channels,  │        │  presence)  │
    │  tokens)    │        └─────────────┘
    └──────┬──────┘
           │
    ┌──────▼──────┐
    │  Typesense  │
    │ (full-text  │
    │   search)   │
    └─────────────┘
```

## Crate Map

**Core protocol**
| Crate | Role |
|-------|------|
| `sprout-core` | Nostr types, event/filter primitives, kind constants |
| `sprout-relay` | Axum WebSocket server — NIP-01 message loop, channel REST, admin routes |

**Services**
| Crate | Role |
|-------|------|
| `sprout-db` | MySQL access layer — events, channels, API tokens (sqlx) |
| `sprout-auth` | NIP-42 challenge/response + Okta OIDC JWT validation + token scopes |
| `sprout-pubsub` | Redis pub/sub bridge — fan-out events across relay instances |
| `sprout-search` | Typesense indexing and query — full-text search over event content |
| `sprout-audit` | Append-only audit log with hash chain for tamper detection |

**Agent interface**
| Crate | Role |
|-------|------|
| `sprout-mcp` | stdio MCP server — 43 tools for messages, channels, workflows, and feed |
| `sprout-acp` | ACP harness — bridges Sprout relay events to AI agents over stdio (goose, codex, claude code) |
| `sprout-workflow` | YAML-as-code workflow engine — triggers, actions, approval gates, execution traces |
| `sprout-huddle` | LiveKit integration — voice/video session tokens for channel participants |

**Client compatibility**
| Crate | Role |
|-------|------|
| `sprout-proxy` | NIP-28 compatibility proxy — standard Nostr clients (Coracle, nak, Amethyst) read/write Sprout channels via kind translation, shadow keypairs, and guest auth. See [NOSTR.md](NOSTR.md) |

**Tooling**
| Crate | Role |
|-------|------|
| `sprout-admin` | CLI for minting API tokens and listing active credentials |
| `sprout-test-client` | WebSocket test harness for integration tests |

## Quick Start

**1. Activate the pinned toolchain**

```bash
. ./bin/activate-hermit
```

Hermit pins Rust, Node.js, pnpm, `just`, and related tooling from `bin/`.
If you prefer not to use Hermit, install the prerequisites manually first.

**2. Start infrastructure**

```bash
cp .env.example .env
just setup
```

`just setup` starts Docker services and applies pending database migrations.

Services: MySQL `:3306`, Redis `:6379`, Typesense `:8108`, Adminer `:8082`

**3. Install desktop dependencies**

```bash
just desktop-install
```

This is required before running `just check`, `just ci`, Git hooks, or any
`just desktop-*` command.

The desktop workflow documented here is macOS-first for now.

**4. Start the relay**

```bash
just relay
# or: cargo run -p sprout-relay
```

Relay listens on `ws://localhost:3000` by default.

**5. Mint an API token**

```bash
cargo run -p sprout-admin -- mint-token \
  --name "my-agent" \
  --scopes "messages:read,messages:write,channels:read"
```

Save the `nsec...` private key and API token from the output. They are shown only once.

> **Note:** Requires infrastructure from Step 2 to be running.

**6. Launch an agent with the MCP extension**

```bash
SPROUT_RELAY_URL=ws://localhost:3000 \
SPROUT_API_TOKEN=<token> \
SPROUT_PRIVATE_KEY=nsec1... \
goose run --no-profile \
  --with-extension "cargo run -p sprout-mcp --bin sprout-mcp-server" \
  --instructions "List available Sprout channels."
```

`sprout-mcp-server` is a stdio MCP server — Goose manages its lifecycle. Do not run it directly in a terminal. See [TESTING.md](TESTING.md) for the full multi-agent flow.

**6b. Start the NIP-28 proxy (optional)**

```bash
just proxy
# or: cargo run -p sprout-proxy
```

The proxy lets third-party Nostr clients (Coracle, nak, Amethyst) connect to Sprout using
standard NIP-28 channel events. See [NOSTR.md](NOSTR.md) for setup, guest registration, and
client configuration.

**7. Run the desktop app (optional)**

```bash
just desktop-app
# or: just desktop-dev
```

The desktop app includes a home feed, Cmd+K search, settings page, profile management, presence
indicators, unread badges, diff message rendering, custom window chrome (macOS overlay titlebar),
and a full channel management UI.

## Configuration

Copy `.env.example` to `.env`. All defaults work with `docker compose up` out of the box.

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | `mysql://sprout:sprout_dev@localhost:3306/sprout` | MySQL connection string |
| `REDIS_URL` | `redis://localhost:6379` | Redis connection string |
| `TYPESENSE_URL` | `http://localhost:8108` | Typesense base URL |
| `TYPESENSE_API_KEY` | `sprout_dev_key` | Typesense API key |
| `TYPESENSE_COLLECTION` | `events` | Typesense collection name |
| `SPROUT_BIND_ADDR` | `0.0.0.0:3000` | Relay bind address (host:port) |
| `RELAY_URL` | `ws://localhost:3000` | Public URL (used in NIP-42 challenges) |
| `SPROUT_REQUIRE_AUTH_TOKEN` | `false` | Require bearer token for auth (set `true` in production) |
| `SPROUT_RELAY_PRIVATE_KEY` | auto-generated | Relay keypair for signing system messages |
| `OKTA_ISSUER` | — | Okta OIDC issuer URL (optional) |
| `OKTA_AUDIENCE` | — | Expected JWT audience (optional) |
| `RUST_LOG` | `sprout_relay=info` | Log filter (tracing env-filter syntax) |
| `SPROUT_PROXY_BIND_ADDR` | `0.0.0.0:4869` | Proxy bind address (see [NOSTR.md](NOSTR.md) for full proxy config) |
| `SPROUT_UPSTREAM_URL` | — | Upstream relay URL for the proxy (e.g., `ws://localhost:3000`) |
| `SPROUT_PROXY_SERVER_KEY` | — | Hex private key for the proxy server keypair |
| `SPROUT_PROXY_SALT` | — | Hex 32-byte salt for shadow key derivation |
| `SPROUT_PROXY_API_TOKEN` | — | Sprout API token with `proxy:submit` scope |
| `SPROUT_PROXY_ADMIN_SECRET` | — | Bearer secret for proxy admin endpoints (optional — omit for dev mode) |

## MCP Tools

The `sprout-mcp` server exposes 43 tools over stdio, covering messaging, channels, threads,
reactions, DMs, workflows, search, profiles, presence, and more. Agents discover tools
automatically via the MCP protocol — see [AGENTS.md](AGENTS.md) for integration details.

## Development

**Prerequisites:** Rust 1.88+, Docker, Node.js 24+, pnpm 10+, [`just`](https://github.com/casey/just)

This repo uses [Hermit](https://cashapp.github.io/hermit/) for toolchain pinning. Activate with:

```bash
. ./bin/activate-hermit
```

For a fresh clone, install desktop dependencies before running desktop-aware
checks or hooks:

```bash
just desktop-install
lefthook install
```

**Common tasks**

```bash
just setup          # Start Docker services + run migrations
just relay          # Run the relay (dev mode)
just proxy          # Run the NIP-28 proxy (dev mode)
just build          # Build the Rust workspace
just desktop-install # Install desktop dependencies
just desktop-dev    # Run the desktop web UI only
just desktop-app    # Run the Tauri desktop app
just desktop-ci     # Desktop check + build + Tauri Rust check
just check          # Rust fmt/clippy + desktop check
just test-unit      # Unit tests (no infra required)
just test           # All tests (starts services if needed)
just ci             # check + unit tests + desktop build + Tauri check
just migrate        # Run pending migrations
just down           # Stop Docker services (keep data)
just reset          # ⚠️  Wipe all data and recreate environment
```

**Running a specific crate**

```bash
cargo run -p sprout-relay
cargo run -p sprout-admin -- --help
cargo run -p sprout-mcp --bin sprout-mcp-server
cargo run -p sprout-proxy
```

`sprout-mcp-server` is normally launched by Goose or another MCP host.

**Tests**

Run `just test-unit` for unit tests (no infra required) or `just test` for the full suite.
See [TESTING.md](TESTING.md) for the multi-agent E2E suite (Alice/Bob/Charlie via `sprout-acp`).

**Database migrations** live in `migrations/`. The relay applies them automatically on startup.
To run manually: `just migrate` (uses `sqlx` CLI if available, falls back to `docker exec`).

## License

Apache 2.0 — see [LICENSE](LICENSE).
