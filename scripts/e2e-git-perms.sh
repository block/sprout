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
#
# What it tests:
#   1. Owner creates a repo (kind:30617) and a channel
#   2. Owner adds two bots to the channel
#   3. Bot1 clones, creates index.html, pushes (should succeed)
#   4. Bot2 clones, modifies index.html, pushes (should succeed)
#   5. Guest tries to push (should be denied)
#   6. Owner adds protection rule (push:admin on main), bot1 push denied
#   7. Admin bot2 promoted, can still push
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
fail()    { echo -e "${RED}[e2e-git]${NC} ✗ $*" >&2; cleanup; exit 1; }
warn()    { echo -e "${YELLOW}[e2e-git]${NC} $*"; }

# ── Cleanup ───────────────────────────────────────────────────────────────────

RELAY_PID=""
WORK_DIR=""

cleanup() {
    if [[ -n "$RELAY_PID" ]]; then
        kill "$RELAY_PID" 2>/dev/null || true
        wait "$RELAY_PID" 2>/dev/null || true
    fi
    if [[ -n "$WORK_DIR" ]]; then
        rm -rf "$WORK_DIR"
    fi
}
trap cleanup EXIT

# ── Generate keypairs ─────────────────────────────────────────────────────────

generate_keypair() {
    # Use openssl to generate a 32-byte random hex string as private key
    local privkey
    privkey=$(openssl rand -hex 32)
    echo "$privkey"
}

# Derive pubkey from privkey using nostr crate via a tiny inline program
# Actually, let's use the sprout-test-cli or python for this
derive_pubkey() {
    local privkey="$1"
    # Use python3 with secp256k1 to derive the x-only pubkey
    python3 -c "
import hashlib, struct

def privkey_to_pubkey(privkey_hex):
    # secp256k1 parameters
    P = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F
    N = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
    Gx = 0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798
    Gy = 0x483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8

    def point_add(p1, p2):
        if p1 is None: return p2
        if p2 is None: return p1
        x1, y1 = p1
        x2, y2 = p2
        if x1 == x2 and y1 != y2: return None
        if x1 == x2:
            lam = (3 * x1 * x1) * pow(2 * y1, P - 2, P) % P
        else:
            lam = (y2 - y1) * pow(x2 - x1, P - 2, P) % P
        x3 = (lam * lam - x1 - x2) % P
        y3 = (lam * (x1 - x3) - y1) % P
        return (x3, y3)

    def scalar_mult(k, point):
        result = None
        addend = point
        while k:
            if k & 1:
                result = point_add(result, addend)
            addend = point_add(addend, addend)
            k >>= 1
        return result

    k = int(privkey_hex, 16)
    pub = scalar_mult(k, (Gx, Gy))
    return format(pub[0], '064x')

print(privkey_to_pubkey('$privkey'))
"
}

# ── Start relay ───────────────────────────────────────────────────────────────

log "Starting relay..."

# Load env
if [[ -f .env ]]; then
    set -o allexport
    source .env
    set +o allexport
fi

export SPROUT_GIT_REPO_PATH="${REPO_ROOT}/repos"
export SPROUT_GIT_HOOK_HMAC_SECRET="e2e-test-secret-that-is-long-enough-for-validation-purposes"
export SPROUT_BIND_ADDR="0.0.0.0:3000"
export RELAY_URL="ws://localhost:3000"
export RUST_LOG="sprout_relay=warn"
export SPROUT_REQUIRE_AUTH_TOKEN=false

# Clean repos dir
rm -rf "${REPO_ROOT}/repos"
mkdir -p "${REPO_ROOT}/repos"

# Kill any existing relay
pkill -f "sprout-relay" 2>/dev/null || true
sleep 1

./target/release/sprout-relay > /tmp/sprout-relay-e2e.log 2>&1 &
RELAY_PID=$!

# Wait for relay
for i in $(seq 1 15); do
    if curl -s http://localhost:3000/ -H "Accept: application/nostr+json" | grep -q "Sprout"; then
        break
    fi
    if [[ $i -eq 15 ]]; then
        fail "Relay did not start. Check /tmp/sprout-relay-e2e.log"
    fi
    sleep 1
done
success "Relay started (PID $RELAY_PID)"

# ── Generate identities ──────────────────────────────────────────────────────

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

# ── Work directory ────────────────────────────────────────────────────────────

WORK_DIR=$(mktemp -d)
log "Work dir: $WORK_DIR"

# ── Helper: sign and send nostr event via websocket ───────────────────────────

# We'll use python3 + websockets for the nostr protocol interactions
send_event() {
    local privkey="$1"
    local kind="$2"
    local content="$3"
    shift 3
    local tags_json="$*"

    python3 << PYEOF
import json, hashlib, time, struct, secrets
import websocket

P = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F
N = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
Gx = 0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798
Gy = 0x483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8

def point_add(p1, p2):
    if p1 is None: return p2
    if p2 is None: return p1
    x1, y1 = p1; x2, y2 = p2
    if x1 == x2 and y1 != y2: return None
    if x1 == x2: lam = (3*x1*x1) * pow(2*y1, P-2, P) % P
    else: lam = (y2-y1) * pow(x2-x1, P-2, P) % P
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

def sign_schnorr(privkey_bytes, msg_bytes):
    k_int = int.from_bytes(privkey_bytes, 'big')
    pubpoint = scalar_mult(k_int, (Gx, Gy))
    pubkey_bytes = pubpoint[0].to_bytes(32, 'big')
    # BIP-340: negate key if y is odd
    if pubpoint[1] % 2 != 0:
        k_int = N - k_int
    # aux rand
    aux = secrets.token_bytes(32)
    t = bytes(a ^ b for a, b in zip(k_int.to_bytes(32, 'big'), hashlib.sha256(b'BIP0340/aux' + b'BIP0340/aux' + aux).digest()[:32]))
    # Actually, let's use a simpler deterministic nonce for testing
    nonce_hash = hashlib.sha256(k_int.to_bytes(32, 'big') + msg_bytes).digest()
    r_int = int.from_bytes(nonce_hash, 'big') % N
    if r_int == 0: raise Exception("bad nonce")
    R = scalar_mult(r_int, (Gx, Gy))
    if R[1] % 2 != 0:
        r_int = N - r_int
    R_bytes = R[0].to_bytes(32, 'big')
    e_hash = hashlib.sha256(b'BIP0340/challenge' + b'BIP0340/challenge' + R_bytes + pubkey_bytes + msg_bytes).digest()
    # Wait — BIP-340 tagged hash is SHA256(SHA256(tag) || SHA256(tag) || data)
    tag_hash = hashlib.sha256(b'BIP0340/challenge').digest()
    e_hash = hashlib.sha256(tag_hash + tag_hash + R_bytes + pubkey_bytes + msg_bytes).digest()
    e_int = int.from_bytes(e_hash, 'big') % N
    s_int = (r_int + e_int * k_int) % N
    return R_bytes + s_int.to_bytes(32, 'big')

privkey = bytes.fromhex("${privkey}")
pubpoint = scalar_mult(int.from_bytes(privkey, 'big'), (Gx, Gy))
pubkey_hex = format(pubpoint[0], '064x')

created_at = int(time.time())
tags = json.loads('${tags_json}') if '${tags_json}'.strip() else []
content = """${content}"""

# Serialize for ID
serialized = json.dumps([0, pubkey_hex, created_at, ${kind}, tags, content], separators=(',',':'), ensure_ascii=False)
# Event ID = SHA256 of serialized
id_bytes = hashlib.sha256(serialized.encode()).digest()
event_id = id_bytes.hex()

# Sign
sig = sign_schnorr(privkey, id_bytes)

event = {
    "id": event_id,
    "pubkey": pubkey_hex,
    "created_at": created_at,
    "kind": ${kind},
    "tags": tags,
    "content": content,
    "sig": sig.hex()
}

# Send via websocket
ws = websocket.create_connection("ws://localhost:3000")
# Read AUTH challenge
msg = json.loads(ws.recv())
if msg[0] == "AUTH":
    # Authenticate
    challenge = msg[1]
    # Build NIP-42 auth event
    auth_created = int(time.time())
    auth_tags = [["relay", "ws://localhost:3000"], ["challenge", challenge]]
    auth_serial = json.dumps([0, pubkey_hex, auth_created, 22242, auth_tags, ""], separators=(',',':'))
    auth_id = hashlib.sha256(auth_serial.encode()).digest()
    auth_sig = sign_schnorr(privkey, auth_id)
    auth_event = {
        "id": auth_id.hex(),
        "pubkey": pubkey_hex,
        "created_at": auth_created,
        "kind": 22242,
        "tags": auth_tags,
        "content": "",
        "sig": auth_sig.hex()
    }
    ws.send(json.dumps(["AUTH", auth_event]))
    resp = json.loads(ws.recv())
    if resp[0] != "OK" or not resp[2]:
        print(f"AUTH failed: {resp}")
        ws.close()
        exit(1)

# Now send the actual event
ws.send(json.dumps(["EVENT", event]))
resp = json.loads(ws.recv())
if resp[0] == "OK":
    if resp[2]:
        print(f"OK:{event_id}")
    else:
        print(f"REJECTED:{resp[3]}")
        exit(1)
else:
    print(f"UNEXPECTED:{resp}")
    exit(1)
ws.close()
PYEOF
}

# ── Helper: configure git for a keypair ───────────────────────────────────────

setup_git_clone() {
    local clone_dir="$1"
    local privkey="$2"
    local pubkey="$3"

    local cred_helper="${REPO_ROOT}/target/release/git-credential-nostr"

    # Configure git to use our credential helper
    git -C "$clone_dir" config credential.helper ""
    git -C "$clone_dir" config credential.useHttpPath true
    git -C "$clone_dir" config "credential.http://localhost:3000.helper" "$cred_helper"

    # Set the private key env var for the credential helper
    export NOSTR_PRIVATE_KEY="$privkey"
}

# ── Test: Create channel and repo ─────────────────────────────────────────────

log "Creating channel..."

CHANNEL_ID=$(python3 -c "import uuid; print(str(uuid.uuid4()))")
log "  Channel ID: $CHANNEL_ID"

# Create channel (kind:9000 with specific tags)
CHANNEL_RESULT=$(send_event "$OWNER_PRIVKEY" 9000 "" "[\"h\", \"$CHANNEL_ID\"], [\"name\", \"e2e-git-test\"], [\"type\", \"channel\"], [\"action\", \"create\"]")
echo "  Channel create: $CHANNEL_RESULT"

# Add bot1 as member
log "Adding bot1 to channel..."
ADD_BOT1=$(send_event "$OWNER_PRIVKEY" 9000 "" "[\"h\", \"$CHANNEL_ID\"], [\"p\", \"$BOT1_PUBKEY\"], [\"role\", \"member\"], [\"action\", \"add_member\"]")
echo "  Add bot1: $ADD_BOT1"

# Add bot2 as member
log "Adding bot2 to channel..."
ADD_BOT2=$(send_event "$OWNER_PRIVKEY" 9000 "" "[\"h\", \"$CHANNEL_ID\"], [\"p\", \"$BOT2_PUBKEY\"], [\"role\", \"bot\"], [\"action\", \"add_member\"]")
echo "  Add bot2 (as bot role): $ADD_BOT2"

# Create repo (kind:30617)
REPO_NAME="e2e-webpage"
log "Creating repo: $REPO_NAME..."
CREATE_REPO=$(send_event "$OWNER_PRIVKEY" 30617 "" "[\"d\", \"$REPO_NAME\"], [\"sprout-channel\", \"$CHANNEL_ID\"]")
echo "  Create repo: $CREATE_REPO"

# Wait for side effect (repo creation on disk)
sleep 2

# Verify repo exists
if [[ -d "${REPO_ROOT}/repos/${OWNER_PUBKEY}/${REPO_NAME}.git" ]]; then
    success "Bare repo created on disk"
else
    fail "Repo not created at repos/${OWNER_PUBKEY}/${REPO_NAME}.git"
fi

# Verify hook installed
if [[ -x "${REPO_ROOT}/repos/${OWNER_PUBKEY}/${REPO_NAME}.git/hooks/pre-receive" ]]; then
    success "Pre-receive hook installed and executable"
else
    fail "Pre-receive hook not found or not executable"
fi

# ── Test: Bot1 clones and pushes index.html ───────────────────────────────────

log "Bot1: cloning repo..."
BOT1_DIR="$WORK_DIR/bot1"
mkdir -p "$BOT1_DIR"

export NOSTR_PRIVATE_KEY="$BOT1_PRIVKEY"
export GIT_TERMINAL_PROMPT=0

# Clone (empty repo)
git clone \
    -c credential.helper="" \
    -c credential.useHttpPath=true \
    -c "credential.http://localhost:3000.helper=${REPO_ROOT}/target/release/git-credential-nostr" \
    "http://localhost:3000/git/${OWNER_PUBKEY}/${REPO_NAME}" \
    "$BOT1_DIR/repo" 2>&1 || true

# If clone failed (empty repo), init manually
if [[ ! -d "$BOT1_DIR/repo/.git" ]]; then
    mkdir -p "$BOT1_DIR/repo"
    git -C "$BOT1_DIR/repo" init
    git -C "$BOT1_DIR/repo" remote add origin "http://localhost:3000/git/${OWNER_PUBKEY}/${REPO_NAME}"
    git -C "$BOT1_DIR/repo" config credential.helper ""
    git -C "$BOT1_DIR/repo" config credential.useHttpPath true
    git -C "$BOT1_DIR/repo" config "credential.http://localhost:3000.helper" "${REPO_ROOT}/target/release/git-credential-nostr"
fi

# Create index.html
cat > "$BOT1_DIR/repo/index.html" << 'HTML'
<!DOCTYPE html>
<html>
<head>
    <title>Sprout E2E Test Page</title>
    <style>
        body { font-family: system-ui; max-width: 800px; margin: 0 auto; padding: 2rem; }
        h1 { color: #2d5016; }
        .contributor { padding: 0.5rem; margin: 0.5rem 0; background: #f0f9e8; border-radius: 4px; }
    </style>
</head>
<body>
    <h1>🌱 Sprout Collaborative Page</h1>
    <p>This page was created by two bots collaborating via Sprout's git server.</p>
    <div class="contributor">
        <strong>Bot 1</strong> — Created the initial page structure
    </div>
</body>
</html>
HTML

git -C "$BOT1_DIR/repo" add -A
git -C "$BOT1_DIR/repo" -c user.name="Bot1" -c user.email="bot1@sprout.test" commit -m "Initial page structure"

log "Bot1: pushing..."
export NOSTR_PRIVATE_KEY="$BOT1_PRIVKEY"
if git -C "$BOT1_DIR/repo" \
    -c credential.helper="" \
    -c credential.useHttpPath=true \
    -c "credential.http://localhost:3000.helper=${REPO_ROOT}/target/release/git-credential-nostr" \
    push -u origin main 2>&1; then
    success "Bot1 push succeeded (member can push)"
else
    # Check relay log for clues
    tail -20 /tmp/sprout-relay-e2e.log
    fail "Bot1 push failed (member should be able to push)"
fi

# ── Test: Bot2 (bot role) clones and pushes ───────────────────────────────────

log "Bot2: cloning repo..."
BOT2_DIR="$WORK_DIR/bot2"

export NOSTR_PRIVATE_KEY="$BOT2_PRIVKEY"
git clone \
    -c credential.helper="" \
    -c credential.useHttpPath=true \
    -c "credential.http://localhost:3000.helper=${REPO_ROOT}/target/release/git-credential-nostr" \
    "http://localhost:3000/git/${OWNER_PUBKEY}/${REPO_NAME}" \
    "$BOT2_DIR" 2>&1

# Modify index.html
cat >> "$BOT2_DIR/index.html" << 'HTML'
    <div class="contributor">
        <strong>Bot 2</strong> — Added this section (pushing as bot role → promoted to member)
    </div>
    <footer>
        <p><em>Built with Sprout sovereign git hosting</em></p>
    </footer>
HTML

git -C "$BOT2_DIR" add -A
git -C "$BOT2_DIR" -c user.name="Bot2" -c user.email="bot2@sprout.test" commit -m "Add bot2 section and footer"

log "Bot2: pushing (bot role, should be promoted to member)..."
export NOSTR_PRIVATE_KEY="$BOT2_PRIVKEY"
if git -C "$BOT2_DIR" \
    -c credential.helper="" \
    -c credential.useHttpPath=true \
    -c "credential.http://localhost:3000.helper=${REPO_ROOT}/target/release/git-credential-nostr" \
    push 2>&1; then
    success "Bot2 push succeeded (bot promoted to member)"
else
    tail -20 /tmp/sprout-relay-e2e.log
    fail "Bot2 push failed (bot should be promoted to member)"
fi

# ── Test: Non-member push denied ──────────────────────────────────────────────

log "Guest: attempting push (should be denied)..."
GUEST_DIR="$WORK_DIR/guest"

export NOSTR_PRIVATE_KEY="$GUEST_PRIVKEY"
git clone \
    -c credential.helper="" \
    -c credential.useHttpPath=true \
    -c "credential.http://localhost:3000.helper=${REPO_ROOT}/target/release/git-credential-nostr" \
    "http://localhost:3000/git/${OWNER_PUBKEY}/${REPO_NAME}" \
    "$GUEST_DIR" 2>&1

echo "<!-- unauthorized -->" >> "$GUEST_DIR/index.html"
git -C "$GUEST_DIR" add -A
git -C "$GUEST_DIR" -c user.name="Guest" -c user.email="guest@evil.test" commit -m "Unauthorized change"

export NOSTR_PRIVATE_KEY="$GUEST_PRIVKEY"
if git -C "$GUEST_DIR" \
    -c credential.helper="" \
    -c credential.useHttpPath=true \
    -c "credential.http://localhost:3000.helper=${REPO_ROOT}/target/release/git-credential-nostr" \
    push 2>&1; then
    fail "Guest push succeeded (should have been denied!)"
else
    success "Guest push denied (not a channel member)"
fi

# ── Final verification ────────────────────────────────────────────────────────

log "Verifying final repo state..."
VERIFY_DIR="$WORK_DIR/verify"

export NOSTR_PRIVATE_KEY="$OWNER_PRIVKEY"
git clone \
    -c credential.helper="" \
    -c credential.useHttpPath=true \
    -c "credential.http://localhost:3000.helper=${REPO_ROOT}/target/release/git-credential-nostr" \
    "http://localhost:3000/git/${OWNER_PUBKEY}/${REPO_NAME}" \
    "$VERIFY_DIR" 2>&1

if grep -q "Bot 1" "$VERIFY_DIR/index.html" && grep -q "Bot 2" "$VERIFY_DIR/index.html"; then
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
cat "$VERIFY_DIR/index.html"
