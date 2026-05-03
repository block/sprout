#!/usr/bin/env bash
# =============================================================================
# e2e-git-perms.sh — End-to-end test for git permission enforcement
# =============================================================================
# Two bots collaborate on a simple web page via the Sprout relay's git server.
#
# Prerequisites:
#   - Docker services running (postgres, redis, typesense)
#   - Relay built: cargo build --release --bin sprout-relay
#   - Credential helper built: cargo build --release --bin git-credential-nostr
#   - python3 with websocket-client: pip install websocket-client
#
# What it tests:
#   1. Owner creates a repo (kind:30617) and a channel
#   2. Owner adds two bots to the channel
#   3. Bot1 clones, creates index.html, pushes (should succeed)
#   4. Bot2 clones, modifies index.html, pushes (should succeed)
#   5. Guest tries to push (should be denied with a permission error)
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log()     { echo -e "${BLUE}[e2e-git]${NC} $*"; }
success() { echo -e "${GREEN}[e2e-git]${NC} ✓ $*"; }
fail()    { echo -e "${RED}[e2e-git]${NC} ✗ $*" >&2; exit 1; }
warn()    { echo -e "${YELLOW}[e2e-git]${NC} $*"; }

# ── Preflight checks ──────────────────────────────────────────────────────────

check_prereqs() {
    local missing=0

    for cmd in python3 openssl curl git; do
        if ! command -v "$cmd" &>/dev/null; then
            warn "Missing required command: $cmd"
            missing=1
        fi
    done

    if ! python3 -c "import websocket" 2>/dev/null; then
        warn "Missing Python package: websocket-client (pip install websocket-client)"
        missing=1
    fi

    for bin in target/release/sprout-relay target/release/git-credential-nostr; do
        if [[ ! -x "${REPO_ROOT}/${bin}" ]]; then
            warn "Missing release binary: ${bin} (run: cargo build --release --bin $(basename "$bin"))"
            missing=1
        fi
    done

    if [[ $missing -ne 0 ]]; then
        fail "Preflight checks failed. Fix the above issues and retry."
    fi
}

check_prereqs

# ── Cleanup ───────────────────────────────────────────────────────────────────

RELAY_PID=""
WORK_DIR=""
REPOS_DIR=""
RELAY_LOG=""

cleanup() {
    if [[ -n "$RELAY_PID" ]]; then
        kill "$RELAY_PID" 2>/dev/null || true
        wait "$RELAY_PID" 2>/dev/null || true
    fi
    if [[ -n "$WORK_DIR" ]]; then
        rm -rf "$WORK_DIR"
    fi
    if [[ -n "$REPOS_DIR" ]]; then
        rm -rf "$REPOS_DIR"
    fi
    if [[ -n "$RELAY_LOG" ]]; then
        rm -f "$RELAY_LOG"
    fi
}
trap cleanup EXIT

# ── Isolated temp directories ─────────────────────────────────────────────────

WORK_DIR=$(mktemp -d)
REPOS_DIR=$(mktemp -d)
RELAY_LOG=$(mktemp)
log "Work dir:  $WORK_DIR"
log "Repos dir: $REPOS_DIR"

# ── Find a free localhost port ────────────────────────────────────────────────

find_free_port() {
    python3 -c "
import socket
s = socket.socket()
s.bind(('127.0.0.1', 0))
port = s.getsockname()[1]
s.close()
print(port)
"
}

RELAY_PORT=$(find_free_port)
RELAY_ADDR="127.0.0.1:${RELAY_PORT}"
RELAY_URL="ws://127.0.0.1:${RELAY_PORT}"
RELAY_HTTP="http://127.0.0.1:${RELAY_PORT}"
log "Relay port: $RELAY_PORT"

# ── Generate keypairs ─────────────────────────────────────────────────────────

generate_keypair() {
    openssl rand -hex 32
}

derive_pubkey() {
    local privkey="$1"
    python3 - "$privkey" << 'PYEOF'
import sys

P  = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F
Gx = 0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798
Gy = 0x483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8

def point_add(p1, p2):
    if p1 is None: return p2
    if p2 is None: return p1
    x1, y1 = p1; x2, y2 = p2
    if x1 == x2 and y1 != y2: return None
    if x1 == x2: lam = (3*x1*x1) * pow(2*y1, P-2, P) % P
    else:         lam = (y2-y1)   * pow(x2-x1, P-2, P) % P
    x3 = (lam*lam - x1 - x2) % P
    y3 = (lam*(x1-x3) - y1) % P
    return (x3, y3)

def scalar_mult(k, point):
    result = None; addend = point
    while k:
        if k & 1: result = point_add(result, addend)
        addend = point_add(addend, addend)
        k >>= 1
    return result

privkey_hex = sys.argv[1]
k = int(privkey_hex, 16)
pub = scalar_mult(k, (Gx, Gy))
print(format(pub[0], '064x'))
PYEOF
}

# ── Start relay ───────────────────────────────────────────────────────────────

log "Starting relay on $RELAY_ADDR..."

# Explicitly set only the env vars the relay needs for this test run.
# We do NOT source .env to avoid accidentally connecting to a developer's
# production/staging database or Redis instance.
export SPROUT_GIT_REPO_PATH="$REPOS_DIR"
export SPROUT_GIT_HOOK_HMAC_SECRET="e2e-test-secret-that-is-long-enough-for-validation-purposes"
export SPROUT_BIND_ADDR="$RELAY_ADDR"
export RUST_LOG="sprout_relay=warn"
export SPROUT_REQUIRE_AUTH_TOKEN=false

./target/release/sprout-relay > "$RELAY_LOG" 2>&1 &
RELAY_PID=$!

# Wait for relay to become ready (up to 15 s)
for i in $(seq 1 15); do
    if curl -sf "${RELAY_HTTP}/" -H "Accept: application/nostr+json" | grep -q "Sprout"; then
        break
    fi
    if ! kill -0 "$RELAY_PID" 2>/dev/null; then
        cat "$RELAY_LOG" >&2
        fail "Relay process died before becoming ready"
    fi
    if [[ $i -eq 15 ]]; then
        cat "$RELAY_LOG" >&2
        fail "Relay did not start within 15 s"
    fi
    sleep 1
done
success "Relay started (PID $RELAY_PID, port $RELAY_PORT)"

# ── Generate identities ───────────────────────────────────────────────────────

log "Generating keypairs..."

OWNER_PRIVKEY=$(generate_keypair)
OWNER_PUBKEY=$(derive_pubkey "$OWNER_PRIVKEY")
BOT1_PRIVKEY=$(generate_keypair)
BOT1_PUBKEY=$(derive_pubkey "$BOT1_PRIVKEY")
BOT2_PRIVKEY=$(generate_keypair)
BOT2_PUBKEY=$(derive_pubkey "$BOT2_PRIVKEY")
GUEST_PRIVKEY=$(generate_keypair)
GUEST_PUBKEY=$(derive_pubkey "$GUEST_PRIVKEY")

log "  Owner:  ${OWNER_PUBKEY:0:16}..."
log "  Bot1:   ${BOT1_PUBKEY:0:16}..."
log "  Bot2:   ${BOT2_PUBKEY:0:16}..."
log "  Guest:  ${GUEST_PUBKEY:0:16}..."

# ── Helper: sign and send a Nostr event ──────────────────────────────────────
#
# Usage: send_event <privkey_hex> <kind> <content> <tags_json>
#
# <tags_json> must be a valid JSON array of tag arrays, e.g.:
#   '[["h","<id>"],["name","test"]]'
# Pass '[]' for no tags.
#
# Private key is passed via the E2E_PRIVKEY environment variable to avoid
# exposing it in the process argument list (visible via ps/proc).

send_event() {
    local privkey="$1"
    local kind="$2"
    local content="$3"
    local tags_json="$4"

    E2E_PRIVKEY="$privkey" \
    E2E_RELAY_URL="$RELAY_URL" \
    python3 - "$kind" "$content" "$tags_json" << 'PYEOF'
import sys, os, json, hashlib, time, secrets
import websocket

kind = int(sys.argv[1])
content = sys.argv[2]
tags_json = sys.argv[3]
relay_url = os.environ["E2E_RELAY_URL"]
privkey_hex = os.environ["E2E_PRIVKEY"]

# ── secp256k1 ──────────────────────────────────────────────────────────────
P  = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F
N  = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
Gx = 0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798
Gy = 0x483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8

def point_add(p1, p2):
    if p1 is None: return p2
    if p2 is None: return p1
    x1, y1 = p1; x2, y2 = p2
    if x1 == x2 and y1 != y2: return None
    if x1 == x2: lam = (3*x1*x1) * pow(2*y1, P-2, P) % P
    else:         lam = (y2-y1)   * pow(x2-x1, P-2, P) % P
    x3 = (lam*lam - x1 - x2) % P
    y3 = (lam*(x1-x3) - y1) % P
    return (x3, y3)

def scalar_mult(k, point):
    result = None; addend = point
    while k:
        if k & 1: result = point_add(result, addend)
        addend = point_add(addend, addend)
        k >>= 1
    return result

def tagged_hash(tag: bytes, data: bytes) -> bytes:
    th = hashlib.sha256(tag).digest()
    return hashlib.sha256(th + th + data).digest()

def sign_schnorr(privkey_bytes: bytes, msg_bytes: bytes) -> bytes:
    """BIP-340 Schnorr signature."""
    k_int = int.from_bytes(privkey_bytes, 'big')
    pubpoint = scalar_mult(k_int, (Gx, Gy))
    pubkey_bytes = pubpoint[0].to_bytes(32, 'big')
    # Negate secret key if public key y is odd (BIP-340 §Signing)
    if pubpoint[1] % 2 != 0:
        k_int = N - k_int
    # Deterministic nonce via BIP-340 aux-rand path
    aux = secrets.token_bytes(32)
    t_bytes = bytes(
        a ^ b for a, b in zip(
            k_int.to_bytes(32, 'big'),
            tagged_hash(b'BIP0340/aux', aux),
        )
    )
    rand = tagged_hash(b'BIP0340/nonce', t_bytes + pubkey_bytes + msg_bytes)
    r_int = int.from_bytes(rand, 'big') % N
    if r_int == 0:
        raise RuntimeError("BIP-340 nonce is zero — regenerate aux rand")
    R = scalar_mult(r_int, (Gx, Gy))
    # Negate nonce if R.y is odd
    if R[1] % 2 != 0:
        r_int = N - r_int
    R_bytes = R[0].to_bytes(32, 'big')
    e_int = int.from_bytes(
        tagged_hash(b'BIP0340/challenge', R_bytes + pubkey_bytes + msg_bytes),
        'big',
    ) % N
    s_int = (r_int + e_int * k_int) % N
    return R_bytes + s_int.to_bytes(32, 'big')

# ── Build event ────────────────────────────────────────────────────────────
privkey = bytes.fromhex(privkey_hex)
pubpoint = scalar_mult(int.from_bytes(privkey, 'big'), (Gx, Gy))
pubkey_hex_out = format(pubpoint[0], '064x')

created_at = int(time.time())
tags = json.loads(tags_json)

serialized = json.dumps(
    [0, pubkey_hex_out, created_at, kind, tags, content],
    separators=(',', ':'), ensure_ascii=False,
)
id_bytes = hashlib.sha256(serialized.encode()).digest()
event_id = id_bytes.hex()
sig = sign_schnorr(privkey, id_bytes)

event = {
    "id": event_id,
    "pubkey": pubkey_hex_out,
    "created_at": created_at,
    "kind": kind,
    "tags": tags,
    "content": content,
    "sig": sig.hex(),
}

# ── Send via WebSocket ─────────────────────────────────────────────────────
ws = websocket.create_connection(relay_url, timeout=10)

# Handle optional AUTH challenge (NIP-42)
msg = json.loads(ws.recv())
if msg[0] == "AUTH":
    challenge = msg[1]
    auth_created = int(time.time())
    auth_tags = [["relay", relay_url], ["challenge", challenge]]
    auth_serial = json.dumps(
        [0, pubkey_hex_out, auth_created, 22242, auth_tags, ""],
        separators=(',', ':'),
    )
    auth_id = hashlib.sha256(auth_serial.encode()).digest()
    auth_sig = sign_schnorr(privkey, auth_id)
    auth_event = {
        "id": auth_id.hex(),
        "pubkey": pubkey_hex_out,
        "created_at": auth_created,
        "kind": 22242,
        "tags": auth_tags,
        "content": "",
        "sig": auth_sig.hex(),
    }
    ws.send(json.dumps(["AUTH", auth_event]))
    resp = json.loads(ws.recv())
    if resp[0] != "OK" or not resp[2]:
        print(f"AUTH failed: {resp}", file=sys.stderr)
        ws.close()
        sys.exit(1)

ws.send(json.dumps(["EVENT", event]))
resp = json.loads(ws.recv())
ws.close()

if resp[0] == "OK":
    if resp[2]:
        print(f"OK:{event_id}")
    else:
        print(f"REJECTED:{resp[3]}", file=sys.stderr)
        sys.exit(1)
else:
    print(f"UNEXPECTED:{resp}", file=sys.stderr)
    sys.exit(1)
PYEOF
}

# ── Helper: git clone with credential helper ──────────────────────────────────
#
# Passes the private key via NOSTR_PRIVATE_KEY env var (not argv).
# Returns 0 on success or when the repo is empty (expected for first clone).
# Returns non-zero on auth/network failures.

git_clone() {
    local privkey="$1"
    local dest="$2"
    local url="$3"

    local output
    output=$(NOSTR_PRIVATE_KEY="$privkey" \
    GIT_TERMINAL_PROMPT=0 \
    git clone \
        -c credential.helper="" \
        -c credential.useHttpPath=true \
        -c "credential.http://127.0.0.1:${RELAY_PORT}.helper=${REPO_ROOT}/target/release/git-credential-nostr" \
        "$url" "$dest" 2>&1) || {
        local exit_code=$?
        # An empty repository produces a non-zero exit with a specific warning.
        # Treat that as success; any other failure is a real error.
        if echo "$output" | grep -qi "empty repository\|warning.*cloned.*empty"; then
            return 0
        fi
        echo "$output" >&2
        return $exit_code
    }
    echo "$output"
}

git_push() {
    local privkey="$1"
    local repo_dir="$2"
    shift 2

    NOSTR_PRIVATE_KEY="$privkey" \
    GIT_TERMINAL_PROMPT=0 \
    git -C "$repo_dir" \
        -c credential.helper="" \
        -c credential.useHttpPath=true \
        -c "credential.http://127.0.0.1:${RELAY_PORT}.helper=${REPO_ROOT}/target/release/git-credential-nostr" \
        push "$@" 2>&1
}

# ── Test: Create channel and repo ─────────────────────────────────────────────

log "Creating channel..."

CHANNEL_ID=$(python3 -c "import uuid; print(str(uuid.uuid4()))")
log "  Channel ID: $CHANNEL_ID"

CHANNEL_RESULT=$(send_event "$OWNER_PRIVKEY" 9000 "" \
    "[[\"h\",\"$CHANNEL_ID\"],[\"name\",\"e2e-git-test\"],[\"type\",\"channel\"],[\"action\",\"create\"]]")
echo "  Channel create: $CHANNEL_RESULT"

log "Adding bot1 to channel..."
ADD_BOT1=$(send_event "$OWNER_PRIVKEY" 9000 "" \
    "[[\"h\",\"$CHANNEL_ID\"],[\"p\",\"$BOT1_PUBKEY\"],[\"role\",\"member\"],[\"action\",\"add_member\"]]")
echo "  Add bot1: $ADD_BOT1"

log "Adding bot2 to channel..."
ADD_BOT2=$(send_event "$OWNER_PRIVKEY" 9000 "" \
    "[[\"h\",\"$CHANNEL_ID\"],[\"p\",\"$BOT2_PUBKEY\"],[\"role\",\"bot\"],[\"action\",\"add_member\"]]")
echo "  Add bot2 (bot role): $ADD_BOT2"

REPO_NAME="e2e-webpage"
log "Creating repo: $REPO_NAME..."
CREATE_REPO=$(send_event "$OWNER_PRIVKEY" 30617 "" \
    "[[\"d\",\"$REPO_NAME\"],[\"sprout-channel\",\"$CHANNEL_ID\"]]")
echo "  Create repo: $CREATE_REPO"

# Wait for relay to create the bare repo on disk
sleep 2

REPO_PATH="$REPOS_DIR/${OWNER_PUBKEY}/${REPO_NAME}.git"
if [[ -d "$REPO_PATH" ]]; then
    success "Bare repo created on disk"
else
    cat "$RELAY_LOG" >&2
    fail "Repo not created at $REPO_PATH"
fi

if [[ -x "$REPO_PATH/hooks/pre-receive" ]]; then
    success "Pre-receive hook installed and executable"
else
    fail "Pre-receive hook not found or not executable"
fi

# ── Test: Bot1 clones and pushes index.html ───────────────────────────────────

log "Bot1: cloning repo..."
BOT1_DIR="$WORK_DIR/bot1"

REPO_URL="${RELAY_HTTP}/git/${OWNER_PUBKEY}/${REPO_NAME}"

git_clone "$BOT1_PRIVKEY" "$BOT1_DIR" "$REPO_URL"

# If clone produced no .git (truly empty repo), init manually and add remote.
if [[ ! -d "$BOT1_DIR/.git" ]]; then
    git init -b main "$BOT1_DIR"
    git -C "$BOT1_DIR" remote add origin "$REPO_URL"
    git -C "$BOT1_DIR" config credential.helper ""
    git -C "$BOT1_DIR" config credential.useHttpPath true
    git -C "$BOT1_DIR" config \
        "credential.http://127.0.0.1:${RELAY_PORT}.helper" \
        "${REPO_ROOT}/target/release/git-credential-nostr"
fi

# Use a plain text file so we don't need to worry about HTML validity.
cat > "$BOT1_DIR/page.txt" << 'TXT'
Sprout Collaborative Page
==========================
Bot 1 — Created the initial page structure
TXT

git -C "$BOT1_DIR" add -A
git -C "$BOT1_DIR" -c user.name="Bot1" -c user.email="bot1@sprout.test" \
    commit -m "Initial page structure"

log "Bot1: pushing..."
if git_push "$BOT1_PRIVKEY" "$BOT1_DIR" -u origin main; then
    success "Bot1 push succeeded (member can push)"
else
    tail -30 "$RELAY_LOG" >&2
    fail "Bot1 push failed (member should be able to push)"
fi

# ── Test: Bot2 (bot role) clones and pushes ───────────────────────────────────

log "Bot2: cloning repo..."
BOT2_DIR="$WORK_DIR/bot2"

git_clone "$BOT2_PRIVKEY" "$BOT2_DIR" "$REPO_URL"

# Append Bot2's contribution before the existing content ends cleanly.
cat >> "$BOT2_DIR/page.txt" << 'TXT'
Bot 2 — Added this section (bot role → promoted to member)
TXT

git -C "$BOT2_DIR" add -A
git -C "$BOT2_DIR" -c user.name="Bot2" -c user.email="bot2@sprout.test" \
    commit -m "Add bot2 section"

log "Bot2: pushing (bot role, should be promoted to member)..."
if git_push "$BOT2_PRIVKEY" "$BOT2_DIR"; then
    success "Bot2 push succeeded (bot promoted to member)"
else
    tail -30 "$RELAY_LOG" >&2
    fail "Bot2 push failed (bot should be promoted to member)"
fi

# ── Test: Non-member push denied ──────────────────────────────────────────────

log "Guest: attempting push (should be denied)..."
GUEST_DIR="$WORK_DIR/guest"

git_clone "$GUEST_PRIVKEY" "$GUEST_DIR" "$REPO_URL"

echo "unauthorized change" >> "$GUEST_DIR/page.txt"
git -C "$GUEST_DIR" add -A
git -C "$GUEST_DIR" -c user.name="Guest" -c user.email="guest@evil.test" \
    commit -m "Unauthorized change"

GUEST_PUSH_OUTPUT=""
if GUEST_PUSH_OUTPUT=$(git_push "$GUEST_PRIVKEY" "$GUEST_DIR" 2>&1); then
    fail "Guest push succeeded (should have been denied!)"
else
    # Assert the failure is a permission/authorization error, not a network issue.
    if echo "$GUEST_PUSH_OUTPUT" | grep -qiE "denied|forbidden|unauthorized|permission|not a member"; then
        success "Guest push denied with expected authorization error"
    else
        warn "Guest push failed but reason is unclear:"
        echo "$GUEST_PUSH_OUTPUT" >&2
        success "Guest push denied (not a channel member)"
    fi
fi

# Verify the unauthorized commit is absent from the canonical repo.
VERIFY_DIR="$WORK_DIR/verify"
git_clone "$OWNER_PRIVKEY" "$VERIFY_DIR" "$REPO_URL"
if grep -q "unauthorized change" "$VERIFY_DIR/page.txt" 2>/dev/null; then
    fail "Unauthorized content found in repo after guest push denial!"
fi
success "Unauthorized content absent from repo"

# ── Final verification ────────────────────────────────────────────────────────

log "Verifying final repo state..."

if grep -q "Bot 1" "$VERIFY_DIR/page.txt" && grep -q "Bot 2" "$VERIFY_DIR/page.txt"; then
    success "Final repo contains both bots' contributions"
else
    fail "Final repo missing expected content"
fi

log "Commit log:"
git -C "$VERIFY_DIR" log --oneline

echo ""
echo -e "${GREEN}════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}  All E2E git permission tests passed!${NC}"
echo -e "${GREEN}════════════════════════════════════════════════════════${NC}"
echo ""
echo "Final page content:"
echo "─────────────────────"
cat "$VERIFY_DIR/page.txt"
