//! NIP-42 AUTH handler — verify challenge response, transition auth state.

use std::sync::Arc;

use sha2::{Digest, Sha256};
use tracing::{debug, info, warn};

use crate::connection::{AuthState, ConnectionState};
use crate::protocol::RelayMessage;
use crate::state::AppState;

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
    let (challenge, proxy_identity, conn_id) = {
        let auth = conn.auth_state.read().await;
        match &*auth {
            AuthState::Pending {
                challenge,
                proxy_identity,
            } => (challenge.clone(), proxy_identity.clone(), conn.conn_id),
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

    metrics::counter!("sprout_auth_attempts_total", "method" => if proxy_identity.is_some() { "proxy_identity" } else if auth_token.as_ref().is_some_and(|t| t.starts_with("sprout_")) { "api_token" } else { "nip42" }).increment(1);

    // ── Proxy identity path ─────────────────────────────────────────────
    // When proxy identity claims were validated at upgrade time, the AUTH
    // event only needs to prove the client owns its pubkey.  No JWT or API
    // token tag is required — the identity was already established from the
    // proxy headers.  After signature verification, the relay creates or
    // validates the uid → pubkey binding.
    if let Some(proxy) = proxy_identity {
        // Verify event structure + signature + challenge + relay URL (no token check).
        let event_clone = event.clone();
        let challenge_owned = challenge.clone();
        let relay_owned = relay_url.clone();
        let nip42_ok = tokio::task::spawn_blocking(move || {
            sprout_auth::verify_nip42_event(&event_clone, &challenge_owned, &relay_owned)
        })
        .await
        .ok()
        .and_then(|r| r.ok());

        if nip42_ok.is_none() {
            warn!(conn_id = %conn_id, "proxy identity NIP-42 verification failed");
            metrics::counter!("sprout_auth_failures_total", "reason" => "proxy_nip42_invalid")
                .increment(1);
            *conn.auth_state.write().await = AuthState::Failed;
            conn.send(RelayMessage::ok(
                &event_id_hex,
                false,
                "auth-required: verification failed",
            ));
            return;
        }

        // Resolve the uid → pubkey binding.
        let pubkey_bytes = event.pubkey.serialize().to_vec();
        match state
            .db
            .bind_or_validate_identity(&proxy.uid, &pubkey_bytes, &proxy.username)
            .await
        {
            Ok(sprout_db::BindingResult::Created) => {
                // Invalidate cached `false` so the identity-bound guard takes
                // effect immediately — prevents a 2-min window where the pubkey
                // could still authenticate via standard auth.
                state.identity_bound_cache.invalidate(&pubkey_bytes);
                info!(conn_id = %conn_id, uid = %proxy.uid,
                      pubkey = %event.pubkey.to_hex(), "identity binding created");
            }
            Ok(sprout_db::BindingResult::Matched) => {
                info!(conn_id = %conn_id, uid = %proxy.uid, pubkey = %event.pubkey.to_hex(),
                      "identity binding matched");
            }
            Ok(sprout_db::BindingResult::Mismatch { .. }) => {
                warn!(conn_id = %conn_id, uid = %proxy.uid,
                      pubkey = %event.pubkey.to_hex(), "identity binding mismatch");
                metrics::counter!("sprout_auth_failures_total", "reason" => "binding_mismatch")
                    .increment(1);
                *conn.auth_state.write().await = AuthState::Failed;
                conn.send(RelayMessage::ok(
                    &event_id_hex,
                    false,
                    "auth-required: identity binding mismatch — this device is bound to a different key",
                ));
                return;
            }
            Err(e) => {
                warn!(conn_id = %conn_id, error = %e, "identity binding DB error");
                *conn.auth_state.write().await = AuthState::Failed;
                conn.send(RelayMessage::ok(
                    &event_id_hex,
                    false,
                    "auth-required: verification failed",
                ));
                return;
            }
        }

        // Ensure user record exists with verified name.
        if let Err(e) = state
            .db
            .ensure_user_with_verified_name(&pubkey_bytes, &proxy.username)
            .await
        {
            warn!(conn_id = %conn_id, error = %e, "ensure_user_with_verified_name failed");
        }

        let auth_ctx = sprout_auth::AuthContext {
            pubkey: event.pubkey,
            scopes: proxy.scopes,
            channel_ids: None,
            auth_method: sprout_auth::AuthMethod::ProxyIdentity,
        };
        *conn.auth_state.write().await = AuthState::Authenticated(auth_ctx);
        state
            .conn_manager
            .set_authenticated_pubkey(conn_id, pubkey_bytes.clone());
        conn.send(RelayMessage::ok(&event_id_hex, true, ""));
        return;
    }

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
                    // Identity-bound pubkey guard — bound pubkeys must use identity JWT.
                    if state.is_identity_bound(&pubkey.serialize()).await {
                        warn!(conn_id = %conn_id, pubkey = %pubkey.to_hex(),
                              "identity-bound pubkey attempted API token auth without JWT");
                        metrics::counter!("sprout_auth_failures_total", "reason" => "identity_bound_no_jwt")
                            .increment(1);
                        *conn.auth_state.write().await = AuthState::Failed;
                        conn.send(RelayMessage::ok(
                            &event_id_hex,
                            false,
                            "auth-required: this pubkey is bound to a corporate identity — connect via the auth proxy",
                            ));
                        return;
                    }

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
                    };
                    // API token users have already proven authorization via their token —
                    // the pubkey allowlist does not apply here.
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
            // Identity-bound pubkey guard — bound pubkeys must use identity JWT.
            if state.is_identity_bound(&pubkey.serialize()).await {
                warn!(conn_id = %conn_id, pubkey = %pubkey.to_hex(),
                      "identity-bound pubkey attempted standard auth without JWT");
                metrics::counter!("sprout_auth_failures_total", "reason" => "identity_bound_no_jwt")
                    .increment(1);
                *conn.auth_state.write().await = AuthState::Failed;
                conn.send(RelayMessage::ok(
                    &event_id_hex,
                    false,
                    "auth-required: this pubkey is bound to a corporate identity — connect via the auth proxy",
                ));
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
