# Sprout — development task runner

set dotenv-load := true

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

# Build the entire workspace
build:
    cargo build --workspace

# Build in release mode
build-release:
    cargo build --workspace --release

# Run all lints and formatting checks
check: fmt-check clippy

# Format all Rust code
fmt:
    cargo fmt --all

# Check formatting without modifying files
fmt-check:
    cargo fmt --all -- --check

# Run clippy with warnings as errors
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Run all checks suitable for CI / pre-push (no infra needed)
ci: fmt-check clippy test-unit

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
            docker exec -i sprout-mysql mysql -u sprout -psprout_dev sprout < "$sql_file" 2>/dev/null || true
        done
        echo "Migrations applied."
    fi

# ─── Utilities ────────────────────────────────────────────────────────────────

# Remove build artifacts
clean:
    cargo clean

# Check the workspace compiles without producing binaries
check-compile:
    cargo check --workspace --all-targets
