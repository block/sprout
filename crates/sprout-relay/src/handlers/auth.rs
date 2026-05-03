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
            warn!(
                conn_id = %conn_id,
                pubkey = %pubkey_hex,
                error = %e,
                "relay membership check failed, denying (fail-closed)"
            );
            false
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
            *conn.auth_state.write().await = AuthState::Failed;
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
            *conn.auth_state.write().await = AuthState::Failed;
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
    let (challenge, conn_id) = {
        let auth = conn.auth_state.read().await;
        match &*auth {
            AuthState::Pending { challenge } => (challenge.clone(), conn.conn_id),
            AuthState::Authenticated(_) => {
                debug!(conn_id = %conn.conn_id, "AUTH received but already authenticated");
                conn.send(RelayMessage::ok(
                    &event_id_hex_early,
                    false,
                    "auth-required: already authenticated",
                ));
                return;
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
                    *conn.auth_state.write().await = AuthState::Failed;
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
                    *conn.auth_state.write().await = AuthState::Failed;
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
                    *conn.auth_state.write().await = AuthState::Failed;
                    conn.send(RelayMessage::ok(
                        &event_id_hex,
                        false,
                        "auth-required: invalid token",
                    ));
                    return;
                }
                Err(e) => {
                    warn!(conn_id = %conn_id, error = %e, "API token lookup failed");
                    *conn.auth_state.write().await = AuthState::Failed;
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
                    *conn.auth_state.write().await = AuthState::Failed;
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

                    let (auth_method, owner_pubkey) = match nip_aa_owner {
                        Some(owner) => (sprout_auth::AuthMethod::Nip42AgentAuth, Some(owner)),
                        None => (sprout_auth::AuthMethod::Nip42ApiToken, None),
                    };

                    let auth_ctx = sprout_auth::AuthContext {
                        pubkey,
                        scopes,
                        channel_ids: record.channel_ids,
                        auth_method,
                        owner_pubkey,
                    };

                    *conn.auth_state.write().await = AuthState::Authenticated(auth_ctx);
                    state
                        .conn_manager
                        .set_authenticated_pubkey(conn_id, pubkey.serialize().to_vec());
                    conn.send(RelayMessage::ok(&event_id_hex, true, ""));
                }
                Err(e) => {
                    warn!(conn_id = %conn_id, error = %e, "API token verification failed");
                    metrics::counter!("sprout_auth_failures_total", "reason" => "api_token_invalid").increment(1);
                    *conn.auth_state.write().await = AuthState::Failed;
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
                    *conn.auth_state.write().await = AuthState::Failed;
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
            let final_ctx = match nip_aa_owner {
                Some(owner_pubkey) => {
                    info!(
                        conn_id = %conn_id,
                        pubkey = %pubkey.to_hex(),
                        owner = %owner_pubkey.to_hex(),
                        "NIP-42 auth successful via NIP-AA"
                    );
                    sprout_auth::AuthContext {
                        owner_pubkey: Some(owner_pubkey),
                        auth_method: sprout_auth::AuthMethod::Nip42AgentAuth,
                        ..auth_ctx
                    }
                }
                None => {
                    info!(conn_id = %conn_id, pubkey = %pubkey.to_hex(), "NIP-42 auth successful");
                    auth_ctx
                }
            };

            *conn.auth_state.write().await = AuthState::Authenticated(final_ctx);
            state
                .conn_manager
                .set_authenticated_pubkey(conn_id, pubkey.serialize().to_vec());
            conn.send(RelayMessage::ok(&event_id_hex, true, ""));
        }
        Err(e) => {
            // NIP-42 verification failure — use "auth-required:" prefix per NIP-42 spec.
            //
            // NIP-AA spec §Step 1 says to use "invalid:" for Step 1 failures, but that
            // language applies to the relay's response *after* it has determined this is
            // a NIP-AA attempt (i.e., after Step 2 fails and Step 3 finds an auth tag).
            // Standard NIP-42 verification happens *before* NIP-AA is even considered, so
            // "auth-required:" is the correct prefix here. Changing it would break
            // standard NIP-42 clients that expect "auth-required:" on verification failure.
            warn!(conn_id = %conn_id, error = %e, "NIP-42 auth failed");
            metrics::counter!("sprout_auth_failures_total", "reason" => "nip42_invalid")
                .increment(1);
            *conn.auth_state.write().await = AuthState::Failed;
            conn.send(RelayMessage::ok(
                &event_id_hex,
                false,
                "auth-required: verification failed",
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
}
