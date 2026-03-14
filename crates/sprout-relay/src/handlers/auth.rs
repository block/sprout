//! NIP-42 AUTH handler — verify challenge response, transition auth state.

use std::sync::Arc;

use sha2::{Digest, Sha256};
use tracing::{debug, info, warn};

use crate::connection::{AuthState, ConnectionState};
use crate::protocol::RelayMessage;
use crate::state::AppState;

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

    if let Some(ref token) = auth_token {
        if token.starts_with("sprout_") {
            // ── API token path ──────────────────────────────────────────────
            // Hash the raw token and look it up in the DB. The relay owns this
            // path; sprout-auth has no DB access.
            let hash: [u8; 32] = Sha256::digest(token.as_bytes()).into();

            let record = match state.db.get_api_token_by_hash(&hash).await {
                Ok(r) => r,
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
                        auth_method: sprout_auth::AuthMethod::Nip42ApiToken,
                    };
                    // API token users have already proven authorization via their token —
                    // the pubkey allowlist does not apply here.
                    *conn.auth_state.write().await = AuthState::Authenticated(auth_ctx);
                    conn.send(RelayMessage::ok(&event_id_hex, true, ""));
                }
                Err(e) => {
                    warn!(conn_id = %conn_id, error = %e, "API token verification failed");
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
                if !state
                    .db
                    .is_pubkey_allowed(&pubkey.serialize())
                    .await
                    .unwrap_or(false)
                {
                    warn!(conn_id = %conn_id, pubkey = %pubkey.to_hex(), "pubkey not in allowlist");
                    *conn.auth_state.write().await = AuthState::Failed;
                    conn.send(RelayMessage::ok(
                        &event_id_hex,
                        false,
                        "auth-required: verification failed",
                    ));
                    return;
                }
            }
            info!(conn_id = %conn_id, pubkey = %pubkey.to_hex(), "NIP-42 auth successful");
            *conn.auth_state.write().await = AuthState::Authenticated(auth_ctx);
            conn.send(RelayMessage::ok(&event_id_hex, true, ""));
        }
        Err(e) => {
            warn!(conn_id = %conn_id, error = %e, "NIP-42 auth failed");
            *conn.auth_state.write().await = AuthState::Failed;
            conn.send(RelayMessage::ok(
                &event_id_hex,
                false,
                "auth-required: verification failed",
            ));
        }
    }
}
