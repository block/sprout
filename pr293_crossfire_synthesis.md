# PR #293 Crossfire Review: Identity-Pubkey Binding

**Reviewed by:** 4 independent subagents (Security/Codex, Rust Backend, Frontend/Tauri, Architecture/Docs)  
**Date:** 2026-04-10  
**Branch:** `identity-pubkey-binding`

---

## TL;DR

PR #293 introduces a corporate identity-to-Nostr-pubkey binding system: employees prove their identity via JWT/SSO, and their Nostr pubkey is cryptographically associated with their verified corporate identity. The feature uses NIP-42 for WebSocket auth and NIP-98 for REST bootstrap — the right shape for this problem. However, **three security issues must be fixed before merge**: the pubkey uniqueness invariant is unenforced (same key can bind to multiple identities), the proxy-to-relay trust boundary is documentation-only (header spoofing = identity spoofing), and `validate_identity_jwt()` grants all non-admin scopes regardless of JWT claims (privilege expansion). A fourth issue — fail-open DB error handling in `is_identity_bound()` — is a security guard that silently degrades to "not bound" on DB error. The frontend has two blockers as well. **Verdict: REQUEST_CHANGES.** Fix the security issues; the architecture is sound and worth landing.

---

## Reviewer Agreement Matrix

| Issue | Codex | Rust Backend | Frontend/Tauri | Arch/Docs | Confidence |
|-------|-------|--------------|----------------|-----------|------------|
| Pubkey uniqueness not enforced | ✅ Critical | ✅ (noted as TODO) | — | ✅ (noted as TODO) | **High** |
| Proxy trust boundary / header spoofing | ✅ Critical | — | — | — | Medium |
| JWT grants all scopes regardless of claims | ✅ Critical | — | — | — | Medium |
| Fail-open `is_identity_bound()` on DB error | — | ✅ Must-Fix | — | — | Medium |
| `device_cn` silently defaults to "default" | ✅ | ✅ | — | — | **High** |
| SELECT FOR UPDATE race on first bind | ✅ (broken) | ❌ (fine) | — | — | **Resolved — see below** |
| Missing migration file | — | ✅ (flagged) | — | — | **Non-issue (pgschema)** |
| E2E bridge missing identity mock | — | — | ✅ Blocker | — | Medium |
| Registration pubkey not validated | — | — | ✅ Blocker | — | Medium |
| ARCHITECTURE.md "Four paths" stale | — | — | — | ✅ Must-Fix | Medium |
| `verified_name` cleared on unbind w/ active bindings | ✅ | — | — | — | Medium |
| NIP-98 URL canonicalization mismatch | ✅ | — | ✅ (payload tag) | — | **High** |
| `identity_bound_cache` visibility | — | ✅ | — | — | Low |
| `unreachable!()` in api/identity.rs:219 | — | ✅ | — | — | Low |
| preauthenticated mode is dead code | — | — | ✅ | — | Low |
| Cache invalidation local-only (2-min window) | — | — | — | ✅ | Low |

---

## Critical Issues (must fix before merge)

### 1. Pubkey uniqueness not enforced — same key can bind to multiple identities

**What's wrong:** The schema makes `(uid, device_cn)` unique but `pubkey` is only indexed, not UNIQUE. A single Nostr key can be bound to multiple employees. This breaks the core invariant: "a pubkey represents exactly one principal." Downstream, `verified_name` becomes meaningless — you can't trust that a pubkey maps to a specific identity.

**Why it matters:** This is the foundational security property of the entire feature. Without it, verified identity is decorative. An attacker who controls one account can bind their Nostr key to a second account, making their messages appear verified under either identity.

**Flagged by:** Codex (Critical), Rust Backend (noted TODO), Architecture/Docs (noted TODO)

**Fix:** Add `UNIQUE (pubkey)` to `identity_bindings`. Handle the resulting 409 on conflict. Update `verified_name` logic to be safe only when this constraint is in place. The "acknowledged TODO" status is not acceptable for a security invariant — this must be enforced at the schema level, not deferred.

---

### 2. Proxy-to-relay trust boundary is documentation-only

**What's wrong:** The proxy passes identity via HTTP headers (e.g., `X-Sprout-Identity`). There is no allowlist, no mTLS, no loopback binding, and no verification that the request actually came from the proxy. If the relay port is exposed directly — even on localhost — any process can forge these headers and impersonate any identity.

**Why it matters:** Header spoofing = identity spoofing. The entire proxy-mode identity model collapses if the relay is reachable without going through the proxy. This is not a theoretical concern: default OS firewall rules do not block loopback-to-loopback connections between processes.

**Flagged by:** Codex (Critical)

**Fix:** At minimum, bind the relay's identity-header-accepting endpoint to loopback only and document the required network posture. Better: add a shared secret or mTLS between proxy and relay so headers cannot be forged even by local processes. Fail closed if the shared secret is absent.

---

### 3. `validate_identity_jwt()` grants all non-admin scopes regardless of JWT claims

**What's wrong:** The function resolves identity from the JWT but then grants all non-admin scopes unconditionally, ignoring what the JWT actually claims. This is privilege expansion: a JWT with minimal scope (e.g., read-only) gets full non-admin access.

**Why it matters:** The JWT is the trust anchor for corporate identity. Ignoring its claims defeats the purpose of scoped tokens and violates least-privilege. An attacker with a low-privilege JWT gets full non-admin access.

**Flagged by:** Codex (Critical)

**Fix:** Extract the scope claims from the JWT and map them to Sprout scopes. Do not grant scopes not present in the JWT. Add a test that verifies a read-only JWT cannot perform write operations.

---

### 4. Fail-open `is_identity_bound()` on DB error

**What's wrong:** `is_identity_bound()` returns `unwrap_or(false)` on DB error. A transient DB failure causes the security guard to return "not bound," allowing the system to downgrade to unauthenticated behavior silently.

**Why it matters:** Security guards must fail closed. A DB hiccup should not silently grant access. This is especially dangerous in hybrid-mode where identity binding is the gating condition.

**Flagged by:** Rust Backend (Must-Fix)

**Fix:** Change `unwrap_or(false)` to `unwrap_or(true)` (treat DB error as "bound" = deny) or, better, propagate the error and return a 503 to the caller so the failure is visible and retryable.

---

### 5. E2E bridge missing `initialize_identity` mock — all E2E tests broken

**What's wrong:** `IdentityGate` wraps the entire application. `e2eBridge.ts` has no handler for `initialize_identity`. Every E2E test that exercises any authenticated path will hang or fail at the gate.

**Why it matters:** This is a test infrastructure blocker. Merging this breaks the E2E suite for the entire application, not just identity tests.

**Flagged by:** Frontend/Tauri (Blocker)

**Fix:** Add `initialize_identity` mock handler to `e2eBridge.ts`. It should return a configurable response (bound/unbound) to allow testing both paths.

---

## Important Issues (should fix before merge)

### 6. SELECT FOR UPDATE does not protect against phantom reads on first bind

**What's wrong:** The "race-safe first bind" claim is overstated. `SELECT FOR UPDATE` locks *existing* rows. For a first bind where no row exists yet, two concurrent transactions can both `SELECT FOR UPDATE` on a non-existent row, both see nothing, and both proceed to `INSERT`. The `lock_timeout` only affects contention on *existing* rows — it does nothing for the missing-row case.

**Disagreement resolved:** Codex is correct; the Rust subagent is wrong on the mechanism. However, the practical severity is bounded: the `UNIQUE (uid, device_cn)` constraint will catch the duplicate at the DB level, so the result is a 409 error rather than silent data corruption. The race is real but not catastrophic given the existing constraint. Severity is "important" not "critical."

**Fix:** Use `INSERT ... ON CONFLICT (uid, device_cn) DO NOTHING` with a follow-up SELECT, or use a Postgres advisory lock keyed on `uid`. Remove the "race-safe" claim from comments until the fix is in.

---

### 7. Registration response pubkey not validated

**What's wrong:** After calling `/api/identity/register`, the client accepts whatever pubkey the relay returns without verifying it matches the locally-generated key. A compromised or misconfigured relay could substitute a different pubkey, silently binding the wrong key to the user's identity.

**Flagged by:** Frontend/Tauri (Blocker — elevated here to Important since it requires a malicious/buggy relay)

**Fix:** After registration, assert `response.pubkey === localKeypair.publicKey`. Treat a mismatch as a fatal error and surface it to the user.

---

### 8. NIP-98 URL canonicalization mismatch

**What's wrong:** Two reviewers independently flagged inconsistency in NIP-98 handling: Codex noted a mismatch between relay config and `relay_api_base_url()`, and the Frontend reviewer noted a payload tag inconsistency between `initialize_identity` and `build_nip98_auth_header`. NIP-98 auth is URL-sensitive — a trailing slash difference will cause auth failures.

**Flagged by:** Codex, Frontend/Tauri

**Fix:** Canonicalize URLs in a single shared function used by both the relay config and the desktop client. Add a test that verifies NIP-98 auth succeeds with the exact URL format the relay expects.

---

### 9. `device_cn` silently defaults to "default" when header is missing

**What's wrong:** When the device CN header is absent, the code silently falls back to `"default"` instead of failing. This masks misconfiguration and can cause all devices to share a single binding slot.

**Flagged by:** Codex, Rust Backend (both independently)

**Fix:** Fail closed — return 400 Bad Request if the device CN header is required and missing. If "default" is a legitimate value, document it explicitly and add a test for the fallback path.

---

### 10. `verified_name` cleared on unbind while active bindings remain

**What's wrong:** Unbinding clears `verified_name` even if the user has other active bindings. This is both incorrect (the user is still verified via another device) and a hard delete that destroys forensic history.

**Flagged by:** Codex

**Fix:** Only clear `verified_name` when the last binding is removed. Use soft-delete (add `unbound_at` timestamp) rather than hard-delete for audit trail.

---

### 11. `handleAuthChallenge` not gated on `authMode`

**What's wrong:** In `preauthenticated` mode, the client still responds to NIP-42 AUTH challenges from the relay. This is dead code today (backend never returns `preauthenticated`), but it's a latent bug waiting to activate.

**Flagged by:** Frontend/Tauri

**Fix:** Gate `handleAuthChallenge` on `authMode !== 'preauthenticated'`. While fixing this, remove or clearly tombstone the dead `preauthenticated` mode or document when it will be used.

---

### 12. Auth rejection is a silent dead connection

**What's wrong:** When the relay rejects NIP-42 auth, the WebSocket connection dies with no user-visible error. The user sees a disconnected client with no explanation.

**Flagged by:** Frontend/Tauri

**Fix:** Surface auth rejection as a user-visible error with a clear message (e.g., "Identity verification failed — check your corporate credentials"). Add a reconnect-with-backoff path.

---

## Minor Issues (nice to have)

**M1. ARCHITECTURE.md "Four authentication paths" is stale (×3 occurrences)**  
Now five paths. Update all three occurrences and add the proxy identity row to the auth paths table. Add `/api/identity/register` to the REST API table. *(Arch/Docs)*

**M2. `ARCHITECTURE.md` missing `ProxyIdentity` variant in `AuthMethod` enum**  
The enum in the docs doesn't match the code. *(Arch/Docs)*

**M3. `identity_bound_cache` should be `pub(crate)` not `pub`**  
Unnecessary public surface. *(Rust Backend)*

**M4. `unreachable!()` in `api/identity.rs:219` is fragile**  
Replace with a proper error return. `unreachable!()` panics in production if the assumption is ever violated. *(Rust Backend)*

**M5. `is_identity_bound` is a no-op in Proxy mode — undocumented**  
Safe but confusing. Add a comment explaining why. *(Rust Backend)*

**M6. `.env.example` has duplicate `SPROUT_REQUIRE_AUTH_TOKEN=false`**  
Trivial cleanup. *(Arch/Docs)*

**M7. `config.rs` override log should be `warn!` not `info!`**  
Config overrides are operationally significant. *(Arch/Docs)*

**M8. `VerifiedBadge` tooltip is ambiguous**  
"Verified as {name}" doesn't say who verified or what the verification means. Consider "Verified corporate identity: {name}" or similar. *(Frontend/Tauri)*

**M9. Dead error regex in frontend**  
Regex checks for a string that doesn't match what Rust actually emits. *(Frontend/Tauri)*

**M10. `last_seen_at` has no consumer**  
Either wire it up or remove it to avoid dead schema. *(Arch/Docs)*

**M11. Cache invalidation is local-only with 2-minute window**  
On multi-node deployments, a binding change on one node won't be seen by others for up to 2 minutes. Document this limitation explicitly. *(Arch/Docs)*

**M12. Add `CHECK` constraints on `uid`/`device_cn` length**  
Prevents pathological inputs from reaching application logic. *(Rust Backend)*

---

## What's Done Well

- **NIP-42 + NIP-98 is the right shape.** Using NIP-42 for WebSocket auth and NIP-98 for REST bootstrap is the correct protocol pairing. The architecture is sound.
- **Auth wired through shared backend primitives.** No bespoke auth paths; identity flows through the same middleware as everything else.
- **`verified_name` separate from `display_name`.** Correct UI model — verified corporate identity is distinct from user-chosen display name.
- **IPC boundary is clean.** Private key never crosses to the renderer process. Key storage uses `0o600`, atomic write, and corruption quarantine. This is done right.
- **Dev feature gating is correct.** Identity features are properly gated behind the dev feature flag.
- **409 Conflict on binding mismatch is correct semantic.** Right HTTP status for this case.
- **Auth events never stored/logged in proxy path.** Good hygiene — auth material doesn't end up in logs.
- **`SELECT FOR UPDATE` + `lock_timeout` shows awareness of concurrency.** The intent is right even if the implementation has a gap (see Issue #6).

---

## Overall Assessment

**Verdict: REQUEST_CHANGES**

**Score: 5/10**

The feature architecture is correct and the implementation is largely solid. The NIP-42/NIP-98 pairing, clean IPC boundary, and proper key storage show real care. But there are **four security issues that must be fixed before merge**, two of which (pubkey uniqueness, JWT scope bypass) are fundamental to the feature's security model. Merging without these fixes ships a "verified identity" feature that doesn't actually enforce identity uniqueness and grants more privilege than intended.

The path to merge is clear: fix the four critical security issues, the two frontend blockers, and the NIP-98 canonicalization mismatch. The remaining issues are important but can be addressed in follow-up PRs with tracking tickets. This is not a "close and reopen" situation — the bones are good. Fix the critical issues and this is ready.

**Prioritized fix order:**
1. `UNIQUE (pubkey)` constraint on `identity_bindings`
2. `validate_identity_jwt()` scope enforcement
3. Proxy trust boundary (loopback bind + shared secret)
4. `is_identity_bound()` fail-closed on DB error
5. E2E bridge `initialize_identity` mock
6. Registration pubkey validation
7. NIP-98 URL canonicalization
8. `device_cn` fail-closed
