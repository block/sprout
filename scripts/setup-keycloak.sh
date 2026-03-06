#!/usr/bin/env bash
# =============================================================================
# setup-keycloak.sh — Configure Keycloak for local OAuth testing
# =============================================================================
# Usage: ./scripts/setup-keycloak.sh
#
# Creates the `sprout` realm with:
#   - sprout-desktop client (public, direct access grants)
#   - Test users: tyler, alice, bob, charlie (password: password123)
#   - nostr_pubkey custom attribute on each user
#   - Protocol mapper: nostr_pubkey → JWT access token claim
#
# Keycloak is a LOCAL DEV STAND-IN for Okta/generic OIDC providers.
# It is NOT a production dependency.
#
# Prerequisites:
#   - Keycloak running at http://localhost:8180 (docker compose up -d)
#   - curl and jq installed
# =============================================================================
set -euo pipefail

KEYCLOAK_URL="${KEYCLOAK_URL:-http://localhost:8180}"
ADMIN_USER="${KEYCLOAK_ADMIN:-admin}"
ADMIN_PASS="${KEYCLOAK_ADMIN_PASSWORD:-admin}"
REALM="sprout"
CLIENT_ID="sprout-desktop"
TIMEOUT=120  # seconds to wait for Keycloak

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log()    { echo -e "${BLUE}[keycloak-setup]${NC} $*"; }
success(){ echo -e "${GREEN}[keycloak-setup]${NC} ✅ $*"; }
warn()   { echo -e "${YELLOW}[keycloak-setup]${NC} ⚠️  $*"; }
error()  { echo -e "${RED}[keycloak-setup]${NC} ❌ $*" >&2; }

# ---- Preflight --------------------------------------------------------------

for cmd in curl jq; do
  if ! command -v "$cmd" &>/dev/null; then
    error "Required tool not found: $cmd"
    exit 1
  fi
done

# ---- Wait for Keycloak ------------------------------------------------------

log "Waiting for Keycloak at ${KEYCLOAK_URL}..."
elapsed=0
interval=5
until curl -sf "${KEYCLOAK_URL}/health/ready" -o /dev/null 2>/dev/null; do
  if [[ ${elapsed} -ge ${TIMEOUT} ]]; then
    error "Timed out waiting for Keycloak (${TIMEOUT}s). Is it running?"
    error "  docker compose up -d keycloak"
    exit 1
  fi
  echo -n "."
  sleep "${interval}"
  elapsed=$((elapsed + interval))
done
echo ""
success "Keycloak is ready"

# ---- Get admin token --------------------------------------------------------

log "Authenticating as admin..."
ADMIN_TOKEN=$(curl -sf \
  -X POST "${KEYCLOAK_URL}/realms/master/protocol/openid-connect/token" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "client_id=admin-cli" \
  -d "username=${ADMIN_USER}" \
  -d "password=${ADMIN_PASS}" \
  -d "grant_type=password" \
  | jq -r '.access_token')

if [[ -z "${ADMIN_TOKEN}" || "${ADMIN_TOKEN}" == "null" ]]; then
  error "Failed to get admin token. Check KEYCLOAK_ADMIN / KEYCLOAK_ADMIN_PASSWORD."
  exit 1
fi
success "Admin token obtained"

# Helper: authenticated API call
kc() {
  local method="$1"; shift
  local path="$1"; shift
  curl -sf \
    -X "${method}" \
    "${KEYCLOAK_URL}/admin/realms${path}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    "$@"
}

kc_root() {
  local method="$1"; shift
  local path="$1"; shift
  curl -sf \
    -X "${method}" \
    "${KEYCLOAK_URL}/admin${path}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    "$@"
}

# ---- Create realm -----------------------------------------------------------

log "Checking for realm '${REALM}'..."
REALM_EXISTS=$(curl -sf \
  "${KEYCLOAK_URL}/admin/realms/${REALM}" \
  -H "Authorization: Bearer ${ADMIN_TOKEN}" \
  -o /dev/null -w "%{http_code}" 2>/dev/null || true)

if [[ "${REALM_EXISTS}" == "200" ]]; then
  warn "Realm '${REALM}' already exists — skipping creation"
else
  log "Creating realm '${REALM}'..."
  curl -sf \
    -X POST "${KEYCLOAK_URL}/admin/realms" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -d '{
      "realm": "'"${REALM}"'",
      "displayName": "Sprout",
      "enabled": true,
      "registrationAllowed": false,
      "loginWithEmailAllowed": true,
      "duplicateEmailsAllowed": false,
      "resetPasswordAllowed": false,
      "editUsernameAllowed": false,
      "bruteForceProtected": false
    }'
  success "Realm '${REALM}' created"
fi

# ---- Create client ----------------------------------------------------------

log "Checking for client '${CLIENT_ID}'..."
EXISTING_CLIENT=$(kc GET "/${REALM}/clients?clientId=${CLIENT_ID}" | jq -r '.[0].id // empty')

if [[ -n "${EXISTING_CLIENT}" ]]; then
  warn "Client '${CLIENT_ID}' already exists (id: ${EXISTING_CLIENT}) — skipping creation"
  CLIENT_UUID="${EXISTING_CLIENT}"
else
  log "Creating client '${CLIENT_ID}'..."
  kc POST "/${REALM}/clients" -d '{
    "clientId": "'"${CLIENT_ID}"'",
    "name": "Sprout Desktop",
    "enabled": true,
    "publicClient": true,
    "directAccessGrantsEnabled": true,
    "standardFlowEnabled": true,
    "implicitFlowEnabled": false,
    "serviceAccountsEnabled": false,
    "redirectUris": [
      "http://localhost:*",
      "sprout://*"
    ],
    "webOrigins": ["*"],
    "protocol": "openid-connect"
  }'

  CLIENT_UUID=$(kc GET "/${REALM}/clients?clientId=${CLIENT_ID}" | jq -r '.[0].id')
  success "Client '${CLIENT_ID}' created (id: ${CLIENT_UUID})"
fi

# ---- Add nostr_pubkey protocol mapper ---------------------------------------

log "Checking for nostr_pubkey protocol mapper..."
MAPPER_EXISTS=$(kc GET "/${REALM}/clients/${CLIENT_UUID}/protocol-mappers/models" \
  | jq -r '.[] | select(.name == "nostr_pubkey") | .id // empty')

if [[ -n "${MAPPER_EXISTS}" ]]; then
  warn "Protocol mapper 'nostr_pubkey' already exists — skipping"
else
  log "Creating nostr_pubkey → JWT claim mapper..."
  kc POST "/${REALM}/clients/${CLIENT_UUID}/protocol-mappers/models" -d '{
    "name": "nostr_pubkey",
    "protocol": "openid-connect",
    "protocolMapper": "oidc-usermodel-attribute-mapper",
    "consentRequired": false,
    "config": {
      "userinfo.token.claim": "true",
      "user.attribute": "nostr_pubkey",
      "id.token.claim": "true",
      "access.token.claim": "true",
      "claim.name": "nostr_pubkey",
      "jsonType.label": "String"
    }
  }'
  success "Protocol mapper 'nostr_pubkey' created"
fi

# ---- Create users -----------------------------------------------------------

# Format: "username:nostr_pubkey"
declare -a USERS=(
  "tyler:e5ebc6cdb579be112e336cc319b5989b4bb6af11786ea90dbe52b5f08d741b34"
  "alice:953d3363262e86b770419834c53d2446409db6d918a57f8f339d495d54ab001f"
  "bob:bb22a5299220cad76ffd46190ccbeede8ab5dc260faa28b6e5a2cb31b9aff260"
  "charlie:554cef57437abac34522ac2c9f0490d685b72c80478cf9f7ed6f9570ee8624ea"
)

for entry in "${USERS[@]}"; do
  username="${entry%%:*}"
  pubkey="${entry##*:}"

  log "Checking for user '${username}'..."
  EXISTING_USER=$(kc GET "/${REALM}/users?username=${username}&exact=true" | jq -r '.[0].id // empty')

  if [[ -n "${EXISTING_USER}" ]]; then
    warn "User '${username}' already exists (id: ${EXISTING_USER}) — updating nostr_pubkey attribute"
    kc PUT "/${REALM}/users/${EXISTING_USER}" -d '{
      "attributes": {
        "nostr_pubkey": ["'"${pubkey}"'"]
      }
    }'
    success "User '${username}' updated"
  else
    log "Creating user '${username}'..."
    kc POST "/${REALM}/users" -d '{
      "username": "'"${username}"'",
      "email": "'"${username}"'@sprout.local",
      "firstName": "'"${username^}"'",
      "lastName": "Test",
      "enabled": true,
      "emailVerified": true,
      "credentials": [{
        "type": "password",
        "value": "password123",
        "temporary": false
      }],
      "attributes": {
        "nostr_pubkey": ["'"${pubkey}"'"]
      }
    }'
    success "User '${username}' created (nostr_pubkey: ${pubkey:0:16}...)"
  fi
done

# ---- Summary ----------------------------------------------------------------

echo ""
echo -e "${GREEN}═══════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}  Keycloak realm setup complete! 🔑${NC}"
echo -e "${GREEN}═══════════════════════════════════════════════════════${NC}"
echo ""
echo -e "  ${BLUE}Admin UI${NC}       http://localhost:8180  (admin / admin)"
echo -e "  ${BLUE}Realm${NC}          ${REALM}"
echo -e "  ${BLUE}Client${NC}         ${CLIENT_ID}  (public, direct access grants)"
echo ""
echo -e "  ${BLUE}Test users${NC}     (password: password123)"
echo -e "    tyler    e5ebc6cdb579be112e336cc319b5989b4bb6af11786ea90dbe52b5f08d741b34"
echo -e "    alice    953d3363262e86b770419834c53d2446409db6d918a57f8f339d495d54ab001f"
echo -e "    bob      bb22a5299220cad76ffd46190ccbeede8ab5dc260faa28b6e5a2cb31b9aff260"
echo -e "    charlie  554cef57437abac34522ac2c9f0490d685b72c80478cf9f7ed6f9570ee8624ea"
echo ""
echo -e "  ${YELLOW}Relay env vars for Keycloak:${NC}"
echo -e "    OKTA_JWKS_URI=http://localhost:8180/realms/sprout/protocol/openid-connect/certs"
echo -e "    OKTA_ISSUER=http://localhost:8180/realms/sprout"
echo -e "    OKTA_AUDIENCE=sprout-desktop"
echo -e "    OKTA_PUBKEY_CLAIM=nostr_pubkey"
echo ""
echo -e "  ${YELLOW}Get a token (direct grant):${NC}"
echo -e "    curl -s -X POST http://localhost:8180/realms/sprout/protocol/openid-connect/token \\"
echo -e "      -d 'client_id=sprout-desktop&grant_type=password&username=tyler&password=password123' \\"
echo -e "      | jq -r .access_token"
echo ""

exit 0
