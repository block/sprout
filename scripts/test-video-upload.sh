#!/usr/bin/env bash
# test-video-upload.sh — Live validation of the Blossom video upload flow.
#
# Prerequisites:
#   - Relay running at $RELAY_URL (default: http://localhost:3000)
#   - Dev mode (SPROUT_REQUIRE_AUTH_TOKEN=false) or valid API token
#   - ffmpeg, nak, curl, jq, shasum on PATH
#
# Usage:
#   ./scripts/test-video-upload.sh              # run all tests
#   RELAY_URL=http://host:3000 ./scripts/...    # custom relay URL
#   NSEC=nsec1... ./scripts/...                 # use existing key

set -euo pipefail

RELAY_URL="${RELAY_URL:-http://localhost:3000}"
TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

PASS=0
FAIL=0

pass() { echo "  ✅ $1"; PASS=$((PASS + 1)); }
fail() { echo "  ❌ $1"; FAIL=$((FAIL + 1)); }

# ── Dependencies ───────────────────────────────────────────────────────────────

for cmd in ffmpeg nak curl jq shasum; do
    command -v "$cmd" >/dev/null 2>&1 || { echo "Missing: $cmd"; exit 1; }
done

# ── Key generation ─────────────────────────────────────────────────────────────

if [ -z "${NSEC:-}" ]; then
    NSEC="$(nak key generate)"
    echo "Generated key: $(echo "$NSEC" | nak key public)"
fi
NPUB="$(echo "$NSEC" | nak key public)"
echo "Using pubkey: $NPUB"
echo "Relay: $RELAY_URL"
echo ""

# ── Generate test MP4 ─────────────────────────────────────────────────────────
# Minimal 1-second H.264 video with moov at front (faststart).

TEST_MP4="$TMPDIR/test.mp4"
ffmpeg -y -f lavfi -i "color=c=blue:s=320x240:d=1" \
    -c:v libx264 -profile:v baseline -pix_fmt yuv420p \
    -movflags +faststart \
    "$TEST_MP4" 2>/dev/null

FILE_SIZE=$(wc -c < "$TEST_MP4" | tr -d ' ')
SHA256=$(shasum -a 256 "$TEST_MP4" | cut -d' ' -f1)
echo "Test MP4: ${FILE_SIZE} bytes, sha256=${SHA256:0:16}..."
echo ""

# ── Helper: build Blossom auth header ──────────────────────────────────────────
# Creates a kind:24242 event with t=upload, x=<sha256>, expiration=+5min.

blossom_auth() {
    local sha256="$1"
    local now exp auth_event auth_b64

    now=$(date +%s)
    exp=$((now + 300))

    auth_event=$(nak event \
        --sec "$NSEC" \
        -k 24242 \
        -c "Upload test video" \
        -t t=upload \
        -t "x=$sha256" \
        -t "expiration=$exp" \
        2>/dev/null)

    auth_b64=$(echo -n "$auth_event" | base64 | tr -d '\n')
    echo "Nostr $auth_b64"
}

# ── Test 1: Upload MP4 ────────────────────────────────────────────────────────

echo "Test 1: Upload MP4 via PUT /media/upload"
AUTH="$(blossom_auth "$SHA256")"

UPLOAD_RESP=$(curl -s -w "\n%{http_code}" \
    -X PUT "$RELAY_URL/media/upload" \
    -H "Authorization: $AUTH" \
    -H "Content-Type: video/mp4" \
    -H "X-SHA-256: $SHA256" \
    --data-binary "@$TEST_MP4")

UPLOAD_HTTP=$(echo "$UPLOAD_RESP" | tail -1)
UPLOAD_BODY=$(echo "$UPLOAD_RESP" | sed '$d')

if [ "$UPLOAD_HTTP" = "200" ]; then
    pass "Upload returned 200"
    BLOB_URL=$(echo "$UPLOAD_BODY" | jq -r '.url // empty')
    if [ -n "$BLOB_URL" ]; then
        pass "Response contains url: ${BLOB_URL:0:60}..."
    else
        fail "Response missing url field"
    fi
    # Check duration field present
    DURATION=$(echo "$UPLOAD_BODY" | jq -r '.duration // empty')
    if [ -n "$DURATION" ]; then
        pass "Response contains duration: ${DURATION}s"
    else
        fail "Response missing duration field"
    fi
else
    fail "Upload returned $UPLOAD_HTTP (expected 200)"
    echo "    Body: $UPLOAD_BODY"
fi
echo ""

# ── Test 2: GET full blob ─────────────────────────────────────────────────────

echo "Test 2: GET /media/${SHA256}.mp4 (full download)"
GET_RESP=$(curl -s -o "$TMPDIR/downloaded.mp4" -w "%{http_code}" \
    "$RELAY_URL/media/${SHA256}.mp4")

if [ "$GET_RESP" = "200" ]; then
    pass "GET returned 200"
    DL_SIZE=$(wc -c < "$TMPDIR/downloaded.mp4" | tr -d ' ')
    if [ "$DL_SIZE" = "$FILE_SIZE" ]; then
        pass "Downloaded size matches ($DL_SIZE bytes)"
    else
        fail "Size mismatch: expected $FILE_SIZE, got $DL_SIZE"
    fi
else
    fail "GET returned $GET_RESP (expected 200)"
fi
echo ""

# ── Test 3: HEAD with Accept-Ranges ──────────────────────────────────────────

echo "Test 3: HEAD /media/${SHA256}.mp4 (Accept-Ranges)"
HEAD_RESP=$(curl -s -I "$RELAY_URL/media/${SHA256}.mp4")
HEAD_HTTP=$(echo "$HEAD_RESP" | head -1 | grep -o '[0-9]\{3\}')
ACCEPT_RANGES=$(echo "$HEAD_RESP" | grep -i "accept-ranges" | tr -d '\r')

if [ "$HEAD_HTTP" = "200" ]; then
    pass "HEAD returned 200"
else
    fail "HEAD returned $HEAD_HTTP (expected 200)"
fi

if echo "$ACCEPT_RANGES" | grep -qi "bytes"; then
    pass "Accept-Ranges: bytes present"
else
    fail "Accept-Ranges header missing or wrong: '$ACCEPT_RANGES'"
fi
echo ""

# ── Test 4: Range GET (206 Partial Content) ──────────────────────────────────

echo "Test 4: Range GET bytes=0-499 (206 Partial Content)"
RANGE_RESP=$(curl -s -o "$TMPDIR/range.bin" -w "%{http_code}" \
    -H "Range: bytes=0-499" \
    "$RELAY_URL/media/${SHA256}.mp4")

if [ "$RANGE_RESP" = "206" ]; then
    pass "Range GET returned 206"
    RANGE_SIZE=$(wc -c < "$TMPDIR/range.bin" | tr -d ' ')
    if [ "$RANGE_SIZE" = "500" ]; then
        pass "Received exactly 500 bytes"
    else
        fail "Expected 500 bytes, got $RANGE_SIZE"
    fi
else
    fail "Range GET returned $RANGE_RESP (expected 206)"
fi
echo ""

# ── Test 5: Range GET past EOF (416) ─────────────────────────────────────────

echo "Test 5: Range GET bytes=999999999- (416 Range Not Satisfiable)"
RANGE416_RESP=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Range: bytes=999999999-" \
    "$RELAY_URL/media/${SHA256}.mp4")

if [ "$RANGE416_RESP" = "416" ]; then
    pass "Past-EOF range returned 416"
else
    fail "Past-EOF range returned $RANGE416_RESP (expected 416)"
fi
echo ""

# ── Test 6: Content-Type spoofing rejection ──────────────────────────────────
# Send video/mp4 Content-Type but with a PNG body — should be rejected.

echo "Test 6: Content-Type spoofing (video/mp4 header, PNG body)"
PNG_FILE="$TMPDIR/fake.png"
# Minimal valid PNG (1x1 red pixel)
printf '\x89PNG\r\n\x1a\n' > "$PNG_FILE"
dd if=/dev/zero bs=100 count=1 >> "$PNG_FILE" 2>/dev/null

PNG_SHA=$(shasum -a 256 "$PNG_FILE" | cut -d' ' -f1)
SPOOF_AUTH="$(blossom_auth "$PNG_SHA")"

SPOOF_RESP=$(curl -s -w "\n%{http_code}" \
    -X PUT "$RELAY_URL/media/upload" \
    -H "Authorization: $SPOOF_AUTH" \
    -H "Content-Type: video/mp4" \
    -H "X-SHA-256: $PNG_SHA" \
    --data-binary "@$PNG_FILE")

SPOOF_HTTP=$(echo "$SPOOF_RESP" | tail -1)

if [ "$SPOOF_HTTP" = "415" ] || [ "$SPOOF_HTTP" = "400" ]; then
    pass "Spoofed upload rejected with $SPOOF_HTTP"
else
    fail "Spoofed upload returned $SPOOF_HTTP (expected 400 or 415)"
fi
echo ""

# ── Test 7: Idempotent re-upload ─────────────────────────────────────────────

echo "Test 7: Idempotent re-upload (same file, same hash)"
REUP_AUTH="$(blossom_auth "$SHA256")"
REUP_RESP=$(curl -s -w "\n%{http_code}" \
    -X PUT "$RELAY_URL/media/upload" \
    -H "Authorization: $REUP_AUTH" \
    -H "Content-Type: video/mp4" \
    -H "X-SHA-256: $SHA256" \
    --data-binary "@$TEST_MP4")

REUP_HTTP=$(echo "$REUP_RESP" | tail -1)
if [ "$REUP_HTTP" = "200" ]; then
    pass "Re-upload returned 200 (idempotent)"
else
    fail "Re-upload returned $REUP_HTTP (expected 200)"
fi
echo ""

# ── Summary ───────────────────────────────────────────────────────────────────

echo "════════════════════════════════════════"
echo "  Results: $PASS passed, $FAIL failed"
echo "════════════════════════════════════════"

[ "$FAIL" -eq 0 ] && exit 0 || exit 1
