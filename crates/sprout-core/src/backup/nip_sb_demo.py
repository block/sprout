#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["PyNaCl>=1.5", "secp256k1>=0.14"]
# ///
"""
NIP-SB v3 Steganographic Key Backup — Protocol Demo

Exercises the full NIP-SB v3 backup/recovery cycle with real crypto:
  - scrypt (hashlib, stdlib — log_n reduced to 14 for demo speed)
  - HKDF-SHA256 (hmac, stdlib)
  - XChaCha20-Poly1305 (libsodium via PyNaCl)
  - secp256k1 key derivation (secp256k1 lib)
  - Reed-Solomon erasure coding over GF(2^8) (pure Python)

v3 additions over v1:
  - P=2 Reed-Solomon parity blobs (tolerates loss of any 2 blobs)
  - D=4-12 variable dummy blobs (encrypted random garbage)
  - Cover key for cheap dummy derivation (1 scrypt, rest HKDF)
  - Random-order publication and recovery
  - d-tag-only queries (no authors filter)

The relay is simulated as an in-memory dict. Nostr event structure
(kind, id, sig) is not modeled — this demo covers the cryptographic
protocol, not the Nostr event layer.

Simplifications vs. a full implementation:
  - scrypt log_n=14 (spec requires 20) for demo speed
  - No Nostr event id/sig generation or validation
  - Simulated relay (dict) instead of real WebSocket relay
  - No jittered timestamps or publication delays

Usage:
    uv run crates/sprout-core/src/backup/nip_sb_demo.py
"""

from __future__ import annotations

import base64
import hashlib
import hmac
import os
import random
import sys
import unicodedata
from dataclasses import dataclass

import nacl.bindings as sodium
import secp256k1

# ── NIP-SB Constants (spec §Constants) ────────────────────────────────────────

SCRYPT_LOG_N = 14       # Reduced from spec's 20 for demo speed (~0.1s vs ~2s).
                        # Real implementations MUST use 20.
SCRYPT_R = 8
SCRYPT_P = 1
MIN_CHUNKS = 3
MAX_CHUNKS = 16
CHUNK_RANGE = MAX_CHUNKS - MIN_CHUNKS + 1  # 14
PARITY_BLOBS = 2
MIN_DUMMIES = 4
MAX_DUMMIES = 12
DUMMY_RANGE = MAX_DUMMIES - MIN_DUMMIES + 1  # 9
CHUNK_PAD_LEN = 16
AAD = b"\x02"           # key_security_byte per NIP-49


# ── GF(2^8) arithmetic for Reed-Solomon ───────────────────────────────────────
# Field: GF(2^8) with irreducible polynomial x^8+x^4+x^3+x+1 (0x11B, AES).
# Primitive element α = 0x03 (order 255, generates full multiplicative group).

GF_POLY = 0x11B

def gf_mul(a: int, b: int) -> int:
    """Multiply two elements in GF(2^8)."""
    p = 0
    for _ in range(8):
        if b & 1:
            p ^= a
        hi = a & 0x80
        a = (a << 1) & 0xFF
        if hi:
            a ^= GF_POLY & 0xFF
        b >>= 1
    return p

def gf_pow(a: int, n: int) -> int:
    """Exponentiate in GF(2^8)."""
    result = 1
    base = a
    while n > 0:
        if n & 1:
            result = gf_mul(result, base)
        base = gf_mul(base, base)
        n >>= 1
    return result

def gf_inv(a: int) -> int:
    """Multiplicative inverse in GF(2^8). a^254 = a^(-1) since a^255 = 1."""
    assert a != 0, "Cannot invert zero"
    return gf_pow(a, 254)

# Precompute evaluation points: α^0, α^1, ..., α^(MAX_CHUNKS+PARITY_BLOBS-1)
ALPHA = 0x03
EVAL_POINTS = [gf_pow(ALPHA, i) for i in range(MAX_CHUNKS + PARITY_BLOBS)]


def rs_encode(data_symbols: list[int], n_parity: int = 2) -> list[int]:
    """
    Systematic RS encode: given N data symbols, produce n_parity parity symbols.
    Uses Lagrange interpolation at evaluation points α^0..α^{N-1} for data,
    then evaluates at α^N..α^{N+n_parity-1} for parity.
    All arithmetic in GF(2^8).
    """
    n = len(data_symbols)
    points = EVAL_POINTS[:n]
    parity = []
    for k in range(n_parity):
        x = EVAL_POINTS[n + k]
        # Lagrange interpolation: P(x) = sum_i data[i] * prod_{j!=i} (x - points[j]) / (points[i] - points[j])
        val = 0
        for i in range(n):
            num = data_symbols[i]
            for j in range(n):
                if j != i:
                    num = gf_mul(num, x ^ points[j])
                    num = gf_mul(num, gf_inv(points[i] ^ points[j]))
            val ^= num
        parity.append(val)
    return parity


def rs_decode(symbols: list[int | None], n_data: int) -> list[int]:
    """
    RS erasure decode: given n_data+2 symbol slots (some None = erased),
    reconstruct all n_data data symbols using any n_data available symbols.
    Returns the n_data data symbols.
    """
    n_total = n_data + PARITY_BLOBS
    assert len(symbols) == n_total

    # Collect known positions and values
    known_pos = []
    known_val = []
    for i, s in enumerate(symbols):
        if s is not None:
            known_pos.append(EVAL_POINTS[i])
            known_val.append(s)

    assert len(known_pos) >= n_data, f"Need at least {n_data} symbols, got {len(known_pos)}"

    # Use first n_data known symbols for interpolation
    pos = known_pos[:n_data]
    val = known_val[:n_data]

    # Reconstruct data symbols by evaluating polynomial at data positions
    result = []
    for k in range(n_data):
        x = EVAL_POINTS[k]
        # Check if this position is already known
        found = False
        for i, s in enumerate(symbols):
            if i == k and s is not None:
                result.append(s)
                found = True
                break
        if found:
            continue
        # Lagrange interpolation at x using known points
        v = 0
        for i in range(n_data):
            num = val[i]
            for j in range(n_data):
                if j != i:
                    num = gf_mul(num, x ^ pos[j])
                    num = gf_mul(num, gf_inv(pos[i] ^ pos[j]))
            v ^= num
        result.append(v)
    return result


def rs_encode_rows(padded_chunks: list[bytes]) -> tuple[bytes, bytes]:
    """
    Compute 2 parity rows across N padded chunks using 16 parallel RS codes.
    Each byte position gets its own RS(N+2, N) code over GF(2^8).
    Returns (parity_row_0, parity_row_1), each 16 bytes.
    """
    n = len(padded_chunks)
    parity_0 = bytearray(CHUNK_PAD_LEN)
    parity_1 = bytearray(CHUNK_PAD_LEN)
    for b in range(CHUNK_PAD_LEN):
        data = [padded_chunks[i][b] for i in range(n)]
        p = rs_encode(data, PARITY_BLOBS)
        parity_0[b] = p[0]
        parity_1[b] = p[1]
    return bytes(parity_0), bytes(parity_1)


def rs_decode_rows(
    padded_slots: list[bytes | None],
    n_data: int,
) -> list[bytes]:
    """
    RS erasure decode across 16 parallel byte positions.
    padded_slots has n_data + 2 entries (real + parity), some may be None.
    Returns the n_data reconstructed padded chunks.
    """
    n_total = n_data + PARITY_BLOBS
    assert len(padded_slots) == n_total
    result = [bytearray(CHUNK_PAD_LEN) for _ in range(n_data)]
    for b in range(CHUNK_PAD_LEN):
        symbols: list[int | None] = []
        for i in range(n_total):
            if padded_slots[i] is None:
                symbols.append(None)
            else:
                symbols.append(padded_slots[i][b])
        decoded = rs_decode(symbols, n_data)
        for i in range(n_data):
            result[i][b] = decoded[i]
    return [bytes(r) for r in result]


# ── Simulated Relay ───────────────────────────────────────────────────────────

@dataclass
class RelayEvent:
    pubkey: str     # throwaway signing pubkey (hex, 32 bytes x-only)
    d_tag: str      # NIP-33 d-tag (hex, 32 bytes)
    content: str    # base64-encoded blob (56 bytes: 24 nonce + 32 ciphertext)

SimulatedRelay = dict[str, list[RelayEvent]]

def relay_publish(relay: SimulatedRelay, event: RelayEvent) -> None:
    relay.setdefault(event.d_tag, []).append(event)

def relay_query(relay: SimulatedRelay, d_tag: str) -> list[RelayEvent]:
    """Query by d-tag only (v3: no authors filter)."""
    return relay.get(d_tag, [])


# ── Crypto helpers ────────────────────────────────────────────────────────────

def nfkc(password: str) -> bytes:
    return unicodedata.normalize("NFKC", password).encode("utf-8")

def nip_sb_scrypt(input_bytes: bytes, salt: bytes = b"") -> bytes:
    return hashlib.scrypt(
        input_bytes, salt=salt,
        n=2**SCRYPT_LOG_N, r=SCRYPT_R, p=SCRYPT_P, dklen=32,
    )

def nip_sb_hkdf(ikm: bytes, info: bytes, length: int = 32) -> bytes:
    prk = hmac.new(b"\x00" * 32, ikm, "sha256").digest()
    return hmac.new(prk, info + b"\x01", "sha256").digest()[:length]

def xchacha20poly1305_encrypt(key: bytes, nonce: bytes, plaintext: bytes, aad: bytes) -> bytes:
    return sodium.crypto_aead_xchacha20poly1305_ietf_encrypt(plaintext, aad, nonce, key)

def xchacha20poly1305_decrypt(key: bytes, nonce: bytes, ciphertext: bytes, aad: bytes) -> bytes:
    return sodium.crypto_aead_xchacha20poly1305_ietf_decrypt(ciphertext, aad, nonce, key)

def secret_to_pubkey(secret_bytes: bytes) -> bytes:
    sk = secp256k1.PrivateKey(secret_bytes)
    return sk.pubkey.serialize(compressed=True)[1:]


# ── Backup (spec §Steps 1-5) ─────────────────────────────────────────────────

@dataclass
class BlobInfo:
    index: int
    role: str       # "real", "parity", "dummy"
    d_tag: str
    sign_pk: str

def backup(
    nsec_bytes: bytes,
    pubkey_bytes: bytes,
    password: str,
    relay: SimulatedRelay,
) -> list[BlobInfo]:
    base = nfkc(password) + pubkey_bytes

    # Step 1: Determine N and D
    h = nip_sb_scrypt(base, salt=b"")
    n = (h[0] % CHUNK_RANGE) + MIN_CHUNKS
    h_d = nip_sb_scrypt(base, salt=b"dummies")
    d = (h_d[0] % DUMMY_RANGE) + MIN_DUMMIES
    p = PARITY_BLOBS

    # Step 2: Master encryption key
    h_enc = nip_sb_scrypt(base, salt=b"encrypt")
    enc_key = nip_sb_hkdf(h_enc, b"key")

    # Step 3: Split nsec into N chunks
    remainder = 32 % n
    base_len = 32 // n
    chunks: list[bytes] = []
    offset = 0
    for i in range(n):
        chunk_len = base_len + (1 if i < remainder else 0)
        chunks.append(nsec_bytes[offset : offset + chunk_len])
        offset += chunk_len
    assert offset == 32 and b"".join(chunks) == nsec_bytes

    # Step 3b: Pad chunks and compute RS parity
    padded_chunks: list[bytes] = []
    for i in range(n):
        padded = chunks[i] + os.urandom(CHUNK_PAD_LEN - len(chunks[i]))
        padded_chunks.append(padded)
    parity_row_0, parity_row_1 = rs_encode_rows(padded_chunks)

    # Step 3c: Cover key for dummy blobs
    h_cover = nip_sb_scrypt(base, salt=b"cover")

    # Step 4 + 5: Derive keys, encrypt, collect all blobs
    all_blobs: list[tuple[BlobInfo, RelayEvent]] = []

    # Real chunk blobs (indices 0..N-1)
    for i in range(n):
        base_i = nfkc(password) + pubkey_bytes + str(i).encode("ascii")
        h_i = nip_sb_scrypt(base_i, salt=b"")
        d_tag = nip_sb_hkdf(h_i, b"d-tag").hex()
        sign_sk = _derive_signing_key(h_i, b"signing-key")
        sign_pk = secret_to_pubkey(sign_sk).hex()

        nonce = os.urandom(24)
        ct = xchacha20poly1305_encrypt(enc_key, nonce, padded_chunks[i], AAD)
        content = base64.b64encode(nonce + ct).decode("ascii")

        info = BlobInfo(i, "real", d_tag, sign_pk)
        event = RelayEvent(pubkey=sign_pk, d_tag=d_tag, content=content)
        all_blobs.append((info, event))

    # Parity blobs (indices N..N+1)
    parity_rows = [parity_row_0, parity_row_1]
    for k in range(p):
        i = n + k
        base_i = nfkc(password) + pubkey_bytes + str(i).encode("ascii")
        h_i = nip_sb_scrypt(base_i, salt=b"")
        d_tag = nip_sb_hkdf(h_i, b"d-tag").hex()
        sign_sk = _derive_signing_key(h_i, b"signing-key")
        sign_pk = secret_to_pubkey(sign_sk).hex()

        nonce = os.urandom(24)
        ct = xchacha20poly1305_encrypt(enc_key, nonce, parity_rows[k], AAD)
        content = base64.b64encode(nonce + ct).decode("ascii")

        info = BlobInfo(i, "parity", d_tag, sign_pk)
        event = RelayEvent(pubkey=sign_pk, d_tag=d_tag, content=content)
        all_blobs.append((info, event))

    # Dummy blobs (indices 0..D-1, separate namespace)
    for j in range(d):
        d_tag = nip_sb_hkdf(h_cover, f"dummy-d-tag-{j}".encode()).hex()
        sign_sk = _derive_dummy_signing_key(h_cover, j)
        sign_pk = secret_to_pubkey(sign_sk).hex()

        dummy_payload = os.urandom(CHUNK_PAD_LEN)
        nonce = os.urandom(24)
        ct = xchacha20poly1305_encrypt(enc_key, nonce, dummy_payload, AAD)
        content = base64.b64encode(nonce + ct).decode("ascii")

        info = BlobInfo(n + p + j, "dummy", d_tag, sign_pk)
        event = RelayEvent(pubkey=sign_pk, d_tag=d_tag, content=content)
        all_blobs.append((info, event))

    # Shuffle and publish in random order (spec: MUST shuffle)
    random.shuffle(all_blobs)
    blob_infos = []
    for info, event in all_blobs:
        relay_publish(relay, event)
        blob_infos.append(info)

    # Sort for display (publication was shuffled)
    blob_infos.sort(key=lambda b: ({"real": 0, "parity": 1, "dummy": 2}[b.role], b.index))
    return blob_infos


def _derive_signing_key(h_i: bytes, prefix: bytes) -> bytes:
    """Reject-and-retry signing key derivation (spec §Step 4)."""
    for retry in range(256):
        info = prefix if retry == 0 else prefix + f"-{retry}".encode()
        sk = nip_sb_hkdf(h_i, info)
        try:
            secret_to_pubkey(sk)  # validates scalar
            return sk
        except Exception:
            continue
    raise RuntimeError("All 256 signing key derivations invalid")


def _derive_dummy_signing_key(h_cover: bytes, j: int) -> bytes:
    """Reject-and-retry for dummy signing keys (spec §Step 4, dummy section)."""
    for retry in range(256):
        suffix = f"-{retry}" if retry > 0 else ""
        info = f"dummy-signing-key-{j}{suffix}".encode()
        sk = nip_sb_hkdf(h_cover, info)
        try:
            secret_to_pubkey(sk)
            return sk
        except Exception:
            continue
    raise RuntimeError(f"Dummy {j}: all 256 signing key derivations invalid")


# ── Recovery (spec §Recovery) ─────────────────────────────────────────────────

def recover(
    pubkey_bytes: bytes,
    password: str,
    relay: SimulatedRelay,
    delete_indices: set[int] | None = None,
) -> bytes:
    """
    Recover nsec from password + pubkey + relay.
    delete_indices: if set, simulate missing blobs by skipping these real/parity indices.
    """
    base = nfkc(password) + pubkey_bytes

    # Step 3: Derive N, D, enc_key, cover key
    h = nip_sb_scrypt(base, salt=b"")
    n = (h[0] % CHUNK_RANGE) + MIN_CHUNKS
    h_d = nip_sb_scrypt(base, salt=b"dummies")
    d = (h_d[0] % DUMMY_RANGE) + MIN_DUMMIES
    p = PARITY_BLOBS
    h_enc = nip_sb_scrypt(base, salt=b"encrypt")
    enc_key = nip_sb_hkdf(h_enc, b"key")
    h_cover = nip_sb_scrypt(base, salt=b"cover")

    remainder = 32 % n
    base_len = 32 // n

    # Step 4: Derive all d-tags and expected signing pubkeys
    all_queries: list[tuple[str, str, str, int]] = []  # (d_tag, expected_pk, role, index)

    for i in range(n + p):
        base_i = nfkc(password) + pubkey_bytes + str(i).encode("ascii")
        h_i = nip_sb_scrypt(base_i, salt=b"")
        d_tag = nip_sb_hkdf(h_i, b"d-tag").hex()
        sign_sk = _derive_signing_key(h_i, b"signing-key")
        sign_pk = secret_to_pubkey(sign_sk).hex()
        role = "real" if i < n else "parity"
        all_queries.append((d_tag, sign_pk, role, i))

    for j in range(d):
        d_tag = nip_sb_hkdf(h_cover, f"dummy-d-tag-{j}".encode()).hex()
        sign_sk = _derive_dummy_signing_key(h_cover, j)
        sign_pk = secret_to_pubkey(sign_sk).hex()
        all_queries.append((d_tag, sign_pk, "dummy", n + p + j))

    # Step 5: Shuffle and query all d-tags (spec: random order, d-tag only)
    random.shuffle(all_queries)

    # Collect results by role
    padded_slots: list[bytes | None] = [None] * (n + p)  # real + parity
    for d_tag, expected_pk, role, idx in all_queries:
        if delete_indices and idx < (n + p) and idx in delete_indices:
            continue  # simulate missing blob

        events = relay_query(relay, d_tag)
        matched = [e for e in events if e.pubkey == expected_pk]

        if role == "dummy":
            continue  # discard dummies

        if not matched:
            continue  # missing blob — will try RS recovery

        event = matched[0]
        content = event.content
        # Spec §Event Validation steps 6-7: validate content and decrypt
        try:
            content = event.content
            if len(content) % 4:
                content += "=" * (4 - len(content) % 4)
            raw = base64.b64decode(content)
            if len(raw) != 56:
                # Malformed content → treat as erasure (spec §Event Validation step 6)
                continue
            nonce = raw[:24]
            ciphertext = raw[24:]
            padded = xchacha20poly1305_decrypt(enc_key, nonce, ciphertext, AAD)
        except Exception:
            # Base64 decode failure or AEAD failure → treat as erasure
            # (spec §Event Validation steps 6-7)
            continue
        padded_slots[idx] = padded

    # Step 8: Reassemble
    missing = [i for i in range(n + p) if padded_slots[i] is None]
    if len(missing) > p:
        raise ValueError(f"Too many blobs missing ({len(missing)} missing, max tolerated: {p})")

    if missing:
        # RS erasure decode
        reconstructed = rs_decode_rows(padded_slots, n)
        for i in range(n):
            padded_slots[i] = reconstructed[i]

    # Extract chunks from padded data
    nsec_parts = []
    for i in range(n):
        chunk_len = base_len + (1 if i < remainder else 0)
        nsec_parts.append(padded_slots[i][:chunk_len])

    nsec_bytes = b"".join(nsec_parts)
    assert len(nsec_bytes) == 32

    # Step 9: Validate
    try:
        recovered_pk = secret_to_pubkey(nsec_bytes)
    except Exception:
        raise ValueError("Recovered key is not a valid secp256k1 scalar")
    if recovered_pk != pubkey_bytes:
        raise ValueError("Pubkey mismatch — wrong password")

    return nsec_bytes


# ── Main ──────────────────────────────────────────────────────────────────────

def main() -> None:
    print("╔══════════════════════════════════════════════════════════════╗")
    print("║  NIP-SB v3 Protocol Demo — Real Crypto, Simulated Relay      ║")
    print("║                                                              ║")
    print("║  scrypt + HKDF-SHA256 + XChaCha20-Poly1305 + secp256k1       ║")
    print("║  + Reed-Solomon GF(2^8) + Dummy Blobs                        ║")
    print("╚══════════════════════════════════════════════════════════════╝")
    print()

    relay: SimulatedRelay = {}

    # Generate a test identity
    sk = secp256k1.PrivateKey()
    nsec_bytes = sk.private_key
    pubkey_bytes = secret_to_pubkey(nsec_bytes)
    password = "correct-horse-battery-staple-orange-purple-mountain"

    print(f"Identity:  {pubkey_bytes.hex()[:16]}…")
    print(f"Password:  {password}")
    print()

    # ── Phase 1: Backup ───────────────────────────────────────────────────

    print("── Phase 1: Backup ──────────────────────────────────────────")
    blobs = backup(nsec_bytes, pubkey_bytes, password, relay)
    n_real = sum(1 for b in blobs if b.role == "real")
    n_parity = sum(1 for b in blobs if b.role == "parity")
    n_dummy = sum(1 for b in blobs if b.role == "dummy")
    print(f"   N={n_real} real + P={n_parity} parity + D={n_dummy} dummy = {len(blobs)} total")
    for b in blobs:
        print(f"   Blob {b.index:2d} [{b.role:6s}]: d={b.d_tag[:12]}… pk={b.sign_pk[:12]}… ✅")

    # Add decoy events (simulates other kind:30078 data)
    for _ in range(5):
        fake_sk = secp256k1.PrivateKey()
        relay_publish(relay, RelayEvent(
            pubkey=secret_to_pubkey(fake_sk.private_key).hex(),
            d_tag=os.urandom(32).hex(),
            content=base64.b64encode(os.urandom(56)).decode(),
        ))

    total = sum(len(v) for v in relay.values())
    print(f"\n   Relay: {total} total events ({len(blobs)} backup + 5 decoy)")

    # ── Phase 2: Full Recovery ────────────────────────────────────────────

    print("\n── Phase 2: Full Recovery (all blobs present) ────────────────")
    recovered = recover(pubkey_bytes, password, relay)
    assert recovered == nsec_bytes
    print(f"   ✅ RECOVERED — secret key matches (byte-for-byte)")

    # ── Phase 3: Recovery with 1 Missing Blob ─────────────────────────────

    print("\n── Phase 3: Recovery with 1 missing real chunk ───────────────")
    recovered = recover(pubkey_bytes, password, relay, delete_indices={0})
    assert recovered == nsec_bytes
    print(f"   ✅ RECOVERED — RS parity reconstructed chunk 0")

    # ── Phase 4: Recovery with 2 Missing Blobs ────────────────────────────

    print("\n── Phase 4: Recovery with 2 missing blobs (1 real + 1 parity)")
    recovered = recover(pubkey_bytes, password, relay, delete_indices={1, n_real})
    assert recovered == nsec_bytes
    print(f"   ✅ RECOVERED — RS parity reconstructed mixed erasures")

    # ── Phase 4b: Recovery with 2 Missing Real Chunks ────────────────────

    print("\n── Phase 4b: Recovery with 2 missing real chunks ─────────────")
    recovered = recover(pubkey_bytes, password, relay, delete_indices={0, n_real - 1})
    assert recovered == nsec_bytes
    print(f"   ✅ RECOVERED — RS parity reconstructed 2 missing real chunks")

    # ── Phase 4c: Recovery with 2 Missing Parity Blobs ────────────────────

    print("\n── Phase 4c: Recovery with 2 missing parity blobs ────────────")
    recovered = recover(pubkey_bytes, password, relay, delete_indices={n_real, n_real + 1})
    assert recovered == nsec_bytes
    print(f"   ✅ RECOVERED — all real chunks present, parity not needed")

    # ── Phase 4d: Recovery with corrupted blob (AEAD failure → erasure) ──

    print("\n── Phase 4d: Recovery with 1 corrupted blob (AEAD erasure) ───")
    # Corrupt a real blob's content to trigger AEAD failure
    real_blobs = [b for b in blobs if b.role == "real"]
    target_tag = real_blobs[0].d_tag
    original_content = relay[target_tag][0].content
    relay[target_tag][0].content = base64.b64encode(os.urandom(56)).decode()
    recovered = recover(pubkey_bytes, password, relay)
    assert recovered == nsec_bytes
    relay[target_tag][0].content = original_content  # restore
    print(f"   ✅ RECOVERED — AEAD failure treated as erasure, RS reconstructed")

    # ── Phase 5: Recovery with 3 Missing (should fail) ────────────────────

    print("\n── Phase 5: Recovery with 3 missing blobs (should fail) ──────")
    try:
        recover(pubkey_bytes, password, relay, delete_indices={0, 1, 2})
        print("   ❌ UNEXPECTED SUCCESS")
        sys.exit(1)
    except ValueError as e:
        print(f"   ✅ Correctly rejected: {e}")

    # ── Phase 6: Wrong Password ───────────────────────────────────────────

    print("\n── Phase 6: Wrong Password ──────────────────────────────────")
    try:
        recover(pubkey_bytes, "wrong-password-totally-different-words", relay)
        print("   ❌ UNEXPECTED SUCCESS")
        sys.exit(1)
    except ValueError as e:
        print(f"   ✅ Correctly rejected: {e}")

    # ── Phase 7: Different User, Same Password ────────────────────────────

    print("\n── Phase 7: Different User, Same Password ───────────────────")
    other_sk = secp256k1.PrivateKey()
    other_pk = secret_to_pubkey(other_sk.private_key)
    try:
        recover(other_pk, password, relay)
        print("   ❌ UNEXPECTED SUCCESS")
        sys.exit(1)
    except ValueError as e:
        print(f"   ✅ Correctly rejected: {e}")

    # ── Phase 8: What an Attacker Sees ────────────────────────────────────

    print("\n── Phase 8: What an Attacker Sees (relay dump) ──────────────")
    backup_tags = {b.d_tag for b in blobs}
    for events in relay.values():
        for evt in events:
            label = ""
            if evt.d_tag in backup_tags:
                role = next((b.role for b in blobs if b.d_tag == evt.d_tag), "?")
                label = f" ← {role.upper()}"
            print(f"   pk={evt.pubkey[:12]}…  d={evt.d_tag[:12]}…  "
                  f"content={evt.content[:16]}…{label}")
    print(f"\n   {len(blobs)} backup + 5 decoy = {total} total")
    print(f"   Labels are only visible because this demo knows the password.")
    print(f"   An attacker cannot distinguish real/parity/dummy/decoy.")

    # ── Phase 9: RS Test Vectors ──────────────────────────────────────────

    print("\n── Phase 9: RS Test Vectors ─────────────────────────────────")
    # GF(2^8) arithmetic verification (NORMATIVE — spec §Test Vectors)
    assert gf_mul(0x03, 0x03) == 0x05, f"gf_mul(0x03,0x03)={hex(gf_mul(0x03,0x03))}"
    assert gf_mul(0x03, 0x05) == 0x0F, f"gf_mul(0x03,0x05)={hex(gf_mul(0x03,0x05))}"
    assert gf_mul(0x57, 0x83) == 0xC1, f"gf_mul(0x57,0x83)={hex(gf_mul(0x57,0x83))}"
    assert gf_inv(0x03) == 0xF6, f"gf_inv(0x03)={hex(gf_inv(0x03))}"
    assert gf_mul(0x03, 0xF6) == 0x01, "gf_mul(0x03,0xF6) should be 0x01"
    print(f"   ✅ GF(2^8) multiplication vectors match spec")

    # Verify α=0x03 is primitive in GF(2^8) under 0x11B
    x = 1
    for i in range(1, 256):
        x = gf_mul(x, ALPHA)
        if x == 1:
            assert i == 255, f"α=0x03 has order {i}, expected 255"
            break
    print(f"   ✅ α=0x03 is primitive (order 255 in GF(2^8)/0x11B)")

    # Small RS test: 3 data symbols, 2 parity (NORMATIVE — spec §Test Vectors)
    test_data = [0x42, 0xAB, 0x07]
    test_parity = rs_encode(test_data, 2)
    assert test_parity == [0x62, 0x59], f"RS parity mismatch: {[hex(p) for p in test_parity]}"
    print(f"   RS encode [0x42, 0xAB, 0x07] → parity {[hex(p) for p in test_parity]}")
    print(f"   ✅ RS parity matches normative vector [0x62, 0x59]")

    # Verify decode with no erasures
    full = test_data + test_parity
    decoded = rs_decode([full[0], full[1], full[2], full[3], full[4]], 3)
    assert decoded == test_data
    print(f"   ✅ RS decode (no erasures): {[hex(d) for d in decoded]}")

    # Verify decode with 1 erasure (position 1)
    erased1 = [full[0], None, full[2], full[3], full[4]]
    decoded1 = rs_decode(erased1, 3)
    assert decoded1 == test_data
    print(f"   ✅ RS decode (1 erasure at pos 1): {[hex(d) for d in decoded1]}")

    # Verify decode with 2 erasures (positions 0 and 2)
    erased2 = [None, full[1], None, full[3], full[4]]
    decoded2 = rs_decode(erased2, 3)
    assert decoded2 == test_data
    print(f"   ✅ RS decode (2 erasures at pos 0,2): {[hex(d) for d in decoded2]}")

    # Verify decode with mixed erasure (1 data + 1 parity)
    erased3 = [full[0], None, full[2], None, full[4]]
    decoded3 = rs_decode(erased3, 3)
    assert decoded3 == test_data
    print(f"   ✅ RS decode (mixed: data pos 1 + parity pos 3): {[hex(d) for d in decoded3]}")

    print()
    print("╔══════════════════════════════════════════════════════════════╗")
    print("║                      ALL TESTS PASSED                        ║")
    print("╚══════════════════════════════════════════════════════════════╝")


if __name__ == "__main__":
    main()
