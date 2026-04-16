NIP-AB
======

Device Pairing
--------------

`draft` `optional`

This NIP defines a protocol for securely transferring secrets between two devices over standard Nostr relays using QR-code-initiated, end-to-end encrypted channels with visual confirmation.

## Motivation

Users need their Nostr identity on multiple devices. Today the options are:

- Paste a raw `nsec` — insecure, no authentication, no encryption in transit
- Use [NIP-46](46.md) remote signing — requires the signer device to be online for every operation
- Enter a [NIP-06](06.md) mnemonic — manual, error-prone, not all clients support it

NIP-46 solves *ongoing delegation*: the key stays on one device and signs remotely. This NIP solves *one-time transfer*: the key moves to the new device, which then operates independently. They are complementary — this NIP can even bootstrap a NIP-46 session as one of its payload types.

This NIP provides a secure, authenticated channel between two devices that can carry any secret payload — a private key, a [NIP-46](46.md) session bootstrap, or application-specific data — without trusting the relay.

## Terminology

- **source**: The device that holds the secret and initiates pairing (e.g., a desktop app).
- **target**: The device that wants to receive the secret (e.g., a mobile phone).
- **pairing relay**: Any [NIP-01](01.md) compliant relay used to route pairing events. The relay learns nothing about the payload.
- **session secret**: A 32-byte random value shared via QR code, used to derive encryption keys.
- **SAS (Short Authentication String)**: A short code displayed on both devices for the user to visually confirm, preventing man-in-the-middle attacks.

## Overview

1. _source_ generates an ephemeral keypair and a session secret, encodes them in a QR code.
2. _target_ scans the QR code, generates its own ephemeral keypair.
3. Both devices connect to the pairing relay and exchange ephemeral public keys via `kind:24134` events.
4. Both devices derive a shared secret via ECDH and display a SAS code for the user to confirm.
5. After confirmation, _source_ sends the encrypted payload via a `kind:24134` event.
6. _target_ decrypts and imports the payload.

All events use ephemeral keypairs that are discarded after the session. The relay sees only opaque ciphertext addressed to throwaway public keys.

## QR Code Format

The _source_ generates:

- An ephemeral secp256k1 keypair (`source_ephemeral_privkey`, `source_ephemeral_pubkey`)
- A 32-byte cryptographically random `session_secret`

The QR code encodes a URI:

```
nostrpair://<source_ephemeral_pubkey_hex>?secret=<session_secret_hex>&relay=<wss://relay.example.com>
```

- `source_ephemeral_pubkey_hex`: 64-character lowercase hex-encoded 32-byte x-only public key (as used throughout Nostr per [BIP-340](https://github.com/bitcoin/bips/blob/master/bip-0340.mediawiki))
- `session_secret_hex`: 64-character lowercase hex-encoded 32 random bytes
- `relay`: percent-encoded WebSocket URL of the pairing relay. SHOULD appear at least once. MAY appear multiple times for redundancy. When multiple relays are specified, _target_ SHOULD attempt them in order and use the first that connects successfully.

The QR code MUST NOT contain any private key material. If intercepted, an attacker obtains only an ephemeral public key and a session secret, which are useless without completing the handshake within the session timeout.

Clients MAY support additional query parameters for forward compatibility. Unknown parameters MUST be ignored.

## Event Kind

All pairing messages use a single event kind:

```
kind: 24134
```

This kind is in the ephemeral event range. Relays SHOULD treat these events as ephemeral and MAY delete them after delivery or after a short TTL (e.g., 5 minutes). Relays do not need any special handling for this kind — standard NIP-01 event routing is sufficient.

## Event Structure

All `kind:24134` events follow this structure:

```jsonc
{
  "id": "<sha256 hash per NIP-01>",
  "pubkey": "<sender's ephemeral pubkey>",
  "kind": 24134,
  "content": "<NIP-44 encrypted JSON>",
  "tags": [["p", "<recipient's ephemeral pubkey>"]],
  "created_at": <unix timestamp>,
  "sig": "<schnorr signature per NIP-01>"
}
```

The `content` field is always [NIP-44](44.md) encrypted using the conversation key derived from the sender's ephemeral private key and the recipient's ephemeral public key, as specified in NIP-44.

The encrypted plaintext is always a JSON object containing a `type` field that identifies the message:

```jsonc
{
  "type": "<message_type>",
  // ... type-specific fields
}
```

Message types are: `offer`, `sas-confirm`, `payload`, `complete`, `abort`.

There are no unencrypted type indicators in tags or other visible fields. The relay sees only the `p` tag (an ephemeral pubkey with no link to any real identity) and opaque ciphertext.

## Event Validation

Before processing any `kind:24134` event, implementations MUST:

1. Validate the event `id` and `sig` per [NIP-01](01.md).
2. Validate that `pubkey` is a valid, non-zero secp256k1 curve point per [BIP-340](https://github.com/bitcoin/bips/blob/master/bip-0340.mediawiki).
3. Validate that the event contains a `p` tag whose value matches the local device's ephemeral public key. This guards against misdelivery by a malicious or buggy relay.
4. Validate that `pubkey` matches the expected peer for the current session state:
   - _source_ expects events from `target_ephemeral_pubkey` (learned from the first valid `offer`).
   - _target_ expects events from `source_ephemeral_pubkey` (learned from the QR code).
   - Before the first valid `offer`, _source_ accepts events from any `pubkey` (since `target_ephemeral_pubkey` is not yet known), but MUST lock to that pubkey after accepting.
5. Decrypt `content` per [NIP-44](44.md).
6. Parse the decrypted JSON and validate the `type` field against the expected message for the current state.

Events that fail any validation step MUST be silently discarded. Implementations MUST NOT reveal validation failure details to the relay or to the sender.

## Pairing Protocol

### Step 1: Source Subscribes

After displaying the QR code, _source_ subscribes to the pairing relay for events tagged to its ephemeral public key:

```json
["REQ", "<sub_id>", {"kinds": [24134], "#p": ["<source_ephemeral_pubkey>"]}]
```

### Step 2: Target Sends Offer

_target_ scans the QR code, generates its own ephemeral secp256k1 keypair (`target_ephemeral_privkey`, `target_ephemeral_pubkey`), and publishes an `offer` event:

```jsonc
{
  "kind": 24134,
  "pubkey": "<target_ephemeral_pubkey>",
  "content": "<NIP-44 encrypted>",
  "tags": [["p", "<source_ephemeral_pubkey>"]],
  "created_at": <unix_timestamp>,
  // id, sig per NIP-01
}
```

Encrypted plaintext:

```jsonc
{
  "type": "offer",
  "session_id": "<hex, 32 bytes>"
}
```

Where `session_id` is derived as:

```
session_id = HKDF-SHA256(
    IKM  = session_secret,   // 32 bytes from QR code
    salt = "",               // empty
    info = "nostr-pair-session-id",
    L    = 32
)
```

The `session_id` proves the _target_ possesses the QR code's `session_secret` without revealing the secret on the wire.

_source_ MUST verify the `session_id` matches its own derivation. _source_ MUST accept at most one valid `offer` per session. After accepting an offer, _source_ MUST ignore all subsequent `offer` events and MUST record `target_ephemeral_pubkey` as the only valid peer for the remainder of the session.

### Step 3: SAS Verification

Both devices now have each other's ephemeral public keys. Both compute:

```
ecdh_shared = ECDH(own_ephemeral_privkey, other_ephemeral_pubkey)
```

Where `ecdh_shared` is the 32-byte x-coordinate of the shared point (unhashed), as produced by standard secp256k1 scalar multiplication.

Then:

```
sas_input = HKDF-SHA256(
    IKM  = ecdh_shared,       // 32 bytes
    salt = session_secret,    // 32 bytes from QR code
    info = "nostr-pair-sas-v1",
    L    = 32
)

sas_code = be_u32(sas_input[0..4]) mod 1000000
```

Where `be_u32(bytes)` interprets the first 4 bytes of `sas_input` as a big-endian unsigned 32-bit integer.

Both devices display the `sas_code` as a zero-padded 6-digit decimal string (e.g., `"047291"`). The user MUST visually confirm the codes match on both screens before proceeding.

**UX requirement**: The confirmation prompt MUST clearly state what is being authorized. Example: *"You are about to transfer your Nostr identity to another device. Does your other device show: **047291**?"* with prominent Confirm and Deny buttons.

After the user confirms on the _source_ device, _source_ publishes a `sas-confirm` event:

```jsonc
{
  "kind": 24134,
  "pubkey": "<source_ephemeral_pubkey>",
  "content": "<NIP-44 encrypted>",
  "tags": [["p", "<target_ephemeral_pubkey>"]],
  // ...
}
```

Encrypted plaintext:

```jsonc
{
  "type": "sas-confirm",
  "transcript_hash": "<hex, 32 bytes>"
}
```

Where `transcript_hash` binds the confirmation to the full session transcript:

```
transcript = session_id
           || source_ephemeral_pubkey   // 32 bytes, x-coordinate
           || target_ephemeral_pubkey   // 32 bytes, x-coordinate
           || sas_input                 // 32 bytes

transcript_hash = HKDF-SHA256(
    IKM  = transcript,                  // 128 bytes
    salt = session_secret,
    info = "nostr-pair-transcript-v1",
    L    = 32
)
```

_target_ MUST compute the same `transcript_hash` and verify it matches before proceeding. A mismatch indicates a MITM attack or protocol error; _target_ MUST abort.

### Step 4: Payload Transfer

After receiving and verifying the `sas-confirm`, _source_ publishes a `payload` event:

Encrypted plaintext:

```jsonc
{
  "type": "payload",
  "payload_type": "<string>",
  "payload": "<string>"
}
```

Defined payload types:

| `payload_type` | Description | `payload` format |
|----------------|-------------|------------------|
| `nsec` | Private key transfer | [NIP-49](49.md) `ncryptsec1...` string (recommended) or `nsec1...` bech32 |
| `bunker` | NIP-46 signer-initiated session | `bunker://...` URI as defined in [NIP-46](46.md) |
| `connect` | NIP-46 client-initiated session | `nostrconnect://...` URI as defined in [NIP-46](46.md) |
| `custom` | Application-specific data | Any string; interpretation is application-defined |

For `nsec` payloads using [NIP-49](49.md) `ncryptsec` format, clients SHOULD set `KEY_SECURITY_BYTE = 0x02` (client does not track provenance) unless the client can positively assert the key has never been handled insecurely, in which case `0x01` MAY be used.

### Step 5: Completion

_target_ decrypts the payload, imports the secret into secure storage, and publishes a `complete` event:

Encrypted plaintext:

```jsonc
{
  "type": "complete",
  "success": true
}
```

Both devices close their subscriptions and discard their ephemeral keypairs. Implementations MUST zero the ephemeral private keys and session secret from memory before freeing.

### Abort

Either device MAY send an `abort` message at any point during the protocol:

Encrypted plaintext:

```jsonc
{
  "type": "abort",
  "reason": "<string>"
}
```

Defined reason strings:

| `reason` | Meaning |
|----------|---------|
| `"sas_mismatch"` | User observed mismatched SAS codes |
| `"user_denied"` | User explicitly denied the pairing |
| `"timeout"` | Session timed out |
| `"protocol_error"` | Unexpected message or validation failure |

Upon receiving an `abort`, the other device MUST terminate the session, discard ephemeral keys, and inform the user. Implementations MAY define additional reason strings; unknown reasons SHOULD be treated as `"protocol_error"`.

## Protocol Diagram

```
  Source (Desktop)                    Relay                     Target (Phone)
  ────────────────                    ─────                     ───────────────
  Generate ephemeral keypair
  Generate session_secret
  Display QR code
  Subscribe: kind:24134
  #p: source_ephemeral_pubkey ──────►
                                                               Scan QR code
                                                               Generate ephemeral keypair
                                      ◄─────────────────────── Publish offer
                                                               {type:"offer", session_id}
  ◄──────────────────────────────────
  Validate sig, pubkey, session_id
  Accept offer, lock to this peer
  Compute SAS code ◄─────────────────────────────────────────► Compute SAS code
  Display: "047291"                                            Display: "047291"

  [User confirms codes match on both devices]

  Publish sas-confirm ──────────────►
  {type:"sas-confirm",                ──────────────────────►
   transcript_hash}                                            Verify transcript_hash

  Publish payload ──────────────────►
  {type:"payload",                    ──────────────────────►
   payload_type:"nsec",                                        Decrypt payload
   payload:"ncryptsec1..."}                                    Import to secure storage
                                      ◄─────────────────────── Publish complete
  ◄──────────────────────────────────                          {type:"complete"}

  Discard ephemeral keys                                       Discard ephemeral keys
  Zero session_secret                                          Zero session_secret
```

## Security Considerations

### Man-in-the-Middle Attacks

An attacker who intercepts the QR code (e.g., by photographing the screen or creating a fake QR code) could attempt to race the legitimate _target_ and establish their own session. The SAS verification step prevents this: the attacker's ECDH shared secret will differ from the legitimate pair, producing a different SAS code. The user will observe mismatched codes and abort.

This is the same defense used by Matrix (emoji verification), Bluetooth Secure Simple Pairing, and ZRTP. Signal's device linking omitted SAS verification and was subsequently exploited by state-level attackers who created fake QR codes to silently link unauthorized devices.

Clients MUST display an unambiguous confirmation prompt. The prompt SHOULD explicitly state what is being authorized and display the SAS code prominently with a clear option to deny.

### Relay Compromise

A compromised relay can:
- **Drop events** (denial of service) — mitigated by session timeout and retry with alternate relays
- **Delay events** — mitigated by session timeout
- **Attempt MITM** — defeated by SAS verification (relay does not possess ephemeral private keys)

A compromised relay **cannot**:
- Read the payload (NIP-44 encrypted with ECDH keys the relay does not possess)
- Forge events (events are signed by ephemeral keys; signatures are validated before processing)
- Correlate pairing sessions with real user identities (ephemeral keys are unlinked to real identities)

### QR Code Exposure

The QR code contains only an ephemeral public key and a session secret. If an attacker captures the QR code and races the legitimate _target_ to send the first `offer`, the _source_ will accept the attacker's offer and compute a SAS using the attacker's ephemeral key. However:

1. The _source_ displays a SAS code derived from the ECDH shared secret with the attacker.
2. The user's physical phone (the legitimate _target_) either (a) failed to connect (if the attacker's offer was accepted first) and shows an error, or (b) is not displaying any SAS code at all.
3. The user observes that their phone does not show the expected SAS code and denies the pairing on the _source_.

The defense is **user verification against their physical device**, not cryptographic impossibility. This is the same security model as Bluetooth Secure Simple Pairing and ZRTP: the SAS step converts a network-level MITM into a physical-presence requirement.

The _source_ MUST reject additional `offer` events after accepting one. If the legitimate _target_'s offer arrives after an attacker's, the _target_ will receive no response and should time out.

### Session Timeout

Implementations MUST enforce a session timeout (recommended: 120 seconds from QR display). After timeout, the _source_ MUST discard the ephemeral keypair and session secret. A new QR code must be generated for a new attempt.

### Key Material on Two Devices

After an `nsec` transfer, the private key exists on both devices. This is an inherent tradeoff of key transfer versus remote signing ([NIP-46](46.md)). Clients SHOULD store imported keys in platform-secure storage (iOS Keychain, Android Keystore, OS-level credential managers).

### Replay Protection

Session secrets are random and single-use. Ephemeral keypairs are generated per session. Replaying captured events to a different session will fail because the ECDH shared secret — and therefore the NIP-44 conversation key — will differ.

### Metadata Privacy

All pairing events use ephemeral pubkeys that are unlinked to the user's real Nostr identity. The relay cannot determine which real user is pairing devices.

Implementations SHOULD set `created_at` to the current time minus a random value between 0 and 60 seconds. Implementations MUST NOT set `created_at` to a future time, as some relays reject future-dated events.

## HKDF Construction

This NIP uses HKDF-SHA256 as defined in [RFC 5869](https://datatracker.ietf.org/doc/html/rfc5869). All HKDF calls in this NIP use:

- **Hash**: SHA-256
- **Extract**: `PRK = HMAC-SHA256(salt, IKM)`. When `salt` is specified as `""` (empty string), use a zero-length byte array (not the string literal).
- **Expand**: `OKM = HKDF-Expand(PRK, info, L)` where `info` is the UTF-8 encoding of the specified string and `L` is the output length in bytes.

All byte array concatenations (`||`) are simple concatenation with no length prefixes or delimiters.

## Test Vectors

```
session_secret (hex):
  a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2

source_ephemeral_privkey (hex):
  7f4c11a9c9d1e3b5a7f2e4d6c8b0a2f4e6d8c0b2a4f6e8d0c2b4a6f8e0d2c4b5

source_ephemeral_pubkey (hex):
  199e64ca60662cb2d6e91d16cb065be51ad74a6ee5f8c5b0fdc53d246611ed9a

target_ephemeral_privkey (hex):
  3a5b7c9d1e3f5a7b9c1d3e5f7a9b1c3d5e7f9a1b3c5d7e9f1a3b5c7d9e1f3a5b

target_ephemeral_pubkey (hex):
  89a9fa762105d0aee2b19678246fe7b823aabbc4f4bf691a1ce8a70fcd36d6e4

session_id = HKDF-SHA256(IKM=session_secret, salt="", info="nostr-pair-session-id", L=32):
  fb357d0f8e8d5a5ba3b2a91cb18c119e1567b07ffa38cdebb73e68df78f5a380

ecdh_shared = ECDH(source_priv, target_pub) x-coordinate:
  9b4b6d6990713d89d6d9982e506ee1bbcde6f05c54d9d2978696e8a7274d4408

sas_input = HKDF-SHA256(IKM=ecdh_shared, salt=session_secret, info="nostr-pair-sas-v1", L=32):
  e8b03a329f3a0ac37fe7fbe929171e14b72812be67e33c5d6e193543c41798d3

sas_code = be_u32(sas_input[0..4]) mod 1000000:
  863346

transcript = session_id || source_pubkey || target_pubkey || sas_input  (128 bytes)

transcript_hash = HKDF-SHA256(IKM=transcript, salt=session_secret, info="nostr-pair-transcript-v1", L=32):
  d662818ff8911fc60a2d025f8b8b4756107104e85888dd202d28db5ca2cf28d3
```

Implementations MUST validate against these vectors. They can be reproduced with `sprout-pair test-vectors`.

## Implementation Notes

### Choosing a Pairing Relay

The _source_ encodes the relay URL in the QR code. Implementations MAY:
- Use the user's preferred relay from [NIP-65](65.md)
- Use a hardcoded default relay
- Allow the user to choose

The protocol is secure regardless of relay trustworthiness. For additional metadata privacy, a relay that supports [NIP-42](42.md) AUTH is preferred but not required.

### SAS Display

Implementations MUST display the SAS code as a zero-padded 6-digit decimal number (e.g., `047291`). Implementations MAY additionally display an emoji representation for improved usability, but the 6-digit decimal MUST always be shown as the canonical representation to ensure cross-client compatibility.

### Secure Storage

After importing a key, clients MUST store it in platform-secure storage:
- **iOS**: Keychain Services with `kSecAttrAccessibleWhenUnlockedThisDeviceOnly`
- **Android**: Android Keystore or EncryptedSharedPreferences
- **Desktop**: OS credential manager or encrypted keyring

### Error Handling

If _source_ receives an `offer` with an invalid `session_id`, it MUST silently ignore it and continue waiting for a valid offer (up to the session timeout).

If either device receives an event with an unexpected `type` for the current state, it SHOULD send an `abort` with reason `"protocol_error"` and terminate the session.

If either device does not receive the expected next message within a reasonable time (recommended: 30 seconds per step), it SHOULD send an `abort` with reason `"timeout"` and terminate the session.

## Relation to Other NIPs

- [NIP-01](01.md): All pairing events are valid NIP-01 events.
- [NIP-44](44.md): Used for all encryption within pairing events.
- [NIP-46](46.md): This NIP can bootstrap a NIP-46 session via the `bunker` or `connect` payload types. NIP-46 provides ongoing remote signing; this NIP provides one-time secure transfer. They are complementary.
- [NIP-49](49.md): Recommended format for `nsec` payloads.
- [NIP-59](59.md): Gift Wrap uses ephemeral keys for metadata privacy; this NIP uses ephemeral keys for session isolation. Both demonstrate the pattern of throwaway Nostr identities for protocol-level operations.
