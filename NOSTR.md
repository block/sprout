# Using Third-Party Nostr Clients with Sprout

Sprout speaks the Nostr wire protocol internally, but uses NIP-29 group chat events (kind:9) and
channel-scoped `#h` tags that standard NIP-28 clients don't understand. **sprout-proxy** bridges
this gap — it translates between Sprout's internal protocol and standard NIP-28 (Public Chat
Channels), so any Nostr client that supports NIP-28 and NIP-42 can read and write Sprout channels.

## Quick Start

```bash
# 1. Start infrastructure
docker compose up -d && sqlx migrate run --source migrations
cargo run -p sprout-relay &          # relay on :3000

# 2. Generate proxy server key and derive its pubkey
export SPROUT_PROXY_SERVER_KEY=$(openssl rand -hex 32)
PROXY_PUBKEY=$(echo $SPROUT_PROXY_SERVER_KEY | nak key public)

# 3. Mint a proxy API token for that pubkey
cargo run -p sprout-admin -- mint-token \
  --name "sprout-proxy" \
  --scopes "proxy:submit,channels:read,messages:read" \
  --pubkey $PROXY_PUBKEY

# 4. Start the proxy
export SPROUT_UPSTREAM_URL=ws://localhost:3000
export SPROUT_PROXY_SALT=$(openssl rand -hex 32)
export SPROUT_PROXY_API_TOKEN=<token from step 3>
export SPROUT_PROXY_ADMIN_SECRET=$(openssl rand -hex 16)
cargo run -p sprout-proxy             # proxy on :4869

# 5. Register a guest
curl -X POST http://localhost:4869/admin/guests \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $SPROUT_PROXY_ADMIN_SECRET" \
  -d '{"pubkey": "<guest-hex-pubkey>", "channels": "<channel-uuid>"}'

# 6. Connect any NIP-28 + NIP-42 client to ws://localhost:4869
```

## What Works

| Feature | Status | Notes |
|---------|:------:|-------|
| **NIP-11 relay info** | ✅ | Standard relay info document at `GET /` |
| **NIP-42 authentication** | ✅ | Proactive challenge + reactive-auth compatible |
| **Channel discovery (kind:40)** | ✅ | Synthesized from Sprout REST API; served locally |
| **Channel metadata (kind:41)** | ✅ | Name, description, picture; served locally |
| **Channel messages (kind:42)** | ✅ | Translated to/from Sprout kind:9 |
| **Message editing** | ✅ | Sprout kind:40003 ↔ kind:41 (NIP-28 uses kind:41 for both metadata and edits; the proxy routes to both local metadata and upstream edits) |
| **Real-time streaming** | ✅ | Live event delivery via open subscriptions |
| **Multi-channel access** | ✅ | Guests can be granted access to multiple channels |
| **Shadow identity** | ✅ | Each guest gets a deterministic shadow keypair |

## What Doesn't Work

| Feature | Status | Why |
|---------|:------:|-----|
| **Channel creation (kind:40 write)** | ❌ | Channels are created via Sprout's REST API, not via Nostr events |
| **NIP-10 reply threading** | ⚠️ | `#e` reply tags are preserved but Sprout doesn't model threads natively |
| **DMs (kind:4, NIP-04/NIP-44)** | ❌ | Proxy only handles NIP-28 channel events |
| **Reactions (kind:7)** | ❌ | Not translated; dropped silently |
| **User profiles (kind:0)** | ❌ | Sprout manages profiles via REST API |
| **Relay lists (kind:10002)** | ❌ | Not applicable to a private relay |
| **NIP-50 search** | ❌ | Sprout has its own search (Typesense); not exposed via NIP-50 |
| **File uploads (NIP-94/96)** | ❌ | Use Sprout's REST API for file attachments |
| **Outbox model (NIP-65)** | ❌ | Single-relay architecture; no relay discovery needed |

## Channel UUIDs vs Event IDs

Sprout identifies channels by UUID (e.g., `7f608b22-ddb9-4331-b56a-dc32107694e8`). NIP-28 clients
identify channels by the event ID of the kind:40 channel creation event. The proxy translates
between these automatically, but you need the event ID to send messages.

**To get a channel's event ID**, query for kind:40 and look at the `id` field:

```bash
nak req -k 40 --auth --sec <privkey> ws://localhost:4869
# Returns: {"kind":40,"id":"8155f2a8...","content":"{\"name\":\"my-channel\",...}"}
#                          ^^^^^^^^^^^^ this is the channel event ID
```

Use this event ID in `#e` tags when sending kind:42 messages or subscribing to a channel.

## Tested Clients

We verified end-to-end functionality with three popular, independent Nostr clients/libraries:

### nak v0.18.7 (Go CLI)

The "Nostr Army Knife" — the most popular CLI tool for interacting with Nostr relays.

```bash
# Discover channels
nak req -k 40 -l 10 --auth --sec <privkey> ws://localhost:4869

# Read messages
nak req -k 42 -l 10 --auth --sec <privkey> ws://localhost:4869

# Send a message
nak event -k 42 -c "Hello!" --tag e=<channel-event-id> \
  --auth --sec <privkey> ws://localhost:4869

# Stream live messages
nak req -k 42 --stream --auth --sec <privkey> ws://localhost:4869
```

**Verified:** NIP-42 auth, channel discovery, metadata, send, receive, streaming. All pass.

### nostr-tools v2.23 (JavaScript)

The most widely-used Nostr library in the ecosystem — powers Coracle, Snort, Damus Web, and
hundreds of other web clients.

```javascript
import { Relay } from 'nostr-tools/relay'
import { finalizeEvent } from 'nostr-tools/pure'
import { channelMessageEvent } from 'nostr-tools/nip28'

const relay = new Relay('ws://localhost:4869', { websocketImplementation: WebSocket })
relay.onauth = async (template) => finalizeEvent(template, secretKey)
await relay.connect()

// Send a NIP-28 channel message
const event = channelMessageEvent({
  channel_create_event_id: '<kind:40 event ID>',
  relay_url: 'ws://localhost:4869',
  content: 'Hello from nostr-tools!',
  created_at: Math.floor(Date.now() / 1000),
}, secretKey)
await relay.publish(event)
```

**Verified:** NIP-42 auth (onauth callback), channel discovery, metadata, send, receive, streaming.
All pass. Test script: `scripts/test-proxy-nostr-tools.mjs`.

### nostr-sdk v0.44 (Python — rust-nostr bindings)

The official Rust Nostr SDK with Python/Swift/Flutter bindings. Second most popular SDK after
nostr-tools, powers native mobile and desktop clients.

```python
import nostr_sdk

keys = nostr_sdk.Keys.parse("<hex-privkey>")
signer = nostr_sdk.NostrSigner.keys(keys)
client = nostr_sdk.ClientBuilder().signer(signer).build()
client.automatic_authentication(True)

await client.add_relay(nostr_sdk.RelayUrl.parse("ws://localhost:4869"))
await client.connect()

# Send a NIP-28 channel message
builder = nostr_sdk.EventBuilder.channel_msg(channel_eid, relay_url, "Hello from Python!")
await client.send_event_builder(builder)
```

**Verified:** NIP-42 auth (automatic_authentication), channel discovery, metadata, send, round-trip.
All pass. Test script: `scripts/test-proxy-nostr-sdk-python.py`.

## Clients Expected to Work (Not Yet Tested)

These clients support NIP-28 and NIP-42, so they should work with sprout-proxy:

| Client | Platform | NIP-28 | NIP-42 | Notes |
|--------|----------|:------:|:------:|-------|
| **Coracle** | Web | ✅ | ✅ | Best GUI option — renders kind:42 in chat UI |
| **Amethyst** | Android | ✅ | ✅ | NIP-28 public chat view |
| **Nostrudel** | Web | ✅ | ✅ | Good NIP-28 support |
| **Damus** | iOS | ❌ | ✅ | NIP-42 works but no NIP-28 channel UI |

## Clients That Won't Work

| Client | Why |
|--------|-----|
| **Primal** | Uses caching relay infrastructure — doesn't connect to relays directly |
| **Clients without NIP-42** | The proxy requires authentication; no anonymous access |

## Authentication

The proxy supports two authentication methods:

### Pubkey-Based (Primary)

Register a guest's Nostr public key with specific channel access. The proxy authenticates
via NIP-42 — the client's pubkey is matched against the guest registry.

```bash
# Register
curl -X POST http://localhost:4869/admin/guests \
  -H "Authorization: Bearer $ADMIN_SECRET" \
  -H "Content-Type: application/json" \
  -d '{"pubkey": "<hex>", "channels": "<uuid1>,<uuid2>"}'

# List
curl http://localhost:4869/admin/guests \
  -H "Authorization: Bearer $ADMIN_SECRET"

# Revoke
curl -X DELETE http://localhost:4869/admin/guests \
  -H "Authorization: Bearer $ADMIN_SECRET" \
  -H "Content-Type: application/json" \
  -d '{"pubkey": "<hex>"}'
```

### Invite Tokens (Secondary)

For ad-hoc sharing. Tokens are scoped to specific channels with optional expiry and use limits.

```bash
# Create (channels is comma-separated, hours defaults to 24, max_uses to 10)
curl -X POST http://localhost:4869/admin/invite \
  -H "Authorization: Bearer $ADMIN_SECRET" \
  -H "Content-Type: application/json" \
  -d '{"channels": "<uuid1>,<uuid2>", "max_uses": 5, "hours": 48}'

# Connect with token
# Pass as query parameter: ws://localhost:4869?token=<invite_token>
```

## Architecture

```
┌─────────────────────┐        ┌───────────────────────┐        ┌──────────────────┐
│  Nostr Client        │        │  sprout-proxy          │        │  Sprout Relay     │
│  (Coracle, nak,      │◄──────►│  :4869                 │◄──────►│  :3000            │
│   nostr-tools, etc.) │ NIP-28 │                        │internal│                   │
└─────────────────────┘        │  kind:42 ↔ kind:9       │        └──────────────────┘
                                │  #e(id)  ↔ #h(uuid)   │
                                │  shadow key re-signing │
                                └───────────────────────┘
```

**Translation pipeline:**
- **Outbound** (relay → client): kind:9 + `#h(uuid)` → kind:42 + `#e(event_id)`
- **Inbound** (client → relay): kind:42 + `#e(event_id)` → kind:9 + `#h(uuid)`
- **Channel metadata**: kind:40/41 synthesized locally from Sprout REST API (never forwarded upstream)
- **Shadow keys**: Each guest gets a deterministic keypair via HMAC-SHA256 for re-signing translated events

## Operational Notes

- **State is in-memory.** Guest registrations and invite tokens are lost on proxy restart. Re-register guests after restarting the proxy.
- **Channel map loads at startup.** Channels created after the proxy starts won't appear until the proxy is restarted.
- **Shadow keys are deterministic.** As long as `SPROUT_PROXY_SALT` stays the same, each guest's shadow keypair is stable across restarts.

## Further Reading

- [`crates/sprout-proxy/README.md`](crates/sprout-proxy/README.md) — crate-level quick start and env vars
- [`GUIDES/NOSTR_CLIENT_GUIDE.md`](GUIDES/NOSTR_CLIENT_GUIDE.md) — comprehensive 13-section guide with step-by-step instructions for each client, admin endpoints, security model, and troubleshooting
