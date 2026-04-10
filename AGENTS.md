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
  sprout-sdk          # Typed Nostr event builders (used by sprout-mcp and sprout-cli)
  sprout-media        # Blossom/S3 media storage
  sprout-cli          # Agent-first CLI

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

Run `just ci` before every PR. It runs: Rust `fmt` + `clippy`, desktop lint
(Biome), unit tests, desktop build, and Tauri check. All must pass.

Run `just test` for integration tests if you touched `sprout-relay`,
`sprout-db`, or `sprout-auth` — these require a running Postgres and Redis.

Additional rules:
- No `unsafe` code
- Do not introduce new `unwrap()` or `expect()` in production paths — use `?` and proper error types
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

**MCP tools — dual transport**: The MCP server in `sprout-mcp` uses two
patterns: write operations send signed Nostr events over WebSocket; read
operations call REST endpoints (see `relay_client.rs` for the HTTP helpers).
Add the REST endpoint or event handler first, then add the MCP tool that calls
it. Do not implement logic directly in MCP handlers.

**Workflow conditions**: `sprout-workflow` uses
[evalexpr](https://docs.rs/evalexpr) for condition evaluation. Keep expressions
simple and testable.

**Thread counters**: `reply_count` and `descendant_count` are materialized on
thread root events. Any code that inserts replies must update these counters —
check existing reply handlers for the pattern.

**Identity binding (proxy mode)**: In corporate deployments the relay sits
behind a trusted auth proxy that injects identity JWT and device CN headers
(configured via `SPROUT_IDENTITY_JWT_HEADER` and `SPROUT_IDENTITY_DEVICE_CN_HEADER`).
`SPROUT_IDENTITY_MODE` controls behaviour:

- `disabled` (default) — standard Nostr key-based auth only.
- `proxy` — all connections must present a valid identity JWT; the relay binds
  (uid, device\_cn) → pubkey in the `identity_bindings` table. NIP-42 is still
  required to prove pubkey ownership.
- `hybrid` — identity JWT preferred for humans; connections without the header
  fall through to standard auth (API tokens, Okta JWTs) for agents.

Identity bindings are **immutable** — once a (uid, device\_cn) is bound to a
pubkey, a different pubkey for the same slot returns a mismatch error. Use
`sprout-admin unbind-identity` to clear a binding (e.g., key rotation, device
reset, offboarding).

**Trusted-proxy security invariant**: The relay trusts proxy headers
unconditionally. It **must** be deployed behind the trusted reverse proxy —
direct access to the relay port would allow header injection.

---

## Testing

```bash
just test-unit    # unit tests, no infrastructure needed
just test         # full integration suite (requires Postgres + Redis)
```

E2E tests live in `crates/sprout-test-client/tests/`:
- `e2e_relay.rs` — WebSocket relay protocol
- `e2e_rest_api.rs` — REST endpoint coverage
- `e2e_mcp.rs` — MCP tool surface
- `e2e_tokens.rs` — auth token flows
- `e2e_workflows.rs` — workflow engine
- `e2e_media.rs` — media upload/download (Blossom)
- `e2e_media_extended.rs` — extended media scenarios
- `e2e_nostr_interop.rs` — Nostr interop (NIP-50 search, NIP-10 threads, NIP-17 gift wraps)

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

## Mobile App (Flutter)

The mobile app lives in `mobile/` — a Flutter app using Riverpod + Hooks.

### Architecture

- **State management:** Riverpod + `flutter_hooks` (`HookConsumerWidget`)
- **Theme:** Catppuccin Latte (light) / Macchiato (dark) — matches desktop
- **Features:** Isolated under `lib/features/`, shared code in `lib/shared/`
- **Nostr models:** `lib/shared/relay/nostr_models.dart` — event kinds must
  stay in sync with `desktop/src/shared/constants/kinds.ts`

### Rules

- **NEVER use `StatefulWidget`** — always use `HookConsumerWidget` or
  `ConsumerWidget` with `flutter_hooks` for local state.
- **NEVER run `flutter run`, `flutter build`, `flutter clean`, or
  `flutter upgrade`** — only `flutter test`, `flutter analyze`, and
  `dart format` are safe for agents to run.
- **Do NOT use `print()`** — use `debugPrint()` or structured logging.
- Prefer `context.colors` and `context.textTheme` (via theme extensions)
  over raw `Theme.of(context)` calls.
- Keep widgets small and composable.
- Feature modules must not import from other feature modules — only from
  `shared/`.
- Use `Grid` tokens for spacing, `Radii` for border radius.

### Quality Checks

```bash
cd mobile
dart format --output=none --set-exit-if-changed .
flutter analyze
flutter test
```

Or from repo root: `just mobile-check` and `just mobile-test`.

### Testing Conventions

- Prefer **widget tests** over unit tests for UI components — test the
  whole widget tree, not individual methods.
- Use `ProviderScope(overrides: [...])` to inject fake notifiers.
- Fake notifiers should extend the real notifier class and override `build()`.
- Use the `WidgetHelpers.testable()` wrapper for simple widget tests or
  build a custom `ProviderScope` + `MaterialApp` when you need specific overrides.

---

## See Also

- [CONTRIBUTING.md](CONTRIBUTING.md) — setup, code style, PR process, how to add event kinds / MCP tools / API endpoints
- [TESTING.md](TESTING.md) — multi-agent E2E test guide
- [ARCHITECTURE.md](ARCHITECTURE.md) — system design and component relationships
- [README.md](README.md) — project overview and quick start
