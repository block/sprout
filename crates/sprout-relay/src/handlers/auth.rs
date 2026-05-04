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
/// - `Some(None)`                              — direct member (or membership not required), access granted
/// - `Some(Some((owner_pubkey, expiry)))`      — NIP-AA virtual member, access granted; expiry from `created_at<T`
/// - `None`                                    — access denied (rejection message already sent)
async fn enforce_ws_relay_membership(
    state: &AppState,
    conn: &Arc<ConnectionState>,
    conn_id: uuid::Uuid,
    pubkey: &nostr::PublicKey,
    event_id_hex: &str,
    tags: &[nostr::Tag],
    event_created_at: u64,
) -> Option<Option<(nostr::PublicKey, Option<u64>)>> {
    if !state.config.require_relay_membership {
        return Some(None);
    }

    let pubkey_hex = pubkey.to_hex();
    let is_member = match state.db.is_relay_member(&pubkey_hex).await {
        Ok(v) => v,
        Err(e) => {
            // Fail closed — cannot authoritatively determine membership, must deny.
            // Do NOT fall through to NIP-AA. Caller uses reauth_fail! per NIP-AA §6.
            warn!(
                conn_id = %conn_id,
                pubkey = %pubkey_hex,
                error = %e,
                "relay membership check failed, denying (fail-closed)"
            );
            metrics::counter!("sprout_auth_failures_total", "reason" => "membership_db_error")
                .increment(1);
            conn.send(RelayMessage::ok(
                event_id_hex,
                false,
                "restricted: agent authentication failed",
            ));
            return None;
        }
    };

    if is_member {
        return Some(None);
    }

    // Not a direct member — try NIP-AA fallback.
    // Caller uses reauth_fail! on None returns per NIP-AA §6.
    match nip_aa::verify_nip_aa(state, pubkey, tags, event_created_at).await {
        Ok(Some(result)) => {
            debug!(
                conn_id = %conn_id,
                agent = %pubkey_hex,
                owner = %result.owner_pubkey.to_hex(),
                "NIP-AA: virtual membership granted"
            );
            Some(Some((result.owner_pubkey, result.session_expiry)))
        }
        Ok(None) => {
            warn!(conn_id = %conn_id, pubkey = %pubkey_hex, "not a relay member");
            metrics::counter!("sprout_auth_failures_total", "reason" => "not_relay_member")
                .increment(1);
            conn.send(RelayMessage::ok(
                event_id_hex,
                false,
                "restricted: not a relay member",
            ));
            None
        }
        Err(reason) => {
            warn!(conn_id = %conn_id, pubkey = %pubkey_hex, reason = %reason, "NIP-AA verification failed");
            metrics::counter!("sprout_auth_failures_total", "reason" => "nip_aa_invalid")
                .increment(1);
            conn.send(RelayMessage::ok(event_id_hex, false, &reason));
            None
        }
    }
}

/// Clear all subscriptions for a connection on re-auth.
///
/// Must be called BEFORE writing the new `AuthState` to prevent a privilege-leak
/// window where old subscriptions (potentially wider scopes) could still receive
/// events after the identity has changed or scopes have narrowed.
///
/// No-op on initial auth (`prev_auth_ctx` is `None`).
async fn clear_subscriptions_on_reauth(
    conn: &Arc<ConnectionState>,
    state: &AppState,
    conn_id: uuid::Uuid,
    prev_auth_ctx: &Option<sprout_auth::AuthContext>,
) {
    if prev_auth_ctx.is_some() {
        conn.subscriptions.lock().await.clear();
        state.sub_registry.remove_connection(conn_id);
    }
}

fn verify_api_token_nip42_binding(
    event: &nostr::Event,
    challenge: &str,
    relay_url: &str,
) -> Result<(), sprout_auth::AuthError> {
    sprout_auth::verify_nip42_event(event, challenge, relay_url)
}

// NOTE: Sprout uses a single-pubkey-at-a-time model — re-auth replaces the current
// credential. A failed re-auth preserves the existing identity per NIP-AA §6.
// TODO: per-pubkey auth state map for simultaneous multi-identity connections.

/// Handle a NIP-42 AUTH message: verify the challenge response and transition the connection to authenticated state.
pub async fn handle_auth(event: nostr::Event, conn: Arc<ConnectionState>, state: Arc<AppState>) {
    let event_id_hex_early = event.id.to_hex();

    // Rate-limit ALL auth attempts (initial and re-auth) to 500ms per connection.
    // Acquire the sync mutex briefly to read/update the timestamp, then drop it
    // before any await so we never hold a sync lock across an await point.
    {
        const AUTH_MIN_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);
        let sleep_for = {
            let mut last = match conn.last_auth_at.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    warn!("last_auth_at mutex poisoned — recovering");
                    poisoned.into_inner()
                }
            };
            let remaining = last
                .map(|t| {
                    let elapsed = t.elapsed();
                    if elapsed < AUTH_MIN_INTERVAL {
                        Some(AUTH_MIN_INTERVAL - elapsed)
                    } else {
                        None
                    }
                })
                .flatten();
            *last = Some(std::time::Instant::now());
            remaining
        };
        if let Some(delay) = sleep_for {
            tokio::time::sleep(delay).await;
        }
    }

    // Extract challenge, conn_id, and (for re-auth) the previous AuthContext.
    // NIP-AA §6: a failed re-auth must not invalidate the existing identity —
    // we save prev_auth_ctx and restore it on failure instead of going to Failed.
    let (challenge, conn_id, prev_auth_ctx) = {
        let auth = conn.auth_state.read().await;
        match &*auth {
            AuthState::Pending { challenge } => (challenge.clone(), conn.conn_id, None),
            AuthState::Authenticated { challenge, ctx } => {
                if event.pubkey != ctx.pubkey {
                    info!(
                        conn_id = %conn.conn_id,
                        existing = %ctx.pubkey.to_hex(),
                        incoming = %event.pubkey.to_hex(),
                        "re-auth: identity switch (single-pubkey-at-a-time model)"
                    );
                } else {
                    debug!(
                        conn_id = %conn.conn_id,
                        pubkey = %ctx.pubkey.to_hex(),
                        "NIP-AA §6: re-auth credential replacement"
                    );
                }
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

    // On re-auth failure, restore the previous AuthContext (NIP-AA §6).
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

    // Extract auth_token tag — sprout_* API tokens need DB access, so they're
    // handled here rather than in verify_auth_event().
    let auth_token = event.tags.iter().find_map(|tag| {
        let vec = tag.as_slice();
        if vec.len() >= 2 && vec[0] == "auth_token" {
            Some(vec[1].to_string())
        } else {
            None
        }
    });

    metrics::counter!("sprout_auth_attempts_total", "method" => if auth_token.as_ref().is_some_and(|t| t.starts_with("sprout_")) { "api_token" } else { "nip42" }).increment(1);

    // When both `auth_token` and `auth` (NIP-OA) tags are present, the token path
    // runs first. A bad token is rejected immediately — we do NOT fall through to
    // NIP-AA. This prevents a confused-deputy attack where an attacker appends a
    // valid NIP-OA credential to a stolen token hoping the relay accepts it via NIP-AA.
    // The NIP-AA path only runs when `auth_token` is absent entirely.
    if let Some(ref token) = auth_token {
        if token.starts_with("sprout_") {
            // ── API token path ──────────────────────────────────────────────
            // event_tags intentionally NOT extracted — token path passes empty slice
            // to enforce_ws_relay_membership (confused-deputy rule).
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

                    // Relay membership gate — applies to ALL auth methods.
                    // Confused-deputy rule: pass empty tag slice so verify_nip_aa cannot
                    // escalate a valid token to NIP-AA virtual membership.
                    let nip_aa_owner = match enforce_ws_relay_membership(
                        &state,
                        &conn,
                        conn_id,
                        &pubkey,
                        &event_id_hex,
                        &[], // Token path: auth tag intentionally ignored per confused-deputy rule
                        event_created_at_ts,
                    )
                    .await
                    {
                        Some(owner) => owner,
                        None => {
                            reauth_fail!(AuthState::Failed);
                            return;
                        }
                    };

                    // NIP-AA: intersect token scopes with virtual-member set — do NOT replace.
                    // Replacing could widen a read-only token to full write access.
                    // Empty intersection (e.g. admin-only token) → deny rather than grant.
                    //
                    // NOTE: The Some(owner) arm below is intentionally unreachable in practice.
                    // The token path always passes `&[]` (empty tags) to enforce_ws_relay_membership
                    // per the confused-deputy rule, so verify_nip_aa always returns Ok(None) here.
                    // We retain the full match for defense-in-depth: if the empty-tags invariant is
                    // ever violated, the scope intersection logic still prevents privilege escalation.
                    #[allow(unreachable_patterns)]
                    let (auth_method, final_owner_pubkey, final_scopes, final_session_expiry) =
                        match nip_aa_owner {
                            Some((owner, expiry)) => {
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
                                    expiry,
                                )
                            }
                            None => (sprout_auth::AuthMethod::Nip42ApiToken, None, scopes, None),
                        };

                    let auth_ctx = sprout_auth::AuthContext {
                        pubkey,
                        scopes: final_scopes,
                        channel_ids: record.channel_ids,
                        auth_method,
                        owner_pubkey: final_owner_pubkey,
                        session_expiry: final_session_expiry,
                    };

                    // 1. Bump epoch first — invalidates any in-flight REQs using the old epoch.
                    if prev_auth_ctx.is_some() {
                        conn.auth_epoch
                            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    }
                    // 2. Clear old subscriptions.
                    clear_subscriptions_on_reauth(&conn, &state, conn_id, &prev_auth_ctx).await;
                    // 3. Write new auth state.
                    *conn.auth_state.write().await = AuthState::Authenticated {
                        ctx: auth_ctx,
                        challenge: challenge.clone(),
                    };
                    state
                        .conn_manager
                        .set_authenticated_pubkey(conn_id, pubkey.serialize().to_vec());
                    if let Some(owner) = final_owner_pubkey {
                        state
                            .conn_manager
                            .set_owner_pubkey(conn_id, owner.serialize().to_vec());
                    } else {
                        // Clear stale NIP-AA owner if re-authing as direct member.
                        state.conn_manager.clear_owner_pubkey(conn_id);
                    }
                    // NIP-AA §Expiry: schedule connection teardown at session expiry.
                    // This ensures open subscriptions don't survive past the delegation window.
                    // The connection's CancellationToken triggers graceful shutdown of all loops.
                    if let Some(expiry_ts) = final_session_expiry {
                        let now_ts = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        if now_ts < expiry_ts {
                            let duration = std::time::Duration::from_secs(expiry_ts - now_ts);
                            let cancel_token = conn.cancel.clone();
                            let conn_id_for_log = conn_id;
                            let epoch_at_spawn =
                                conn.auth_epoch.load(std::sync::atomic::Ordering::SeqCst);
                            let conn_for_expiry = Arc::clone(&conn);
                            tokio::spawn(async move {
                                tokio::time::sleep(duration).await;
                                let current_epoch = conn_for_expiry
                                    .auth_epoch
                                    .load(std::sync::atomic::Ordering::SeqCst);
                                if current_epoch != epoch_at_spawn {
                                    debug!(
                                        conn_id = %conn_id_for_log,
                                        "NIP-AA expiry timer stale (epoch {epoch_at_spawn} → {current_epoch}) — skipping"
                                    );
                                    return;
                                }
                                info!(
                                    conn_id = %conn_id_for_log,
                                    "NIP-AA session expired — closing connection"
                                );
                                cancel_token.cancel();
                            });
                        } else {
                            // Already expired — close immediately
                            warn!(conn_id = %conn_id, "NIP-AA session already expired at auth time — closing");
                            conn.cancel.cancel();
                            return;
                        }
                    }
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
                    "invalid: verification failed",
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
                    "invalid: verification failed",
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
            Some(Some((owner_pubkey, session_expiry))) => {
                // NIP-AA virtual member — grant access with agent auth method.
                // NIP-AA spec: virtual members MUST NOT gain admin privileges.
                let auth_ctx = sprout_auth::AuthContext {
                    pubkey,
                    scopes: sprout_auth::Scope::nip_aa_virtual_member(),
                    channel_ids: None,
                    auth_method: sprout_auth::AuthMethod::Nip42AgentAuth,
                    owner_pubkey: Some(owner_pubkey),
                    session_expiry,
                };
                info!(
                    conn_id = %conn_id,
                    pubkey = %pubkey.to_hex(),
                    owner = %owner_pubkey.to_hex(),
                    "NIP-AA agent auth successful"
                );
                // 1. Bump epoch first — invalidates any in-flight REQs using the old epoch.
                if prev_auth_ctx.is_some() {
                    conn.auth_epoch
                        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
                // 2. Clear old subscriptions.
                clear_subscriptions_on_reauth(&conn, &state, conn_id, &prev_auth_ctx).await;
                // 3. Write new auth state.
                *conn.auth_state.write().await = AuthState::Authenticated {
                    ctx: auth_ctx,
                    challenge: challenge.clone(),
                };
                state
                    .conn_manager
                    .set_authenticated_pubkey(conn_id, pubkey.serialize().to_vec());
                state
                    .conn_manager
                    .set_owner_pubkey(conn_id, owner_pubkey.serialize().to_vec());
                // NIP-AA §Expiry: schedule connection teardown at session expiry.
                // This ensures open subscriptions don't survive past the delegation window.
                // The connection's CancellationToken triggers graceful shutdown of all loops.
                if let Some(expiry_ts) = session_expiry {
                    let now_ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    if now_ts < expiry_ts {
                        let duration = std::time::Duration::from_secs(expiry_ts - now_ts);
                        let cancel_token = conn.cancel.clone();
                        let conn_id_for_log = conn_id;
                        let epoch_at_spawn =
                            conn.auth_epoch.load(std::sync::atomic::Ordering::SeqCst);
                        let conn_for_expiry = Arc::clone(&conn);
                        tokio::spawn(async move {
                            tokio::time::sleep(duration).await;
                            let current_epoch = conn_for_expiry
                                .auth_epoch
                                .load(std::sync::atomic::Ordering::SeqCst);
                            if current_epoch != epoch_at_spawn {
                                debug!(
                                    conn_id = %conn_id_for_log,
                                    "NIP-AA expiry timer stale (epoch {epoch_at_spawn} → {current_epoch}) — skipping"
                                );
                                return;
                            }
                            info!(
                                conn_id = %conn_id_for_log,
                                "NIP-AA session expired — closing connection"
                            );
                            cancel_token.cancel();
                        });
                    } else {
                        // Already expired — close immediately
                        warn!(conn_id = %conn_id, "NIP-AA session already expired at auth time — closing");
                        conn.cancel.cancel();
                        return;
                    }
                }
                conn.send(RelayMessage::ok(&event_id_hex, true, ""));
                return;
            }
            Some(None) => {
                // Direct member with a dummy auth tag — fall through to verify_auth_event.
            }
            None => {
                // enforce_ws_relay_membership already sent the rejection message.
                reauth_fail!(AuthState::Failed);
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
            // Confused-deputy prevention: pass empty tag slice so a non-agent user who
            // happens to carry an `auth` tag cannot be escalated via the NIP-AA path.
            // Only the dedicated NIP-AA agent path (above) passes real event tags.
            let nip_aa_owner = match enforce_ws_relay_membership(
                &state,
                &conn,
                conn_id,
                &pubkey,
                &event_id_hex,
                &[], // Okta/JWT path: auth tags intentionally ignored per confused-deputy rule
                event_created_at_ts,
            )
            .await
            {
                Some(owner) => owner,
                None => {
                    reauth_fail!(AuthState::Failed);
                    return;
                }
            };

            // NIP-AA: intersect original scopes with virtual-member set — do NOT replace.
            // Pubkey-only auth has empty scopes; use the full virtual-member set to avoid
            // the empty-means-unrestricted footgun (virtual-member set excludes admin).
            let final_ctx = match nip_aa_owner {
                Some((owner_pubkey, session_expiry)) => {
                    info!(
                        conn_id = %conn_id,
                        pubkey = %pubkey.to_hex(),
                        owner = %owner_pubkey.to_hex(),
                        "NIP-42 auth successful via NIP-AA"
                    );
                    let allowed = sprout_auth::Scope::nip_aa_virtual_member();
                    let final_scopes = if auth_ctx.scopes.is_empty() {
                        // Pubkey-only / NIP-98: no token scopes to intersect — use full set.
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
                        session_expiry,
                        ..auth_ctx
                    }
                }
                None => {
                    info!(conn_id = %conn_id, pubkey = %pubkey.to_hex(), "NIP-42 auth successful");
                    // Direct member: ensure session_expiry is None (already set by verify_auth_event)
                    auth_ctx
                }
            };

            let nip_aa_owner_for_conn = final_ctx.owner_pubkey;
            let final_ctx_session_expiry = final_ctx.session_expiry;
            // 1. Bump epoch first — invalidates any in-flight REQs using the old epoch.
            if prev_auth_ctx.is_some() {
                conn.auth_epoch
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
            // 2. Clear old subscriptions.
            clear_subscriptions_on_reauth(&conn, &state, conn_id, &prev_auth_ctx).await;
            // 3. Write new auth state.
            *conn.auth_state.write().await = AuthState::Authenticated {
                ctx: final_ctx,
                challenge: challenge.clone(),
            };
            state
                .conn_manager
                .set_authenticated_pubkey(conn_id, pubkey.serialize().to_vec());
            if let Some(owner) = nip_aa_owner_for_conn {
                state
                    .conn_manager
                    .set_owner_pubkey(conn_id, owner.serialize().to_vec());
            } else {
                // Clear stale NIP-AA owner if re-authing as direct member.
                state.conn_manager.clear_owner_pubkey(conn_id);
            }
            // NIP-AA §Expiry: schedule connection teardown at session expiry.
            // This ensures open subscriptions don't survive past the delegation window.
            // The connection's CancellationToken triggers graceful shutdown of all loops.
            if let Some(expiry_ts) = final_ctx_session_expiry {
                let now_ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                if now_ts < expiry_ts {
                    let duration = std::time::Duration::from_secs(expiry_ts - now_ts);
                    let cancel_token = conn.cancel.clone();
                    let conn_id_for_log = conn_id;
                    let epoch_at_spawn = conn.auth_epoch.load(std::sync::atomic::Ordering::SeqCst);
                    let conn_for_expiry = Arc::clone(&conn);
                    tokio::spawn(async move {
                        tokio::time::sleep(duration).await;
                        let current_epoch = conn_for_expiry
                            .auth_epoch
                            .load(std::sync::atomic::Ordering::SeqCst);
                        if current_epoch != epoch_at_spawn {
                            debug!(
                                conn_id = %conn_id_for_log,
                                "NIP-AA expiry timer stale (epoch {epoch_at_spawn} → {current_epoch}) — skipping"
                            );
                            return;
                        }
                        info!(
                            conn_id = %conn_id_for_log,
                            "NIP-AA session expired — closing connection"
                        );
                        cancel_token.cancel();
                    });
                } else {
                    // Already expired — close immediately
                    warn!(conn_id = %conn_id, "NIP-AA session already expired at auth time — closing");
                    conn.cancel.cancel();
                    return;
                }
            }
            conn.send(RelayMessage::ok(&event_id_hex, true, ""));
        }
        Err(e) => {
            // NIP-AA §Step 1: use "invalid:" prefix when an auth tag is present,
            // "auth-required:" for standard NIP-42 events without one.
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

    /// Verify that intersecting admin-only scopes with the NIP-AA virtual
    /// member set produces an empty result — admin privileges must never be
    /// granted through the NIP-AA path.
    #[test]
    fn nip_aa_scope_intersection_strips_admin() {
        let admin_only = [
            sprout_auth::Scope::AdminChannels,
            sprout_auth::Scope::AdminUsers,
        ];
        let allowed = sprout_auth::Scope::nip_aa_virtual_member();
        let intersected: Vec<_> = admin_only
            .iter()
            .filter(|s| allowed.contains(s))
            .cloned()
            .collect();
        assert!(
            intersected.is_empty(),
            "admin scopes must not survive NIP-AA intersection"
        );
    }

    /// Verify that normal read/write scopes survive intersection with the
    /// virtual member set — NIP-AA agents must retain messaging access.
    #[test]
    fn nip_aa_scope_intersection_preserves_read_write() {
        let token_scopes = [
            sprout_auth::Scope::MessagesRead,
            sprout_auth::Scope::MessagesWrite,
        ];
        let allowed = sprout_auth::Scope::nip_aa_virtual_member();
        let intersected: Vec<_> = token_scopes
            .iter()
            .filter(|s| allowed.contains(s))
            .cloned()
            .collect();
        assert_eq!(
            intersected, token_scopes,
            "read/write scopes must survive NIP-AA intersection"
        );
    }

    /// Verify the virtual member scope set itself: admin scopes absent,
    /// core messaging scopes present.
    #[test]
    fn nip_aa_virtual_member_excludes_admin_scopes() {
        let vm = sprout_auth::Scope::nip_aa_virtual_member();
        assert!(
            !vm.contains(&sprout_auth::Scope::AdminChannels),
            "virtual member must not include AdminChannels"
        );
        assert!(
            !vm.contains(&sprout_auth::Scope::AdminUsers),
            "virtual member must not include AdminUsers"
        );
        assert!(
            vm.contains(&sprout_auth::Scope::MessagesRead),
            "virtual member must include MessagesRead"
        );
        assert!(
            vm.contains(&sprout_auth::Scope::MessagesWrite),
            "virtual member must include MessagesWrite"
        );
    }
}
