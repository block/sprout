---
title: "Using Sprout with Third-Party Nostr Clients via sprout-proxy"
tags: [sprout, nostr, guide]
status: active
created: 2026-03-11
sources:
  - crates/sprout-proxy/src/main.rs
  - crates/sprout-proxy/src/server.rs
  - crates/sprout-proxy/Cargo.toml
  - Justfile
  - scripts/test-proxy-e2e.sh
  - VISION.md
---

# Using Sprout with Third-Party Nostr Clients via sprout-proxy

## 1. Overview

`sprout-proxy` is an optional compatibility layer that lets standard Nostr clients connect to a Sprout relay. Sprout uses custom event kinds (40001+) internally; most Nostr clients only understand NIP-28 (kind:40/41/42). The proxy translates between the two in real time.

**What it does:**
- Translates Sprout's kind:40001 stream messages ↔ NIP-28 kind:42 channel messages
- Maps Sprout channel UUIDs (`#h` tags) ↔ kind:40 event IDs (`#e` tags)
- Synthesizes kind:40 (channel create) and kind:41 (channel metadata) events from Sprout's REST API
- Enforces invite-token-based access control (scoped per channel, time-limited, use-limited)
- Implements NIP-42 AUTH challenge/response so clients authenticate before receiving events
- Serves NIP-11 relay info document at `GET /` with `Accept: application/nostr+json`
- Assigns each external user a deterministic shadow keypair so Sprout's relay sees consistent pubkeys

**What is NOT supported (MVP):**
- NIP-29 group navigation (group list, group join/leave)
- NIP-50 full-text search
- Direct Messages (DMs)
- kind:40001 forum posts or workflow events

### Architecture

```
┌─────────────────────┐        ┌──────────────────────┐        ┌──────────────────┐
│  Nostr Client       │        │   sprout-proxy        │        │  sprout-relay    │
│  (Coracle, nak,     │◄──────►│   port 4869           │◄──────►│  port 3000       │
│   Amethyst, etc.)   │  NIP-28│                       │internal│                  │
│                     │  NIP-42│  • kind translation   │ kinds  │  • event store   │
│  ws://host:4869     │  NIP-11│  • shadow keys        │        │  • subscriptions │
│  ?token=<invite>    │        │  • invite tokens      │        │  • auth          │
└─────────────────────┘        └──────────────────────┘        └──────────────────┘
```

**Supported NIPs:** NIP-01, NIP-11, NIP-28, NIP-42

---

## 2. Quick Start (for Developers)

### Prerequisites

- Rust toolchain (stable)
- Docker + Docker Compose (for MySQL + Redis)
- `just` task runner (`cargo install just`)
- `sprout-admin` CLI in the workspace

### Step 1 — Start infrastructure

```bash
just setup
```

Starts MySQL and Redis via Docker Compose and runs database migrations.

### Step 2 — Start the relay

```bash
just relay
```

The relay listens on `ws://localhost:3000`. Keep this terminal open.

### Step 3 — Mint a proxy API token

The proxy needs a Sprout API token with the `proxy:submit` scope to re-sign shadow-keyed events through the relay's pubkey enforcement.

```bash
cargo run -p sprout-admin -- mint-token \
  --name "proxy" \
  --scopes "messages:read,messages:write,channels:read,admin:channels,proxy:submit"
```

Save the output — you'll need both the **hex nsec** (server key) and the **API token** string.

### Step 4 — Set environment variables

```bash
export SPROUT_UPSTREAM_URL=ws://localhost:3000
export SPROUT_PROXY_BIND_ADDR=0.0.0.0:4869
export SPROUT_PROXY_SERVER_KEY=<hex nsec from step 3>
export SPROUT_PROXY_SALT=$(openssl rand -hex 32)
export SPROUT_PROXY_API_TOKEN=<api token from step 3>
export SPROUT_PROXY_ADMIN_SECRET=<any secret string for admin API>
```

> **Tip:** Put these in a `.env` file at the repo root — `just` loads it automatically (`set dotenv-load := true` in Justfile).

### Step 5 — Start the proxy

```bash
just proxy
```

The proxy starts on `http://localhost:4869` / `ws://localhost:4869`.

### Step 6 — Create an invite token

Invite tokens are scoped to specific channels and are required for all WebSocket connections.

```bash
curl -X POST http://localhost:4869/admin/invite \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <SPROUT_PROXY_ADMIN_SECRET>" \
  -d '{"channels":"<channel-uuid>","hours":24,"max_uses":10}'
```

Response:
```json
{
  "token": "sprout_invite_<uuid>",
  "channels": ["<channel-uuid>"],
  "expires_at": "2026-03-12T22:00:00Z",
  "max_uses": 10
}
```

> **Multiple channels:** Pass a comma-separated list: `"channels":"<uuid1>,<uuid2>"`.

> **Unauthenticated dev mode:** If `SPROUT_PROXY_ADMIN_SECRET` is not set, the admin endpoint requires no `Authorization` header. The proxy logs a warning at startup.

### Step 7 — Connect a client

```
ws://localhost:4869?token=<invite_token>
```

The proxy will send a NIP-42 AUTH challenge immediately. Clients that support NIP-42 will respond automatically. See sections 4–6 for client-specific instructions.

---

## 3. Recommended Clients

| Client | Platform | NIP-28 | NIP-42 | Priority | Notes |
|--------|----------|:------:|:------:|----------|-------|
| **Coracle** | Web | ✅ | ✅ | P1 | Best overall — renders kind:42 in chat UI; NIP-29 group support (group nav not in MVP) |
| **Nostrudel** | Web | ✅ | ✅ | P1 | Good NIP-28 support; NIP-29 group navigation not in MVP scope |
| **Amethyst** | Android | ✅ | ✅ | P2 | NIP-28 public chat view works |
| **Damus** | iOS | ❌ | ✅ | P2 | No NIP-28 UI — kind:42 messages not rendered |
| **nak** | CLI | ✅ | ✅ | — | Best for scripting and automated testing |
| **websocat** | CLI | ✅ | — | — | Raw WebSocket testing; no built-in NIP-42 signing |
| **Primal** | All | ❌ | ❌ | N/A | Uses caching relay infrastructure — not direct relay; incompatible |

**Recommended for dev testing:** `nak` (CLI) for scripted tests, Coracle for UI verification.

---

## 4. Connecting with Coracle (Step-by-Step)

[Coracle](https://coracle.social) is the recommended GUI client for testing sprout-proxy. It supports NIP-28 channel rendering and NIP-42 auth.

1. Open **https://coracle.social** in a browser.
2. Navigate to **Settings → Relays → Add Relay**.
3. Enter the proxy WebSocket URL:
   ```
   ws://localhost:4869?token=<invite_token>
   ```
   For remote access, use an ngrok tunnel:
   ```bash
   ngrok http 4869
   # Then use: wss://<ngrok-subdomain>.ngrok.io?token=<invite_token>
   ```
4. Coracle will open the WebSocket connection. The proxy sends a NIP-42 `["AUTH","<challenge>"]` message. Coracle signs a kind:22242 event and responds automatically.
5. Navigate to **Public Channels** — Sprout channels appear as kind:40 channel create events.
6. Click a channel — messages appear as kind:42 in Coracle's chat UI.

> **If channels don't appear:** The proxy loads the channel map at startup from the relay's REST API. If channels were created after the proxy started, restart the proxy to refresh the map.

---

## 5. Connecting with nak (CLI Testing)

[`nak`](https://github.com/fiatjaf/nak) is the recommended tool for scripted and automated testing. It handles NIP-42 auth natively with the `--auth` flag.

### Install

```bash
go install github.com/fiatjaf/nak@latest
```

### Generate a test keypair

```bash
nak key generate
# Output: nsec1... (save this)
# Public key: npub1...
```

### Query channel list (kind:40)

```bash
nak req -k 40 -l 10 --auth \
  "ws://localhost:4869?token=<invite_token>"
```

### Subscribe to channel messages (kind:42)

```bash
# First get the kind:40 event ID for the channel
KIND40_EVENT_ID=$(nak req -k 40 -l 1 --auth \
  "ws://localhost:4869?token=<invite_token>" | jq -r '.id')

# Then subscribe to messages in that channel
nak req -k 42 --tag "e=$KIND40_EVENT_ID" -l 20 --auth \
  "ws://localhost:4869?token=<invite_token>"
```

### Publish a message (kind:42)

```bash
nak event \
  -k 42 \
  -c "Hello from nak!" \
  --tag "e=$KIND40_EVENT_ID" \
  --sec <nsec> \
  "ws://localhost:4869?token=<invite_token>"
```

### Query channel metadata (kind:41)

```bash
nak req -k 41 -l 10 --auth \
  "ws://localhost:4869?token=<invite_token>"
```

> **Note:** kind:40 and kind:41 events are served directly from the proxy's channel map (synthesized from the Sprout REST API). They are never forwarded to the upstream relay. kind:42 queries are translated to kind:40001 + `#h` tags and forwarded upstream.

---

## 6. Testing with websocat (Raw Protocol)

[`websocat`](https://github.com/vi/websocat) lets you interact with the raw NIP-01 protocol. Useful for debugging the proxy's message handling.

### Install

```bash
cargo install websocat
```

### Connect

```bash
websocat "ws://localhost:4869?token=<invite_token>"
```

### Protocol flow

Immediately after connecting, the proxy sends an AUTH challenge:
```json
["AUTH","<challenge-uuid>"]
```

You must respond with a signed kind:22242 event. Since websocat has no built-in signing, use `nak` for auth-required testing or use websocat for NIP-11 and pre-auth inspection only.

### Query channels (kind:40)

After connecting (auth challenge will appear — you can proceed with REQ before auth):
```json
["REQ","sub1",{"kinds":[40],"limit":10}]
```

### Subscribe to messages (kind:42)

```json
["REQ","sub2",{"kinds":[42],"#e":["<kind40_event_id>"],"limit":20}]
```

### Close a subscription

```json
["CLOSE","sub1"]
```

### NIP-11 relay info (no WebSocket needed)

```bash
curl -H "Accept: application/nostr+json" http://localhost:4869/
```

Response:
```json
{
  "name": "sprout-proxy",
  "description": "Sprout NIP-28 guest proxy for standard Nostr clients",
  "supported_nips": [1, 11, 28, 42],
  "software": "sprout-proxy",
  "version": "...",
  "limitation": {
    "auth_required": true
  }
}
```

---

## 7. Running the E2E Test Script

The repo includes a shell-based end-to-end test that validates NIP-11, invite creation, and WebSocket connectivity.

### Prerequisites

- Relay running: `just relay`
- Proxy running: `just proxy`
- Tools installed: `websocat`, `curl`, `jq`

### Run

```bash
./scripts/test-proxy-e2e.sh
```

### What it tests

1. **NIP-11** — `GET /` with `Accept: application/nostr+json` returns valid relay info
2. **Invite creation** — `POST /admin/invite` returns a token (fetches a channel UUID from the relay first)
3. **WebSocket connection** — connects with the token, sends a kind:40 REQ, verifies AUTH challenge is received

### Environment overrides

```bash
PROXY_URL=ws://localhost:4869 \
PROXY_HTTP=http://localhost:4869 \
RELAY_HTTP=http://localhost:3000 \
./scripts/test-proxy-e2e.sh
```

> **Note:** The test script does not perform full NIP-42 authentication (websocat has no signing). It verifies the AUTH challenge is sent and the connection is accepted. For full auth testing, use `nak` (see Section 5).

---

## 8. Environment Variables Reference

All env vars are read at startup. Required vars cause the proxy to exit with an error if missing.

| Variable | Required | Default | Description |
|----------|:--------:|---------|-------------|
| `SPROUT_UPSTREAM_URL` | ✅ | — | WebSocket URL of the Sprout relay (e.g., `ws://localhost:3000` or `wss://relay.example.com`) |
| `SPROUT_PROXY_SERVER_KEY` | ✅ | — | Hex-encoded nsec (secp256k1 secret key) for the proxy server. Used to sign REST API requests to the relay and synthesize channel events. |
| `SPROUT_PROXY_SALT` | ✅ | — | Hex-encoded 32-byte random salt for deterministic shadow key derivation. **Keep secret and stable** — changing it invalidates all shadow keypairs. |
| `SPROUT_PROXY_API_TOKEN` | ✅ | — | Sprout API token with `proxy:submit` scope. Used to authenticate REST API calls to the relay (channel map init) and to submit shadow-signed events. |
| `SPROUT_PROXY_BIND_ADDR` | ❌ | `0.0.0.0:4869` | TCP address and port for the proxy to listen on. |
| `SPROUT_PROXY_ADMIN_SECRET` | ❌ | — | Bearer token secret for the `POST /admin/invite` endpoint. If unset, the endpoint is unauthenticated (dev mode). Set this in production. |
| `RUST_LOG` | ❌ | `sprout_proxy=info,tower_http=info` | Log level filter. Use `sprout_proxy=debug` for verbose output. |

### Example `.env` file

```bash
SPROUT_UPSTREAM_URL=ws://localhost:3000
SPROUT_PROXY_BIND_ADDR=0.0.0.0:4869
SPROUT_PROXY_SERVER_KEY=<64-char hex nsec>
SPROUT_PROXY_SALT=<64-char hex random>
SPROUT_PROXY_API_TOKEN=sprout_tok_<...>
SPROUT_PROXY_ADMIN_SECRET=dev-secret-change-in-prod
RUST_LOG=sprout_proxy=debug
```

---

## 9. How It Works (Architecture)

### Kind Translation

The proxy translates between Sprout's internal event kinds and NIP-28:

| Direction | Sprout Internal | NIP-28 External | Notes |
|-----------|----------------|-----------------|-------|
| Outbound (relay → client) | kind:40001 | kind:42 | Stream message |
| Outbound (synthesized) | — | kind:40 | Channel create (from REST API) |
| Outbound (synthesized) | — | kind:41 | Channel metadata (from REST API) |
| Inbound (client → relay) | kind:40001 | kind:42 | Re-signed with shadow key |

### Channel ID Mapping

Sprout identifies channels by UUID and uses `#h` tags. NIP-28 uses `#e` tags pointing to kind:40 event IDs.

- **Outbound:** `#h <uuid>` → `#e <kind40_event_id>` (looked up in channel map)
- **Inbound:** `#e <kind40_event_id>` → `#h <uuid>` (reverse lookup in channel map)
- **kind:40/41 REQs:** Served entirely from the local channel map — never forwarded upstream

### Shadow Keys

Each external Nostr pubkey gets a deterministic shadow keypair derived from:
```
HMAC-SHA256(salt, external_pubkey_hex)
```

The shadow key is stable across proxy restarts (same salt → same shadow key). All events sent to the upstream relay are re-signed with the shadow key. This means:
- The relay sees consistent pubkeys per external user
- The relay's `proxy:submit` scope enforcement allows these re-signed events through
- External users' real keypairs are never exposed to the relay

### Invite Tokens

Invite tokens are created via `POST /admin/invite` and stored in-memory (lost on restart).

Each token encodes:
- A list of allowed channel UUIDs
- An expiry timestamp
- A maximum use count (decremented on each successful connection)

The token is passed as a query parameter: `ws://host:4869?token=<token>`. It is validated before the NIP-42 AUTH challenge is sent. A consumed or expired token causes immediate disconnect with a NOTICE message.

### `proxy:submit` Scope

The `proxy:submit` API token scope is what allows the proxy to submit shadow-signed events to the relay. Without it, the relay would reject events whose pubkey doesn't match the authenticated API token's pubkey. This scope tells the relay: "trust this token to submit events on behalf of other pubkeys."

### Pre-Auth REQ Buffering

Clients may send REQ messages before completing NIP-42 auth (some clients do this immediately on connect). The proxy buffers up to 20 REQ messages (max 64 KiB) during the auth handshake and replays them after successful authentication. The auth deadline is 30 seconds.

### Subscription Namespacing

All subscriptions are prefixed with a per-connection UUID prefix (8 chars) before being forwarded upstream. This prevents subscription ID collisions across multiple clients sharing the single upstream connection. The prefix is stripped before sending events back to the client.

---

## 10. Troubleshooting

### "auth-required: not authenticated"

The invite token was rejected before the NIP-42 challenge. Check:
- Token string is complete and unmodified
- Token has not expired (`expires_at` in the creation response)
- Token has not exceeded `max_uses`
- The proxy was not restarted (invite tokens are in-memory only)

**Fix:** Create a new invite token via `POST /admin/invite`.

### "error: invite token not found"

The token string doesn't exist in the proxy's in-memory store. Either:
- The proxy was restarted (tokens are lost on restart)
- The token string was mistyped or truncated

**Fix:** Create a new invite token.

### "error: channel not found"

The channel UUID in the invite token's channel list is not in the proxy's channel map. The channel map is loaded once at startup from the relay's REST API.

**Fix:** Restart the proxy to refresh the channel map. If the channel was just created, wait for the relay to commit it, then restart the proxy.

### Connection drops immediately (no NOTICE)

The WebSocket connection is being refused or dropped before any messages are exchanged.

Check:
1. Is the proxy running? `just proxy` or `ps aux | grep sprout-proxy`
2. Is `SPROUT_UPSTREAM_URL` correct and reachable? `curl http://localhost:3000/`
3. Is the relay running? `just relay`
4. Any errors in proxy logs? `RUST_LOG=sprout_proxy=debug just proxy`

### No messages appearing after auth

Authenticated successfully but kind:42 messages aren't coming through.

Check:
1. Is the channel UUID in the invite token's channel list?
2. Are you subscribing with the correct kind:40 event ID? (Get it from a kind:40 REQ first)
3. Are there actually messages in the channel? Post one with `nak` (Section 5) to verify.
4. Check proxy logs for "dropping upstream event" debug messages: `RUST_LOG=sprout_proxy=debug just proxy`

### AUTH challenge not received / client doesn't authenticate

Some clients don't support NIP-42. The proxy requires authentication — clients that don't respond to the AUTH challenge within 30 seconds are disconnected with `"auth-required: authentication timeout"`.

**Workaround:** Use a client that supports NIP-42 (see Section 3 table). For raw testing, use `nak --auth`.

### kind:40 channels not appearing

The proxy serves kind:40/41 from its local channel map. If the map is empty:
1. Check that `SPROUT_PROXY_API_TOKEN` has `channels:read` scope
2. Check relay logs for REST API errors at proxy startup
3. Verify the relay has channels: `curl http://localhost:3000/api/channels`

### Proxy startup fails: "failed to initialize channel map"

The proxy couldn't reach the relay's REST API at startup.

Check:
- Relay is running and healthy
- `SPROUT_UPSTREAM_URL` is correct (proxy derives HTTP base URL from it: `ws://` → `http://`)
- `SPROUT_PROXY_API_TOKEN` is valid and has `channels:read` scope
