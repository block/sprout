#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["PyNaCl>=1.5", "secp256k1>=0.14", "websockets>=13.0"]
# ///
"""
NIP-SB v3 Live Relay Test

Exercises the full NIP-SB v3 backup/recovery cycle against a REAL running
Sprout relay, including NIP-42 authentication and kind:30078 event
publishing/querying.

Verifies:
  1. Backup: publish N+P+D blobs as real Nostr events via WebSocket
  2. Recovery: query by d-tag only (no authors filter), retrieve all blobs
  3. RS erasure: delete 1 blob from relay, recover via parity
  4. d-tag-only filtering works (no authors needed)

Prerequisites:
  - Sprout relay running at ws://localhost:3000 (see TESTING.md)
  - Docker services running (postgres, redis, etc.)

Usage:
    uv run crates/sprout-core/src/backup/nip_sb_live_test.py
"""

from __future__ import annotations

import asyncio
import base64
import hashlib
import hmac
import json
import os
import random
import struct
import sys
import time
import unicodedata
from dataclasses import dataclass

import nacl.bindings as sodium
import secp256k1
import websockets

# ── NIP-SB Constants ──────────────────────────────────────────────────────────

SCRYPT_LOG_N = 14       # Reduced for demo speed. Real: 20.
SCRYPT_R = 8
SCRYPT_P = 1
MIN_CHUNKS = 3
MAX_CHUNKS = 16
CHUNK_RANGE = 14
PARITY_BLOBS = 2
MIN_DUMMIES = 4
MAX_DUMMIES = 12
DUMMY_RANGE = 9
CHUNK_PAD_LEN = 16
AAD = b"\x02"
RELAY_URL = "ws://localhost:3000"

# ── GF(2^8) + RS (same as nip_sb_demo.py) ────────────────────────────────────

GF_POLY = 0x11B
ALPHA = 0x03

def gf_mul(a: int, b: int) -> int:
    p = 0
    for _ in range(8):
        if b & 1: p ^= a
        hi = a & 0x80
        a = (a << 1) & 0xFF
        if hi: a ^= GF_POLY & 0xFF
        b >>= 1
    return p

def gf_pow(a: int, n: int) -> int:
    r = 1
    while n > 0:
        if n & 1: r = gf_mul(r, a)
        a = gf_mul(a, a)
        n >>= 1
    return r

def gf_inv(a: int) -> int:
    return gf_pow(a, 254)

EVAL_POINTS = [gf_pow(ALPHA, i) for i in range(MAX_CHUNKS + PARITY_BLOBS)]

def rs_encode(data: list[int], n_parity: int = 2) -> list[int]:
    n = len(data)
    points = EVAL_POINTS[:n]
    parity = []
    for k in range(n_parity):
        x = EVAL_POINTS[n + k]
        val = 0
        for i in range(n):
            num = data[i]
            for j in range(n):
                if j != i:
                    num = gf_mul(num, x ^ points[j])
                    num = gf_mul(num, gf_inv(points[i] ^ points[j]))
            val ^= num
        parity.append(val)
    return parity

def rs_encode_rows(padded_chunks: list[bytes]) -> tuple[bytes, bytes]:
    n = len(padded_chunks)
    p0 = bytearray(CHUNK_PAD_LEN)
    p1 = bytearray(CHUNK_PAD_LEN)
    for b in range(CHUNK_PAD_LEN):
        data = [padded_chunks[i][b] for i in range(n)]
        p = rs_encode(data, PARITY_BLOBS)
        p0[b] = p[0]
        p1[b] = p[1]
    return bytes(p0), bytes(p1)

def rs_decode(symbols: list[int | None], n_data: int) -> list[int]:
    n_total = n_data + PARITY_BLOBS
    known_pos, known_val = [], []
    for i, s in enumerate(symbols):
        if s is not None:
            known_pos.append(EVAL_POINTS[i])
            known_val.append(s)
    pos = known_pos[:n_data]
    val = known_val[:n_data]
    result = []
    for k in range(n_data):
        x = EVAL_POINTS[k]
        found = False
        for i, s in enumerate(symbols):
            if i == k and s is not None:
                result.append(s)
                found = True
                break
        if found: continue
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

def rs_decode_rows(padded_slots: list[bytes | None], n_data: int) -> list[bytes]:
    n_total = n_data + PARITY_BLOBS
    result = [bytearray(CHUNK_PAD_LEN) for _ in range(n_data)]
    for b in range(CHUNK_PAD_LEN):
        symbols = [None if s is None else s[b] for s in padded_slots]
        decoded = rs_decode(symbols, n_data)
        for i in range(n_data):
            result[i][b] = decoded[i]
    return [bytes(r) for r in result]


# ── Crypto helpers ────────────────────────────────────────────────────────────

def nfkc(password: str) -> bytes:
    return unicodedata.normalize("NFKC", password).encode("utf-8")

def nip_sb_scrypt(input_bytes: bytes, salt: bytes = b"") -> bytes:
    return hashlib.scrypt(input_bytes, salt=salt, n=2**SCRYPT_LOG_N, r=SCRYPT_R, p=SCRYPT_P, dklen=32)

def nip_sb_hkdf(ikm: bytes, info: bytes, length: int = 32) -> bytes:
    prk = hmac.new(b"\x00" * 32, ikm, "sha256").digest()
    return hmac.new(prk, info + b"\x01", "sha256").digest()[:length]

def xchacha_encrypt(key, nonce, pt, aad):
    return sodium.crypto_aead_xchacha20poly1305_ietf_encrypt(pt, aad, nonce, key)

def xchacha_decrypt(key, nonce, ct, aad):
    return sodium.crypto_aead_xchacha20poly1305_ietf_decrypt(ct, aad, nonce, key)

def secret_to_pubkey(secret: bytes) -> bytes:
    sk = secp256k1.PrivateKey(secret)
    return sk.pubkey.serialize(compressed=True)[1:]

def derive_signing_key(h_i: bytes, prefix: bytes) -> bytes:
    for retry in range(256):
        info = prefix if retry == 0 else prefix + f"-{retry}".encode()
        sk = nip_sb_hkdf(h_i, info)
        try:
            secret_to_pubkey(sk)
            return sk
        except Exception:
            continue
    raise RuntimeError("signing key derivation failed")

def derive_dummy_signing_key(h_cover: bytes, j: int) -> bytes:
    for retry in range(256):
        suffix = f"-{retry}" if retry > 0 else ""
        sk = nip_sb_hkdf(h_cover, f"dummy-signing-key-{j}{suffix}".encode())
        try:
            secret_to_pubkey(sk)
            return sk
        except Exception:
            continue
    raise RuntimeError("dummy signing key derivation failed")


# ── Nostr event helpers ───────────────────────────────────────────────────────

def sha256(data: bytes) -> bytes:
    return hashlib.sha256(data).digest()

def sign_event(event_dict: dict, secret_key: bytes) -> dict:
    """Sign a Nostr event (NIP-01). Returns the event with id and sig."""
    serialized = json.dumps([
        0,
        event_dict["pubkey"],
        event_dict["created_at"],
        event_dict["kind"],
        event_dict["tags"],
        event_dict["content"],
    ], separators=(",", ":"), ensure_ascii=False)
    event_id = sha256(serialized.encode("utf-8"))
    event_dict["id"] = event_id.hex()

    sk = secp256k1.PrivateKey(secret_key)
    # schnorr sign (BIP-340): sign the 32-byte event ID
    sig = sk.schnorr_sign(event_id, bip340tag=b"", raw=True)
    event_dict["sig"] = sig.hex()
    return event_dict

def make_nip42_auth_event(challenge: str, relay_url: str, secret_key: bytes, pubkey_hex: str) -> dict:
    """Create a NIP-42 AUTH event."""
    event = {
        "pubkey": pubkey_hex,
        "created_at": int(time.time()),
        "kind": 22242,
        "tags": [
            ["relay", relay_url],
            ["challenge", challenge],
        ],
        "content": "",
    }
    return sign_event(event, secret_key)

def make_kind30078_event(signing_key: bytes, d_tag: str, content_b64: str) -> dict:
    """Create a kind:30078 parameterized replaceable event."""
    pubkey_hex = secret_to_pubkey(signing_key).hex()
    # Spec says ±1 hour jitter, but relays may have tighter windows.
    # Use ±5 minutes for live testing; real implementations should tune to relay tolerance.
    jitter = random.randint(-300, 300)
    event = {
        "pubkey": pubkey_hex,
        "created_at": int(time.time()) + jitter,
        "kind": 30078,
        "tags": [
            ["d", d_tag],
            ["alt", "application data"],
        ],
        "content": content_b64,
    }
    return sign_event(event, signing_key)


# ── WebSocket relay client ────────────────────────────────────────────────────

class RelayClient:
    def __init__(self, url: str):
        self.url = url
        self.ws = None
        self.auth_key = None  # secret key for NIP-42 auth
        self.auth_pubkey = None

    async def connect(self, auth_secret: bytes):
        """Connect and complete NIP-42 auth."""
        self.auth_key = auth_secret
        self.auth_pubkey = secret_to_pubkey(auth_secret).hex()
        self.ws = await websockets.connect(self.url)

        # Receive AUTH challenge
        msg = json.loads(await self.ws.recv())
        assert msg[0] == "AUTH", f"Expected AUTH, got {msg[0]}"
        challenge = msg[1]

        # Send AUTH response
        auth_event = make_nip42_auth_event(challenge, self.url, self.auth_key, self.auth_pubkey)
        await self.ws.send(json.dumps(["AUTH", auth_event]))

        # Receive OK for auth
        resp = json.loads(await self.ws.recv())
        assert resp[0] == "OK", f"Auth failed: {resp}"
        assert resp[2] is True, f"Auth rejected: {resp}"
        print(f"   Authenticated as {self.auth_pubkey[:16]}…")

    async def publish(self, event: dict) -> bool:
        """Publish a Nostr event. Returns True if accepted."""
        await self.ws.send(json.dumps(["EVENT", event]))
        resp = json.loads(await self.ws.recv())
        if resp[0] == "OK":
            return resp[2]
        return False

    @staticmethod
    async def publish_as(url: str, signing_key: bytes, event: dict) -> bool:
        """Open a fresh connection, auth as the signing key, publish, close.
        Sprout requires event.pubkey == authenticated identity, so each
        throwaway blob needs its own authenticated session."""
        ws = await websockets.connect(url)
        try:
            msg = json.loads(await ws.recv())
            assert msg[0] == "AUTH"
            challenge = msg[1]
            pk_hex = secret_to_pubkey(signing_key).hex()
            auth_event = make_nip42_auth_event(challenge, url, signing_key, pk_hex)
            await ws.send(json.dumps(["AUTH", auth_event]))
            resp = json.loads(await ws.recv())
            if resp[0] != "OK" or resp[2] is not True:
                return False
            await ws.send(json.dumps(["EVENT", event]))
            resp = json.loads(await ws.recv())
            if resp[0] == "OK" and not resp[2]:
                print(f"      REJECT: {resp[3] if len(resp) > 3 else 'no reason'}")
            return resp[0] == "OK" and resp[2]
        finally:
            await ws.close()

    async def query(self, sub_id: str, filter_dict: dict) -> list[dict]:
        """Send REQ, collect events until EOSE.
        Uses a unique sub_id per query. Sends CLOSE after EOSE and
        drains the CLOSED ack to prevent it from leaking into the
        next query's response stream."""
        await self.ws.send(json.dumps(["REQ", sub_id, filter_dict]))
        events = []
        while True:
            msg = json.loads(await self.ws.recv())
            if msg[0] == "EVENT" and msg[1] == sub_id:
                events.append(msg[2])
            elif msg[0] == "EOSE" and msg[1] == sub_id:
                break
            elif msg[0] == "CLOSED" and msg[1] == sub_id:
                # Subscription was closed by relay before EOSE
                return events
            elif msg[0] == "NOTICE":
                print(f"   NOTICE: {msg[1]}")
                break
            # Ignore messages for other sub_ids (stale CLOSED acks, etc.)
        # Close subscription and drain the ack
        await self.ws.send(json.dumps(["CLOSE", sub_id]))
        try:
            # Wait briefly for CLOSED ack — don't block forever
            msg = await asyncio.wait_for(self.ws.recv(), timeout=0.5)
            # Silently consume the CLOSED ack
        except asyncio.TimeoutError:
            pass
        return events

    async def close(self):
        if self.ws:
            await self.ws.close()


# ── NIP-SB backup/recovery against live relay ────────────────────────────────

@dataclass
class BlobMeta:
    index: int
    role: str
    d_tag: str
    sign_sk: bytes
    sign_pk: str

async def run_test():
    print("╔══════════════════════════════════════════════════════════════╗")
    print("║  NIP-SB v3 Live Relay Test                                   ║")
    print("║  Target: ws://localhost:3000                                  ║")
    print("╚══════════════════════════════════════════════════════════════╝")
    print()

    # Generate test identity
    identity_sk = secp256k1.PrivateKey()
    nsec_bytes = identity_sk.private_key
    pubkey_bytes = secret_to_pubkey(nsec_bytes)
    password = "correct-horse-battery-staple-orange-purple-mountain"

    # Generate a separate auth identity (not the backup identity)
    auth_sk = secp256k1.PrivateKey()

    print(f"   Identity:  {pubkey_bytes.hex()[:16]}…")
    print(f"   Password:  {password}")
    print()

    # ── Phase 1: Derive backup parameters ─────────────────────────────────

    print("── Phase 1: Derive backup parameters ────────────────────────")
    base = nfkc(password) + pubkey_bytes

    h = nip_sb_scrypt(base, salt=b"")
    n = (h[0] % CHUNK_RANGE) + MIN_CHUNKS
    h_d = nip_sb_scrypt(base, salt=b"dummies")
    d = (h_d[0] % DUMMY_RANGE) + MIN_DUMMIES
    p = PARITY_BLOBS
    h_enc = nip_sb_scrypt(base, salt=b"encrypt")
    enc_key = nip_sb_hkdf(h_enc, b"key")
    h_cover = nip_sb_scrypt(base, salt=b"cover")

    print(f"   N={n} real + P={p} parity + D={d} dummy = {n+p+d} total blobs")

    # Split nsec
    remainder = 32 % n
    base_len = 32 // n
    chunks, offset = [], 0
    for i in range(n):
        cl = base_len + (1 if i < remainder else 0)
        chunks.append(nsec_bytes[offset:offset+cl])
        offset += cl

    # Pad and RS encode
    padded_chunks = [ch + os.urandom(CHUNK_PAD_LEN - len(ch)) for ch in chunks]
    parity_0, parity_1 = rs_encode_rows(padded_chunks)

    # ── Phase 2: Publish all blobs to live relay ──────────────────────────

    print("\n── Phase 2: Publish blobs to live relay ─────────────────────")
    print("   (Each blob authenticates as its own throwaway key)")

    all_blobs: list[BlobMeta] = []

    # Build all blob events first, then publish in random order
    blob_events: list[tuple[BlobMeta, dict]] = []

    # Real chunks
    for i in range(n):
        base_i = nfkc(password) + pubkey_bytes + str(i).encode()
        h_i = nip_sb_scrypt(base_i, salt=b"")
        d_tag = nip_sb_hkdf(h_i, b"d-tag").hex()
        sign_sk = derive_signing_key(h_i, b"signing-key")
        sign_pk = secret_to_pubkey(sign_sk).hex()
        nonce = os.urandom(24)
        ct = xchacha_encrypt(enc_key, nonce, padded_chunks[i], AAD)
        content = base64.b64encode(nonce + ct).decode()
        event = make_kind30078_event(sign_sk, d_tag, content)
        meta = BlobMeta(i, "real", d_tag, sign_sk, sign_pk)
        blob_events.append((meta, event))

    # Parity blobs
    parity_rows = [parity_0, parity_1]
    for k in range(p):
        i = n + k
        base_i = nfkc(password) + pubkey_bytes + str(i).encode()
        h_i = nip_sb_scrypt(base_i, salt=b"")
        d_tag = nip_sb_hkdf(h_i, b"d-tag").hex()
        sign_sk = derive_signing_key(h_i, b"signing-key")
        sign_pk = secret_to_pubkey(sign_sk).hex()
        nonce = os.urandom(24)
        ct = xchacha_encrypt(enc_key, nonce, parity_rows[k], AAD)
        content = base64.b64encode(nonce + ct).decode()
        event = make_kind30078_event(sign_sk, d_tag, content)
        meta = BlobMeta(i, "parity", d_tag, sign_sk, sign_pk)
        blob_events.append((meta, event))

    # Dummy blobs
    for j in range(d):
        d_tag = nip_sb_hkdf(h_cover, f"dummy-d-tag-{j}".encode()).hex()
        sign_sk = derive_dummy_signing_key(h_cover, j)
        sign_pk = secret_to_pubkey(sign_sk).hex()
        nonce = os.urandom(24)
        ct = xchacha_encrypt(enc_key, nonce, os.urandom(CHUNK_PAD_LEN), AAD)
        content = base64.b64encode(nonce + ct).decode()
        event = make_kind30078_event(sign_sk, d_tag, content)
        meta = BlobMeta(n+p+j, "dummy", d_tag, sign_sk, sign_pk)
        blob_events.append((meta, event))

    # Shuffle and publish (spec: MUST shuffle before publication)
    random.shuffle(blob_events)
    for meta, event in blob_events:
        ok = await RelayClient.publish_as(RELAY_URL, meta.sign_sk, event)
        all_blobs.append(meta)
        print(f"   Blob {meta.index:2d} [{meta.role:6s}] d={meta.d_tag[:12]}… → {'✅' if ok else '❌'}")

    # Sort for display
    all_blobs.sort(key=lambda b: ({"real": 0, "parity": 1, "dummy": 2}[b.role], b.index))

    # ── Phase 3: Recovery — d-tag-only queries ────────────────────────────

    print("\n── Phase 3: Recovery (d-tag only, no authors) ────────────────")
    client2 = RelayClient(RELAY_URL)
    await client2.connect(auth_sk.private_key)

    # Re-derive everything from password + pubkey (simulating fresh recovery)
    base = nfkc(password) + pubkey_bytes
    h = nip_sb_scrypt(base, salt=b"")
    n_r = (h[0] % CHUNK_RANGE) + MIN_CHUNKS
    h_d = nip_sb_scrypt(base, salt=b"dummies")
    d_r = (h_d[0] % DUMMY_RANGE) + MIN_DUMMIES
    h_enc = nip_sb_scrypt(base, salt=b"encrypt")
    enc_key_r = nip_sb_hkdf(h_enc, b"key")
    h_cover_r = nip_sb_scrypt(base, salt=b"cover")

    assert n_r == n and d_r == d, "Parameter mismatch"

    remainder_r = 32 % n_r
    base_len_r = 32 // n_r

    # Build query list: all N+P+D d-tags with expected pubkeys
    queries = []
    for i in range(n_r + p):
        base_i = nfkc(password) + pubkey_bytes + str(i).encode()
        h_i = nip_sb_scrypt(base_i, salt=b"")
        d_tag = nip_sb_hkdf(h_i, b"d-tag").hex()
        sign_sk = derive_signing_key(h_i, b"signing-key")
        sign_pk = secret_to_pubkey(sign_sk).hex()
        role = "real" if i < n_r else "parity"
        queries.append((d_tag, sign_pk, role, i))

    for j in range(d_r):
        d_tag = nip_sb_hkdf(h_cover_r, f"dummy-d-tag-{j}".encode()).hex()
        sign_sk = derive_dummy_signing_key(h_cover_r, j)
        sign_pk = secret_to_pubkey(sign_sk).hex()
        queries.append((d_tag, sign_pk, "dummy", n_r + p + j))

    # Verify d-tags match between backup and recovery derivation
    published_real_dtags = {b.d_tag for b in all_blobs if b.role in ("real", "parity")}
    recovery_real_dtags = {dt for dt, _, role, _ in queries if role in ("real", "parity")}
    if published_real_dtags != recovery_real_dtags:
        print(f"   ⚠️  D-TAG MISMATCH!")
        print(f"   Published: {sorted(list(published_real_dtags))[:3]}")
        print(f"   Recovery:  {sorted(list(recovery_real_dtags))[:3]}")
    else:
        print(f"   ✅ All {len(published_real_dtags)} real+parity d-tags match between backup and recovery")

    # Shuffle queries (spec: random order)
    random.shuffle(queries)

    # Query each d-tag — NO authors filter
    padded_slots: list[bytes | None] = [None] * (n_r + p)
    found = 0
    for d_tag, expected_pk, role, idx in queries:
        events = await client2.query(f"q-{idx}", {"kinds": [30078], "#d": [d_tag]})

        if role == "dummy":
            status = f"{'found' if events else 'missing'} (dummy, ignored)"
            print(f"   Query d={d_tag[:12]}… [{role:6s}] → {status}")
            continue

        matched = [e for e in events if e["pubkey"] == expected_pk]
        if not matched:
            print(f"   Query d={d_tag[:12]}… [{role:6s}] → ❌ NOT FOUND")
            continue

        event = matched[0]
        raw = base64.b64decode(event["content"])
        assert len(raw) == 56, f"Content length {len(raw)}, expected 56"
        nonce = raw[:24]
        ciphertext = raw[24:]
        try:
            padded = xchacha_decrypt(enc_key_r, nonce, ciphertext, AAD)
        except Exception:
            print(f"   Query d={d_tag[:12]}… [{role:6s}] → ❌ AEAD FAILURE (erasure)")
            continue

        padded_slots[idx] = padded
        found += 1
        print(f"   Query d={d_tag[:12]}… [{role:6s}] → ✅ decrypted")

    # Reassemble
    missing = [i for i in range(n_r + p) if padded_slots[i] is None]
    print(f"\n   Found {found}/{n_r+p} real+parity blobs, {len(missing)} missing")

    if len(missing) > p:
        print(f"   ❌ Too many missing ({len(missing)} > {p})")
        await client2.close()
        sys.exit(1)

    if missing:
        print(f"   RS erasure decode for positions: {missing}")
        reconstructed = rs_decode_rows(padded_slots, n_r)
        for i in range(n_r):
            padded_slots[i] = reconstructed[i]

    nsec_parts = []
    for i in range(n_r):
        cl = base_len_r + (1 if i < remainder_r else 0)
        nsec_parts.append(padded_slots[i][:cl])

    recovered = b"".join(nsec_parts)
    recovered_pk = secret_to_pubkey(recovered)

    if recovered == nsec_bytes and recovered_pk == pubkey_bytes:
        print(f"\n   ✅ RECOVERY SUCCESSFUL — secret key matches byte-for-byte")
    else:
        print(f"\n   ❌ RECOVERY FAILED — key mismatch")
        await client2.close()
        sys.exit(1)

    # ── Phase 4: Delete 1 blob, recover via RS ────────────────────────────

    print("\n── Phase 4: Delete blob 0, recover via RS parity ─────────────")

    # Publish a deletion event for blob 0 (NIP-09) — auth as blob 0's throwaway key
    blob0 = [b for b in all_blobs if b.role == "real"][0]
    delete_event = {
        "pubkey": blob0.sign_pk,
        "created_at": int(time.time()),
        "kind": 5,
        "tags": [["a", f"30078:{blob0.sign_pk}:{blob0.d_tag}"]],
        "content": "",
    }
    delete_event = sign_event(delete_event, blob0.sign_sk)
    ok = await RelayClient.publish_as(RELAY_URL, blob0.sign_sk, delete_event)
    print(f"   Deletion event for blob 0: {'✅ accepted' if ok else '❌ rejected'}")

    # Re-run recovery (blob 0 should be missing now)
    padded_slots2: list[bytes | None] = [None] * (n_r + p)
    random.shuffle(queries)
    for d_tag, expected_pk, role, idx in queries:
        if role == "dummy":
            continue
        events = await client2.query(f"r2-{idx}", {"kinds": [30078], "#d": [d_tag]})
        matched = [e for e in events if e["pubkey"] == expected_pk]
        if not matched:
            continue
        raw = base64.b64decode(matched[0]["content"])
        nonce, ct = raw[:24], raw[24:]
        try:
            padded_slots2[idx] = xchacha_decrypt(enc_key_r, nonce, ct, AAD)
        except Exception:
            continue

    missing2 = [i for i in range(n_r + p) if padded_slots2[i] is None]
    print(f"   Found {n_r+p-len(missing2)}/{n_r+p} blobs, {len(missing2)} missing")

    if missing2:
        print(f"   RS erasure decode for positions: {missing2}")
        reconstructed2 = rs_decode_rows(padded_slots2, n_r)
        for i in range(n_r):
            padded_slots2[i] = reconstructed2[i]

    nsec_parts2 = []
    for i in range(n_r):
        cl = base_len_r + (1 if i < remainder_r else 0)
        nsec_parts2.append(padded_slots2[i][:cl])

    recovered2 = b"".join(nsec_parts2)
    if recovered2 == nsec_bytes:
        print(f"   ✅ RS RECOVERY SUCCESSFUL after blob deletion")
    else:
        print(f"   ❌ RS RECOVERY FAILED after blob deletion")
        await client2.close()
        sys.exit(1)

    await client2.close()

    print()
    print("╔══════════════════════════════════════════════════════════════╗")
    print("║              ALL LIVE RELAY TESTS PASSED                     ║")
    print("╚══════════════════════════════════════════════════════════════╝")


def main():
    asyncio.run(run_test())

if __name__ == "__main__":
    main()
