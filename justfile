# Sprout — development task runner

set dotenv-load := true

desktop_dir := "desktop"
desktop_tauri_manifest := "desktop/src-tauri/Cargo.toml"

# List all available tasks
default:
    @just --list

# ─── Dev Environment ─────────────────────────────────────────────────────────

# Start all dev services (Docker Compose) and run migrations
setup:
    ./scripts/dev-setup.sh

# ⚠️  Wipe ALL data and recreate a clean environment
[confirm("This will DELETE all local data. Continue? (y/N)")]
reset:
    ./scripts/dev-reset.sh --yes

# Stop all dev services (keep data)
down:
    docker compose down

# Show dev service status
ps:
    docker compose ps

# Tail all service logs
logs *ARGS:
    docker compose logs -f {{ARGS}}

# ─── Build & Check ───────────────────────────────────────────────────────────

# Build the Rust workspace
build:
    cargo build --workspace

# Build the Rust workspace in release mode
build-release:
    cargo build --workspace --release

# Run repo lint and formatting checks
check: fmt-check clippy desktop-check

# Format all Rust code
fmt:
    cargo fmt --all

# Check formatting without modifying files
fmt-check:
    cargo fmt --all -- --check

# Run clippy with warnings as errors
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Install desktop JS dependencies
desktop-install:
    cd {{desktop_dir}} && pnpm install

# Install desktop JS dependencies reproducibly for CI
desktop-install-ci:
    cd {{desktop_dir}} && pnpm install --frozen-lockfile

# Run desktop lint and format checks
desktop-check:
    cd {{desktop_dir}} && pnpm check

# Run desktop TypeScript checks
desktop-typecheck:
    cd {{desktop_dir}} && pnpm typecheck

# Build desktop frontend assets
desktop-build:
    cd {{desktop_dir}} && pnpm build

# Check the desktop Tauri Rust crate compiles
desktop-tauri-check:
    cargo check --manifest-path {{desktop_tauri_manifest}}

# Run desktop checks suitable for CI / pre-push
desktop-ci: desktop-check desktop-build desktop-tauri-check

# Seed deterministic channel data for desktop Playwright tests
desktop-e2e-seed:
    ./scripts/setup-desktop-test-data.sh

# Run desktop browser smoke tests
desktop-e2e-smoke:
    cd {{desktop_dir}} && pnpm test:e2e:smoke

# Run desktop relay-backed e2e tests
desktop-e2e-integration:
    cd {{desktop_dir}} && pnpm test:e2e:integration

# Run all checks suitable for CI / pre-push (no infra needed)
ci: check test-unit desktop-build desktop-tauri-check

# ─── Test ─────────────────────────────────────────────────────────────────────

# Run all tests (unit + integration)
test:
    ./scripts/run-tests.sh all

# Run unit tests only (no infra needed)
test-unit:
    ./scripts/run-tests.sh unit

# Run integration tests only (starts services if needed)
test-integration:
    ./scripts/run-tests.sh integration

# ─── Run ──────────────────────────────────────────────────────────────────────

# Start the relay server
relay:
    cargo run -p sprout-relay

# Start the relay server in release mode
relay-release:
    cargo run -p sprout-relay --release

# Start sprout-proxy (dev mode)
proxy:
    cargo run -p sprout-proxy

# Start sprout-proxy (release mode)
proxy-release:
    cargo run -p sprout-proxy --release

# Run the desktop Tauri app in dev mode
dev *ARGS:
    cd {{desktop_dir}} && pnpm tauri dev {{ARGS}}

# Run the desktop frontend dev server
desktop-dev:
    cd {{desktop_dir}} && pnpm dev

# Run the desktop Tauri app
desktop-app *ARGS:
    cd {{desktop_dir}} && pnpm tauri dev {{ARGS}}

# ─── Database ─────────────────────────────────────────────────────────────────

# Apply schema migrations via pgschema
migrate:
    ./bin/pgschema apply --file schema/schema.sql --auto-approve

# ─── Utilities ────────────────────────────────────────────────────────────────

# Remove build artifacts
clean:
    cargo clean

# Check the Rust workspace compiles without producing binaries
check-compile:
    cargo check --workspace --all-targets

# ─── Agent Harness ────────────────────────────────────────────────────────────

# Run a goose agent connected to a Sprout relay (foreground)
goose relay="ws://localhost:3000" agents="1" heartbeat="0" prompt="" key="$SPROUT_PRIVATE_KEY" token="$SPROUT_ACP_API_TOKEN":
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release -p sprout-acp -p sprout-mcp
    env_args=(
        SPROUT_RELAY_URL="{{relay}}"
        SPROUT_PRIVATE_KEY="{{key}}"
        SPROUT_ACP_AGENT_COMMAND=goose
        SPROUT_ACP_AGENT_ARGS=acp
        SPROUT_ACP_MCP_COMMAND=./target/release/sprout-mcp-server
        SPROUT_ACP_AGENTS="{{agents}}"
        GOOSE_MODE=auto
    )
    [[ -n "{{token}}"  ]] && env_args+=(SPROUT_ACP_API_TOKEN="{{token}}")
    [[ -n "{{prompt}}" ]] && env_args+=(SPROUT_ACP_SYSTEM_PROMPT="{{prompt}}")
    if [[ "{{heartbeat}}" != "0" ]]; then
        env_args+=(SPROUT_ACP_HEARTBEAT_INTERVAL={{heartbeat}})
    fi
    exec env "${env_args[@]}" ./target/release/sprout-acp

# Run a goose agent in the background (screen session named 'goose-agent-N')
goose-bg relay="ws://localhost:3000" agents="1" heartbeat="0" prompt="" key="$SPROUT_PRIVATE_KEY" token="$SPROUT_ACP_API_TOKEN":
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release -p sprout-acp -p sprout-mcp
    env_args=(
        SPROUT_RELAY_URL="{{relay}}"
        SPROUT_PRIVATE_KEY="{{key}}"
        SPROUT_ACP_AGENT_COMMAND=goose
        SPROUT_ACP_AGENT_ARGS=acp
        SPROUT_ACP_MCP_COMMAND=./target/release/sprout-mcp-server
        SPROUT_ACP_AGENTS="{{agents}}"
        GOOSE_MODE=auto
    )
    [[ -n "{{token}}"  ]] && env_args+=(SPROUT_ACP_API_TOKEN="{{token}}")
    [[ -n "{{prompt}}" ]] && env_args+=(SPROUT_ACP_SYSTEM_PROMPT="{{prompt}}")
    if [[ "{{heartbeat}}" != "0" ]]; then
        env_args+=(SPROUT_ACP_HEARTBEAT_INTERVAL={{heartbeat}})
    fi
    screen -dmS goose-agent-{{agents}} bash -c "$(printf '%q ' env "${env_args[@]}") ./target/release/sprout-acp"
    echo "Agent running in screen session 'goose-agent-{{agents}}'. Attach with: screen -r goose-agent-{{agents}}"
