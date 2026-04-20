NIP-SB
======

Steganographic Key Backup
--------------------------

`draft` `optional`

Your Nostr identity is a single private key. If you lose it, you lose everything — your name, your messages, your connections. There's no "forgot my password" button, no customer support, no recovery email. The key IS the identity.

This NIP lets you back up your key to any Nostr relay using just a password. The backup is invisible — it hides in plain sight among normal relay data. Nobody with a copy of the relay's database can tell it exists. Nobody can tell which events belong to your backup, or how many there are. To recover, you just need your password and your public key (which is your Nostr identity — you know it, or you can look it up).

The backup is split into multiple pieces, each stored as a separate Nostr event signed by a different throwaway key. Without your password, the pieces are indistinguishable from any other data on the relay, unlinkable to each other, and unlinkable to you.

## Versions

This NIP is versioned to allow future algorithm upgrades without breaking existing implementations.

Currently defined versions:

| Version | Status | Description |
|---------|--------|-------------|
| `1` | Active | scrypt KDF, HKDF-SHA256, XChaCha20-Poly1305, kind:30078 |

Blobs do not carry an on-wire version indicator — the version is implicit in the constants and algorithms used. Future versions will use different scrypt parameters, HKDF info strings, or event kinds, ensuring that v1 blobs are never misinterpreted by a v2 implementation. Implementations SHOULD document which version(s) they support.

## Motivation

[NIP-49](49.md) provides password-encrypted key export (`ncryptsec1`) but explicitly warns against publishing to relays: *"cracking a key may become easier when an attacker can amass many encrypted private keys."* This warning is well-founded: with NIP-49, an attacker who dumps a relay can grep for `ncryptsec1` and instantly build a list of every user's encrypted backup, then try one password against all blobs simultaneously — the cost is `|passwords| × 1 scrypt`, tested against all targets in parallel.

This NIP eliminates the accumulation problem. An attacker who dumps a relay sees thousands of `kind:30078` events from unrelated throwaway pubkeys with random-looking d-tags and constant-size content. No field in any blob contains or reveals the user's real pubkey — while the KDF inputs include the pubkey, the outputs (throwaway signing keys, d-tags, ciphertext) are computationally unlinkable to it without the password. The attacker cannot identify which events are backup blobs (versus Cashu wallets, app settings, drafts, or any other `kind:30078` data), cannot link blobs to each other, and cannot confirm whether a specific user has a backup at all without guessing that user's password.

### Prior Art

| System | Pattern | Gap |
|--------|---------|-----|
| NIP-49 | Single identifiable `ncryptsec1` blob | Accumulation-vulnerable, linkable to user |
| BIP-38 | Single identifiable `6P…` blob | Same |
| satnam_pub | Shamir + relay, uses identity npub | Fully linkable |
| NIP-59 | Throwaway keys for gift wrap | Messaging, not backup |
| Shufflecake | Plausible deniability | Local disk only |
| **This NIP** | Per-blob throwaway keys + password-derived tags + variable N + constant-size blobs | Novel combination |

### Design Principles

1. **No bootstrap problem** — everything derives from `password ‖ pubkey`. No salt to store, no chicken-and-egg. The user knows their pubkey at recovery time (it is the identity they are trying to recover).
2. **Constant-size blobs** — every blob is the same byte length regardless of payload. An attacker cannot infer N from content sizes.
3. **Per-blob isolation** — each blob has its own scrypt derivation, its own throwaway keypair, its own d-tag. Compromise of one blob's metadata reveals nothing about others.
4. **Per-user uniqueness** — the user's pubkey is mixed into every derivation. Identical passwords for different users produce completely unrelated blobs. No cross-user interference, no d-tag collisions.
5. **No new crypto** — scrypt (NIP-49 parameters), HKDF-SHA256, XChaCha20-Poly1305. All battle-tested.
6. **Just Nostr events** — `kind:30078` parameterized replaceable events. No special relay support needed.

## Encoding Conventions

- **Strings to bytes**: All string-to-bytes conversions use UTF-8 encoding. The NFKC-normalized password is UTF-8 encoded before concatenation.
- **Concatenation (`‖`)**: Raw byte concatenation with no length prefixes or delimiters.
- **`pubkey_bytes`**: The 32-byte raw x-only public key (as used throughout Nostr per [BIP-340](https://github.com/bitcoin/bips/blob/master/bip-0340.mediawiki)), NOT hex-encoded.
- **`to_string(i)`**: The ASCII decimal representation of the blob index `i`, with no leading zeros or padding. Examples: `"0"`, `"1"`, `"15"`. UTF-8 encoded (ASCII is a subset of UTF-8).
- **Hex encoding**: Lowercase hexadecimal, no `0x` prefix. Used for d-tags and pubkeys in JSON.
- **Base64**: RFC 4648 standard alphabet (`A-Z`, `a-z`, `0-9`, `+`, `/`) with `=` padding. NOT URL-safe alphabet. The `content` field of each blob event is base64-encoded and MUST decode to exactly 56 bytes. This produces 76 base64 characters including one trailing `=` padding character (`56 mod 3 = 2`, so padding is required). Implementations MUST accept both padded and unpadded base64 on input, and MUST produce padded base64 on output.

## Terminology

- **backup password**: User-chosen password used to derive all backup parameters. MUST be normalized to NFKC before use. Combined with the user's pubkey before hashing, guaranteeing that identical passwords for different users produce completely unrelated blobs.
- **blob**: A single `kind:30078` event containing one encrypted chunk of the private key. Each blob is signed by a different throwaway keypair and is indistinguishable from any other `kind:30078` application data.
- **chunk**: A fragment of the raw 32-byte private key. Chunks are padded to constant size before encryption.
- **N**: The number of blobs in a backup set. Derived deterministically from the password and pubkey. Range: 3–16. Unknown to an attacker without the password.
- **throwaway keypair**: An ephemeral secp256k1 keypair generated for signing a single blob. Deterministically derived from the password, pubkey, and blob index. Has no relationship to the user's real identity and is not reused across backup operations.
- **enc_key**: A 32-byte symmetric key derived from the password and pubkey, shared across all blobs in a backup set. Used for XChaCha20-Poly1305 encryption.
- **d-tag**: The NIP-33 `d` parameter uniquely identifying a parameterized replaceable event. Each blob's d-tag is derived from its per-blob scrypt output and is indistinguishable from random data.

## Limitations

This NIP provides relay-based steganographic backup and recovery of a Nostr private key. It does not provide:

- **No threshold tolerance**: loss of any single blob makes the backup unrecoverable. Multi-relay publication and periodic health checks are strongly recommended.
- **No post-quantum security**: scrypt and XChaCha20-Poly1305 are not quantum-resistant.
- **Password strength is the security floor**: weak passwords make the backup crackable regardless of the steganographic properties. Implementations MUST enforce minimum entropy (see §Specification).
- **No automatic relay discovery**: the user must know which relay(s) hold their backup blobs. There is no relay discovery mechanism in this NIP.
- **Relay retention not guaranteed**: events from throwaway keypairs may be garbage-collected by relays that do not recognize them. Multi-relay publication and periodic health checks are recommended.
- **Deniability is probabilistic, not absolute**: if a relay's ambient `kind:30078` traffic is very sparse, the presence of backup-shaped events may be statistically detectable. Deniability improves as the relay's `kind:30078` population grows.
- **No key rotation or migration**: this NIP provides backup and recovery only. It does not provide key rotation, key migration, or ongoing key management.
- **No fault tolerance**: this NIP does not use erasure coding or threshold schemes. Any missing blob makes recovery impossible. Future versions MAY add Reed-Solomon coding for fault tolerance.
- **Chunks are byte slices, not independent shares**: unlike Shamir's Secret Sharing, each chunk is a contiguous slice of the encrypted key, not an information-theoretically independent share. A compromised chunk reveals its portion of the ciphertext (though not the plaintext, which requires `enc_key`).

## Overview

```
base = NFKC(password) ‖ pubkey_bytes

base ──→ scrypt(base, salt="") ──→ H ──→ N = (H[0] % 14) + 3   (range: 3..16)

base ──→ scrypt(base, salt="encrypt") ──→ H_enc ──→ enc_key = HKDF(H_enc, "key")

nsec_bytes (32 bytes) split into N variable-length chunks

For each blob i in 0..N-1:
  base_i = base ‖ to_string(i)
  H_i = scrypt(base_i, salt="")
  d_tag_i       = hex(HKDF(H_i, "d-tag",      length=32))
  signing_key_i =      HKDF(H_i, "signing-key", length=32)  → reject if zero/≥n → throwaway keypair

  nonce_i     = random(24)                    ← fresh per blob, stored in the clear
  padded_i    = chunk_i ‖ random_bytes(16 - len(chunk_i))
  ciphertext_i = XChaCha20-Poly1305(enc_key, nonce_i, padded_i, aad=0x02)

  publish: kind:30078, d=d_tag_i,
           content = base64(nonce_i ‖ ciphertext_i)   (56 bytes constant)
           signed by signing_key_i
```

Recovery requires only the password, the user's pubkey, and a relay URL. No salt storage, no bootstrap problem, no special relay API.

## Specification

### Constants

```
SCRYPT_LOG_N     = 20          # 2^20 iterations (NIP-49 default)
SCRYPT_R         = 8
SCRYPT_P         = 1

MIN_CHUNKS       = 3
MAX_CHUNKS       = 16
CHUNK_RANGE      = 14          # MAX_CHUNKS - MIN_CHUNKS + 1

CHUNK_PAD_LEN    = 16          # pad each chunk to this size before encryption
BLOB_CONTENT_LEN = 56          # 24-byte nonce + 32-byte ciphertext (16 padded + 16 tag)
EVENT_KIND       = 30078       # NIP-78 application-specific data
```

### Password Requirements

Implementations MUST normalize passwords to NFKC Unicode normalization form before any use.

Implementations MUST enforce minimum password entropy of 80 bits. The specific entropy estimation method is implementation-defined (e.g., zxcvbn, wordlist-based calculation, or other validated estimator). Implementations MUST refuse to create a backup if the password does not meet this threshold. Implementations SHOULD recommend generated passphrases of four or more words from a standard wordlist (e.g., EFF large wordlist, BIP-39 English wordlist).

### Step 1: Determine N

```
base = NFKC(password) ‖ pubkey_bytes    # pubkey_bytes is 32 bytes (raw x-only, not hex)

H = scrypt(
    password = base,
    salt     = b"",
    N        = 2^SCRYPT_LOG_N,
    r        = SCRYPT_R,
    p        = SCRYPT_P,
    dkLen    = 32
)
N = (H[0] % CHUNK_RANGE) + MIN_CHUNKS   # result in [3, 16]
```

The empty salt is intentional — this derivation exists solely to determine N and is not used for encryption. Each blob receives its own full-strength scrypt derivation in Step 4. The pubkey is appended to the password to guarantee per-user uniqueness: identical passwords for different users produce completely unrelated N values and blob chains.

Note: `H[0] % 14` has slight modular bias (256 mod 14 = 4, so values 0–3 are approximately 0.4% more likely). This is acceptable for this use case. Implementations MAY use rejection sampling if strict uniformity is required.

### Step 2: Derive the Master Encryption Key

```
base = NFKC(password) ‖ pubkey_bytes

H_enc = scrypt(
    password = base,
    salt     = b"encrypt",
    N        = 2^SCRYPT_LOG_N,
    r        = SCRYPT_R,
    p        = SCRYPT_P,
    dkLen    = 32
)
enc_key = HKDF-SHA256(ikm=H_enc, salt=b"", info=b"key", length=32)
```

`enc_key` is shared across all blobs in the backup set. It is derived once and used for all XChaCha20-Poly1305 operations.

### Step 3: Split the Private Key into Chunks

The raw 32-byte private key is split into N variable-length chunks using integer division:

```
remainder = 32 % N
base_len  = 32 // N     # integer division

# Chunks 0..(remainder-1) are (base_len + 1) bytes.
# Chunks remainder..(N-1) are base_len bytes.
# Example: N=7 → 32 = 4×5 + 3×4 → chunks 0-3 are 5 bytes, chunks 4-6 are 4 bytes.

offset = 0
for i in 0..N-1:
    chunk_len_i = base_len + 1 if i < remainder else base_len
    chunk_i     = nsec_bytes[offset : offset + chunk_len_i]
    offset     += chunk_len_i
```

### Step 4: Derive Per-Blob Keys and Tags

For each blob `i` in `0..N-1`:

```
base_i = NFKC(password) ‖ pubkey_bytes ‖ to_string(i)
         # to_string(i) is the ASCII decimal representation, e.g. "0", "1", "15"

H_i = scrypt(
    password = base_i,
    salt     = b"",
    N        = 2^SCRYPT_LOG_N,
    r        = SCRYPT_R,
    p        = SCRYPT_P,
    dkLen    = 32
)

d_tag_i = hex(HKDF-SHA256(ikm=H_i, salt=b"", info=b"d-tag",      length=32))

signing_secret_i = HKDF-SHA256(ikm=H_i, salt=b"", info=b"signing-key", length=32)
# Interpret signing_secret_i as a 256-bit big-endian unsigned integer.
# If the value is zero or ≥ secp256k1 order n, REJECT and re-derive:
#   info=b"signing-key-1", then b"signing-key-2", etc.
# Do NOT reduce mod n (reject-and-retry avoids modular bias).
# Implementations MUST retry up to 255 times. If all attempts produce
# an invalid scalar, the backup MUST fail.
# (Probability of even one retry: ~3.7×10^-39. This will never happen.)
signing_keypair_i = keypair_from_secret(signing_secret_i)
```

Each blob's `H_i` is fully independent: different scrypt input, different output. Compromise of any `H_i` reveals nothing about any other blob's d-tag, signing key, or the enc_key.

### Step 5: Encrypt and Publish

For each blob `i`:

```
nonce_i       = random(24)    # MUST be fresh cryptographically random bytes per blob
padded_i      = chunk_i ‖ random_bytes(CHUNK_PAD_LEN - len(chunk_i))
                # random padding, NOT zero-padding — indistinguishable from ciphertext
ciphertext_i  = XChaCha20-Poly1305.encrypt(
    key       = enc_key,
    nonce     = nonce_i,
    plaintext = padded_i,          # 16 bytes
    aad       = b"\x02"            # key_security_byte per NIP-49
)
# ciphertext_i = 16 bytes plaintext + 16 bytes Poly1305 tag = 32 bytes
blob_content_i = nonce_i ‖ ciphertext_i    # 24 + 32 = 56 bytes, constant
```

Implementations MUST use fresh random 24-byte nonces for each blob. Deterministic nonces are not permitted. The random nonce ensures that re-running backup with the same password produces completely different ciphertext, preventing clustering attacks.

Publish each blob as a NIP-01 event (see §Event Structure).

Implementations SHOULD publish blobs with random delays of 100ms–2s between events to prevent timing correlation.

Implementations SHOULD jitter `created_at` timestamps within ±1 hour of the current time.

Implementations SHOULD publish to at least 2 relays for redundancy.

Implementations SHOULD periodically verify blob existence (for example, on login) and re-publish any missing blobs.

### Recovery

```
1. User provides: password, pubkey (npub or hex), relay URL(s)

2. base = NFKC(password) ‖ pubkey_bytes

3. H     = scrypt(base, salt="")        → N = (H[0] % 14) + 3
   H_enc = scrypt(base, salt="encrypt") → enc_key = HKDF(H_enc, "key")

4. For i in 0..N-1:
     H_i              = scrypt(base ‖ to_string(i), salt="")
     d_tag_i          = hex(HKDF(H_i, "d-tag"))
     signing_secret_i = HKDF(H_i, info="signing-key", length=32)
     # Interpret as big-endian uint256. If zero or ≥ n, reject and retry
     # with counter suffix (identical to Step 4 — reject-and-retry, no mod n)
     signing_pubkey_i = pubkey_from_secret(signing_secret_i)

     Query relay: REQ { "kinds": [30078], "#d": [d_tag_i], "authors": [signing_pubkey_i] }

     Verify event.pubkey == signing_pubkey_i   (reject impostors)
     Verify event.id and event.sig per NIP-01  (reject forgeries)

5. For each blob i:
     raw          = base64_decode(event.content)   # 56 bytes
     nonce_i      = raw[0:24]
     ciphertext_i = raw[24:56]
     padded_i     = XChaCha20-Poly1305.decrypt(enc_key, nonce_i, ciphertext_i, aad=b"\x02")
     chunk_len_i  = base_len + 1 if i < remainder else base_len
     chunk_i      = padded_i[0 : chunk_len_i]      # discard padding

6. nsec_bytes = chunk_0 ‖ chunk_1 ‖ … ‖ chunk_{N-1}   # 32 bytes

7. Validate the recovered nsec_bytes:
   a. Check nsec_bytes is a valid secp256k1 scalar: interpret as a 256-bit
      big-endian unsigned integer; MUST be in range [1, n-1] where n is the
      secp256k1 group order. If not → wrong password.
   b. Derive pubkey from nsec_bytes.
   c. If derived pubkey == provided pubkey → recovery successful.
      If not → wrong password (or corrupted blob). Do not use the key.
```

Total scrypt calls at recovery: 1 (for N) + 1 (for enc_key) + N (for blob tags) = N+2.
At N=8: 10 scrypt calls. At approximately 1 second each on consumer hardware: approximately 10 seconds. This is acceptable for a one-time recovery operation.

### Password Rotation

```
1. Enter old password → recover nsec (full recovery flow above)
2. Enter new password → run full backup flow (new N, new blobs, new throwaway keys)
3. Delete old blobs:
     For each old blob i in 0..old_N-1:
       Re-derive old_H_i, old signing_keypair_i (Step 4 with old password)
       Re-derive old d_tag_i
       Publish a NIP-09 kind:5 deletion event:
         {
           "kind": 5,
           "pubkey": old_signing_keypair_i.public_key,
           "tags": [
             ["a", "30078:<old_signing_pubkey_i>:<old_d_tag_i>"]
           ],
           "content": "",
           ...
         }
       signed by old_signing_keypair_i
```

Deletion uses NIP-09 `a`-tag targeting (referencing the parameterized replaceable event by `kind:pubkey:d-tag`). Each old blob requires its own deletion event signed by that blob's throwaway key — one deletion per blob.

This works because signing keys are deterministically derived from `password ‖ pubkey ‖ i` — they can be reconstructed from the old password and pubkey at any time.

Note: deletion is best-effort. Relays MAY or MAY NOT honor `kind:5` deletions. Old blobs may persist in relay archives. Since the nsec has not changed (only the backup encryption changed), old blobs still decrypt to the valid nsec with the old password. If the old password was compromised, the user SHOULD rotate their nsec entirely (a separate concern outside the scope of this NIP).

### Memory Safety

Implementations MUST zero sensitive memory after use. This includes: the password string, nsec bytes, enc_key, all H_i values, all signing_secret_i values, and all chunk_i values. Implementations SHOULD use a dedicated zeroing primitive (e.g., `zeroize` in Rust) rather than relying on language runtime garbage collection.

## Event Structure

Each backup blob is a standard NIP-01 event with the following structure:

```jsonc
{
  "id": "<sha256 hash per NIP-01>",
  "pubkey": "<signing_keypair_i.public_key>",
  "kind": 30078,
  "created_at": <unix timestamp, jittered ±1 hour>,
  "tags": [
    ["d", "<d_tag_i>"],
    ["alt", "application data"]
  ],
  "content": "<base64(nonce_i ‖ ciphertext_i)>",
  "sig": "<schnorr signature by signing_keypair_i per NIP-01>"
}
```

- `pubkey`: the throwaway signing public key for blob `i`. Has no relationship to the user's real identity.
- `kind`: `30078` (NIP-78 application-specific data, NIP-33 parameterized replaceable event).
- `tags[d]`: the derived d-tag for blob `i`. Indistinguishable from random 64-character hex.
- `tags[alt]`: the literal string `"application data"`. This is the standard NIP-31 alt tag for `kind:30078` and provides steganographic cover — it is identical to any other `kind:30078` event.
- `content`: base64-encoded 56-byte blob: 24-byte random nonce followed by 32-byte authenticated ciphertext.
- `sig`: Schnorr signature by `signing_keypair_i` over the NIP-01 event hash.

The `content` field MUST be 76 characters of base64 (56 bytes; includes one `=` padding character since `56 mod 3 = 2`). Implementations MUST reject blobs whose decoded content is not exactly 56 bytes.

No field in any blob contains or reveals the user's real pubkey. While the user's pubkey is an input to the KDF chain, the outputs (throwaway signing keys, d-tags, ciphertext) are computationally unlinkable to it without the password. The throwaway signing keys are the only pubkeys visible to the relay.

## Event Validation

Before processing any `kind:30078` event as a backup blob during recovery, implementations MUST:

1. Validate the event `id` and `sig` per [NIP-01](01.md). Events with invalid IDs or signatures MUST be silently discarded.
2. Validate that `pubkey` is a valid, non-zero secp256k1 curve point per [BIP-340](https://github.com/bitcoin/bips/blob/master/bip-0340.mediawiki).
3. Validate that `event.pubkey` matches the locally derived `signing_pubkey_i` for the queried blob index `i`. Events whose pubkey does not match MUST be silently discarded. This guards against relay-injected impostor events.
4. Validate that `event.kind` is `30078`.
5. Validate that the event contains a `d` tag whose value matches the locally derived `d_tag_i`. Events with a mismatched d-tag MUST be silently discarded.
6. Validate that `event.content` is valid base64 and decodes to exactly 56 bytes. Events with content of any other length MUST be silently discarded.
7. Decrypt `event.content` using XChaCha20-Poly1305 with `enc_key`, the 24-byte nonce (first 24 bytes of decoded content), and AAD `0x02`. If decryption fails (authentication tag mismatch), the blob MUST be rejected and recovery MUST fail for that blob index.
8. Validate that the recovered `nsec_bytes` (after reassembly) produces a pubkey matching the pubkey provided by the user. If not, the recovery MUST be rejected and the recovered key MUST NOT be used.

Events that fail any validation step MUST be silently discarded. Implementations MUST NOT reveal validation failure details to the relay.

If any blob index `i` in `0..N-1` returns no matching event from the relay, recovery MUST fail. Implementations SHOULD surface a clear error: "Backup incomplete — blob {i} not found. Check relay URL or re-publish backup."

## Security Analysis

### Threat: Multi-target accumulation (NIP-49's concern)

**Eliminated.** This is the primary security property of the scheme.

With NIP-49, an attacker who dumps a relay can grep for `ncryptsec1` and instantly build a list of every user's encrypted backup. They then try one password against all blobs simultaneously — the cost is `|passwords| × 1 scrypt`, tested against all targets in parallel.

With this NIP, the attacker sees thousands of `kind:30078` events from unrelated throwaway pubkeys with random-looking d-tags and constant-size content. **No field in any blob contains or reveals the user's real pubkey — the KDF outputs are computationally unlinkable to it without the password.** The throwaway signing keys sever the connection between the backup and the user entirely.

The attacker cannot:
- Identify which events are backup blobs (versus Cashu wallets, app settings, drafts, or any other `kind:30078` data)
- Determine whether a specific user has a backup at all
- Build a list of backup targets for batch cracking
- Link any blob to any other blob (each has a different throwaway pubkey and an unrelated d-tag)

To attack a specific user P, the attacker must already know P and then guess passwords: `|passwords| × (N+2) scrypt calls`, all bound to that one pubkey. To attack "any user," the cost is `|users| × |passwords| × (N+2) scrypt calls` — multiplying the NIP-49 accumulation cost by `|users| × (N+2)`.

The backup's **existence** is hidden, not just its contents. An attacker cannot confirm whether user P has a backup without guessing P's password. This is a qualitative security property that NIP-49 and BIP-38 do not have.

### Threat: Full relay database dump

The attacker has all events but cannot identify which events are backup blobs. No field in any blob references a real user pubkey. The throwaway signing keys are unrelated to any known identity. The d-tags are indistinguishable from any other `kind:30078` application data.

To attack a **specific known user** P:
1. `scrypt(password ‖ P)` → N (one scrypt call)
2. For i in 0..N-1: `scrypt(password ‖ P ‖ i)` → d_tag_i (N scrypt calls)
3. Search dump for events matching d_tag_i (cheap, indexed lookup)
4. If all N found: reassemble, derive enc_key, decrypt, validate

Cost per guess for one target: `(N+2) × scrypt`. For N=8, that is 10× the cost of cracking a single NIP-49 blob.

To attack **any user** (the accumulation scenario NIP-49 warns about): the attacker must iterate over every known pubkey AND every candidate password. Cost: `|users| × |passwords| × (N+2) × scrypt`. For a relay with 10,000 users, that is 100,000× the cost of the NIP-49 accumulation attack.

### Threat: Blob content size analysis

**Eliminated.** All blobs are exactly 56 bytes: 24-byte random nonce + 16-byte padded-and-encrypted chunk + 16-byte Poly1305 tag. Padding is random bytes, encrypted alongside the chunk — indistinguishable from ciphertext. An attacker cannot infer N, chunk sizes, or the total key size from content lengths.

### Threat: Content-matching / clustering attack

**Eliminated.** Each blob uses a fresh random 24-byte nonce. Re-running backup with the same password produces completely different ciphertext. Publishing to multiple relays produces non-matching blobs across relays. An attacker cannot cluster events by content to identify blob sets, even across repeated backups or multi-relay publication.

### Threat: Timing correlation

If all N blobs are published simultaneously, an attacker could cluster events by timestamp. **Mitigation**: implementations SHOULD jitter `created_at` timestamps within ±1 hour and SHOULD introduce random delays of 100ms–2s between blob publications.

### Threat: Relay garbage collection of throwaway-key events

Events from unknown pubkeys with no followers or profile are candidates for relay garbage collection. **Mitigation**: implementations SHOULD publish to at least 2 relays and SHOULD periodically verify blob existence. For corporate relays (e.g., Sprout), operators SHOULD pin `kind:30078` events to prevent GC.

### Threat: Missing blob — total loss

Any missing blob makes recovery impossible. This is the primary fragility of the scheme. **Mitigations**: multi-relay publication, periodic health checks on login, and relay pinning for managed deployments. Future versions of this NIP MAY add erasure coding (e.g., Reed-Solomon) for fault tolerance.

### Threat: Password weakness

Same as any password-based scheme. **Mitigation**: implementations MUST enforce minimum password entropy of 80 bits (see §Password Requirements). The specific entropy estimation method is implementation-defined. Implementations SHOULD recommend generated passphrases of four or more words.

### Threat: Known plaintext structure

An attacker knows the plaintext is a 32-byte secp256k1 private key. This is irrelevant — XChaCha20-Poly1305 is IND-CPA secure regardless of plaintext structure.

### Cost Comparison

| | NIP-49 single blob | This NIP (N=8) |
|---|---|---|
| Attacker cost: targeted (1 user) | 1× scrypt per guess | (N+2)× scrypt per guess = 10× |
| Attacker cost: batch (all users) | 1× scrypt per guess, tested against all blobs | `|users| × (N+2)×` scrypt per guess |
| Attacker can identify backup blobs | Yes (`ncryptsec1` prefix) | No — indistinguishable from other `kind:30078` data |
| Attacker can confirm backup exists | Yes (blob is visible) | No — requires guessing the password |
| Attacker can link blobs to user | Yes (signed by user's key) | No — throwaway keys, no reference to real pubkey |
| Deniability | No — backup existence is provable | Yes — backup existence is undetectable without password |
| Relay storage | ~400 bytes | ~3.6 KB (N=8 × ~450 bytes/event) |
| Client complexity | Low | Medium |

### Comparison to Prior Art

| Property | NIP-49 | BIP-38 | satnam_pub | This NIP |
|----------|--------|--------|------------|----------|
| Public ciphertext | Single identifiable blob | Single identifiable blob | Linkable to identity | N unlinkable constant-size blobs, indistinguishable from other relay data |
| Multi-target accumulation | Vulnerable | Vulnerable | Vulnerable | **Eliminated** |
| Backup existence detectable | Yes | Yes | Yes | **No** |
| Offline cracking cost (1 target) | 1× scrypt per guess | 1× scrypt per guess | 1× PBKDF2 per guess | (N+2)× scrypt per guess |
| Offline cracking cost (all users) | 1× scrypt, all blobs | 1× scrypt, all blobs | 1× PBKDF2, all blobs | `|users| × (N+2)×` scrypt |
| Linkability to user | Signed by user's key | Encoded with user's address | Uses identity npub | **None** |
| Deniability | No | No | No | **Yes** |
| Bootstrap problem | No (salt in blob) | No (salt in blob) | No | No (everything from password + pubkey) |
| Fault tolerance | Single blob (robust) | Single blob | Shamir threshold | No threshold (mitigated by multi-relay) |

## Relation to Other NIPs

- [NIP-01](01.md): All backup blobs are valid NIP-01 events. Implementations MUST compute `event.id` and `event.sig` per NIP-01.
- [NIP-09](09.md): Password rotation uses `kind:5` deletion events signed by the old throwaway keypairs to request deletion of superseded blobs.
- [NIP-31](31.md): Blobs include an `["alt", "application data"]` tag per NIP-31, providing steganographic cover identical to any other `kind:30078` event.
- [NIP-33](33.md): Blobs use parameterized replaceable events (kind 30000–39999). The `d` tag uniquely identifies each blob within its throwaway pubkey's namespace.
- [NIP-49](49.md): This NIP uses NIP-49's scrypt parameters (`log_N=20`, `r=8`, `p=1`) and the `key_security_byte` AAD convention (`0x02`), but does NOT use the `ncryptsec1` format. NIP-49 explicitly warns against publishing encrypted keys to relays; this NIP solves that problem.
- [NIP-59](59.md): Both NIPs use throwaway keypairs for metadata privacy. NIP-59 uses them for messaging (gift wrap); this NIP uses them for backup steganography. The pattern is the same: ephemeral Nostr identities for protocol-level operations that must not be linked to real identities.
- [NIP-78](78.md): Blobs use `kind:30078` (application-specific data) for steganographic cover. The `kind:30078` namespace is shared with Cashu wallets, app settings, drafts, and other application data, making backup blobs indistinguishable from legitimate application use.
- [NIP-AB](NIP-AB.md): NIP-AB provides device-to-device key transfer (primary backup via a second device). This NIP provides password-based relay backup (secondary "break glass" recovery for when no second device is available). They are complementary: NIP-AB is the preferred backup mechanism; this NIP is the fallback.

## Implementation Notes

### Rust

- `scrypt` crate (RustCrypto) — `scrypt::scrypt()`
- `hkdf` crate — `Hkdf::<Sha256>::new()`
- `chacha20poly1305` crate — `XChaCha20Poly1305`
- `zeroize` crate — zero sensitive memory after use; derive `Zeroize` on key structs
- `unicode-normalization` crate — NFKC normalization via `UnicodeNormalization::nfkc()`
- `zxcvbn` crate — password entropy enforcement

### TypeScript

- `@noble/hashes/scrypt` — `scrypt()`
- `@noble/hashes/hkdf` — `hkdf(sha256, ikm, salt, info, length)`
- `@noble/ciphers/chacha` — `xchacha20poly1305(key, nonce)`
- `String.prototype.normalize('NFKC')` — password normalization
- `zxcvbn` package — password entropy enforcement

### Relay Requirements

No special relay support is required. Implementations need only:

- Support `kind:30078` (NIP-78/NIP-33 parameterized replaceable events)
- Store events from unknown pubkeys (throwaway keys have no profile or followers)
- Support `#d` tag filtering in REQ subscriptions (standard NIP-33 behavior)

### Sprout-Specific Notes

- Operators SHOULD pin `kind:30078` events to prevent garbage collection of throwaway-key events.
- Backup blobs are inert database rows: stored with `d_tag` indexed, no subscription fan-out, no WebSocket traffic unless explicitly subscribed.
- Storage cost at N=16: approximately 7.2 KB per user backup (16 × ~450 bytes/event). For 10,000 users: approximately 72 MB. Trivial.

## References

- [NIP-49](https://github.com/nostr-protocol/nips/blob/master/49.md) — Encrypted private key export
- [NIP-78](https://github.com/nostr-protocol/nips/blob/master/78.md) — Application-specific data
- [NIP-33](https://github.com/nostr-protocol/nips/blob/master/33.md) — Parameterized replaceable events
- [NIP-59](https://github.com/nostr-protocol/nips/blob/master/59.md) — Gift wrap / throwaway keys
- [BIP-38](https://github.com/bitcoin/bips/blob/master/bip-0038.mediawiki) — Encrypted Bitcoin private keys
- [BIP-340](https://github.com/bitcoin/bips/blob/master/bip-0340.mediawiki) — Schnorr signatures for secp256k1
- [RFC 7914](https://www.rfc-editor.org/rfc/rfc7914) — scrypt key derivation function
- [RFC 5869](https://www.rfc-editor.org/rfc/rfc5869) — HKDF
- [RFC 2119](https://www.rfc-editor.org/rfc/rfc2119) — Key words for use in RFCs (MUST, SHOULD, MAY)
- [XChaCha20-Poly1305](https://datatracker.ietf.org/doc/html/draft-irtf-cfrg-xchacha) — Extended-nonce ChaCha20-Poly1305
- Apollo — indistinguishable shares (arXiv:2507.19484)
- Kintsugi — password-authenticated key recovery (arXiv:2507.21122)
- SoK: Plausibly Deniable Storage (arXiv:2111.12809)
- Shufflecake — hidden volumes (arXiv:2310.04589)
