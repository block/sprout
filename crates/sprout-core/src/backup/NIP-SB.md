NIP-SB
======

Steganographic Key Backup
--------------------------

`draft` `optional`

Your Nostr identity is a single private key. If you lose it, you lose everything — your name, your messages, your connections. There's no "forgot my password" button, no customer support, no recovery email. The key IS the identity.

This NIP lets you back up your key to any Nostr relay using just a password. The backup hides in plain sight among normal relay data. Against a passive database dump, the backup blobs are computationally indistinguishable from other application data — an attacker cannot identify which events are backup blobs, link them to each other, or link them to you without guessing your password. To recover, you just need your password and your public key (which is your Nostr identity — you know it, or you can look it up).

The backup is split into multiple pieces — real chunks, parity blobs for fault tolerance, and dummy blobs to obscure the count — each stored as a separate Nostr event signed by a different throwaway key. Without your password, the pieces are indistinguishable from any other data on the relay, unlinkable to each other, and unlinkable to you. Deniability is probabilistic and depends on the relay's ambient `kind:30078` traffic (see §Limitations).

## Versions

This NIP is versioned to allow future algorithm upgrades without breaking existing implementations.

Currently defined versions:

| Version | Status | Description |
|---------|--------|-------------|
| `1` | Active | scrypt KDF, HKDF-SHA256, XChaCha20-Poly1305, kind:30078 |

Blobs do not carry an on-wire version indicator — the version is implicit in the constants and algorithms used. Future versions will use different scrypt parameters, HKDF info strings, or event kinds, ensuring that v1 blobs are never misinterpreted by a v2 implementation. Implementations SHOULD document which version(s) they support.

## Motivation

[NIP-49](49.md) provides password-encrypted key export (`ncryptsec1`) but explicitly warns against publishing to relays: *"cracking a key may become easier when an attacker can amass many encrypted private keys."* This warning is well-founded: with NIP-49, an attacker who dumps a relay can grep for `ncryptsec1` and instantly build a list of every user's encrypted backup, then try one password against all blobs simultaneously — the cost is `|passwords| × 1 scrypt`, tested against all targets in parallel.

This NIP substantially mitigates the accumulation problem. An attacker who dumps a relay sees thousands of `kind:30078` events from unrelated throwaway pubkeys with random-looking d-tags and constant-size content. No field in any blob contains or reveals the user's real pubkey — while the KDF inputs include the pubkey, the outputs (throwaway signing keys, d-tags, ciphertext) are computationally unlinkable to it without the password. Against a passive relay-dump adversary, the attacker cannot identify which events are backup blobs (versus Cashu wallets, app settings, drafts, or any other `kind:30078` data), cannot link blobs to each other, and cannot confirm whether a specific user has a backup at all without guessing that user's password. Deniability is probabilistic and depends on the relay's ambient `kind:30078` traffic volume (see §Limitations).

### Prior Art

| System | Pattern | Gap |
|--------|---------|-----|
| NIP-49 | Single identifiable `ncryptsec1` blob | Accumulation-vulnerable, linkable to user |
| BIP-38 | Single identifiable `6P…` blob | Same |
| SLIP-39 | 2-level Shamir, PBKDF2 Feistel | Shares linkable by shared `id` field, no accumulation resistance |
| Kintsugi ([arXiv:2507.21122](https://arxiv.org/abs/2507.21122)) | Decentralized threshold OPRF key recovery | Requires dedicated recovery node infrastructure, no deniability |
| Apollo ([arXiv:2507.19484](https://arxiv.org/abs/2507.19484)) | Indistinguishable shares in social circle | Requires trustees, not relay-native |
| PASSAT ([arXiv:2102.13607](https://arxiv.org/abs/2102.13607)) | XOR secret sharing across cloud storage | No steganography, no throwaway keys, shares linkable |
| NIP-59 | Throwaway keys for gift wrap | Messaging, not backup |
| Shufflecake ([arXiv:2310.04589](https://arxiv.org/abs/2310.04589)) | Plausible deniability for disk volumes | Local disk only |
| **This NIP** | Per-blob throwaway keys + password-derived tags + variable N + RS parity + dummy blobs + constant-size blobs | Novel combination |

### Design Principles

1. **No bootstrap problem** — everything derives from `password ‖ pubkey`. No salt to store, no chicken-and-egg. The user knows their pubkey at recovery time (it is the identity they are trying to recover).
2. **Constant-size blobs** — every blob is the same byte length regardless of payload type (real chunk, parity, or dummy). An attacker cannot infer N, P, or D from content sizes.
3. **Per-blob isolation** — each real and parity blob has its own scrypt derivation, its own throwaway keypair, its own d-tag. Compromise of one blob's metadata reveals nothing about others.
4. **Per-user uniqueness** — the user's pubkey is mixed into every derivation. Identical passwords for different users produce completely unrelated blobs. No cross-user interference, no d-tag collisions.
5. **Fault tolerance** — Reed-Solomon parity (P=2) tolerates loss of up to 2 blobs. Dummy blobs obscure the real chunk count.
6. **No new crypto** — scrypt (NIP-49 parameters), HKDF-SHA256, XChaCha20-Poly1305, Reed-Solomon over GF(2^8). All battle-tested.
7. **Just Nostr events** — `kind:30078` parameterized replaceable events. No special relay support needed.

## Encoding Conventions

- **Strings to bytes**: All string-to-bytes conversions use UTF-8 encoding. The NFKC-normalized password is UTF-8 encoded before concatenation.
- **Concatenation (`‖`)**: Raw byte concatenation with no length prefixes or delimiters.
- **`pubkey_bytes`**: The 32-byte raw x-only public key (as used throughout Nostr per [BIP-340](https://github.com/bitcoin/bips/blob/master/bip-0340.mediawiki)), NOT hex-encoded.
- **`to_string(i)`**: The ASCII decimal representation of the blob index `i`, with no leading zeros or padding. Examples: `"0"`, `"1"`, `"15"`. UTF-8 encoded (ASCII is a subset of UTF-8).
- **Hex encoding**: Lowercase hexadecimal, no `0x` prefix. Used for d-tags and pubkeys in JSON.
- **Base64**: RFC 4648 standard alphabet (`A-Z`, `a-z`, `0-9`, `+`, `/`) with `=` padding. NOT URL-safe alphabet. The `content` field of each blob event is base64-encoded and MUST decode to exactly 56 bytes. This produces 76 base64 characters including one trailing `=` padding character (`56 mod 3 = 2`, so padding is required). Implementations MUST accept both padded and unpadded base64 on input, and MUST produce padded base64 on output.

## Terminology

- **backup password**: User-chosen password used to derive all backup parameters. MUST be normalized to NFKC before use. Combined with the user's pubkey before hashing, guaranteeing that identical passwords for different users produce completely unrelated blobs.
- **blob**: A single `kind:30078` event containing encrypted data. Each blob is signed by a different throwaway keypair and is indistinguishable from any other `kind:30078` application data. A backup set contains three types of blobs: real chunks, parity blobs, and dummy blobs — all identical in format and size.
- **chunk**: A fragment of the raw 32-byte private key. Chunks are padded to constant size before encryption.
- **N**: The number of real chunk blobs in a backup set. Derived deterministically from the password and pubkey. Range: 3–16. Unknown to an attacker without the password.
- **P**: The number of parity blobs. Fixed at 2. Parity blobs contain Reed-Solomon erasure-coding data computed across all N chunks, enabling recovery of up to 2 missing chunks.
- **D**: The number of dummy blobs. Derived deterministically from the password and pubkey. Range: 4–12. Dummy blobs contain encrypted random garbage and are indistinguishable from real and parity blobs.
- **parity blob**: A blob containing Reed-Solomon parity data computed across all N padded chunks. Enables reconstruction of up to P missing chunks during recovery.
- **dummy blob**: A blob containing encrypted random bytes. Published alongside real and parity blobs to obscure the total number of real chunks. Discarded during recovery.
- **throwaway keypair**: An ephemeral secp256k1 keypair generated for signing a single blob. Deterministically derived from the password, pubkey, and blob index. Has no relationship to the user's real identity and is not reused across backup operations.
- **enc_key**: A 32-byte symmetric key derived from the password and pubkey, shared across all blobs in a backup set. Used for XChaCha20-Poly1305 encryption.
- **d-tag**: The NIP-33 `d` parameter uniquely identifying a parameterized replaceable event. Each blob's d-tag is derived from its per-blob key material and is indistinguishable from random data.

## Limitations

This NIP provides relay-based steganographic backup and recovery of a Nostr private key. It does not provide:

- **Limited fault tolerance**: Reed-Solomon parity (P=2) tolerates loss of up to 2 blobs. Loss of more than 2 blobs makes the backup unrecoverable. Multi-relay publication and periodic health checks are strongly recommended.
- **No post-quantum security**: scrypt and XChaCha20-Poly1305 are not quantum-resistant.
- **Password strength is the security floor**: weak passwords make the backup crackable regardless of the steganographic properties. Implementations MUST enforce minimum entropy (see §Specification).
- **No automatic relay discovery**: the user must know which relay(s) hold their backup blobs. There is no relay discovery mechanism in this NIP.
- **Relay retention not guaranteed**: events from throwaway keypairs may be garbage-collected by relays that do not recognize them. Multi-relay publication and periodic health checks are recommended.
- **Deniability is probabilistic, not absolute**: against a passive relay-dump adversary, backup blobs are indistinguishable from other `kind:30078` data. Against an active relay operator with timing and network metadata, the steganographic cover is weaker. Deniability improves as the relay's ambient `kind:30078` population grows.
- **No key rotation or migration**: this NIP provides backup and recovery only. It does not provide key rotation, key migration, or ongoing key management.
- **Chunks are byte slices, not independent shares**: unlike Shamir's Secret Sharing, each chunk is a contiguous slice of the encrypted key, not an information-theoretically independent share. A compromised chunk reveals its portion of the ciphertext (though not the plaintext, which requires `enc_key`).

## Overview

```
base = NFKC(password) ‖ pubkey_bytes

base ──→ scrypt(base, salt="")        ──→ H     ──→ N = (H[0] % 14) + 3       (3..16 real chunks)
base ──→ scrypt(base, salt="dummies") ──→ H_d   ──→ D = (H_d[0] % 9) + 4      (4..12 dummy blobs)
base ──→ scrypt(base, salt="encrypt") ──→ H_enc ──→ enc_key = HKDF(H_enc, "key")
base ──→ scrypt(base, salt="cover")   ──→ H_cover  (for dummy blob key derivation)

P = 2  (fixed Reed-Solomon parity blobs)

nsec_bytes (32 bytes) split into N variable-length chunks
parity = RS(N+2, N) over GF(256), 16 parallel byte codes across padded chunks → 2 parity rows

Total blobs = N + P + D  (range: 9..30, variable per user, all indistinguishable)

For real chunk blobs i in 0..N-1:
  H_i = scrypt(base ‖ to_string(i), salt="")
  d_tag_i       = hex(HKDF(H_i, "d-tag",      length=32))
  signing_key_i =      HKDF(H_i, "signing-key", length=32)  → reject if zero/≥n
  padded_i      = chunk_i ‖ random_bytes(16 - len(chunk_i))

For parity blobs i in N..N+1:
  H_i = scrypt(base ‖ to_string(i), salt="")
  d_tag_i       = hex(HKDF(H_i, "d-tag",      length=32))
  signing_key_i =      HKDF(H_i, "signing-key", length=32)  → reject if zero/≥n
  padded_i      = parity_row_{i-N}                            (16 bytes from RS encoding)

For dummy blobs j in 0..D-1:
  d_tag       = hex(HKDF(H_cover, "dummy-d-tag-"       ‖ to_string(j), length=32))
  signing_key =      HKDF(H_cover, "dummy-signing-key-" ‖ to_string(j), length=32)
  padded      = random_bytes(16)

For ALL blobs (real, parity, dummy):
  nonce_i      = random(24)
  ciphertext_i = XChaCha20-Poly1305(enc_key, nonce_i, padded_i, aad=0x02)
  content_i    = base64(nonce_i ‖ ciphertext_i)   (56 bytes constant)

Collect all N+P+D blobs, shuffle into random order, publish with jittered delays.
```

Recovery requires only the password, the user's pubkey, and a relay URL. The client re-derives N, P, D, all d-tags, and queries all N+P+D d-tags in random order with jittered delays. Under normal conditions all queries return events; if up to 2 real or parity blobs are missing or corrupted, Reed-Solomon erasure decoding reconstructs them. Dummies are discarded. No salt storage, no bootstrap problem, no special relay API.

## Specification

### Constants

```
SCRYPT_LOG_N     = 20          # 2^20 iterations (NIP-49 default)
SCRYPT_R         = 8
SCRYPT_P         = 1

MIN_CHUNKS       = 3
MAX_CHUNKS       = 16
CHUNK_RANGE      = 14          # MAX_CHUNKS - MIN_CHUNKS + 1

PARITY_BLOBS     = 2           # Reed-Solomon parity blobs (tolerates 2 missing chunks)

MIN_DUMMIES      = 4
MAX_DUMMIES      = 12
DUMMY_RANGE      = 9           # MAX_DUMMIES - MIN_DUMMIES + 1

CHUNK_PAD_LEN    = 16          # pad each chunk to this size before encryption
BLOB_CONTENT_LEN = 56          # 24-byte nonce + 32-byte ciphertext (16 padded + 16 tag)
EVENT_KIND       = 30078       # NIP-78 application-specific data
```

### Password Requirements

Implementations MUST normalize passwords to NFKC Unicode normalization form before any use.

Implementations MUST enforce minimum password entropy of 80 bits. The specific entropy estimation method is implementation-defined (e.g., zxcvbn, wordlist-based calculation, or other validated estimator). Implementations MUST refuse to create a backup if the password does not meet this threshold. Implementations SHOULD recommend generated passphrases of seven or more words from a standard wordlist (e.g., EFF large wordlist at ~12.9 bits/word ≥ 90 bits for 7 words, or BIP-39 English wordlist at ~11 bits/word ≥ 88 bits for 8 words). Both exceed the 80-bit minimum with margin.

### Step 1: Determine N and D

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

H_d = scrypt(
    password = base,
    salt     = b"dummies",
    N        = 2^SCRYPT_LOG_N,
    r        = SCRYPT_R,
    p        = SCRYPT_P,
    dkLen    = 32
)
D = (H_d[0] % DUMMY_RANGE) + MIN_DUMMIES   # result in [4, 12]
```

P is fixed at `PARITY_BLOBS = 2`. The total number of blobs in a backup set is `N + P + D`, ranging from 9 to 30.

The empty salt for N derivation is intentional — this derivation exists solely to determine N and is not used for encryption. The `"dummies"` salt provides domain separation for D derivation. Each real and parity blob receives its own full-strength scrypt derivation in Step 4. The pubkey is appended to the password to guarantee per-user uniqueness: identical passwords for different users produce completely unrelated N, D values and blob chains.

Note: `H[0] % 14` and `H_d[0] % 9` have slight modular bias. This is acceptable for this use case. Implementations MAY use rejection sampling if strict uniformity is required.

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

### Step 3b: Compute Reed-Solomon Parity

Compute P=2 parity rows across the N padded chunks using 16 parallel systematic Reed-Solomon codes over GF(2^8):

```
# Pad each chunk to CHUNK_PAD_LEN before RS encoding.
# Use the same padded values that will be encrypted in Step 5.
for i in 0..N-1:
    padded_i = chunk_i ‖ random_bytes(CHUNK_PAD_LEN - len(chunk_i))

# For each byte position b in 0..15:
#   Treat padded_0[b], padded_1[b], ..., padded_{N-1}[b] as N data symbols.
#   Encode using a systematic RS(N+2, N) code over GF(2^8).
#   This produces 2 parity symbols for byte position b.
#   parity_row_0[b] = first parity symbol
#   parity_row_1[b] = second parity symbol

# Result: parity_row_0 and parity_row_1, each 16 bytes.
# These are the plaintext payloads for the 2 parity blobs.
```

The RS code MUST use the following construction: GF(2^8) with the irreducible polynomial `x^8 + x^4 + x^3 + x + 1` (0x11B, the AES polynomial). Evaluation points for the N+2 codeword positions are `α^0, α^1, ..., α^{N+1}` where `α = 0x03` is a primitive element of GF(2^8) under 0x11B (i.e., `0x03` generates the full multiplicative group of order 255). The first N positions are systematic (data), the last 2 are parity.

Concretely, the encoding for each byte position `b` in `0..15`:
- Let `d_0, d_1, ..., d_{N-1}` be the data symbols (byte `b` of each padded chunk).
- Evaluate the unique polynomial of degree `N-1` passing through `(α^0, d_0), (α^1, d_1), ..., (α^{N-1}, d_{N-1})` at the parity points `α^N` and `α^{N+1}`.
- `parity_row_0[b] = P(α^N)`, `parity_row_1[b] = P(α^{N+1})`.

Erasure decoding: given any N of the N+2 symbols (data + parity) at known positions, reconstruct the degree-(N-1) polynomial via Lagrange interpolation over GF(2^8) and evaluate at the missing positions.

Implementations MUST include test vectors (see §Implementation Notes).

Note: The random padding bytes used here MUST be the same bytes encrypted in Step 5. Generate them once and reuse for both RS encoding and encryption.

### Step 3c: Derive Cover Key for Dummy Blobs

```
H_cover = scrypt(
    password = base,
    salt     = b"cover",
    N        = 2^SCRYPT_LOG_N,
    r        = SCRYPT_R,
    p        = SCRYPT_P,
    dkLen    = 32
)
```

`H_cover` is used to derive d-tags and signing keys for all D dummy blobs via HKDF (no per-dummy scrypt call). This keeps the scrypt budget low while producing indistinguishable dummy blob metadata.

### Step 4: Derive Per-Blob Keys and Tags

#### Real chunk blobs (indices 0..N-1) and parity blobs (indices N..N+1)

For each blob `i` in `0..N+P-1` (real chunks and parity):

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

Each real and parity blob's `H_i` is fully independent: different scrypt input, different output. Compromise of any `H_i` reveals nothing about any other blob's d-tag, signing key, or the enc_key.

Parity blobs (indices N and N+1) use the same derivation as real chunks. They carry real recovery data and deserve the same per-blob scrypt isolation.

#### Dummy blobs (indices 0..D-1)

Dummy blob keys are derived from `H_cover` via HKDF, not individual scrypt calls:

```
For each dummy j in 0..D-1:
    d_tag_dummy_j       = hex(HKDF-SHA256(ikm=H_cover, salt=b"",
                              info=b"dummy-d-tag-" ‖ to_string(j),       length=32))

    signing_secret_dummy_j = HKDF-SHA256(ikm=H_cover, salt=b"",
                              info=b"dummy-signing-key-" ‖ to_string(j), length=32)
    # Interpret signing_secret_dummy_j as a 256-bit big-endian unsigned integer.
    # If the value is zero or ≥ secp256k1 order n, REJECT and re-derive:
    #   info=b"dummy-signing-key-" ‖ to_string(j) ‖ b"-1",
    #   then b"dummy-signing-key-" ‖ to_string(j) ‖ b"-2", etc.
    # Do NOT reduce mod n (reject-and-retry avoids modular bias).
    # Implementations MUST retry up to 255 times. If all attempts produce
    # an invalid scalar, the backup MUST fail.
    # (Probability of even one retry: ~3.7×10^-39. This will never happen.)
    signing_keypair_dummy_j = keypair_from_secret(signing_secret_dummy_j)
```

Dummy blobs are indistinguishable from real and parity blobs on the wire. Their d-tags and signing keys are unrelated to those of real blobs.

### Step 5: Encrypt and Publish

For each blob (real, parity, or dummy), prepare the 16-byte plaintext payload:

```
# Real chunk blobs (i in 0..N-1):
padded_i = chunk_i ‖ random_bytes(CHUNK_PAD_LEN - len(chunk_i))
           # random padding, NOT zero-padding — indistinguishable from ciphertext
           # NOTE: these are the same padded values used in Step 3b for RS encoding

# Parity blobs (i in N..N+1):
padded_i = parity_row_{i-N}    # 16 bytes from RS encoding (Step 3b)

# Dummy blobs (j in 0..D-1):
padded_j = random_bytes(CHUNK_PAD_LEN)    # 16 bytes of random garbage
```

Encrypt each payload identically:

```
nonce       = random(24)    # MUST be fresh cryptographically random bytes per blob
ciphertext  = XChaCha20-Poly1305.encrypt(
    key       = enc_key,
    nonce     = nonce,
    plaintext = padded,            # 16 bytes (chunk, parity, or random)
    aad       = b"\x02"            # key_security_byte per NIP-49
)
# ciphertext = 16 bytes plaintext + 16 bytes Poly1305 tag = 32 bytes
blob_content = nonce ‖ ciphertext    # 24 + 32 = 56 bytes, constant for ALL blob types
```

All N+P+D blobs produce identical 56-byte content regardless of type. After encryption, real chunks, parity blobs, and dummies are indistinguishable.

Implementations MUST use fresh random 24-byte nonces for each blob. Deterministic nonces are not permitted. The random nonce ensures that re-running backup with the same password produces completely different ciphertext, preventing clustering attacks.

Collect all N+P+D blobs and publish as NIP-01 events (see §Event Structure):

Implementations MUST shuffle all N+P+D blobs into random order before publication. Publishing in index order would reveal blob roles to a timing observer.

Implementations SHOULD publish blobs with random delays of 100ms–2s between events to prevent timing correlation. Implementations MAY use longer delays (minutes, hours, or days) for stronger steganographic cover.

Implementations SHOULD jitter `created_at` timestamps within ±1 hour of the current time.

Implementations SHOULD publish to at least 2 relays for redundancy.

Implementations SHOULD periodically verify blob existence (for example, on login) and re-publish any missing blobs.

### Recovery

```
1. User provides: password, pubkey (npub or hex), relay URL(s)

2. base = NFKC(password) ‖ pubkey_bytes

3. Derive parameters:
   H       = scrypt(base, salt="")          → N = (H[0] % 14) + 3
   H_d     = scrypt(base, salt="dummies")   → D = (H_d[0] % 9) + 4
   H_enc   = scrypt(base, salt="encrypt")   → enc_key = HKDF(H_enc, "key")
   H_cover = scrypt(base, salt="cover")     (for dummy d-tags)
   P = 2

4. Derive d-tags and signing pubkeys for all N+P+D blobs:

   For real and parity blobs (i in 0..N+P-1):
     H_i              = scrypt(base ‖ to_string(i), salt="")
     d_tag_i          = hex(HKDF(H_i, "d-tag"))
     signing_secret_i = HKDF(H_i, info="signing-key", length=32)
     # Reject-and-retry if zero or ≥ n (identical to Step 4)
     signing_pubkey_i = pubkey_from_secret(signing_secret_i)

   For dummy blobs (j in 0..D-1):
     d_tag_dummy_j          = hex(HKDF(H_cover, "dummy-d-tag-" ‖ to_string(j)))
     signing_secret_dummy_j = HKDF(H_cover, "dummy-signing-key-" ‖ to_string(j))
     signing_pubkey_dummy_j = pubkey_from_secret(signing_secret_dummy_j)

5. Collect all N+P+D (d-tag, signing_pubkey) pairs.
   Shuffle into random order.

6. Query relay for each d-tag with jittered delays:
     For each (d_tag, expected_pubkey) in shuffled order:
       REQ { "kinds": [30078], "#d": [d_tag] }
       # NOTE: query by d-tag only, not by authors.
       # Validate event.pubkey == expected_pubkey client-side (reject impostors).
       # Validate event.id and event.sig per NIP-01 (reject forgeries).

   Implementations SHOULD introduce random delays of 100ms–2s between
   queries to prevent timing correlation. Implementations MAY spread
   recovery queries across multiple relay connections or sessions for
   stronger cover.

   Under normal conditions, all N+P+D queries return events. If a query
   returns no event, that blob is marked as an erasure. Dummy blob
   erasures are ignored. Real or parity blob erasures are tolerated
   up to P (2) total; beyond that, recovery fails.

7. Separate results by role (client knows which indices are real, parity, dummy):
   - Discard dummy blob results (encrypted random garbage)
   - Decrypt real chunk blobs and parity blobs:

     For each real/parity blob:
       raw          = base64_decode(event.content)   # 56 bytes
       nonce        = raw[0:24]
       ciphertext   = raw[24:56]
       padded       = XChaCha20-Poly1305.decrypt(enc_key, nonce, ciphertext, aad=b"\x02")

8. Reassemble the private key:
   a. If all N real chunks present:
        For each real chunk i in 0..N-1:
          chunk_len_i = base_len + 1 if i < remainder else base_len
          chunk_i     = padded_i[0 : chunk_len_i]
        nsec_bytes = chunk_0 ‖ chunk_1 ‖ … ‖ chunk_{N-1}

   b. If up to P (2) blobs are missing from the N+P real-and-parity set:
        The RS(N+2, N) code is an MDS code: any N of the N+2 symbols
        (real chunks + parity) suffice to reconstruct all N data symbols.
        Missing blobs may be any combination of real and parity blobs
        (e.g., 2 real missing, or 1 real + 1 parity missing, or 2 parity
        missing — all are recoverable).
        Use Lagrange interpolation over GF(2^8) at the known N positions
        to reconstruct the degree-(N-1) polynomial, then evaluate at the
        missing positions to recover the missing padded chunks.
        Extract chunks from reconstructed padded blocks.
        nsec_bytes = chunk_0 ‖ chunk_1 ‖ … ‖ chunk_{N-1}

   c. If more than P (2) blobs are missing from the N+P set:
        Recovery MUST fail. Surface error: "Too many blobs missing
        ({missing_count} missing, maximum tolerated: {P}). Check relay
        URL or re-publish backup."

9. Validate the recovered nsec_bytes:
   a. Check nsec_bytes is a valid secp256k1 scalar: interpret as a 256-bit
      big-endian unsigned integer; MUST be in range [1, n-1] where n is the
      secp256k1 group order. If not → wrong password.
   b. Derive pubkey from nsec_bytes.
   c. If derived pubkey == provided pubkey → recovery successful.
      If not → wrong password (or corrupted blob). Do not use the key.
```

Total scrypt calls at recovery: 4 (for N, D, enc_key, cover) + N+P (for real and parity blob tags) = N+6.
At N=8: 14 scrypt calls. At approximately 1 second each on consumer hardware: approximately 14 seconds. This is acceptable for a one-time recovery operation. Dummy blob d-tags are derived via HKDF from the cover key and add negligible cost.

### Password Rotation

```
1. Enter old password → recover nsec (full recovery flow above)
2. Enter new password → run full backup flow (new N, P, D, new blobs, new throwaway keys)
3. Delete ALL old blobs (real + parity + dummy):

   Re-derive old N, P, D, and H_cover from old password + pubkey.

   For each old real/parity blob i in 0..old_N+P-1:
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

   For each old dummy blob j in 0..old_D-1:
     Re-derive old dummy signing_keypair_j and d_tag_j from old H_cover
     Publish a NIP-09 kind:5 deletion event (same format as above)
     signed by old dummy signing_keypair_j
```

Deletion uses NIP-09 `a`-tag targeting (referencing the parameterized replaceable event by `kind:pubkey:d-tag`). Each old blob requires its own deletion event signed by that blob's throwaway key — one deletion per blob.

This works because all signing keys are deterministically derived from `password ‖ pubkey` — they can be reconstructed from the old password and pubkey at any time.

Note: deletion is best-effort. Relays MAY or MAY NOT honor `kind:5` deletions. Old blobs may persist in relay archives. Since the nsec has not changed (only the backup encryption changed), old blobs still decrypt to the valid nsec with the old password. If the old password was compromised, the user SHOULD rotate their nsec entirely (a separate concern outside the scope of this NIP).

### Memory Safety

Implementations MUST zero sensitive memory after use. This includes: the password string, nsec bytes, enc_key, H_cover, all H_i values, all signing_secret_i values, all chunk_i values, and all parity row values. Implementations SHOULD use a dedicated zeroing primitive (e.g., `zeroize` in Rust) rather than relying on language runtime garbage collection.

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
7. Decrypt `event.content` using XChaCha20-Poly1305 with `enc_key`, the 24-byte nonce (first 24 bytes of decoded content), and AAD `0x02`. If decryption fails (authentication tag mismatch), the blob MUST be treated as an erasure (same as a missing blob). A corrupted or tampered blob is operationally equivalent to a lost blob.
8. Validate that the recovered `nsec_bytes` (after reassembly) produces a pubkey matching the pubkey provided by the user. If not, the recovery MUST be rejected and the recovered key MUST NOT be used.

Events that fail validation steps 1–6 MUST be silently discarded (treated as if the blob is missing). Events that fail step 7 (AEAD failure) MUST be treated as erasures. Implementations MUST NOT reveal validation failure details to the relay.

**Erasure model:** A real or parity blob is an erasure if it is missing from the relay, fails event validation (steps 1–6), or fails decryption (step 7). If the total number of erasures among the N+P real-and-parity blobs exceeds P (2), recovery MUST fail. Implementations SHOULD surface a clear error: "Too many blobs missing or corrupted ({count} erasures, maximum tolerated: {P}). Check relay URL or re-publish backup."

Missing or corrupted dummy blobs do not affect recovery. Implementations SHOULD re-publish missing dummies to maintain steganographic cover.

## Security Analysis

### Adversary Classes

NIP-SB's steganographic properties vary by adversary. The protocol is designed for three tiers:

| Adversary | What they observe | NIP-SB protection |
|-----------|-------------------|-------------------|
| **External network observer** (ISP, state actor) | TLS-encrypted WebSocket frames to a relay | **Complete.** All Nostr traffic is indistinguishable at the wire level. The observer cannot determine event kinds, d-tags, content, or pubkeys. NIP-SB backup/recovery traffic is identical to posting a message, updating a profile, or syncing a wallet. |
| **Passive relay-dump adversary** (database leak, subpoena, bulk export) | `kind:30078` events with random d-tags, throwaway pubkeys, constant-size content | **Strong.** Blobs are computationally indistinguishable from other `kind:30078` application data (Cashu wallets, app settings, drafts) without the password. No field references the user's real pubkey. Deniability is probabilistic and improves with ambient `kind:30078` traffic volume. |
| **Active relay operator** (timing, IP, session metadata, multi-snapshot) | Event insertion timing, query patterns, IP addresses, database snapshots over time | **Probabilistic.** Mitigated by jittered timestamps, random publication/query order, publication delays, and dummy blobs. Not guaranteed — a relay operator with network-layer visibility may correlate event bursts with user sessions. Even so, the operator cannot determine *which user* is backing up or recovering without the password. |

*Adversary classes adapted from the taxonomy in [SoK: Plausibly Deniable Storage](https://arxiv.org/abs/2111.12809) (Chen et al., 2021), mapped from disk storage to Nostr's relay architecture.*

The security analysis below evaluates each threat against the relevant adversary class.

### Threat: Multi-target accumulation (NIP-49's concern)

**Substantially mitigated.** This is the primary security property of the scheme.

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

**Content clustering eliminated; metadata clustering remains possible for repeated publications.** Each blob uses a fresh random 24-byte nonce, so re-running backup with the same password produces completely different ciphertext. An attacker cannot cluster events by content.

However, the throwaway signing keys and d-tags are deterministic for a given `password ‖ pubkey ‖ index`. If the same backup is published to multiple relays or re-published during health checks, the `(kind, pubkey, d-tag)` tuples are identical across relays. An attacker with dumps from multiple relays can intersect metadata to identify repeated publications of the same backup set. This does not reveal the user's identity (the throwaway keys are still unlinkable to the real pubkey), but it does link blobs across relays.

### Threat: Timing correlation

If all N blobs are published simultaneously, an attacker could cluster events by timestamp. **Mitigation**: implementations SHOULD jitter `created_at` timestamps within ±1 hour and SHOULD introduce random delays of 100ms–2s between blob publications.

### Threat: Relay garbage collection of throwaway-key events

Events from unknown pubkeys with no followers or profile are candidates for relay garbage collection. **Mitigation**: implementations SHOULD publish to at least 2 relays and SHOULD periodically verify blob existence. For corporate relays (e.g., Sprout), operators SHOULD pin `kind:30078` events to prevent GC.

### Threat: Missing blobs

Reed-Solomon parity (P=2) tolerates loss of up to 2 blobs from the N+P real-and-parity set. Loss of more than 2 blobs makes recovery impossible. **Mitigations**: multi-relay publication, periodic health checks on login, and relay pinning for managed deployments.

Missing dummy blobs do not affect recovery — dummies are discarded during reassembly. However, implementations SHOULD re-publish missing dummies to maintain the full N+P+D blob set for steganographic cover.

### Threat: Blob count analysis

An attacker observing the relay database sees N+P+D events (range: 9–30) from unrelated throwaway pubkeys. The attacker cannot determine which blobs are real chunks, which are parity, and which are dummies — all three types are identical in format, size, and metadata. The variable total (driven by password-derived D) prevents the attacker from inferring N from the blob count. Even if the attacker suspects a backup exists, they cannot determine the number of real chunks without the password.

### Threat: Recovery-time observation

During recovery, the client queries the relay for N+P+D d-tags in random order with jittered delays. Under normal conditions, all queries return events. If some blobs have been garbage-collected or corrupted, those queries return no event or fail AEAD validation — both are treated as erasures, tolerable up to P=2 (see §Event Validation). The relay sees a variable-size batch of d-tag lookups, most or all returning `kind:30078` events.

However, an active relay operator with network-layer visibility (IP, session, timing) may be able to correlate the query burst with a recovery attempt. **Mitigations**: implementations SHOULD jitter recovery queries with random delays of 100ms–2s. Implementations MAY spread queries across multiple relay connections, sessions, or relays. Implementations MAY use Tor or a proxy for recovery to prevent IP correlation.

Note: even if the relay identifies a recovery attempt, it cannot determine which user is recovering — the d-tags and throwaway pubkeys are unlinkable to any real identity without the password.

### Threat: Password weakness

Same as any password-based scheme. **Mitigation**: implementations MUST enforce minimum password entropy of 80 bits (see §Password Requirements). The specific entropy estimation method is implementation-defined. Implementations SHOULD recommend generated passphrases of seven or more words from a standard wordlist (e.g., EFF large wordlist at ≥90 bits for 7 words).

### Threat: Known plaintext structure

An attacker knows the plaintext is a 32-byte secp256k1 private key. This is irrelevant — XChaCha20-Poly1305 is IND-CPA secure regardless of plaintext structure.

### Cost Comparison

| | NIP-49 single blob | This NIP (N=8, P=2, D=8) |
|---|---|---|
| Attacker cost: targeted (1 user) | 1× scrypt per guess | (N+2)× scrypt per guess = 10× |
| Attacker cost: batch (all users) | 1× scrypt per guess, tested against all blobs | `|users| × (N+2)×` scrypt per guess |
| Attacker can identify backup blobs | Yes (`ncryptsec1` prefix) | No — indistinguishable from other `kind:30078` data |
| Attacker can confirm backup exists | Yes (blob is visible) | No — requires guessing the password |
| Attacker can link blobs to user | Yes (signed by user's key) | No — throwaway keys, no reference to real pubkey |
| Deniability | No — backup existence is provable | Yes — probabilistic, against passive dump adversary |
| Fault tolerance | Single blob (robust) | Tolerates loss of up to 2 blobs (RS parity) |
| Relay storage | ~400 bytes | ~8.1 KB (N+P+D=18 × ~450 bytes/event) |
| Client complexity | Low | Medium |

### Comparison to Prior Art

| Property | NIP-49 | BIP-38 | Kintsugi | SLIP-39 | This NIP |
|----------|--------|--------|----------|---------|----------|
| Public ciphertext | Single identifiable blob | Single identifiable blob | Distributed across recovery nodes | Identifiable shares (shared `id` field) | N+P+D unlinkable constant-size blobs, indistinguishable from other relay data |
| Multi-target accumulation | Vulnerable | Vulnerable | Mitigated (threshold OPRF) | Vulnerable | **Substantially mitigated** |
| Backup existence detectable | Yes | Yes | Yes (requires infra) | Yes (shares identifiable) | **No** (against passive dump adversary) |
| Offline cracking cost (1 target) | 1× scrypt per guess | 1× scrypt per guess | Threshold OPRF (no offline attack) | N/A (no password) | (N+2)× scrypt per guess |
| Offline cracking cost (all users) | 1× scrypt, all blobs | 1× scrypt, all blobs | N/A | N/A | `|users| × (N+2)×` scrypt |
| Linkability to user | Signed by user's key | Encoded with user's address | Requires recovery nodes | Shares linked by `id` | **None** |
| Deniability | No | No | No | No | **Yes** (probabilistic) |
| Bootstrap problem | No (salt in blob) | No (salt in blob) | Requires node registration | Requires share distribution | No (everything from password + pubkey) |
| Fault tolerance | Single blob (robust) | Single blob | Threshold (t-of-n) | Threshold (t-of-n) | Tolerates 2 missing blobs (RS parity) |
| Infrastructure required | None | None | Dedicated recovery nodes | Trusted share holders | **None** (standard Nostr relays) |

## Relation to Other NIPs

- [NIP-01](01.md): All backup blobs are valid NIP-01 events. Implementations MUST compute `event.id` and `event.sig` per NIP-01.
- [NIP-09](09.md): Password rotation uses `kind:5` deletion events signed by the old throwaway keypairs to request deletion of superseded blobs.
- [NIP-31](31.md): Blobs include an `["alt", "application data"]` tag per NIP-31, providing steganographic cover identical to any other `kind:30078` event.
- [NIP-33](33.md): Blobs use parameterized replaceable events (kind 30000–39999). The `d` tag uniquely identifies each blob within its throwaway pubkey's namespace.
- [NIP-49](49.md): This NIP uses NIP-49's scrypt parameters (`log_N=20`, `r=8`, `p=1`) and the `key_security_byte` AAD convention (`0x02`), but does NOT use the `ncryptsec1` format. NIP-49 explicitly warns against publishing encrypted keys to relays; this NIP solves that problem.
- [NIP-59](59.md): Both NIPs use throwaway keypairs for metadata privacy. NIP-59 uses them for messaging (gift wrap); this NIP uses them for backup steganography. The pattern is the same: ephemeral Nostr identities for protocol-level operations that must not be linked to real identities.
- [NIP-78](78.md): Blobs use `kind:30078` (application-specific data) for steganographic cover. The `kind:30078` namespace is shared with Cashu wallets, app settings, drafts, and other application data, making backup blobs indistinguishable from legitimate application use.
- [NIP-AB](NIP-AB.md): **NIP-AB is the primary key backup and multi-device mechanism.** It provides device-to-device key transfer via QR code + ECDH + short authentication string — fast, interactive, and cryptographically strong without password-strength dependencies. **This NIP (NIP-SB) is the secondary "break glass" recovery option** for when no second device is available (all devices lost, single-device user, new user who never paired). Implementations SHOULD present NIP-AB as the default backup path and NIP-SB as an optional emergency fallback. The two are complementary: NIP-AB covers the common case; NIP-SB covers the catastrophic case.

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
- Storage cost at N=16, P=2, D=12 (maximum): approximately 13.5 KB per user backup (30 × ~450 bytes/event). For 10,000 users: approximately 135 MB. Trivial.

## Test Vectors

### Reed-Solomon GF(2^8) Verification

Field: GF(2^8) with irreducible polynomial `0x11B` (`x^8 + x^4 + x^3 + x + 1`).
Primitive element: `α = 0x03` (multiplicative order 255).

**Primitive element verification:**
```
α^1 = 0x03, α^2 = 0x05, α^3 = 0x0F, α^4 = 0x11, ...
α^255 = 0x01 (full cycle)
```

**RS encode test (N=3 data, P=2 parity):**
```
Evaluation points: α^0=0x01, α^1=0x03, α^2=0x05, α^3=0x0F, α^4=0x11
Data symbols:      [0x42, 0xAB, 0x07]
Parity symbols:    [0x62, 0x59]
Full codeword:     [0x42, 0xAB, 0x07, 0x62, 0x59]
```

**RS decode tests (all must recover data = [0x42, 0xAB, 0x07]):**
```
No erasures:                [0x42, 0xAB, 0x07, 0x62, 0x59] → [0x42, 0xAB, 0x07] ✓
1 erasure (pos 1):          [0x42, None, 0x07, 0x62, 0x59] → [0x42, 0xAB, 0x07] ✓
2 erasures (pos 0,2):       [None, 0xAB, None, 0x62, 0x59] → [0x42, 0xAB, 0x07] ✓
Mixed (data 1 + parity 3):  [0x42, None, 0x07, None, 0x59] → [0x42, 0xAB, 0x07] ✓
```

### GF(2^8) Multiplication Examples

```
gf_mul(0x03, 0x03) = 0x05
gf_mul(0x03, 0x05) = 0x0F
gf_mul(0x57, 0x83) = 0xC1    (standard AES MixColumns test vector)
gf_inv(0x03)       = 0xF6    (since gf_mul(0x03, 0xF6) = 0x01)
```

Implementations MUST reproduce these test vectors exactly. Any deviation indicates a GF(2^8) arithmetic or RS encoding bug.

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
- [Apollo](https://arxiv.org/abs/2507.19484) — indistinguishable shares for social key recovery (Mishra et al., EPFL, 2025)
- [Kintsugi](https://arxiv.org/abs/2507.21122) — password-authenticated decentralized key recovery (Ma & Kleppmann, Cambridge, 2025)
- [SoK: Plausibly Deniable Storage](https://arxiv.org/abs/2111.12809) — systematization of plausible deniability (Chen et al., Stony Brook, 2021)
- [Shufflecake](https://arxiv.org/abs/2310.04589) — hidden volumes for plausible deniability (Anzuoni & Gagliardoni, ACM CCS 2023)
- [PASSAT](https://arxiv.org/abs/2102.13607) — single-password secret-shared cloud storage (2021)
- [MFKDF](https://arxiv.org/abs/2208.05586) — multi-factor key derivation with public parameters (Nair & Song, USENIX Security 2023)
