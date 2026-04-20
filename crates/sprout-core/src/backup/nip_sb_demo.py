#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["PyNaCl>=1.5", "secp256k1>=0.14"]
# ///
"""
NIP-SB Steganographic Key Backup — Protocol Demo

Exercises the full NIP-SB backup/recovery cycle with real crypto:
  - scrypt (hashlib, stdlib — log_n reduced to 14 for demo speed)
  - HKDF-SHA256 (hmac, stdlib)
  - XChaCha20-Poly1305 (libsodium via PyNaCl)
  - secp256k1 key derivation (secp256k1 lib)

The relay is simulated as an in-memory dict. Everything else follows
the NIP-SB spec exactly.

Usage:
    uv run crates/sprout-core/src/backup/nip_sb_demo.py
"""

from __future__ import annotations

import base64
import hashlib
import hmac
import os
import sys
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
CHUNK_PAD_LEN = 16
AAD = b"\x02"           # key_security_byte per NIP-49

# ── Simulated Relay ───────────────────────────────────────────────────────────
#
# In the real protocol, blobs are kind:30078 Nostr events on a relay.
# Here we simulate the relay as a dict keyed by d_tag.
# The relay stores opaque blobs — it has no idea what's inside them.


@dataclass
class RelayEvent:
    pubkey: str     # throwaway signing pubkey (hex, 32 bytes x-only)
    d_tag: str      # NIP-33 d-tag (hex, 32 bytes)
    content: str    # base64-encoded blob (56 bytes: 24 nonce + 32 ciphertext)


# d_tag → list of events (multiple pubkeys can share a d_tag in theory)
SimulatedRelay = dict[str, list[RelayEvent]]


def relay_publish(relay: SimulatedRelay, event: RelayEvent) -> None:
    relay.setdefault(event.d_tag, []).append(event)


def relay_query(relay: SimulatedRelay, d_tag: str) -> list[RelayEvent]:
    return relay.get(d_tag, [])


# ── Crypto helpers (spec §Step 1–5) ───────────────────────────────────────────

def nip_sb_scrypt(input_bytes: bytes, salt: bytes = b"") -> bytes:
    """scrypt KDF. Returns 32 bytes. Spec: log_n=20, r=8, p=1."""
    return hashlib.scrypt(
        input_bytes, salt=salt,
        n=2**SCRYPT_LOG_N, r=SCRYPT_R, p=SCRYPT_P, dklen=32,
    )


def nip_sb_hkdf(ikm: bytes, info: bytes, length: int = 32) -> bytes:
    """HKDF-SHA256 extract-then-expand. Salt is empty per spec."""
    # Extract
    prk = hmac.new(b"\x00" * 32, ikm, "sha256").digest()
    # Expand (single block — length <= 32)
    return hmac.new(prk, info + b"\x01", "sha256").digest()[:length]


def xchacha20poly1305_encrypt(key: bytes, nonce: bytes, plaintext: bytes, aad: bytes) -> bytes:
    """XChaCha20-Poly1305 AEAD encrypt. Returns ciphertext || tag (len(pt) + 16 bytes)."""
    return sodium.crypto_aead_xchacha20poly1305_ietf_encrypt(plaintext, aad, nonce, key)


def xchacha20poly1305_decrypt(key: bytes, nonce: bytes, ciphertext: bytes, aad: bytes) -> bytes:
    """XChaCha20-Poly1305 AEAD decrypt. Raises on auth failure."""
    return sodium.crypto_aead_xchacha20poly1305_ietf_decrypt(ciphertext, aad, nonce, key)


def secret_to_pubkey(secret_bytes: bytes) -> bytes:
    """Derive 32-byte x-only public key from 32-byte secret key."""
    sk = secp256k1.PrivateKey(secret_bytes)
    # serialize(compressed=True) → 33 bytes (prefix + x). Strip prefix.
    return sk.pubkey.serialize(compressed=True)[1:]


# ── Backup (spec §Step 1–5) ───────────────────────────────────────────────────

@dataclass
class BlobInfo:
    index: int
    d_tag: str
    sign_pk: str


def backup(
    nsec_bytes: bytes,
    pubkey_bytes: bytes,
    password: str,
    relay: SimulatedRelay,
) -> list[BlobInfo]:
    """Create a NIP-SB backup. Returns list of published blob metadata."""

    # base = NFKC(password) || pubkey_bytes  (spec §Encoding Conventions)
    base = password.encode("utf-8") + pubkey_bytes

    # Step 1: Determine N
    h = nip_sb_scrypt(base, salt=b"")
    n = (h[0] % CHUNK_RANGE) + MIN_CHUNKS

    # Step 2: Master encryption key
    h_enc = nip_sb_scrypt(base, salt=b"encrypt")
    enc_key = nip_sb_hkdf(h_enc, b"key")

    # Step 3: Split nsec into N chunks (spec §Step 3)
    remainder = 32 % n
    base_len = 32 // n
    chunks: list[bytes] = []
    offset = 0
    for i in range(n):
        chunk_len = base_len + (1 if i < remainder else 0)
        chunks.append(nsec_bytes[offset : offset + chunk_len])
        offset += chunk_len
    assert offset == 32
    assert b"".join(chunks) == nsec_bytes

    blobs: list[BlobInfo] = []

    for i in range(n):
        # Step 4: Per-blob derivation
        base_i = password.encode("utf-8") + pubkey_bytes + str(i).encode("ascii")
        h_i = nip_sb_scrypt(base_i, salt=b"")
        d_tag = nip_sb_hkdf(h_i, b"d-tag").hex()
        sign_sk_bytes = nip_sb_hkdf(h_i, b"signing-key")

        # Reject-and-retry if invalid scalar (spec §Step 4)
        # secp256k1 order n ≈ 2^256 - 4.3×10^38. Probability of needing retry: ~3.7×10^-39.
        try:
            sign_pk_bytes = secret_to_pubkey(sign_sk_bytes)
        except Exception:
            # Astronomically unlikely. Spec says retry with "signing-key-1", etc.
            raise RuntimeError(f"blob {i}: signing key derivation produced invalid scalar")
        sign_pk_hex = sign_pk_bytes.hex()

        # Step 5: Encrypt chunk
        # Pad to CHUNK_PAD_LEN with random bytes (spec: random, NOT zero)
        padded = chunks[i] + os.urandom(CHUNK_PAD_LEN - len(chunks[i]))
        assert len(padded) == CHUNK_PAD_LEN

        # Fresh random 24-byte nonce (MUST be random per spec)
        nonce = os.urandom(24)

        # XChaCha20-Poly1305 encrypt
        ciphertext = xchacha20poly1305_encrypt(enc_key, nonce, padded, AAD)
        assert len(ciphertext) == CHUNK_PAD_LEN + 16  # 32 bytes

        # Blob content = nonce || ciphertext (56 bytes)
        blob_raw = nonce + ciphertext
        assert len(blob_raw) == 56
        content_b64 = base64.b64encode(blob_raw).decode("ascii")

        # Publish to relay
        relay_publish(relay, RelayEvent(
            pubkey=sign_pk_hex,
            d_tag=d_tag,
            content=content_b64,
        ))

        blobs.append(BlobInfo(index=i, d_tag=d_tag, sign_pk=sign_pk_hex))

    return blobs


# ── Recovery (spec §Recovery) ─────────────────────────────────────────────────
#
# Starts from ONLY: password, pubkey, and the relay.
# No stored state from the backup operation.

def recover(
    pubkey_bytes: bytes,
    password: str,
    relay: SimulatedRelay,
) -> bytes:
    """Recover nsec from password + pubkey + relay. Raises on failure."""

    base = password.encode("utf-8") + pubkey_bytes

    # Step 1: Derive N
    h = nip_sb_scrypt(base, salt=b"")
    n = (h[0] % CHUNK_RANGE) + MIN_CHUNKS

    # Step 2: Derive enc_key
    h_enc = nip_sb_scrypt(base, salt=b"encrypt")
    enc_key = nip_sb_hkdf(h_enc, b"key")

    remainder = 32 % n
    base_len = 32 // n
    recovered_chunks: list[bytes] = []

    for i in range(n):
        # Re-derive per-blob selectors
        base_i = password.encode("utf-8") + pubkey_bytes + str(i).encode("ascii")
        h_i = nip_sb_scrypt(base_i, salt=b"")
        d_tag = nip_sb_hkdf(h_i, b"d-tag").hex()
        sign_sk_bytes = nip_sb_hkdf(h_i, b"signing-key")
        expected_pk = secret_to_pubkey(sign_sk_bytes).hex()

        # Query relay by d-tag (spec: also include authors filter)
        events = relay_query(relay, d_tag)
        matched = [e for e in events if e.pubkey == expected_pk]
        if not matched:
            raise ValueError(f"blob {i}: not found (d_tag={d_tag[:16]}…)")

        event = matched[0]

        # Decode and validate content length (spec §Event Validation step 6)
        raw = base64.b64decode(event.content)
        if len(raw) != 56:
            raise ValueError(f"blob {i}: content is {len(raw)} bytes, expected 56")

        # Decrypt (spec §Recovery step 6)
        nonce = raw[:24]
        ciphertext = raw[24:]
        try:
            padded = xchacha20poly1305_decrypt(enc_key, nonce, ciphertext, AAD)
        except Exception:
            raise ValueError(f"blob {i}: decryption failed (wrong password or corrupted)")

        # Extract chunk, discard padding (spec §Recovery step 6)
        chunk_len = base_len + (1 if i < remainder else 0)
        recovered_chunks.append(padded[:chunk_len])

    # Reassemble (spec §Recovery step 6)
    nsec_bytes = b"".join(recovered_chunks)
    assert len(nsec_bytes) == 32

    # Validate: nsec must be valid secp256k1 scalar (spec §Recovery step 7a)
    try:
        recovered_pk = secret_to_pubkey(nsec_bytes)
    except Exception:
        raise ValueError("recovered key is not a valid secp256k1 scalar")

    # Validate: derived pubkey must match (spec §Recovery step 7b-c)
    if recovered_pk != pubkey_bytes:
        raise ValueError("pubkey mismatch — wrong password")

    return nsec_bytes


# ── Main ──────────────────────────────────────────────────────────────────────

def main() -> None:
    print("╔══════════════════════════════════════════════════════════════╗")
    print("║  NIP-SB Protocol Demo — Real Crypto, Simulated Relay       ║")
    print("║                                                            ║")
    print("║  scrypt + HKDF-SHA256 + XChaCha20-Poly1305 + secp256k1    ║")
    print("╚══════════════════════════════════════════════════════════════╝")
    print()

    relay: SimulatedRelay = {}

    # Generate a test identity
    sk = secp256k1.PrivateKey()
    nsec_bytes = sk.private_key
    pubkey_bytes = secret_to_pubkey(nsec_bytes)
    password = "correct-horse-battery-staple-2026"

    print(f"Identity:  {pubkey_bytes.hex()[:16]}…")
    print(f"Password:  {password}")
    print()

    # ── Phase 1: Backup ───────────────────────────────────────────────────

    print("── Phase 1: Backup ──────────────────────────────────────────")
    blobs = backup(nsec_bytes, pubkey_bytes, password, relay)
    n = len(blobs)
    print(f"   N = {n}")
    for b in blobs:
        print(f"   Blob {b.index:2d}: d={b.d_tag[:12]}… pk={b.sign_pk[:12]}… ✅")

    # Add decoy events (simulates other kind:30078 data on the relay)
    for _ in range(5):
        fake_sk = secp256k1.PrivateKey()
        relay_publish(relay, RelayEvent(
            pubkey=secret_to_pubkey(fake_sk.private_key).hex(),
            d_tag=os.urandom(32).hex(),
            content=base64.b64encode(os.urandom(56)).decode(),
        ))

    total = sum(len(v) for v in relay.values())
    print(f"\n   Relay: {total} total events ({n} backup + 5 decoy)")

    # ── Phase 2: Recovery ─────────────────────────────────────────────────

    print("\n── Phase 2: Recovery (password + pubkey only) ────────────────")
    print(f"   Relay has {total} events. Which are ours? Only the password knows.")
    recovered = recover(pubkey_bytes, password, relay)
    print(f"   ✅ RECOVERED — pubkey matches")
    if recovered == nsec_bytes:
        print(f"   ✅ SECRET KEY MATCHES (byte-for-byte)")
    else:
        print(f"   ❌ SECRET KEY MISMATCH")
        sys.exit(1)

    # ── Phase 3: Wrong Password ───────────────────────────────────────────

    print("\n── Phase 3: Wrong Password ──────────────────────────────────")
    try:
        recover(pubkey_bytes, "wrong-password-totally-different", relay)
        print("   ❌ UNEXPECTED SUCCESS")
        sys.exit(1)
    except ValueError as e:
        print(f"   ✅ Correctly rejected: {e}")

    # ── Phase 4: Different User, Same Password ────────────────────────────

    print("\n── Phase 4: Different User, Same Password ───────────────────")
    other_sk = secp256k1.PrivateKey()
    other_pk = secret_to_pubkey(other_sk.private_key)
    try:
        recover(other_pk, password, relay)
        print("   ❌ UNEXPECTED SUCCESS")
        sys.exit(1)
    except ValueError as e:
        print(f"   ✅ Correctly rejected: {e}")
        print(f"   Same password + different pubkey = completely isolated")

    # ── Phase 5: What an Attacker Sees ────────────────────────────────────

    print("\n── Phase 5: What an Attacker Sees (relay dump) ──────────────")
    backup_pks = {b.sign_pk for b in blobs}
    for events in relay.values():
        for evt in events:
            label = " ← BACKUP" if evt.pubkey in backup_pks else ""
            print(f"   pk={evt.pubkey[:12]}…  d={evt.d_tag[:12]}…  "
                  f"content={evt.content[:16]}…{label}")
    print(f"\n   {n} backup + 5 decoy = {total} total")
    print(f"   The '← BACKUP' labels are only visible because this demo knows.")
    print(f"   An attacker with the full dump cannot tell which are which.")

    # ── Phase 6: Base64 Padding Verification ──────────────────────────────

    print("\n── Phase 6: Base64 Padding Verification ─────────────────────")
    sample_b64 = blobs[0].d_tag  # grab any blob's content from relay
    sample_event = relay_query(relay, blobs[0].d_tag)[0]
    b64_str = sample_event.content
    raw = base64.b64decode(b64_str)
    print(f"   base64 string length: {len(b64_str)} chars")
    print(f"   decoded length: {len(raw)} bytes")
    print(f"   ends with '=': {b64_str.endswith('=')}")
    print(f"   56 mod 3 = {56 % 3} (padding IS required)")
    assert len(raw) == 56, f"Expected 56 bytes, got {len(raw)}"
    assert b64_str.endswith("="), "Expected base64 padding"
    print(f"   ✅ Base64 encoding correct per spec")

    print()
    print("╔══════════════════════════════════════════════════════════════╗")
    print("║                      ALL TESTS PASSED                      ║")
    print("╚══════════════════════════════════════════════════════════════╝")


if __name__ == "__main__":
    main()
