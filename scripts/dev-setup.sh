#!/usr/bin/env bash
# =============================================================================
# dev-setup.sh — One-shot local dev environment setup
# =============================================================================
# Usage: ./scripts/dev-setup.sh
#
# Starts all Docker services, waits for healthy, runs migrations, prints
# connection info and next steps.
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
TIMEOUT=120  # seconds to wait for services to become healthy

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log()    { echo -e "${BLUE}[dev-setup]${NC} $*"; }
success(){ echo -e "${GREEN}[dev-setup]${NC} ✅ $*"; }
warn()   { echo -e "${YELLOW}[dev-setup]${NC} ⚠️  $*"; }
error()  { echo -e "${RED}[dev-setup]${NC} ❌ $*" >&2; }

# ---- Preflight checks -------------------------------------------------------

if ! command -v docker &>/dev/null; then
  error "Docker not found. Install Docker Desktop: https://www.docker.com/products/docker-desktop/"
  exit 1
fi

if ! docker info &>/dev/null; then
  error "Docker daemon is not running. Start Docker Desktop and try again."
  exit 1
fi

cd "${REPO_ROOT}"

# ---- Start services ---------------------------------------------------------

log "Starting services..."
docker compose up -d

# ---- Wait for healthy -------------------------------------------------------

wait_healthy() {
  local service="$1"
  local container="$2"
  local elapsed=0
  local interval=3

  log "Waiting for ${service} to be healthy..."
  while true; do
    local status
    status=$(docker inspect --format='{{.State.Health.Status}}' "${container}" 2>/dev/null || echo "not_found")

    case "${status}" in
      healthy)
        success "${service} is healthy"
        return 0
        ;;
      unhealthy)
        error "${service} is unhealthy. Check logs: docker logs ${container}"
        return 1
        ;;
      not_found)
        error "Container ${container} not found"
        return 1
        ;;
    esac

    if [[ ${elapsed} -ge ${TIMEOUT} ]]; then
      error "Timed out waiting for ${service} (${TIMEOUT}s). Check: docker logs ${container}"
      return 1
    fi

    sleep "${interval}"
    elapsed=$((elapsed + interval))
    echo -n "."
  done
}

echo ""
wait_healthy "MySQL"      "sprout-mysql"
wait_healthy "Redis"      "sprout-redis"
wait_healthy "Typesense"  "sprout-typesense"
echo ""

# ---- Run migrations ---------------------------------------------------------

log "Running database migrations..."

MIGRATION_DIR="${REPO_ROOT}/migrations"

if [[ ! -d "${MIGRATION_DIR}" ]]; then
  warn "No migrations directory found at ${MIGRATION_DIR}. Skipping."
else
  # Check if sqlx CLI is available (preferred)
  if command -v sqlx &>/dev/null; then
    log "Using sqlx CLI for migrations..."
    # MySQL may need a moment after healthcheck before accepting connections
    attempts=0
    max_attempts=10
    until DATABASE_URL="mysql://sprout:sprout_dev@localhost:3306/sprout" \
      sqlx migrate run --source "${MIGRATION_DIR}" 2>/dev/null; do
      attempts=$((attempts + 1))
      if [[ ${attempts} -ge ${max_attempts} ]]; then
        error "Failed to run migrations after ${max_attempts} attempts"
        exit 1
      fi
      log "MySQL not ready for connections yet, retrying in 2s... (${attempts}/${max_attempts})"
      sleep 2
    done
    success "Migrations applied via sqlx"
  else
    error "sqlx CLI not found. Install it with: cargo install sqlx-cli --no-default-features --features mysql"
    error "Running migrations directly via mysql bypasses migration tracking and causes errors."
    exit 1
  fi
fi

# ---- Print connection info --------------------------------------------------

echo ""
echo -e "${GREEN}═══════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}  Sprout dev environment is ready! 🌱${NC}"
echo -e "${GREEN}═══════════════════════════════════════════════════════${NC}"
echo ""
echo -e "  ${BLUE}MySQL${NC}       mysql://sprout:sprout_dev@localhost:3306/sprout"
echo -e "  ${BLUE}Redis${NC}       redis://localhost:6379"
echo -e "  ${BLUE}Typesense${NC}   http://localhost:8108  (key: sprout_dev_key)"
echo -e "  ${BLUE}Adminer${NC}     http://localhost:8082  (DB browser)"
echo -e "  ${BLUE}Keycloak${NC}    http://localhost:8180  (admin / admin — local OAuth testing)"
echo ""
echo -e "  ${YELLOW}Next steps:${NC}"
echo -e "    cp .env.example .env                    # configure your environment"
echo -e "    bash scripts/setup-keycloak.sh          # configure Keycloak for OAuth testing (optional)"
echo -e "    cargo run -p sprout-relay               # start the relay server"
echo -e "    ./scripts/run-tests.sh                  # run all tests"
echo ""
echo -e "  ${YELLOW}Useful commands:${NC}"
echo -e "    docker compose ps             # check service status"
echo -e "    docker compose logs -f        # tail all logs"
echo -e "    docker compose down           # stop services (keep data)"
echo -e "    ./scripts/dev-reset.sh        # wipe and start fresh"
echo ""

exit 0
