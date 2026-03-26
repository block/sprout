# Contributing to Sprout

Welcome, and thank you for your interest in contributing! Sprout is an
open-source project and we're glad you're here. This guide will help you
get from zero to a merged pull request.

If you have questions that aren't answered here, open a GitHub Discussion or
reach out in the community channels.

---

## Table of Contents

1. [Code of Conduct](#code-of-conduct)
2. [Setting Up the Development Environment](#setting-up-the-development-environment)
3. [Running Tests](#running-tests)
4. [Code Style](#code-style)
5. [Making a Pull Request](#making-a-pull-request)
6. [Architecture Overview](#architecture-overview)
7. [How to Add a New Event Kind](#how-to-add-a-new-event-kind)
8. [How to Add a New MCP Tool](#how-to-add-a-new-mcp-tool)
9. [How to Add a New API Endpoint](#how-to-add-a-new-api-endpoint)
10. [License and CLA](#license-and-cla)

---

## Code of Conduct

This project follows the [Contributor Covenant v2.1](CODE_OF_CONDUCT.md).
By participating you agree to uphold these standards. Please report
unacceptable behavior to **conduct@sprout-relay.org**.

---

## Setting Up the Development Environment

### Prerequisites

| Tool | Version | Notes |
|------|---------|-------|
| Rust | 1.88+ | Install via [rustup](https://rustup.rs/) |
| Node.js | 24+ | Required for desktop app commands and `just ci` |
| pnpm | 10+ | Required for desktop app commands and `just ci` |
| Docker | 24+ | For Postgres, Redis, Typesense |
| `just` | latest | Task runner — `cargo install just` |
| `lefthook` | latest | Optional; run `lefthook install` for local Git hooks |
| `sqlx-cli` | latest | Optional; `just migrate` falls back to `docker exec` |

This repo uses [Hermit](https://cashapp.github.io/hermit/) for toolchain
pinning. Activate it once per shell session:

```bash
. ./bin/activate-hermit
```

Hermit pins Rust, `just`, and other tools to the versions in `bin/`. If you
don't use Hermit, make sure your Rust toolchain meets the minimum version.

### First-Time Setup

```bash
# 1. Clone the repo
git clone https://github.com/sprout-rs/sprout.git
cd sprout

# 2. Activate Hermit (optional but recommended)
. ./bin/activate-hermit

# 3. Copy environment config
cp .env.example .env

# 4. Start infrastructure + run migrations
just setup

# 5. Install Git hooks (optional, recommended)
lefthook install
```

`just setup` starts Docker services (Postgres on `:5432`, Redis on `:6379`,
Typesense on `:8108`, Adminer on `:8082`, Keycloak on `:8180` for local
OAuth/OIDC testing) and runs all pending database migrations.

### Running the Relay

```bash
just relay
# or: cargo run -p sprout-relay
```

The relay listens on `ws://localhost:3000` by default. You should see log
output confirming the WebSocket server is up and migrations have run.

### Stopping / Resetting

```bash
just down    # Stop Docker services, keep data
just reset   # ⚠️  Wipe all data and recreate the environment
```

---

## Running Tests

### Unit Tests (no infrastructure required)

```bash
just test-unit
```

Unit tests are self-contained and run without Docker. They cover event
parsing, filter matching, auth logic, workflow YAML parsing, and more.

### Integration Tests (requires running infrastructure)

```bash
just test
```

Integration tests spin up the relay and exercise the full stack — WebSocket
connections, NIP-42 auth, event ingestion, search indexing, and workflow
execution. `just test` starts Docker services automatically if they're not
already running.

### End-to-End Tests

End-to-end tests live in `crates/sprout-test-client/tests/`:

- `e2e_rest_api.rs` — REST API tests
- `e2e_relay.rs` — WebSocket relay tests
- `e2e_mcp.rs` — MCP tool tests
- `e2e_tokens.rs` — token management tests
- `e2e_workflows.rs` — workflow tests

Run them with (requires running infrastructure):

```bash
cargo test -p sprout-test-client
```

See `TESTING.md` for the full multi-agent E2E testing guide.

### CI Gate

Before opening a PR, run the full CI gate locally:

```bash
just ci
# Runs: check + unit tests + desktop build + Tauri check
```

This is the same check that runs in CI. PRs that fail `just ci` will not be
merged.

---

## Code Style

### Formatting

We use `rustfmt` with default settings. Format your code before committing:

```bash
cargo fmt --all
```

To check without modifying:

```bash
cargo fmt --all -- --check
```

### Linting

We use `clippy` with warnings-as-errors:

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

Fix all clippy warnings before submitting a PR. If you believe a warning is
a false positive, add a targeted `#[allow(...)]` with a comment explaining
why.

### No Unsafe Code

All crates enforce `#![deny(unsafe_code)]`. Do not add unsafe blocks. If you
believe unsafe is genuinely necessary, open an issue first to discuss the
approach.

### Error Handling

- Use `thiserror` for library error types.
- Use `anyhow` for binary / application-level error propagation.
- Do not use `unwrap()` or `expect()` in production code paths. Use `?` or
  explicit error handling. `unwrap()` is acceptable in tests.

### Logging and Tracing

Use the `tracing` crate for all instrumentation. Prefer structured fields
over string interpolation:

```rust
// Good
tracing::info!(channel_id = %id, event_kind = kind, "Event ingested");

// Avoid
tracing::info!("Event ingested: channel={id} kind={kind}");
```

### Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
feat(mcp): add get_feed_actions tool
fix(auth): reject expired NIP-42 challenges
docs(agents): document workflow MCP tools
refactor(db): extract channel queries into channel.rs
test(workflow): add approval gate integration test
```

The type prefix (`feat`, `fix`, `docs`, `refactor`, `test`, `chore`) is
required. The scope (in parentheses) is optional but encouraged.

---

## Making a Pull Request

### Before You Start

- Check open issues and PRs to avoid duplicate work.
- For significant changes, open an issue first to discuss the approach.
- For small fixes (typos, doc improvements, obvious bugs), go ahead and open
  a PR directly.

### What a Good PR Looks Like

1. **Focused** — one logical change per PR. If you're fixing a bug and
   refactoring a module, split them into two PRs.

2. **Tested** — new behavior has tests. Bug fixes include a regression test.
   If a test is impractical, explain why in the PR description.

3. **Documented** — public APIs, new event kinds, new MCP tools, and new
   config variables are documented. Update `README.md`, `AGENTS.md`, or
   `VISION.md` as appropriate.

4. **CI passing** — `just ci` passes locally before you push.

5. **Clear description** — the PR description explains:
   - What problem this solves (or what feature it adds)
   - How it was implemented (key decisions, trade-offs)
   - How to test it manually (if applicable)
   - Any follow-up work deferred to a future PR

### PR Checklist

```
- [ ] `just ci` passes (fmt + clippy + unit tests)
- [ ] Integration tests pass (`just test`)
- [ ] New public APIs / tools / endpoints are documented
- [ ] No new `unwrap()` in production code paths
- [ ] No new `unsafe` blocks
```

### Review Process

- A maintainer will review your PR within a few business days.
- Address review comments by pushing new commits (don't force-push during
  review; it makes it hard to see what changed).
- Once approved, a maintainer will squash-merge your PR.

---

## Architecture Overview

See [README.md](README.md) for the full crate map and architecture diagram.
The short version:

```
sprout-relay      ← WebSocket server, REST API, event ingestion
sprout-core       ← Shared types, event verification, filter matching
sprout-db         ← Postgres access layer (sqlx)
sprout-auth       ← NIP-42 + OIDC JWT + API token scopes
sprout-pubsub     ← Redis fan-out
sprout-search     ← Typesense full-text search
sprout-audit      ← Tamper-evident hash-chain audit log
sprout-workflow   ← YAML-as-code workflow engine
sprout-mcp        ← stdio MCP server (agent API surface)
sprout-acp        ← ACP harness (bridges Sprout relay events to AI agents via stdio)
sprout-proxy      ← Nostr client compatibility layer
sprout-huddle     ← LiveKit integration
sprout-admin      ← Operator CLI
sprout-test-client← Integration test harness
desktop/          ← Desktop app (Tauri 2 + React 19 + Vite + Tailwind)
```

**Key design principle:** The relay is the single source of truth. All state
flows through the event store. Crates communicate through the database and
Redis pub/sub — not through direct function calls across crate boundaries
(with the exception of `sprout-core` types, which are shared everywhere).

**Event kinds** are the only switch. Every action in the system — a message,
a reaction, a workflow step, a canvas update — is a Nostr event with a kind
integer. Adding a new feature means defining a new kind. No breaking changes
to existing clients.

---

## How to Add a New Event Kind

1. **Define the kind constant** in `sprout-core/src/kind.rs`:

   ```rust
   /// My new event kind — description of what it represents.
   pub const KIND_MY_FEATURE: u32 = 4XXXX;
   ```

   Pick a kind number in the appropriate sub-range defined in `kind.rs`.
   Check the `ALL_KINDS` array for collisions. Each sub-range is documented
   with comments in the file.

2. **Define the payload type** in the appropriate module in `sprout-core/src/`
   (e.g., alongside `event.rs`) if the content field is structured JSON:

   ```rust
   #[derive(Debug, Serialize, Deserialize)]
   pub struct MyFeaturePayload {
       pub field_one: String,
       pub field_two: Option<u64>,
   }
   ```

3. **Handle the kind in the relay** by adding a match arm in
   `crates/sprout-relay/src/handlers/side_effects.rs` inside the
   `handle_side_effects()` function:

   ```rust
   KIND_MY_FEATURE => handle_my_feature(&state, &event).await?,
   ```

   This is the central dispatch point for event side-effects. If the new
   kind also needs a REST surface (e.g., a query endpoint for clients), add
   a handler in `crates/sprout-relay/src/api/` and register it in
   `crates/sprout-relay/src/router.rs` — that's separate from event
   dispatch.

4. **Persist to the database** — if the event needs to be queryable, add a
   handler in `sprout-db/src/` (e.g., `sprout-db/src/my_feature.rs`) with
   the appropriate `INSERT` and `SELECT` queries.

5. **Index for search** (if applicable) — add the kind to the Typesense
   indexing logic in `sprout-search/src/index.rs`.

6. **Audit** — the audit log captures all events automatically; no changes
   needed unless you need custom audit metadata.

7. **Write tests** — add a unit test for payload serialization in
   `sprout-core` and an integration test in `sprout-test-client` that sends
   the new event kind and verifies the expected behavior.

8. **Document** — `kind.rs` is the authoritative registry of all kind numbers.
   Update `README.md` if it's a user-facing feature.

---

## How to Add a New MCP Tool

MCP tools live in `crates/sprout-mcp/src/server.rs`. The `rmcp` crate
provides the `#[tool]` and `#[tool_router]` macros.

1. **Define a parameter struct:**

   ```rust
   #[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
   pub struct MyToolParams {
       /// UUID of the target channel.
       pub channel_id: String,
       /// Optional limit on results.
       #[serde(default)]
       pub limit: Option<u32>,
   }
   ```

   Use doc comments (`///`) on fields — they become the tool's parameter
   descriptions in the MCP schema.

2. **Implement the handler method** on `SproutMcpServer`:

   ```rust
   #[tool(
       name = "my_tool",
       description = "One-sentence description of what this tool does"
   )]
   pub async fn my_tool(&self, Parameters(p): Parameters<MyToolParams>) -> String {
       // Validate inputs at the boundary
       if uuid::Uuid::parse_str(&p.channel_id).is_err() {
           return format!("Error: channel_id '{}' is not a valid UUID", p.channel_id);
       }
       // Call the relay via self.client
       match self.client.get(&format!("/api/channels/{}/my-resource", p.channel_id)).await {
           Ok(body) => body,
           Err(e) => format!("Error: {e}"),
       }
   }
   ```

3. **The `#[tool_router]` macro** on the `impl SproutMcpServer` block
   automatically discovers all `#[tool]`-annotated methods and registers
   them. The MCP server auto-discovers `#[tool]`-annotated methods — no
   manual registration or doc updates needed.

4. **Write a test** — add an integration test in
   `crates/sprout-test-client/tests/e2e_mcp.rs` that exercises the new tool end-to-end.

---

## How to Add a New API Endpoint

REST endpoints live in `crates/sprout-relay/src/api/` — each resource has
its own submodule (e.g., `channels.rs`, `messages.rs`, `tokens.rs`). Routes
are registered in `crates/sprout-relay/src/router.rs`.

1. **Define the handler function:**

   ```rust
   pub async fn get_my_resource(
       State(state): State<AppState>,
       AuthenticatedUser(user): AuthenticatedUser,
       Path(channel_id): Path<Uuid>,
   ) -> Result<Json<MyResourceResponse>, ApiError> {
       // Check channel membership
       state.db.assert_channel_member(channel_id, user.pubkey).await?;
       // Fetch data
       let data = state.db.get_my_resource(channel_id).await?;
       Ok(Json(data))
   }
   ```

2. **Register the route** in `crates/sprout-relay/src/router.rs`:

   ```rust
   .route("/api/channels/{channel_id}/my-resource", get(get_my_resource))
   ```

3. **Add the database query** in `sprout-db/src/` — follow the existing
   patterns in `channel.rs`, `event.rs`, etc.

4. **Handle errors** — use the `ApiError` type in `sprout-relay/src/error.rs`.
   Map database errors and not-found cases to appropriate HTTP status codes.

5. **Write tests** — add an integration test using the `sprout-test-client`
   harness in `crates/sprout-test-client/tests/e2e_rest_api.rs`.

6. **Document** — if the endpoint is part of the public API surface, add it
   to the API reference section of `README.md` or a dedicated `API.md`.

---

## License and CLA

Sprout is licensed under the **Apache License, Version 2.0**. See
[LICENSE](LICENSE) for the full text.

By submitting a pull request, you agree that your contribution is licensed
under the Apache 2.0 license and that you have the right to submit it.

If your employer has rights to intellectual property you create, you may need
their sign-off. When in doubt, check with your legal team.

---

*Thank you for contributing to Sprout. Every bug report, documentation fix,
and code contribution makes the project better for everyone. 🌱*
