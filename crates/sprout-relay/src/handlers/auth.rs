//! NIP-42 AUTH handler — verify challenge response, transition auth state.

use std::sync::Arc;

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

/// Handle a NIP-42 AUTH message: verify the challenge response and transition
/// the connection to authenticated state.
///
/// Pure crypto verification — no API tokens, no JWT, no DB token lookups.
pub async fn handle_auth(event: nostr::Event, conn: Arc<ConnectionState>, state: Arc<AppState>) {
    let event_id_hex = event.id.to_hex();
    let (challenge, conn_id) = {
        let auth = conn.auth_state.read().await;
        match &*auth {
            AuthState::Pending { challenge } => (challenge.clone(), conn.conn_id),
            AuthState::Authenticated(_) => {
                debug!(conn_id = %conn.conn_id, "AUTH received but already authenticated");
                conn.send(RelayMessage::ok(
                    &event_id_hex,
                    false,
                    "auth-required: already authenticated",
                ));
                return;
            }
            AuthState::Failed => {
                debug!(conn_id = %conn.conn_id, "AUTH received after failed auth");
                conn.send(RelayMessage::ok(
                    &event_id_hex,
                    false,
                    "auth-required: authentication already failed",
                ));
                return;
            }
        }
    };

    let relay_url = state.config.relay_url.clone();
    let auth_svc = Arc::clone(&state.auth);

    metrics::counter!("sprout_auth_attempts_total", "method" => "nip42").increment(1);

    // Pure NIP-42 verification — crypto only, no DB lookups.
    match auth_svc
        .verify_auth_event(event, &challenge, &relay_url)
        .await
    {
        Ok(auth_ctx) => {
            let pubkey = auth_ctx.pubkey;

            // Pubkey allowlist gate — only for pubkey-only auth.
            if state.config.pubkey_allowlist_enabled
                && auth_ctx.auth_method == sprout_auth::AuthMethod::Nip42
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

            // Relay membership gate — applies to all auth methods.
            if !enforce_ws_relay_membership(&state, &conn, conn_id, &pubkey, &event_id_hex).await {
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
