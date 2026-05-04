//! NIP-42 AUTH handler — verify challenge response, transition auth state.

use std::sync::Arc;

use sha2::{Digest, Sha256};
use sprout_sdk::nip_oa;
use tracing::{debug, info, warn};

use crate::connection::{AuthState, ConnectionState};
use crate::protocol::RelayMessage;
use crate::state::AppState;

/// Check relay membership for a pubkey during NIP-42 auth.
///
/// Returns `true` if the pubkey is a relay member (or if membership enforcement
/// is disabled). Returns `false` and sends a rejection message if not a member.
async fn enforce_ws_relay_membership(
    state: &AppState,
    conn: &Arc<ConnectionState>,
    conn_id: uuid::Uuid,
    pubkey: &nostr::PublicKey,
    event_id_hex: &str,
) -> bool {
    if !state.config.require_relay_membership {
        return true;
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

    if !is_member {
        warn!(conn_id = %conn_id, pubkey = %pubkey_hex, "not a relay member");
        metrics::counter!("sprout_auth_failures_total", "reason" => "not_relay_member")
            .increment(1);
        *conn.auth_state.write().await = AuthState::Failed;
        conn.send(RelayMessage::ok(
            event_id_hex,
            false,
            "restricted: not a relay member",
        ));
        return false;
    }

    true
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
                    let auth_ctx = sprout_auth::AuthContext {
                        pubkey,
                        scopes,
                        channel_ids: record.channel_ids,
                        auth_method: sprout_auth::AuthMethod::Nip42ApiToken,
                        owner_pubkey: None,
                    };
                    // API token users have already proven authorization via their token —
                    // the pubkey allowlist does not apply here.

                    // Relay membership gate (NIP-43) — applies to ALL auth methods.
                    if !enforce_ws_relay_membership(&state, &conn, conn_id, &pubkey, &event_id_hex)
                        .await
                    {
                        return;
                    }

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

    // ── NIP-OA: extract auth tag before event is consumed ────────────────
    // Only when the relay operator has opted in via SPROUT_ALLOW_NIP_OA_AUTH=true.
    // NIP-OA spec: exactly 0 or 1 `auth` tags per event; 2+ is invalid.
    let nip_oa_auth_tag: Option<String> = if state.config.allow_nip_oa_auth {
        let auth_tags: Vec<_> = event
            .tags
            .iter()
            .filter(|tag| {
                let s = tag.as_slice();
                s.len() == 4 && s[0] == "auth"
            })
            .collect();

        match auth_tags.len() {
            0 => None,
            1 => {
                let slice = auth_tags[0].as_slice();
                Some(serde_json::json!([slice[0], slice[1], slice[2], slice[3]]).to_string())
            }
            n => {
                warn!(
                    conn_id = %conn.conn_id,
                    count = n,
                    "AUTH event contains multiple auth tags, rejecting"
                );
                metrics::counter!("sprout_auth_failures_total", "reason" => "nip_oa_multiple_tags")
                    .increment(1);
                *conn.auth_state.write().await = AuthState::Failed;
                conn.send(RelayMessage::ok(
                    &event.id.to_hex(),
                    false,
                    "auth-required: multiple auth tags not allowed",
                ));
                return;
            }
        }
    } else {
        None
    };
    let agent_pubkey = event.pubkey;

    match auth_svc
        .verify_auth_event(event, &challenge, &relay_url)
        .await
    {
        Ok(auth_ctx) => {
            let pubkey = auth_ctx.pubkey;
            // Pubkey allowlist gate — only for pubkey-only auth (no JWT/token).
            // NOTE: The allowlist gates which keys may *connect*. For NIP-OA,
            // the agent is the connecting party, so the allowlist correctly
            // checks the agent's pubkey. The NIP-43 membership check (below)
            // separately verifies the owner is a relay member.
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

            // ── NIP-OA: verify owner attestation if present ──────────────────
            let (auth_ctx, membership_pubkey) = if let Some(ref tag_json) = nip_oa_auth_tag {
                // Defense-in-depth: verify_auth_event already checks pubkey match,
                // but NIP-OA verification depends on the agent pubkey being the
                // event signer, so assert the invariant explicitly.
                debug_assert_eq!(
                    agent_pubkey, auth_ctx.pubkey,
                    "agent pubkey must match authenticated pubkey for NIP-OA"
                );
                match nip_oa::verify_auth_tag(tag_json, &agent_pubkey) {
                    Ok(owner_pubkey) => {
                        info!(
                            conn_id = %conn_id,
                            agent = %agent_pubkey.to_hex(),
                            owner = %owner_pubkey.to_hex(),
                            "NIP-OA owner attestation verified"
                        );
                        let mut ctx = auth_ctx;
                        ctx.auth_method = sprout_auth::AuthMethod::Nip42OwnerAttestation;
                        ctx.owner_pubkey = Some(owner_pubkey);
                        (ctx, owner_pubkey)
                    }
                    Err(e) => {
                        warn!(
                            conn_id = %conn_id,
                            agent = %agent_pubkey.to_hex(),
                            error = %e,
                            "NIP-OA auth tag verification failed"
                        );
                        metrics::counter!("sprout_auth_failures_total", "reason" => "nip_oa_invalid")
                            .increment(1);
                        *conn.auth_state.write().await = AuthState::Failed;
                        conn.send(RelayMessage::ok(
                            &event_id_hex,
                            false,
                            "auth-required: owner attestation verification failed",
                        ));
                        return;
                    }
                }
            } else {
                (auth_ctx, pubkey)
            };

            // Relay membership gate (NIP-43) — check owner for NIP-OA, agent otherwise.
            if !enforce_ws_relay_membership(
                &state,
                &conn,
                conn_id,
                &membership_pubkey,
                &event_id_hex,
            )
            .await
            {
                return;
            }

            info!(conn_id = %conn_id, pubkey = %pubkey.to_hex(), "NIP-42 auth successful");
            *conn.auth_state.write().await = AuthState::Authenticated(auth_ctx);
            state
                .conn_manager
                .set_authenticated_pubkey(conn_id, pubkey.serialize().to_vec());
            conn.send(RelayMessage::ok(&event_id_hex, true, ""));
        }
        Err(e) => {
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
