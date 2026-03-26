# AGENTS.md — AI Agent Contributor Guide

This guide is for AI agents contributing to the Sprout codebase. It covers
agent-specific context and conventions. For general contributor info (setup,
code style, PR process, architecture), see [CONTRIBUTING.md](CONTRIBUTING.md).

---

## Repo Structure

```
crates/
  sprout-relay        # WebSocket relay server — main entry point
  sprout-core         # Core types, event verification, filter matching
  sprout-db           # Postgres event store and data access layer
  sprout-auth         # Authentication and authorization
  sprout-pubsub       # Redis pub/sub fan-out, presence, typing indicators
  sprout-mcp          # MCP server providing AI agent tools
  sprout-acp          # ACP harness bridging Sprout events to AI agents
  sprout-workflow     # YAML-as-code workflow engine (evalexpr conditions)
  sprout-search       # Typesense-backed full-text search
  sprout-audit        # Hash-chain audit log
  sprout-huddle       # LiveKit audio/video integration
  sprout-proxy        # Nostr client compatibility proxy
  sprout-admin        # Operator CLI for relay administration
  sprout-test-client  # Integration test client and E2E test suite

desktop/              # Tauri 2 + React 19 desktop app
migrations/           # SQL migrations (auto-applied on relay startup)
scripts/              # Dev tooling
.env.example          # Config template — copy to .env before running
```

---

## Getting Started

```bash
. ./bin/activate-hermit   # activate hermit toolchain (Rust, Node, etc.)
cp .env.example .env      # configure local environment
just setup                # install deps, run migrations
just relay                # start relay at ws://localhost:3000
just ci                   # run before any PR
```

See CONTRIBUTING.md for full setup details and dependency requirements.

---

## Quality Gates

Run `just ci` before every PR. It runs: `fmt`, `clippy`, unit tests, desktop
build, and Tauri check. All must pass.

Run `just test` for integration tests if you touched `sprout-relay`,
`sprout-db`, or `sprout-auth` — these require a running Postgres and Redis.

Additional rules:
- No `unsafe` code
- No `unwrap()` or `expect()` in production paths — use `?` and proper error types
- New public API must have doc comments

---

## Key Patterns

**Dual API surface**: Sprout exposes both a REST API and a NIP-29 WebSocket
relay. Both paths converge on shared DB functions in `sprout-db`. When adding
a feature, implement the shared DB logic first, then wire up both surfaces.

**Event kinds**: All event kind integers are defined in
`sprout-core/src/kind.rs`. New features get new kind integers — add them here
first, then implement handling in the relay.

**Channel scoping**: Channels use `h` tags (NIP-29 group tag), not `e` tags.
Filters and queries must scope to `h` tags when operating within a channel.

**MCP tools proxy REST**: The MCP server in `sprout-mcp` wraps REST endpoints.
Add the REST endpoint first, then add the MCP tool that calls it. Do not
implement logic directly in MCP handlers.

**Workflow conditions**: `sprout-workflow` uses
[evalexpr](https://docs.rs/evalexpr) for condition evaluation. Keep expressions
simple and testable.

**Thread counters**: `reply_count` and `descendant_count` are materialized on
thread root events. Any code that inserts replies must update these counters —
check existing reply handlers for the pattern.

---

## Testing

```bash
just test-unit    # unit tests, no infrastructure needed
just test         # full integration suite (requires Postgres + Redis)
```

E2E tests live in `crates/sprout-test-client/tests/`:
- `e2e_rest_api.rs` — REST endpoint coverage
- `e2e_relay.rs` — WebSocket relay protocol
- `e2e_mcp.rs` — MCP tool surface
- `e2e_tokens.rs` — auth token flows
- `e2e_workflows.rs` — workflow engine

Desktop E2E: `cd desktop && pnpm exec playwright test`

See [TESTING.md](TESTING.md) for the full multi-agent E2E guide.

---

## Desktop App

The desktop app is Tauri 2 + React 19 + Vite + Tailwind CSS. Features are
organized under `desktop/src/features/`. Biome handles linting and formatting.

```bash
just desktop-dev   # web-only dev server (faster iteration)
just desktop-app   # full Tauri app with native shell
```

---

## See Also

- [CONTRIBUTING.md](CONTRIBUTING.md) — setup, code style, PR process, how to add event kinds / MCP tools / API endpoints
- [TESTING.md](TESTING.md) — multi-agent E2E test guide
- [ARCHITECTURE.md](ARCHITECTURE.md) — system design and component relationships
- [README.md](README.md) — project overview and quick start
