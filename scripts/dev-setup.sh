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
success(){ echo -e "${GREEN}[dev-setup]${NC} $*"; }
warn()   { echo -e "${YELLOW}[dev-setup]${NC} $*"; }
error()  { echo -e "${RED}[dev-setup]${NC} $*" >&2; }

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

# ---- Load environment -------------------------------------------------------

load_env() {
  if [[ -f ".env" ]]; then
    log "Loading .env..."
    set -o allexport
    # shellcheck disable=SC1091
    source .env
    set +o allexport
  fi

  export DATABASE_URL="${DATABASE_URL:-postgres://sprout:sprout_dev@localhost:5432/sprout}"
  export PGHOST="${PGHOST:-localhost}"
  export PGPORT="${PGPORT:-5432}"
  export PGUSER="${PGUSER:-sprout}"
  export PGPASSWORD="${PGPASSWORD:-sprout_dev}"
  export PGDATABASE="${PGDATABASE:-sprout}"
  export REDIS_URL="${REDIS_URL:-redis://localhost:6379}"
  export TYPESENSE_API_KEY="${TYPESENSE_API_KEY:-sprout_dev_key}"
  export TYPESENSE_URL="${TYPESENSE_URL:-http://localhost:8108}"
}

postgres_accepting_connections() {
  docker exec sprout-postgres \
    pg_isready -h localhost -p 5432 -U "${PGUSER}" -d "${PGDATABASE}" \
    >/dev/null 2>&1
}

load_env

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
wait_healthy "Postgres"   "sprout-postgres"
wait_healthy "Redis"      "sprout-redis"
wait_healthy "Typesense"  "sprout-typesense"
echo ""

# ---- Run migrations ---------------------------------------------------------

log "Running database migrations..."

PGSCHEMA="${REPO_ROOT}/bin/pgschema"
SCHEMA_FILE="${REPO_ROOT}/schema/schema.sql"

if [[ ! -f "${SCHEMA_FILE}" ]]; then
  warn "No schema.sql found at ${SCHEMA_FILE}. Skipping."
else
  if [[ -x "${PGSCHEMA}" ]]; then
    log "Using pgschema for migrations..."
    attempts=0
    max_attempts=10
    pgschema_output="$(mktemp)"
    trap 'rm -f "${pgschema_output}"' EXIT
    until "${PGSCHEMA}" apply --file "${SCHEMA_FILE}" --auto-approve >"${pgschema_output}" 2>&1; do
      attempts=$((attempts + 1))
      if postgres_accepting_connections; then
        error "pgschema failed even though Postgres is accepting connections"
        cat "${pgschema_output}" >&2
        exit 1
      fi
      if [[ ${attempts} -ge ${max_attempts} ]]; then
        error "Failed to run migrations after ${max_attempts} attempts"
        cat "${pgschema_output}" >&2
        exit 1
      fi
      log "Postgres not ready for connections yet, retrying in 2s... (${attempts}/${max_attempts})"
      sleep 2
    done
    success "Migrations applied via pgschema"
  else
    error "pgschema not found at ${PGSCHEMA}. Run: ./bin/hermit install pgschema"
    exit 1
  fi
fi

# ---- Print connection info --------------------------------------------------

echo ""
echo -e "${GREEN}=======================================================${NC}"
echo -e "${GREEN}  Sprout dev environment is ready!${NC}"
echo -e "${GREEN}=======================================================${NC}"
echo ""
echo -e "  ${BLUE}Postgres${NC}    ${DATABASE_URL}"
echo -e "  ${BLUE}Redis${NC}       ${REDIS_URL}"
echo -e "  ${BLUE}Typesense${NC}   ${TYPESENSE_URL}  (key: ${TYPESENSE_API_KEY})"
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
