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

# Run database migrations (uses sqlx CLI if available, falls back to docker exec)
migrate:
    #!/usr/bin/env bash
    set -euo pipefail
    if command -v sqlx &>/dev/null; then
        echo "Running migrations via sqlx..."
        sqlx migrate run --source migrations
    else
        echo "sqlx CLI not found — applying migrations via docker exec..."
        for sql_file in migrations/*.sql; do
            echo "  Applying $(basename "$sql_file")..."
            docker run --rm -i --network "${SPROUT_DOCKER_NETWORK:-sprout-net}" -e MYSQL_PWD="${SPROUT_DB_PASS:-sprout_dev}" "${SPROUT_DB_CLIENT_IMAGE:-mysql:8.0}" mysql -h"${SPROUT_DOCKER_DB_HOST:-mysql}" -u"${SPROUT_DB_USER:-sprout}" "${SPROUT_DB_NAME:-sprout}" < "$sql_file" 2>/dev/null || true
        done
        echo "Migrations applied."
    fi

# ─── Goose (AI agent) ─────────────────────────────────────────────────────────

# Set up agent identity and start a Goose session with Sprout tools
goose:
    #!/usr/bin/env bash
    set -euo pipefail
    ./scripts/setup-goose-agent.sh
    set -o allexport
    source .sprout-agent.env
    set +o allexport
    exec goose session \
      --with-extension "SPROUT_RELAY_URL=$SPROUT_RELAY_URL SPROUT_PRIVATE_KEY=$SPROUT_PRIVATE_KEY SPROUT_API_TOKEN=$SPROUT_API_TOKEN $(pwd)/target/debug/sprout-mcp-server"

# ─── Utilities ────────────────────────────────────────────────────────────────

# Remove build artifacts
clean:
    cargo clean

# Check the Rust workspace compiles without producing binaries
check-compile:
    cargo check --workspace --all-targets
