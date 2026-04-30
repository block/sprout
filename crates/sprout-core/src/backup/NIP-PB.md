NIP-PB
======

Password-Bound Key Backup
--------------------------

`draft` `optional`

Back up a Nostr private key to any relay using just a password. The backup is a single `kind:30078` event signed by a throwaway key. To recover, you need your password and your public key.

The core security property: **each password guess is bound to one pubkey.** An attacker who dumps a relay cannot batch-test passwords across users. This eliminates the accumulation attack that makes relay-published encrypted keys dangerous.

## Motivation

Existing password-encrypted key formats (NIP-49 `ncryptsec1`, BIP-38) are safe for local storage but dangerous on relays. An attacker who dumps a relay can identify every encrypted backup by its format prefix (`ncryptsec1`), giving them a list of targets. For each password guess, the attacker tries all N blobs — the probability of cracking at least one user grows with N, and the cost per successful crack drops to `|passwords| × scrypt_cost / N`.

This NIP solves the accumulation problem by mixing the user's public key into the KDF input. The backup blob is published under a throwaway identity with no reference to the user's real pubkey. To test a password, the attacker must already know which user they're targeting. One guess, one user. To attack all users: `|users| × |passwords| × 1 KDF call`.

## Security Properties

- **Accumulation resistance.** Each password guess is bound to one pubkey via the KDF input. An attacker cannot amortize a guess across multiple users.
- **Unlinkability.** No field in the blob references the user's real pubkey. The throwaway signing key severs the connection between the backup and the identity.
- **Cross-relay unlinkability.** The relay URL is mixed into metadata derivation. The same backup on different relays produces different throwaway keys and d-tags.
- **No relay trust.** The relay operator — even with full database access, connection logs, and knowledge of the target's pubkey — cannot recover the nsec without the password. The brute-force cost is 2^(entropy−1) KDF calls.
- **Post-quantum confidentiality.** The confidentiality chain (scrypt → HKDF-SHA256 → XChaCha20-Poly1305) uses only symmetric and hash-based primitives. With sufficient password entropy, confidentiality holds against quantum adversaries. See §Post-Quantum Considerations.

## Limitations

- **Single blob.** No fault tolerance on a single relay. If the relay loses the event, the backup is gone. Publish to multiple relays and verify periodically.
- **No steganographic guarantee.** An active relay operator can identify the blob via timing and metadata patterns. The security argument does not depend on cover — even if the adversary knows a blob is a backup, they cannot determine whose backup it is or recover the nsec without the password.
- **Password strength is the security floor.** Weak passwords make the backup crackable regardless of protocol design.
- **No automatic relay discovery.** The user must know which relay(s) hold their backup. Recovery requires three inputs — password, public key, and relay URL — all of which must be correct. Users SHOULD record their backup relay URL(s) alongside their public key.
- **Relay retention not guaranteed.** Events from throwaway keys may be garbage-collected. Publish to multiple relays and verify periodically.
- **No key rotation or migration.** This NIP provides backup and recovery only.

## Constants

```
SCRYPT_LOG_N     = 20          # 2^20 cost parameter (requires ~1 GiB RAM per evaluation)
SCRYPT_R         = 8
SCRYPT_P         = 1
EVENT_KIND       = 30078       # NIP-78 application-specific data
AAD              = b"\x02"     # NIP-49 key_security_byte convention, used here for format consistency
NONCE_LEN        = 24          # XChaCha20-Poly1305 nonce
CONTENT_LEN      = 72          # 24 nonce + 32 ciphertext + 16 tag (decoded bytes)
```

**Resource requirement:** scrypt with these parameters requires approximately 1 GiB of RAM per evaluation (`128 × r × N = 128 × 8 × 2^20`). Implementations that cannot allocate this memory MUST fail with an explicit error. Implementations MUST NOT silently reduce scrypt parameters — doing so would produce a different root key and make the backup unrecoverable.

## Input Requirements

### Backup Inputs

Clients MUST validate before creating a backup:

1. Normalize `password` to NFKC, then UTF-8 encode as `pw_bytes`.
2. REJECT if `len(pw_bytes) == 0`.
3. REJECT if `len(pw_bytes) > 65535`.
4. Enforce minimum password entropy of 128 bits (see §Password Requirements).
5. REJECT unless `pubkey_bytes` is exactly 32 bytes and a valid x-only secp256k1 public key per BIP-340.
6. REJECT unless `nsec_bytes` is exactly 32 bytes and a valid secp256k1 secret scalar (`1 ≤ int_be(nsec_bytes) < secp256k1_n`).
7. REJECT unless `pubkey_from_secret(nsec_bytes) == pubkey_bytes`. This prevents creating an unrecoverable backup from a mismatched key pair.

### Recovery Inputs

Clients MUST validate before attempting recovery:

1. Normalize `password` to NFKC, then UTF-8 encode as `pw_bytes`.
2. REJECT if `len(pw_bytes) == 0`.
3. REJECT if `len(pw_bytes) > 65535`.
4. REJECT unless `pubkey_bytes` is exactly 32 bytes and a valid x-only secp256k1 public key per BIP-340.

## Password Requirements

Implementations MUST enforce minimum password entropy of 128 bits. This threshold provides:
- **Classical security:** 2^127 expected KDF calls for brute-force
- **Post-quantum security:** ~2^64 expected KDF calls under Grover's algorithm

Implementations MUST offer a built-in passphrase generator producing at least 10 words from a standard wordlist (e.g., EFF large wordlist at ~12.9 bits/word ≥ 129 bits for 10 words, or BIP-39 at ~11 bits/word ≥ 132 bits for 12 words). Implementations SHOULD default to the generated passphrase and SHOULD present user-chosen passwords as a secondary option with a prominent warning.

For user-chosen passwords, implementations MUST estimate entropy using a recognized algorithm (e.g., zxcvbn or equivalent) and MUST refuse to create a backup if the estimate is below 128 bits. Implementations MUST NOT create a backup with a password that fails the entropy check.

## Relay URL Normalization

The relay URL is a derivation input — different URLs produce different d-tags and signing keys. Normalization is therefore critical for interoperability.

Normalize using the WHATWG URL Standard:

1. Parse as an absolute URL using the WHATWG URL Standard parsing algorithm.
2. REJECT if the scheme is not `wss`.
3. REJECT if the URL contains userinfo (username or password).
4. REJECT if the URL contains a query string.
5. Serialize using the WHATWG URL Standard serialization algorithm with the "exclude fragment" flag set.
6. UTF-8 encode the result. This is `relay_url_bytes`.

Implementations MUST use a WHATWG-conformant URL parser. Non-conformant parsers may produce different canonical forms and cause recovery failures.

**Examples:**

| Input | Result |
|---|---|
| `wss://Relay.Example.COM` | `wss://relay.example.com/` |
| `wss://relay.example.com:443` | `wss://relay.example.com/` |
| `wss://relay.example.com:8080` | `wss://relay.example.com:8080/` |
| `wss://relay.example.com#frag` | `wss://relay.example.com/` |
| `wss://relay.example.com/v1` | `wss://relay.example.com/v1` |
| `wss://relay.example.com/v1/` | `wss://relay.example.com/v1/` |
| `wss://bücher.example` | `wss://xn--bcher-kva.example/` |
| `wss://relay.example.com?q=1` | REJECTED |
| `ws://relay.example.com` | REJECTED |
| `wss://user:pass@relay.example.com` | REJECTED |

## Specification

### Step 1: Derive Root Key

```
pw_bytes = UTF-8(NFKC(password))
base     = len(pw_bytes).to_bytes(2, 'big') ‖ pw_bytes ‖ pubkey_bytes

H = scrypt(
    password = base,
    salt     = b"nip-pb/v1/root",
    N        = 2^SCRYPT_LOG_N,
    r        = SCRYPT_R,
    p        = SCRYPT_P,
    dkLen    = 32
)
```

One scrypt call. The 2-byte big-endian length prefix on `pw_bytes` ensures injective encoding: distinct (password, pubkey) pairs always produce distinct `base` values. `pubkey_bytes` is the 32-byte raw x-only public key (not hex-encoded).

### Step 2: Derive Encryption Key, D-Tag, and Signing Key

All derived from the single root key `H` via HKDF-SHA256 with distinct, namespaced info strings:

```
enc_key   = HKDF-SHA256(ikm=H, salt=b"", info=b"nip-pb/v1/enc",                  length=32)
d_tag_raw = HKDF-SHA256(ikm=H, salt=b"", info=b"nip-pb/v1/d" ‖ relay_url_bytes,  length=32)
sign_skm  = HKDF-SHA256(ikm=H, salt=b"", info=b"nip-pb/v1/sk" ‖ relay_url_bytes, length=32)

d_tag     = hex_lower(d_tag_raw)
sign_key  = scalar_from_hash(sign_skm)
sign_pk   = secp256k1_xonly_pubkey(sign_key)
```

`relay_url_bytes` in the d-tag and signing-key info strings ensures that the same backup published to different relays produces completely different metadata on each relay.

`enc_key` does NOT include `relay_url_bytes` — the encryption key is the same across all relays. This allows a client to decrypt a blob retrieved from any relay.

### scalar_from_hash

Interpret the 32-byte input as a big-endian unsigned integer. If the value is zero or ≥ the secp256k1 group order `n`, reject and re-derive:

```
scalar_from_hash(seed):
    for ctr in 0..255:
        candidate = HKDF-SHA256(
            ikm    = seed,
            salt   = b"",
            info   = b"nip-pb/v1/scalar:" ‖ to_string(ctr),
            length = 32
        )
        k = int_be(candidate)
        if 1 ≤ k < secp256k1_n:
            return k
    FAIL
```

Do NOT reduce modulo `n` — reject-and-retry avoids modular bias. The probability of even one retry is ~3.7 × 10^-39.

### Step 3: Encrypt

```
nonce      = random(24)         # MUST be fresh random bytes per (relay, publication)
ciphertext = XChaCha20-Poly1305.encrypt(
    key       = enc_key,
    nonce     = nonce,
    plaintext = nsec_bytes,     # 32 bytes
    aad       = AAD             # b"\x02"
)
content_bytes = nonce ‖ ciphertext    # 24 + 48 = 72 bytes
content       = base64(content_bytes) # 96 characters, no padding (72 mod 3 = 0)
```

**Nonce freshness:** `nonce` MUST be generated independently for each publication event. Republishing to a different relay MUST use a fresh nonce. This ensures `content` differs across relays, preventing cross-relay linkability via ciphertext matching.

### Step 4: Publish

Publish as a standard NIP-01 / NIP-33 parameterized replaceable event:

```json
{
  "kind": 30078,
  "pubkey": "<sign_pk hex>",
  "created_at": <unix_timestamp>,
  "tags": [
    ["d", "<d_tag>"],
    ["alt", "application data"]
  ],
  "content": "<base64 of exactly 72 decoded bytes>",
  "id": "<NIP-01 event hash>",
  "sig": "<Schnorr signature by sign_key>"
}
```

- `pubkey`: the throwaway signing public key. No relationship to the user's real identity.
- `kind`: 30078 (NIP-78 application-specific data). Backup blobs share this kind with other application data (Cashu wallets, app settings, drafts), providing ambient cover.
- `d` tag: the derived d-tag. Indistinguishable from random 64-character hex.
- `alt` tag: literal `"application data"` per NIP-31.
- `content`: base64-encoded 72-byte blob.

Implementations SHOULD publish to at least 2 relays for redundancy.

### Base64 Rules

- RFC 4648 standard alphabet (`A-Z`, `a-z`, `0-9`, `+`, `/`).
- 72 decoded bytes produces 96 base64 characters with no `=` padding (`72 mod 3 = 0`).
- Implementations MUST produce unpadded output (no trailing `=`).
- Implementations MUST accept both padded and unpadded input.
- URL-safe base64 is NOT permitted.
- Implementations MUST reject content that does not decode to exactly 72 bytes.

## Recovery

```
1. User provides: password, pubkey (npub or hex), relay URL(s).

2. Derive H, enc_key, d_tag, sign_pk (Steps 1-2, identical to backup).
   Cost: 1 scrypt call.

3. Query relay with exact address:
     { "kinds": [30078], "authors": ["<sign_pk>"], "#d": ["<d_tag>"] }

4. Validate each returned event (see §Event Validation).

5. Among all valid events, select the one with the highest created_at.
   On created_at tie, select the event with the lexicographically
   lowest event id (compared as 64-character lowercase hex strings).
   If no events pass validation, the blob is missing.

6. The selected event has already passed all validation steps including
   AEAD decryption and pubkey verification (see §Event Validation).
   The decrypted plaintext from step 6 of validation is the nsec.

7. If blob is missing on this relay, try other relays.
   A client MAY try all known relays in parallel.
```

Wrong password → `(authors, #d)` miss (no matching event) or AEAD decryption failure. Wrong relay URL → `(authors, #d)` miss (relay URL is a derivation input). Both fail safely with no information leakage.

## Event Validation

For each event returned by the recovery query, implementations MUST apply these checks in order:

1. Validate `event.id` and `event.sig` per NIP-01. Discard on failure.
2. Validate `event.pubkey == sign_pk`. Discard on mismatch.
3. Validate `event.kind == 30078`. Discard on mismatch.
4. Validate the event contains exactly one `d` tag with value `d_tag`. Discard if missing, duplicate, or mismatched. Additional tags (including `alt`) MAY be present and MUST be ignored during validation.
5. Validate `event.content` is valid base64 decoding to exactly 72 bytes. Discard on failure.
6. Attempt AEAD decryption with `enc_key`. Discard if authentication fails.
7. Validate the decrypted plaintext is a valid secp256k1 scalar (`1 ≤ int_be < n`). Discard on failure.
8. Validate `pubkey_from_secret(plaintext) == user-provided pubkey`. Discard on mismatch.

Events that fail any step MUST be silently discarded. Implementations MUST NOT reveal which step failed to the relay.

**Validate-then-select:** A malformed newer event MUST NOT suppress an older valid one. An event is valid only after passing ALL eight steps above, including AEAD decryption and pubkey verification. Validate all candidates first, then select the newest valid one (with event-id tiebreak).

## Password Rotation

```
1. Recover nsec with old password (full recovery flow).
2. Backup with new password (produces new H, new d-tag, new signing key).
3. Delete old blob on each relay:
     Publish NIP-09 kind:5 deletion event:
     {
       "kind": 5,
       "pubkey": "<old_sign_pk>",
       "tags": [["a", "30078:<old_sign_pk>:<old_d_tag>"]],
       "content": ""
     }
     signed by old signing key.

   Deletion is per-relay (d-tags and signing keys are relay-scoped).
```

After publishing the new backup, implementations MUST verify the new blob is retrievable from each relay before publishing deletion events for the old blob. If the new blob cannot be verified on a relay, do NOT delete the old blob on that relay.

Deletion is best-effort. Relays MAY or MAY NOT honor `kind:5` deletions. Old blobs that persist remain encrypted under the old password.

## Security Analysis

### Targeted Attack (known pubkey)

The adversary knows the target's pubkey and relay URL. For each password guess:

1. `base = len(pw) ‖ pw ‖ pubkey`
2. `H = scrypt(base, "nip-pb/v1/root")` — 1 scrypt call
3. Derive `d_tag` and `sign_pk` — microseconds
4. Check `(authors, #d)` in relay or local dump — O(1)
5. Miss → wrong password. **Cost: 1 scrypt call.**

At 128-bit password entropy: 2^127 expected guesses.

### Batch Attack (all users)

Each guess is bound to one pubkey. To test one password against all users: `|users| × 1 scrypt`. With NIP-49, the attacker can identify all encrypted backups by prefix and gets N chances per password guess — the cost per successful crack is `|passwords| × scrypt_cost / N`. With NIP-PB, each user requires independent computation: **|users|× more expensive.**

### What Doesn't Help the Adversary

| Observable | Helps recover nsec? | Why |
|---|---|---|
| The d-tag | No | One-way function output (scrypt → HKDF) |
| The throwaway pubkey | No | One-way (scrypt → HKDF) + discrete log |
| The ciphertext | No | AEAD-encrypted with password-derived key |
| Timing / IP metadata | No | Identifies the blob, not the password |
| Knowledge that this is a backup | No | Still need the password to test a guess |

### Post-Quantum Considerations

The **confidentiality** of the nsec depends on three primitives:

| Primitive | Role | Quantum resistance |
|---|---|---|
| scrypt | Password → root key | Hash-based. Grover's gives √ speedup on search. 128-bit entropy → ~64-bit quantum security. |
| HKDF-SHA256 | Root key → derived keys | Hash-based. No known quantum shortcut beyond Grover's on preimage. |
| XChaCha20-Poly1305 | Symmetric encryption | 256-bit key → 128-bit quantum security. |

The secp256k1 throwaway signing key is NOT post-quantum — Shor's algorithm breaks ECDLP in polynomial time. However, the throwaway key is a Nostr protocol requirement (NIP-01 event signing), not a confidentiality mechanism. If a quantum adversary recovers the throwaway signing secret, they learn a value already derivable from the password. The nsec remains protected by the symmetric/hash-based chain.

**What a quantum adversary CAN do:** forge or delete the throwaway-signed event (break authenticity/availability). **What they CANNOT do:** recover the nsec without brute-forcing the password (confidentiality holds).

The 128-bit entropy floor ensures ≥ 64-bit quantum security for the password search. Implementations that require stronger post-quantum margins SHOULD use 12+ word passphrases (~132 bits with BIP-39 → ~66-bit quantum security).

## Memory Safety

Implementations MUST zero sensitive memory after use: `password`, `pw_bytes`, `nsec_bytes`, `H`, `enc_key`, `sign_key`, and any intermediate key material. Implementations SHOULD use a dedicated zeroing primitive (e.g., `zeroize` in Rust) rather than relying on garbage collection.

## Encoding Conventions

- **Strings to bytes:** UTF-8.
- **Concatenation (‖):** Raw byte concatenation, no delimiters.
- **`pubkey_bytes`:** 32-byte raw x-only public key per BIP-340. NOT hex-encoded.
- **`to_string(i)`:** ASCII decimal, no leading zeros. Examples: `"0"`, `"1"`, `"255"`.
- **`relay_url_bytes`:** UTF-8 encoding of the normalized relay URL.
- **Hex:** Lowercase, no `0x` prefix.
- **HKDF:** HKDF-SHA256 per RFC 5869 (extract-then-expand). All calls in this spec request ≤ 32 bytes, so only one HMAC-SHA256 block is needed for expansion.

## Implementation Notes

### Rust

- `scrypt` crate — `scrypt::scrypt()`
- `hkdf` crate — `Hkdf::<Sha256>::new()`
- `chacha20poly1305` crate — `XChaCha20Poly1305`
- `zeroize` crate — zero sensitive memory
- `unicode-normalization` crate — NFKC
- `url` crate — WHATWG URL normalization

### TypeScript

- `@noble/hashes/scrypt` — `scrypt()`
- `@noble/hashes/hkdf` — `hkdf(sha256, ...)`
- `@noble/ciphers/chacha` — `xchacha20poly1305()`
- `String.prototype.normalize('NFKC')` — password normalization
- `new URL()` — WHATWG URL normalization

### Relay Requirements

No special relay support needed. Standard NIP-33 behavior:
- Accept `kind:30078` events
- Store events from unknown pubkeys (throwaway keys have no profile)
- Support `authors` + `#d` filtering in REQ subscriptions

## Mental Model

```
password + pubkey + relay_url
         │
         ▼
      scrypt (1 call, ~1 GiB RAM)
         │
    ┌────┴────────────────┐
    │                     │
    ▼                     ▼
  enc_key          d_tag + sign_key
  (global)         (relay-scoped)
    │                     │
    ▼                     ▼
  encrypt nsec      publish kind:30078
  (32 → 72 bytes)   throwaway identity
```

Recovery: password + pubkey + relay URL → re-derive → query exact address → decrypt → verify pubkey.

## Test Vector

Implementations MUST reproduce these values exactly. Any deviation indicates a bug in NFKC normalization, base construction, scrypt invocation, HKDF derivation, or AEAD encryption.

```
Inputs:
  password:            "correct horse battery staple orange purple mountain daisy trumpet bicycle"
  pw_bytes (hex):      636f727265637420686f727365206261747465727920737461706c65
                       206f72616e676520707572706c65206d6f756e7461696e2064616973
                       79207472756d7065742062696379636c65
  pw_bytes length:     73
  nsec (hex):          0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
  pubkey (hex):        4646ae5047316b4230d0086c8acec687f00b1cd9d1dc634f6cb358ac0a9a8fff
  relay_url:           wss://relay.example.com/
  scrypt:              log_n=20, r=8, p=1, salt="nip-pb/v1/root"

Base construction:
  len_prefix:          0049  (73 bytes)
  base (hex):          0049636f727265637420686f727365206261747465727920737461706c65
                       206f72616e676520707572706c65206d6f756e7461696e2064616973
                       79207472756d7065742062696379636c65
                       4646ae5047316b4230d0086c8acec687f00b1cd9d1dc634f6cb358ac0a9a8fff

Step 1 — Root key:
  H = scrypt(base, salt="nip-pb/v1/root")
    = d0383c9ef44e9081b30c9334c891d47877708e097d1b852c62c9d2e1c28f7d22

Step 2 — Derived keys:
  enc_key              = 2d65ad598c7d0229319250662c0896432400de36fc35b51f68b206ef0baa99ba
  d_tag                = 730fe439d8d414a230e15bae1ef36604ba0785cf0ccf97266b00e2b36ce0b1ce
  sign_skm             = 331316410b52c2264517bc17f0103d278a90202a7731daeff693b97dbcec8fdf
  sign_key             = bead7b2b832dc3bac2a29916bde0a51c0b37f39cb448c33bb9abb0a80bc63b2b
  sign_pk              = 5e9819c27fa5325d8004e16c697ae1095cea83e6afc9d500afebeb66958ba9bb

Step 3 — Encrypt (with fixed nonce for reproducibility):
  nonce (hex):         000102030405060708090a0b0c0d0e0f1011121314151617
  ciphertext (hex):    3b75519ccb6beded2e7b2765aff77a6a43fe45bde6e138d9
                       fcdd5c3b150b871d6f17904036be48dea2902f45908aff3a
  content (base64):    AAECAwQFBgcICQoLDA0ODxAREhMUFRYXO3VRnMtr7e0ueydlr/d6akP+Rb3m
                       4TjZ/N1cOxULhx1vF5BANr5I3qKQL0WQiv86
  content length:      96 characters (72 decoded bytes)
```

Note: the fixed nonce `000102...1617` is for test vector reproducibility only. Real implementations MUST use fresh random 24-byte nonces.

## References

- [NIP-01](https://github.com/nostr-protocol/nips/blob/master/01.md) — Event structure, signatures
- [NIP-09](https://github.com/nostr-protocol/nips/blob/master/09.md) — Event deletion
- [NIP-31](https://github.com/nostr-protocol/nips/blob/master/31.md) — Alt tag
- [NIP-33](https://github.com/nostr-protocol/nips/blob/master/33.md) — Parameterized replaceable events
- [NIP-49](https://github.com/nostr-protocol/nips/blob/master/49.md) — Encrypted private key export
- [NIP-78](https://github.com/nostr-protocol/nips/blob/master/78.md) — Application-specific data
- [BIP-340](https://github.com/bitcoin/bips/blob/master/bip-0340.mediawiki) — Schnorr signatures for secp256k1
- [RFC 5869](https://www.rfc-editor.org/rfc/rfc5869) — HKDF
- [RFC 7914](https://www.rfc-editor.org/rfc/rfc7914) — scrypt
- [XChaCha20-Poly1305](https://datatracker.ietf.org/doc/html/draft-irtf-cfrg-xchacha) — Extended-nonce ChaCha20-Poly1305
