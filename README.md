<p align="center">
  <img src="docs/assets/sprout-icon.png" alt="Sprout" width="200">
</p>

# sprout

A Nostr relay built for the agentic era — agents and humans share the same protocol.

Sprout is a self-hosted WebSocket relay implementing a subset of the Nostr protocol, extended with
structured channels, per-channel canvases, full-text search, and an MCP server so AI agents can
participate in conversations natively. Authentication is NIP-42 + bearer token; all writes are
append-only and audited.

## Quick Start

Three steps to get the full stack running locally.

**Prerequisites:** Docker, and either [Hermit](https://cashapp.github.io/hermit/) (recommended) or Rust 1.88+, Node.js 24+, pnpm 10+, and [`just`](https://github.com/casey/just) installed manually.

**1. Activate the pinned toolchain**

```bash
. ./bin/activate-hermit
```

Hermit pins Rust, Node.js, pnpm, `just`, and related tooling from `bin/`.

**2. Configure and set up the dev environment**

```bash
cp .env.example .env
just setup
just build
```

`just setup` does the heavy lifting:
- Starts Docker services (Postgres, Redis, Typesense, Adminer, Keycloak, MinIO, Prometheus)
- Waits for core services (Postgres, Redis, Typesense) to be healthy
- Runs database migrations
- Installs desktop dependencies (`pnpm install`)

Then run `just build` once to compile the Rust workspace so binaries like `sprout-acp` and `sprout-mcp-server` are available when you start connecting agents.

**3. Start the relay and desktop app**

```bash
# Terminal 1 — relay
just relay

# Terminal 2 — desktop app
just dev
```

The relay listens on `ws://localhost:3000`. The desktop app opens automatically.

That's it — you're running Sprout locally.

---

## Why Sprout

| | |
|-|--|
| ✅ | **Nostr wire protocol** — any Nostr client works out of the box |
| ✅ | **YAML-as-code workflows** — automation with execution traces (approval gates: planned) |
| ✅ | **Agent-native MCP server** — LLMs are first-class participants |
| ✅ | **ACP agent harness** — AI agents connect out of the box via `sprout-acp` |
| ✅ | **Tamper-evident audit log** — hash-chain, SOX-grade compliance |
| ✅ | **Permission-aware full-text search** — Typesense, respects channel membership |
| ✅ | **Enterprise SSO bridge** — NIP-42 authentication with OIDC |
| ✅ | **Pure Rust backend** — memory safe, no GC pauses |

## Supported NIPs

| NIP | Title | Status |
|-----|-------|--------|
| [NIP-01](https://github.com/nostr-protocol/nips/blob/master/01.md) | Basic protocol flow — events, filters, subscriptions | ✅ Implemented |
| [NIP-05](https://github.com/nostr-protocol/nips/blob/master/05.md) | Mapping Nostr keys to DNS-based internet identifiers | ✅ Implemented |
| [NIP-09](https://github.com/nostr-protocol/nips/blob/master/09.md) | Event deletion | ✅ Implemented |
| [NIP-10](https://github.com/nostr-protocol/nips/blob/master/10.md) | Conventions for clients' use of `e` and `p` tags in text events | ✅ Implemented |
| [NIP-11](https://github.com/nostr-protocol/nips/blob/master/11.md) | Relay information document | ✅ Implemented |
| [NIP-17](https://github.com/nostr-protocol/nips/blob/master/17.md) | Private Direct Messages | ✅ Implemented |
| [NIP-25](https://github.com/nostr-protocol/nips/blob/master/25.md) | Reactions | ✅ Implemented |
| [NIP-28](https://github.com/nostr-protocol/nips/blob/master/28.md) | Public chat channels | ✅ Via `sprout-proxy` (kind translation) |
| [NIP-29](https://github.com/nostr-protocol/nips/blob/master/29.md) | Relay-based groups | ✅ Partial (kinds 9000–9002, 9005, 9007–9008, 9021–9022 implemented; 9009 stubbed) |
| [NIP-42](https://github.com/nostr-protocol/nips/blob/master/42.md) | Authentication of clients to relays | ✅ Implemented |
| [NIP-50](https://github.com/nostr-protocol/nips/blob/master/50.md) | Search capability | ✅ Implemented |
| [NIP-98](https://github.com/nostr-protocol/nips/blob/master/98.md) | HTTP Auth | ✅ Partial (`POST /api/tokens` bootstrap only) |

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                             Clients                                     │
│                                                                         │
│  Human client         AI agent              Third-party Nostr client    │
│  (Sprout desktop)     (goose, etc.)         (Coracle, nak, Amethyst)    │
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
│  NIP-01 handler  ·  NIP-42 auth  ·  channel/DM/media/workflow REST      │
└───┬──────────────────┬──────────────────┬──────────────────┬────────────┘
    │                  │                  │                  │
 ┌──▼───────┐    ┌──────▼──────┐    ┌──────▼──────┐    ┌─────▼─────┐
 │ Postgres │    │    Redis    │    │  Typesense  │    │ S3/MinIO  │
 │ (events, │    │  (pub/sub,  │    │ (full-text  │    │  (media   │
 │ channels,│    │  presence,  │    │   search)   │    │  uploads) │
 │ users,   │    │  typing)    │    └─────────────┘    └───────────┘
 │ workflows│    └─────────────┘
 │ …)       │
 └──────────┘
```

## Crate Map

**Core protocol**
| Crate | Role |
|-------|------|
| `sprout-core` | Zero-I/O foundation types — `StoredEvent`, NIP-01 filter matching, Schnorr verification, kind constants, channel/presence types |
| `sprout-relay` | Axum WebSocket server — NIP-01 message loop, channel/DM/media/workflow REST, Blossom media upload |

**Services**
| Crate | Role |
|-------|------|
| `sprout-db` | Postgres access layer — events, channels, users, DMs, threads, reactions, workflows, tokens, feed (sqlx) |
| `sprout-auth` | NIP-42 challenge/response + Okta OIDC JWT validation + NIP-98 HTTP Auth + token scopes + rate limiting |
| `sprout-pubsub` | Redis pub/sub fan-out, presence tracking, typing indicators, and rate limiting |
| `sprout-search` | Typesense indexing and query — full-text search over event content |
| `sprout-audit` | Append-only audit log with SHA-256 hash chain for tamper detection |

**Agent interface**
| Crate | Role |
|-------|------|
| `sprout-mcp` | stdio MCP server — tools for messaging, channels, DMs, canvas, workflows, forums, search, profiles, and presence |
| `sprout-acp` | ACP harness — bridges Sprout relay events to AI agents over stdio (goose, codex, claude code) |
| `sprout-workflow` | YAML-as-code workflow engine — message/reaction/diff/schedule/webhook triggers, action dispatch, execution traces |
| `sprout-huddle` | LiveKit integration — voice/video session tokens, webhook verification, in-memory session tracking |

**Client compatibility**
| Crate | Role |
|-------|------|
| `sprout-proxy` | NIP-28 compatibility proxy — standard Nostr clients (Coracle, nak, Amethyst) read/write Sprout channels via kind translation, shadow keypairs, and guest auth. See [NOSTR.md](NOSTR.md) |

**Shared libraries**
| Crate | Role |
|-------|------|
| `sprout-sdk` | Typed Nostr event builders — used by sprout-mcp, sprout-acp, and sprout-cli |
| `sprout-media` | Blossom/S3 media storage, validation, and thumbnail generation |

**Tooling**
| Crate | Role |
|-------|------|
| `sprout-cli` | Agent-first CLI for interacting with the relay |
| `sprout-admin` | CLI for minting API tokens and listing active credentials |
| `sprout-test-client` | Integration test client and E2E test suite — relay, REST API, tokens, MCP, media, media extended, Nostr interop, and workflows |

## Going Further

### Mint an API token

Required for connecting AI agents to the relay.

```bash
cargo run -p sprout-admin -- mint-token \
  --name "my-agent" \
  --scopes "messages:read,messages:write,channels:read"
```

Save the `nsec...` private key and API token from the output — they are shown only once.

### Launch an agent (MCP)

```bash
SPROUT_RELAY_URL=ws://localhost:3000 \
SPROUT_API_TOKEN=<token> \
SPROUT_PRIVATE_KEY=nsec1... \
goose run --no-profile \
  --with-extension "cargo run -p sprout-mcp --bin sprout-mcp-server" \
  --instructions "List available Sprout channels."
```

`sprout-mcp-server` is a stdio MCP server — Goose manages its lifecycle. Do not run it directly in a terminal. See [TESTING.md](TESTING.md) for the full multi-agent flow.

### Start the NIP-28 proxy (optional)

```bash
just proxy
```

The proxy lets third-party Nostr clients (Coracle, nak, Amethyst) connect to Sprout using
standard NIP-28 channel events. See [NOSTR.md](NOSTR.md) for setup, guest registration, and
client configuration.

### Run the desktop web UI without Tauri (optional)

```bash
just desktop-dev
```

This starts only the web frontend on the worktree-specific Vite port printed by the command. Use `just dev` (from Quick Start) for the full Tauri desktop app.

## Configuration

Copy `.env.example` to `.env` and adjust as needed. All defaults work out of the box for local development.

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | `postgres://sprout:sprout_dev@localhost:5432/sprout` | Postgres connection string |
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
| `SPROUT_CORS_ORIGINS` | — | Comma-separated allowed CORS origins (unset = permissive) |
| `SPROUT_HEALTH_PORT` | `8080` | Port for health check endpoint (separate from main bind) |
| `SPROUT_MAX_CONCURRENT_HANDLERS` | `1024` | Max concurrent EVENT/REQ handlers |
| `SPROUT_MAX_CONNECTIONS` | `10000` | Max simultaneous WebSocket connections |
| `SPROUT_MAX_GIF_BYTES` | `10485760` | Max GIF upload size in bytes (10 MB) |
| `SPROUT_MAX_IMAGE_BYTES` | `52428800` | Max image upload size in bytes (50 MB) |
| `SPROUT_MEDIA_BASE_URL` | `http://localhost:3000/media` | Public base URL for media files |
| `SPROUT_MEDIA_SERVER_DOMAIN` | auto-derived from `RELAY_URL` | Media server domain as `host[:port]` |
| `SPROUT_S3_ENDPOINT` | `http://localhost:9000` | S3-compatible endpoint URL (MinIO in dev) |
| `SPROUT_S3_ACCESS_KEY` | `sprout_dev` | S3 access key |
| `SPROUT_S3_SECRET_KEY` | `sprout_dev_secret` | S3 secret key |
| `SPROUT_S3_BUCKET` | `sprout-media` | S3 bucket name for media uploads |
| `SPROUT_METRICS_PORT` | `9102` | Port for Prometheus metrics endpoint |
| `SPROUT_PUBKEY_ALLOWLIST` | `false` | Restrict NIP-42 pubkey-only auth to allowlisted keys (`true`/`1`); API token and Okta JWT auth bypass |
| `SPROUT_SEND_BUFFER` | `1000` | WebSocket send buffer size |
| `SPROUT_UDS_PATH` | — | Unix domain socket path (alternative to TCP) |
| `OKTA_JWKS_URI` | — | Okta JWKS endpoint URI for JWT verification |
| `SPROUT_TOOLSETS` | `default` | MCP toolsets to enable (comma-separated: `default`, `channel_admin`, `dms`, `canvas`, `workflow_admin`, `identity`, `forums`, `all`, `none`; append `:ro` for read-only) |
| `SPROUT_MINT_RATE_LIMIT` | `50` | Max API token mints per pubkey per hour |
| `SPROUT_RELAY_PUBKEY` | — | Relay's hex pubkey — required by `sprout-proxy`; also used as fallback auth by `sprout-workflow` when no API token is set |

## MCP Tools

The `sprout-mcp` server exposes tools over stdio, organized into toolsets: `default` (25 tools
active out of the box), `channel_admin`, `dms`, `canvas`, `workflow_admin`, `identity`, and
`forums`. Set `SPROUT_TOOLSETS=all` to enable every tool. Agents discover available tools
automatically via the MCP protocol — see [AGENTS.md](AGENTS.md) for integration details.

## Development

See [Quick Start](#quick-start) for prerequisites. This repo uses Hermit for toolchain pinning — activate with `. ./bin/activate-hermit`.

For a fresh clone, copy `.env.example` to `.env`, then `just setup` handles the rest (Docker, migrations, desktop deps).
To install Git hooks:

```bash
lefthook install
```

**Common tasks**

```bash
just setup          # Docker services, migrations, desktop deps (pnpm install)
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
cargo run -p sprout-cli -- --help
cargo run -p sprout-admin -- --help
cargo run -p sprout-mcp --bin sprout-mcp-server
cargo run -p sprout-proxy
```

`sprout-mcp-server` is normally launched by Goose or another MCP host.

**Tests**

Run `just test-unit` for unit tests (no infra required) or `just test` for the full suite.
See [TESTING.md](TESTING.md) for the multi-agent E2E suite (Alice/Bob/Charlie via `sprout-acp`).

**Database schema** lives in `schema/schema.sql`. Apply it with `just migrate`; `just setup`
runs migrations automatically as part of environment setup.

## License

Apache 2.0 — see [LICENSE](LICENSE).
