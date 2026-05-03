//! NIP-42 AUTH handler — verify challenge response, transition auth state.

use std::sync::Arc;

use sha2::{Digest, Sha256};
use tracing::{debug, info, warn};

use crate::connection::{AuthState, ConnectionState};
use crate::protocol::RelayMessage;
use crate::state::AppState;

use super::nip_aa;

/// Check relay membership for a pubkey during NIP-42 auth, with NIP-AA fallback.
///
/// Returns:
/// - `Some(None)`              — direct member (or membership not required), access granted
/// - `Some(Some(owner_pubkey))`— NIP-AA virtual member, access granted
/// - `None`                    — access denied (rejection message already sent)
async fn enforce_ws_relay_membership(
    state: &AppState,
    conn: &Arc<ConnectionState>,
    conn_id: uuid::Uuid,
    pubkey: &nostr::PublicKey,
    event_id_hex: &str,
    tags: &[nostr::Tag],
    event_created_at: u64,
) -> Option<Option<nostr::PublicKey>> {
    if !state.config.require_relay_membership {
        return Some(None);
    }

    let pubkey_hex = pubkey.to_hex();
    let is_member = match state.db.is_relay_member(&pubkey_hex).await {
        Ok(v) => v,
        Err(e) => {
            // DB error: fail closed immediately. Do NOT fall through to NIP-AA —
            // we cannot authoritatively determine membership, so we must deny.
            warn!(
                conn_id = %conn_id,
                pubkey = %pubkey_hex,
                error = %e,
                "relay membership check failed, denying (fail-closed)"
            );
            metrics::counter!("sprout_auth_failures_total", "reason" => "membership_db_error")
                .increment(1);
            // Do NOT set AuthState here — caller uses reauth_fail! to preserve
            // existing session on re-auth failure per NIP-AA §6.
            conn.send(RelayMessage::ok(
                event_id_hex,
                false,
                "restricted: membership check failed",
            ));
            return None;
        }
    };

    if is_member {
        return Some(None);
    }

    // Not a direct member — try NIP-AA fallback.
    match nip_aa::verify_nip_aa(state, pubkey, tags, event_created_at).await {
        Ok(Some(result)) => {
            debug!(
                conn_id = %conn_id,
                agent = %pubkey_hex,
                owner = %result.owner_pubkey.to_hex(),
                "NIP-AA: virtual membership granted"
            );
            Some(Some(result.owner_pubkey))
        }
        Ok(None) => {
            // No auth tag — not an agent, plain membership failure.
            warn!(conn_id = %conn_id, pubkey = %pubkey_hex, "not a relay member");
            metrics::counter!("sprout_auth_failures_total", "reason" => "not_relay_member")
                .increment(1);
            // Do NOT set AuthState here — caller uses reauth_fail! to preserve
            // existing session on re-auth failure per NIP-AA §6.
            conn.send(RelayMessage::ok(
                event_id_hex,
                false,
                "restricted: not a relay member",
            ));
            None
        }
        Err(reason) => {
            // Auth tag present but invalid.
            warn!(conn_id = %conn_id, pubkey = %pubkey_hex, reason = %reason, "NIP-AA verification failed");
            metrics::counter!("sprout_auth_failures_total", "reason" => "nip_aa_invalid")
                .increment(1);
            // Do NOT set AuthState here — caller uses reauth_fail! to preserve
            // existing session on re-auth failure per NIP-AA §6.
            conn.send(RelayMessage::ok(event_id_hex, false, &reason));
            None
        }
    }
}

fn verify_api_token_nip42_binding(
    event: &nostr::Event,
    challenge: &str,
    relay_url: &str,
) -> Result<(), sprout_auth::AuthError> {
    sprout_auth::verify_nip42_event(event, challenge, relay_url)
}

/// Handle a NIP-42 AUTH message: verify the challenge response and transition the connection to authenticated state.
pub async fn handle_auth(event: nostr::Event, conn: Arc<ConnectionState>, state: Arc<AppState>) {
    let event_id_hex_early = event.id.to_hex();

    // Extract the challenge, conn_id, and (for re-auth) the previous AuthContext.
    //
    // AuthState::Authenticated: NIP-AA §6 allows the same pubkey to re-auth on an
    // already-authenticated connection to replace the stored credential. We retain
    // the challenge in AuthState::Authenticated for exactly this purpose. Only the
    // same pubkey may re-auth — a different pubkey is rejected to prevent session
    // hijacking via credential replacement.
    //
    // NIP-AA §6 also says: "A failed NIP-AA AUTH attempt does not necessarily
    // invalidate other authenticated pubkeys on the same WebSocket connection."
    // We preserve this by saving the previous AuthContext and restoring it if
    // re-auth fails, rather than transitioning to AuthState::Failed.
    let (challenge, conn_id, prev_auth_ctx) = {
        let auth = conn.auth_state.read().await;
        match &*auth {
            AuthState::Pending { challenge } => (challenge.clone(), conn.conn_id, None),
            AuthState::Authenticated { challenge, ctx } => {
                // NIP-AA §6: same-pubkey re-auth MUST replace the stored credential.
                if event.pubkey != ctx.pubkey {
                    warn!(
                        conn_id = %conn.conn_id,
                        existing = %ctx.pubkey.to_hex(),
                        incoming = %event.pubkey.to_hex(),
                        "NIP-AA §6: re-auth pubkey mismatch — rejecting"
                    );
                    conn.send(RelayMessage::ok(
                        &event_id_hex_early,
                        false,
                        "auth-required: pubkey mismatch on re-auth",
                    ));
                    return;
                }
                debug!(
                    conn_id = %conn.conn_id,
                    pubkey = %ctx.pubkey.to_hex(),
                    "NIP-AA §6: re-auth credential replacement"
                );
                (challenge.clone(), conn.conn_id, Some(ctx.clone()))
            }
            AuthState::Failed => {
                debug!(conn_id = %conn.conn_id, "AUTH received after failed auth");
                conn.send(RelayMessage::ok(
                    &event_id_hex_early,
                    false,
                    "auth-required: authentication already failed",
                ));
                return;
            }
        }
    };

    // Helper: on re-auth failure, restore the previous AuthContext rather than
    // transitioning to Failed (per NIP-AA §6 — failed re-auth must not invalidate
    // the existing authenticated identity).
    macro_rules! reauth_fail {
        ($state:expr) => {
            if let Some(ref prev) = prev_auth_ctx {
                *conn.auth_state.write().await = AuthState::Authenticated {
                    ctx: prev.clone(),
                    challenge: challenge.clone(),
                };
            } else {
                *conn.auth_state.write().await = $state;
            }
        };
    }

    let relay_url = state.config.relay_url.clone();
    let auth_svc = Arc::clone(&state.auth);
    let event_id_hex = event.id.to_hex();

    // Extract the auth_token tag before dispatching — API tokens (sprout_*) must be
    // intercepted here because verify_auth_event() has no DB access and rejects them.
    let auth_token = event.tags.iter().find_map(|tag| {
        let vec = tag.as_slice();
        if vec.len() >= 2 && vec[0] == "auth_token" {
            Some(vec[1].to_string())
        } else {
            None
        }
    });

    metrics::counter!("sprout_auth_attempts_total", "method" => if auth_token.as_ref().is_some_and(|t| t.starts_with("sprout_")) { "api_token" } else { "nip42" }).increment(1);

    // ── auth_token + auth tag interaction ──────────────────────────────────
    // If an AUTH event carries BOTH an `auth_token` tag AND an `auth` tag (NIP-OA
    // credential), the token path runs first:
    //   1. If the token is valid → use it (the `auth` tag is ignored).
    //   2. If the token is INVALID (wrong hash, expired, etc.) → reject immediately.
    //      We do NOT fall through to NIP-AA in this case. A client that supplies a
    //      token has declared its intent; a bad token is a hard failure, not a cue
    //      to try a different auth method. This prevents a confused-deputy attack
    //      where an attacker appends a valid NIP-OA credential to a stolen-but-expired
    //      token event hoping the relay will accept it via NIP-AA.
    //
    // The NIP-AA path (below) only runs when `auth_token` is absent entirely.
    if let Some(ref token) = auth_token {
        if token.starts_with("sprout_") {
            // ── API token path ──────────────────────────────────────────────
            // Extract tags and created_at before event is moved into the blocking task.
            let event_tags = event.tags.clone().to_vec();
            let event_created_at_ts = event.created_at.as_u64();
            let event_clone = event.clone();
            let challenge_owned = challenge.clone();
            let relay_owned = relay_url.clone();
            match tokio::task::spawn_blocking(move || {
                verify_api_token_nip42_binding(&event_clone, &challenge_owned, &relay_owned)
            })
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    warn!(conn_id = %conn_id, error = %e, "API token auth failed NIP-42 verification");
                    metrics::counter!("sprout_auth_failures_total", "reason" => "nip42_invalid")
                        .increment(1);
                    reauth_fail!(AuthState::Failed);
                    conn.send(RelayMessage::ok(
                        &event_id_hex,
                        false,
                        "auth-required: verification failed",
                    ));
                    return;
                }
                Err(e) => {
                    warn!(conn_id = %conn_id, error = %e, "API token NIP-42 verification task failed");
                    metrics::counter!("sprout_auth_failures_total", "reason" => "nip42_internal")
                        .increment(1);
                    reauth_fail!(AuthState::Failed);
                    conn.send(RelayMessage::ok(
                        &event_id_hex,
                        false,
                        "auth-required: verification failed",
                    ));
                    return;
                }
            }

            // Hash the raw token and look it up in the DB. The relay owns this
            // path; sprout-auth has no DB access.
            let hash: [u8; 32] = Sha256::digest(token.as_bytes()).into();

            let record = match state.db.get_api_token_by_hash(&hash).await {
                Ok(Some(r)) => r,
                Ok(None) => {
                    warn!(conn_id = %conn_id, "API token not found");
                    reauth_fail!(AuthState::Failed);
                    conn.send(RelayMessage::ok(
                        &event_id_hex,
                        false,
                        "auth-required: invalid token",
                    ));
                    return;
                }
                Err(e) => {
                    warn!(conn_id = %conn_id, error = %e, "API token lookup failed");
                    reauth_fail!(AuthState::Failed);
                    conn.send(RelayMessage::ok(
                        &event_id_hex,
                        false,
                        "auth-required: verification failed",
                    ));
                    return;
                }
            };

            // Reconstruct the owner pubkey from the stored raw bytes.
            let owner_pubkey = match nostr::PublicKey::from_slice(&record.owner_pubkey) {
                Ok(pk) => pk,
                Err(e) => {
                    warn!(conn_id = %conn_id, error = %e, "API token owner pubkey invalid");
                    reauth_fail!(AuthState::Failed);
                    conn.send(RelayMessage::ok(
                        &event_id_hex,
                        false,
                        "auth-required: verification failed",
                    ));
                    return;
                }
            };

            // Verify hash, expiry, and pubkey match via the auth service.
            match auth_svc.verify_api_token_against_hash(
                token,
                &record.token_hash,
                &owner_pubkey,
                &event.pubkey,
                record.expires_at,
                &record.scopes,
            ) {
                Ok((pubkey, scopes)) => {
                    info!(conn_id = %conn_id, pubkey = %pubkey.to_hex(), "API token auth successful");
                    // Update last_used_at asynchronously — non-fatal if it fails.
                    let db = state.db.clone();
                    let hash_owned = hash;
                    tokio::spawn(async move {
                        if let Err(e) = db.update_token_last_used(&hash_owned).await {
                            warn!("update_token_last_used failed: {e}");
                        }
                    });

                    // API token users have already proven authorization via their token —
                    // the pubkey allowlist does not apply here.

                    // Relay membership gate — applies to ALL auth methods; NIP-AA fallback included.
                    let nip_aa_owner = match enforce_ws_relay_membership(
                        &state,
                        &conn,
                        conn_id,
                        &pubkey,
                        &event_id_hex,
                        &event_tags,
                        event_created_at_ts,
                    )
                    .await
                    {
                        Some(owner) => owner,
                        None => return,
                    };

                    // NIP-AA spec: virtual members MUST NOT be granted admin privileges.
                    // Intersect the token's scopes with the NIP-AA virtual member set —
                    // do NOT replace them. Replacing could widen a read-only token to
                    // full write access. Intersection preserves the token's restrictions
                    // while stripping any admin scopes.
                    //
                    // Empty intersection: if the token carries ONLY admin scopes (e.g. a
                    // sprout_admin token), the intersection is empty. Empty scopes are
                    // treated as "unrestricted" by some handlers (NIP-98 / dev-mode paths),
                    // which would widen access. Deny rather than grant with empty scopes.
                    let (auth_method, final_owner_pubkey, final_scopes) = match nip_aa_owner {
                        Some(owner) => {
                            let allowed = sprout_auth::Scope::nip_aa_virtual_member();
                            let intersected: Vec<sprout_auth::Scope> =
                                scopes.into_iter().filter(|s| allowed.contains(s)).collect();
                            if intersected.is_empty() {
                                warn!(conn_id = %conn_id, pubkey = %pubkey.to_hex(),
                                      "NIP-AA: scope intersection is empty — denying (would widen access)");
                                metrics::counter!("sprout_auth_failures_total", "reason" => "nip_aa_empty_scope").increment(1);
                                reauth_fail!(AuthState::Failed);
                                conn.send(RelayMessage::ok(
                                    &event_id_hex,
                                    false,
                                    "restricted: token scopes incompatible with NIP-AA virtual membership",
                                ));
                                return;
                            }
                            (
                                sprout_auth::AuthMethod::Nip42AgentAuth,
                                Some(owner),
                                intersected,
                            )
                        }
                        None => (sprout_auth::AuthMethod::Nip42ApiToken, None, scopes),
                    };

                    let auth_ctx = sprout_auth::AuthContext {
                        pubkey,
                        scopes: final_scopes,
                        channel_ids: record.channel_ids,
                        auth_method,
                        owner_pubkey: final_owner_pubkey,
                    };

                    *conn.auth_state.write().await = AuthState::Authenticated {
                        ctx: auth_ctx,
                        challenge: challenge.clone(),
                    };
                    state
                        .conn_manager
                        .set_authenticated_pubkey(conn_id, pubkey.serialize().to_vec());
                    conn.send(RelayMessage::ok(&event_id_hex, true, ""));
                }
                Err(e) => {
                    warn!(conn_id = %conn_id, error = %e, "API token verification failed");
                    metrics::counter!("sprout_auth_failures_total", "reason" => "api_token_invalid").increment(1);
                    reauth_fail!(AuthState::Failed);
                    conn.send(RelayMessage::ok(
                        &event_id_hex,
                        false,
                        "auth-required: verification failed",
                    ));
                }
            }
            return;
        }
    }

    // ── NIP-AA agent auth path ──────────────────────────────────────────────
    // If the event carries a NIP-OA `auth` tag but no `auth_token`, this is an
    // agent attempting NIP-AA authentication. Handle it directly rather than
    // going through `verify_auth_event`, which requires a token in production
    // mode and would reject the event before NIP-AA membership is checked.
    let has_auth_tag = event.tags.iter().any(|t| {
        let s = t.as_slice();
        !s.is_empty() && s[0] == "auth"
    });

    if auth_token.is_none() && has_auth_tag {
        // Extract tags and created_at before event is moved into spawn_blocking.
        let event_tags = event.tags.clone().to_vec();
        let event_created_at_ts = event.created_at.as_u64();
        let pubkey = event.pubkey;

        // NIP-42 binding verification (challenge, relay URL, sig, freshness).
        // CPU-intensive crypto — run on the blocking thread pool.
        let event_for_verify = event.clone();
        let challenge_owned = challenge.clone();
        let relay_owned = relay_url.clone();
        match tokio::task::spawn_blocking(move || {
            verify_api_token_nip42_binding(&event_for_verify, &challenge_owned, &relay_owned)
        })
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                warn!(conn_id = %conn_id, error = %e, "NIP-AA: NIP-42 binding verification failed");
                metrics::counter!("sprout_auth_failures_total", "reason" => "nip_aa_nip42_invalid")
                    .increment(1);
                reauth_fail!(AuthState::Failed);
                conn.send(RelayMessage::ok(
                    &event_id_hex,
                    false,
                    &format!("invalid: {e}"),
                ));
                return;
            }
            Err(e) => {
                warn!(conn_id = %conn_id, error = %e, "NIP-AA: NIP-42 verification task panicked");
                metrics::counter!("sprout_auth_failures_total", "reason" => "nip_aa_nip42_internal")
                    .increment(1);
                reauth_fail!(AuthState::Failed);
                conn.send(RelayMessage::ok(
                    &event_id_hex,
                    false,
                    "auth-required: verification failed",
                ));
                return;
            }
        }

        // Relay membership + NIP-AA fallback.
        match enforce_ws_relay_membership(
            &state,
            &conn,
            conn_id,
            &pubkey,
            &event_id_hex,
            &event_tags,
            event_created_at_ts,
        )
        .await
        {
            Some(Some(owner_pubkey)) => {
                // NIP-AA virtual member — grant access with agent auth method.
                // NIP-AA spec: virtual members MUST NOT gain admin privileges.
                let auth_ctx = sprout_auth::AuthContext {
                    pubkey,
                    scopes: sprout_auth::Scope::nip_aa_virtual_member(),
                    channel_ids: None,
                    auth_method: sprout_auth::AuthMethod::Nip42AgentAuth,
                    owner_pubkey: Some(owner_pubkey),
                };
                info!(
                    conn_id = %conn_id,
                    pubkey = %pubkey.to_hex(),
                    owner = %owner_pubkey.to_hex(),
                    "NIP-AA agent auth successful"
                );
                *conn.auth_state.write().await = AuthState::Authenticated {
                    ctx: auth_ctx,
                    challenge: challenge.clone(),
                };
                state
                    .conn_manager
                    .set_authenticated_pubkey(conn_id, pubkey.serialize().to_vec());
                conn.send(RelayMessage::ok(&event_id_hex, true, ""));
                return;
            }
            Some(None) => {
                // Direct member with a dummy auth tag — do NOT grant access here.
                // Fall through to verify_auth_event which enforces token requirements.
            }
            None => {
                // enforce_ws_relay_membership already sent the rejection message.
                return;
            }
        }
        // Direct member fell through — continue to verify_auth_event below.
    }

    // ── Okta JWT / pubkey-only path ─────────────────────────────────────────
    // Non-sprout_ tokens (eyJ* JWTs) and no-token (open-relay) fall through here.
    // Extract tags and created_at before event is consumed by verify_auth_event.
    let event_tags = event.tags.clone().to_vec();
    let event_created_at_ts = event.created_at.as_u64();
    match auth_svc
        .verify_auth_event(event, &challenge, &relay_url)
        .await
    {
        Ok(auth_ctx) => {
            let pubkey = auth_ctx.pubkey;
            // Pubkey allowlist gate — only for pubkey-only auth (no JWT/token).
            // Users with valid API tokens or Okta JWTs bypass the allowlist.
            if state.config.pubkey_allowlist_enabled
                && auth_ctx.auth_method == sprout_auth::AuthMethod::Nip42PubkeyOnly
            {
                let allowed = match state.db.is_pubkey_allowed(&pubkey.serialize()).await {
                    Ok(v) => v,
                    Err(e) => {
                        warn!(conn_id = %conn_id, pubkey = %pubkey.to_hex(), error = %e,
                              "allowlist DB lookup failed, denying (fail-closed)");
                        false
                    }
                };
                if !allowed {
                    warn!(conn_id = %conn_id, pubkey = %pubkey.to_hex(), "pubkey not in allowlist");
                    metrics::counter!("sprout_auth_failures_total", "reason" => "allowlist_denied")
                        .increment(1);
                    reauth_fail!(AuthState::Failed);
                    conn.send(RelayMessage::ok(
                        &event_id_hex,
                        false,
                        "auth-required: verification failed",
                    ));
                    return;
                }
            }
            // Relay membership gate — applies to ALL auth methods; NIP-AA fallback included.
            let nip_aa_owner = match enforce_ws_relay_membership(
                &state,
                &conn,
                conn_id,
                &pubkey,
                &event_id_hex,
                &event_tags,
                event_created_at_ts,
            )
            .await
            {
                Some(owner) => owner,
                None => return,
            };

            // If NIP-AA granted access, upgrade the auth context.
            // NIP-AA spec: virtual members MUST NOT be granted admin privileges.
            // Intersect the original scopes with the virtual-member set — do NOT replace
            // them. Replacing could widen a read-only JWT/pubkey-only session to full
            // write access. Intersection preserves the original restrictions while
            // stripping any admin scopes.
            //
            // Empty intersection: pubkey-only auth has empty scopes (treated as
            // unrestricted in dev/NIP-98 paths). For NIP-AA virtual members, we
            // use the full virtual-member scope set instead of the empty intersection
            // to avoid the empty-means-unrestricted footgun. This is safe: the
            // virtual-member set already excludes admin scopes by design.
            let final_ctx = match nip_aa_owner {
                Some(owner_pubkey) => {
                    info!(
                        conn_id = %conn_id,
                        pubkey = %pubkey.to_hex(),
                        owner = %owner_pubkey.to_hex(),
                        "NIP-42 auth successful via NIP-AA"
                    );
                    let allowed = sprout_auth::Scope::nip_aa_virtual_member();
                    let final_scopes = if auth_ctx.scopes.is_empty() {
                        // Pubkey-only / NIP-98 path: no token scopes to intersect.
                        // Use the full virtual-member set (already excludes admin).
                        allowed
                    } else {
                        let intersected: Vec<sprout_auth::Scope> = auth_ctx
                            .scopes
                            .iter()
                            .filter(|s| allowed.contains(s))
                            .cloned()
                            .collect();
                        if intersected.is_empty() {
                            warn!(conn_id = %conn_id, pubkey = %pubkey.to_hex(),
                                  "NIP-AA: scope intersection is empty — denying (would widen access)");
                            metrics::counter!("sprout_auth_failures_total", "reason" => "nip_aa_empty_scope").increment(1);
                            reauth_fail!(AuthState::Failed);
                            conn.send(RelayMessage::ok(
                                &event_id_hex,
                                false,
                                "restricted: token scopes incompatible with NIP-AA virtual membership",
                            ));
                            return;
                        }
                        intersected
                    };
                    sprout_auth::AuthContext {
                        owner_pubkey: Some(owner_pubkey),
                        auth_method: sprout_auth::AuthMethod::Nip42AgentAuth,
                        scopes: final_scopes,
                        ..auth_ctx
                    }
                }
                None => {
                    info!(conn_id = %conn_id, pubkey = %pubkey.to_hex(), "NIP-42 auth successful");
                    auth_ctx
                }
            };

            *conn.auth_state.write().await = AuthState::Authenticated {
                ctx: final_ctx,
                challenge: challenge.clone(),
            };
            state
                .conn_manager
                .set_authenticated_pubkey(conn_id, pubkey.serialize().to_vec());
            conn.send(RelayMessage::ok(&event_id_hex, true, ""));
        }
        Err(e) => {
            // NIP-AA spec §Step 1: when the AUTH event contains an `auth` tag (NIP-AA
            // attempt), Step 1 failures MUST use the "invalid:" prefix. For standard
            // NIP-42 events without an auth tag, "auth-required:" is the correct prefix.
            let has_auth_tag_in_event = event_tags
                .iter()
                .any(|t| !t.as_slice().is_empty() && t.as_slice()[0] == "auth");
            let prefix = if has_auth_tag_in_event {
                "invalid"
            } else {
                "auth-required"
            };
            warn!(conn_id = %conn_id, error = %e, "NIP-42 auth failed");
            metrics::counter!("sprout_auth_failures_total", "reason" => "nip42_invalid")
                .increment(1);
            reauth_fail!(AuthState::Failed);
            conn.send(RelayMessage::ok(
                &event_id_hex,
                false,
                &format!("{prefix}: verification failed"),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use nostr::{Event, EventBuilder, Keys, Tag, Url};
    use sprout_auth::AuthError;

    use super::*;

    const TEST_RELAY: &str = "wss://relay.example.com";

    fn make_api_token_auth_event(
        keys: &Keys,
        challenge: &str,
        relay_url: &str,
        token: &str,
    ) -> Event {
        let url: Url = relay_url.parse().expect("valid relay url");
        let auth_token = Tag::parse(&["auth_token", token]).expect("valid auth_token tag");
        EventBuilder::auth(challenge, url)
            .add_tags(vec![auth_token])
            .sign_with_keys(keys)
            .expect("signing failed")
    }

    #[test]
    fn api_token_auth_still_requires_a_valid_nip42_challenge() {
        let keys = Keys::generate();
        let challenge = sprout_auth::generate_challenge();
        let event =
            make_api_token_auth_event(&keys, &challenge, TEST_RELAY, "sprout_test_api_token");

        assert!(matches!(
            verify_api_token_nip42_binding(&event, "wrong-challenge", TEST_RELAY),
            Err(AuthError::ChallengeMismatch)
        ));
    }

    #[test]
    fn api_token_auth_accepts_a_valid_nip42_proof() {
        let keys = Keys::generate();
        let challenge = sprout_auth::generate_challenge();
        let event =
            make_api_token_auth_event(&keys, &challenge, TEST_RELAY, "sprout_test_api_token");

        assert!(verify_api_token_nip42_binding(&event, &challenge, TEST_RELAY).is_ok());
    }

    /// When an AUTH event has BOTH an `auth_token` AND an `auth` (NIP-OA) tag,
    /// the token path runs first. A bad token must be rejected immediately —
    /// we must NOT fall through to NIP-AA. This test verifies the NIP-42
    /// binding check (which runs before the DB lookup) still rejects on a
    /// wrong challenge even when an `auth` tag is present.
    #[test]
    fn token_path_runs_first_when_both_auth_token_and_auth_tag_present() {
        let keys = Keys::generate();
        let challenge = sprout_auth::generate_challenge();
        let url: Url = TEST_RELAY.parse().expect("valid relay url");
        let auth_token_tag =
            Tag::parse(&["auth_token", "sprout_test"]).expect("valid auth_token tag");
        // Craft a dummy NIP-OA auth tag (content doesn't matter for this test).
        let auth_tag = Tag::parse(&["auth", "owner_hex", "", "sig_hex"]).expect("valid auth tag");
        let event = EventBuilder::auth(&challenge, url)
            .add_tags(vec![auth_token_tag, auth_tag])
            .sign_with_keys(&keys)
            .expect("signing failed");

        // The token path runs first — wrong challenge must be rejected even
        // though a (dummy) NIP-OA auth tag is present.
        assert!(matches!(
            verify_api_token_nip42_binding(&event, "wrong-challenge", TEST_RELAY),
            Err(AuthError::ChallengeMismatch)
        ));
    }
}
