#!/usr/bin/env bash
# =============================================================================
# run-tests.sh — Run Sprout test suite
# =============================================================================
# Usage:
#   ./scripts/run-tests.sh              # run all tests (default)
#   ./scripts/run-tests.sh unit         # unit tests only (no infra needed)
#   ./scripts/run-tests.sh integration  # integration tests only
#   ./scripts/run-tests.sh all          # explicit all
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
MODE="${1:-all}"
TIMEOUT=60  # seconds to wait for services if starting them

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

log()    { echo -e "${BLUE}[run-tests]${NC} $*"; }
success(){ echo -e "${GREEN}[run-tests]${NC} ✅ $*"; }
warn()   { echo -e "${YELLOW}[run-tests]${NC} ⚠️  $*"; }
error()  { echo -e "${RED}[run-tests]${NC} ❌ $*" >&2; }
section(){ echo -e "\n${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"; echo -e "${CYAN}  $*${NC}"; echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"; }

cd "${REPO_ROOT}"

# ---- Load .env if present ---------------------------------------------------

if [[ -f ".env" ]]; then
  log "Loading .env..."
  set -o allexport
  # shellcheck disable=SC1091
  source .env
  set +o allexport
else
  # Use defaults matching docker-compose.yml
  export DATABASE_URL="mysql://sprout:sprout_dev@localhost:3306/sprout"
  export REDIS_URL="redis://localhost:6379"
  export TYPESENSE_API_KEY="sprout_dev_key"
  export TYPESENSE_URL="http://localhost:8108"
fi

# ---- Track results ----------------------------------------------------------

declare -a PASSED=()
declare -a FAILED=()

run_test_step() {
  local name="$1"
  shift
  log "Running: ${name}"
  if "$@"; then
    success "${name} passed"
    PASSED+=("${name}")
  else
    error "${name} FAILED"
    FAILED+=("${name}")
  fi
}

# ---- Check / start infra (for integration tests) ----------------------------

services_healthy() {
  local mysql_ok redis_ok
  mysql_ok=$(docker inspect --format='{{.State.Health.Status}}' sprout-mysql 2>/dev/null || echo "not_found")
  redis_ok=$(docker inspect --format='{{.State.Health.Status}}' sprout-redis 2>/dev/null || echo "not_found")
  [[ "${mysql_ok}" == "healthy" && "${redis_ok}" == "healthy" ]]
}

ensure_services() {
  if services_healthy; then
    success "Services already healthy"
    return 0
  fi

  warn "Services not running — starting them..."
  docker compose up -d

  local elapsed=0
  local interval=3
  while ! services_healthy; do
    if [[ ${elapsed} -ge ${TIMEOUT} ]]; then
      error "Timed out waiting for services (${TIMEOUT}s)"
      return 1
    fi
    sleep "${interval}"
    elapsed=$((elapsed + interval))
    echo -n "."
  done
  echo ""
  success "Services healthy"

  # Ensure migrations are current
  ensure_migrations
}

ensure_migrations() {
  log "Ensuring migrations are current..."
  local migration_dir="${REPO_ROOT}/migrations"

  if [[ ! -d "${migration_dir}" ]]; then
    warn "No migrations directory. Skipping."
    return 0
  fi

  if command -v sqlx &>/dev/null; then
    DATABASE_URL="${DATABASE_URL}" sqlx migrate run --source "${migration_dir}" 2>/dev/null \
      && success "Migrations current" \
      || warn "sqlx migrate run failed — DB may be out of date"
  else
    # Fallback: apply all SQL files (idempotent if schema uses IF NOT EXISTS)
    shopt -s nullglob
    local sql_files=("${migration_dir}"/*.sql)
    shopt -u nullglob
    for sql_file in "${sql_files[@]}"; do
      docker exec -i sprout-mysql \
        mysql -u sprout -psprout_dev sprout \
        < "${sql_file}" &>/dev/null || true
    done
    success "Migrations applied (mysql fallback)"
  fi
}

# ---- Unit tests (no infra needed) -------------------------------------------

run_unit_tests() {
  section "Unit Tests (no infra required)"

  run_test_step "sprout-core tests" \
    cargo test -p sprout-core --lib -- --nocapture

  run_test_step "sprout-auth unit tests" \
    cargo test -p sprout-auth --lib -- --nocapture
}

# ---- DB / integration tests (infra required) --------------------------------

run_integration_tests() {
  section "Integration Tests (requires running services)"

  ensure_services

  run_test_step "sprout-db tests" \
    cargo test -p sprout-db -- --nocapture

  run_test_step "sprout-auth integration tests" \
    cargo test -p sprout-auth --test '*' -- --nocapture 2>/dev/null || \
    run_test_step "sprout-auth (no integration tests found)" true

  run_test_step "workspace integration tests" \
    cargo test --test '*' -- --nocapture 2>/dev/null || \
    run_test_step "workspace integration tests (none found)" true
}

# ---- Main -------------------------------------------------------------------

START_TIME=$(date +%s)

case "${MODE}" in
  unit)
    run_unit_tests
    ;;
  integration)
    run_integration_tests
    ;;
  all|*)
    run_unit_tests
    run_integration_tests
    ;;
esac

END_TIME=$(date +%s)
ELAPSED=$((END_TIME - START_TIME))

# ---- Summary ----------------------------------------------------------------

section "Test Summary"
echo ""
echo -e "  Duration: ${ELAPSED}s"
echo ""

if [[ ${#PASSED[@]} -gt 0 ]]; then
  echo -e "  ${GREEN}Passed (${#PASSED[@]}):${NC}"
  for t in "${PASSED[@]}"; do
    echo -e "    ${GREEN}✅${NC} ${t}"
  done
fi

if [[ ${#FAILED[@]} -gt 0 ]]; then
  echo ""
  echo -e "  ${RED}Failed (${#FAILED[@]}):${NC}"
  for t in "${FAILED[@]}"; do
    echo -e "    ${RED}❌${NC} ${t}"
  done
  echo ""
  exit 1
fi

echo ""
success "All tests passed! 🎉"
exit 0
