//! NIP-42 AUTH handler — verify challenge response, transition auth state.
//!
//! API token authentication (`sprout_*` tokens) is intercepted here before
//! reaching [`AuthService::verify_auth_event`], because token verification
//! requires a database lookup that `sprout-auth` intentionally does not own.

use std::sync::Arc;

use tracing::{debug, info, warn};

use sprout_auth::{hash_token, verify_nip42_event, AuthContext, AuthMethod};

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
    let event_id_hex = event.id.to_hex();

    // ── Check for a `sprout_` API token in the auth event ────────────────
    // API tokens require a DB lookup, so the relay intercepts them here
    // rather than inside AuthService (which has no database access).
    let api_token = event
        .tags
        .iter()
        .find(|t| t.kind().to_string() == "auth_token")
        .and_then(|t| t.content())
        .filter(|v| v.starts_with("sprout_"))
        .map(|s| s.to_string());

    let result = if let Some(raw_token) = api_token {
        verify_api_token_auth(&event, &challenge, &relay_url, &raw_token, &state).await
    } else {
        // JWT or no-token path — delegate entirely to AuthService.
        let auth_svc = Arc::clone(&state.auth);
        auth_svc
            .verify_auth_event(event, &challenge, &relay_url)
            .await
    };

    match result {
        Ok(auth_ctx) => {
            let pubkey = auth_ctx.pubkey;
            info!(conn_id = %conn_id, pubkey = %pubkey.to_hex(), method = ?auth_ctx.auth_method, "NIP-42 auth successful");
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

/// Verify a NIP-42 AUTH event that carries a `sprout_` API token.
///
/// 1. Verify the NIP-42 event structure + Schnorr signature.
/// 2. Hash the raw token and look it up in the database.
/// 3. Delegate to [`AuthService::verify_api_token_against_hash`] for
///    constant-time hash comparison, expiry, pubkey, and scope resolution.
/// 4. Update `last_used_at` on success.
async fn verify_api_token_auth(
    event: &nostr::Event,
    challenge: &str,
    relay_url: &str,
    raw_token: &str,
    state: &AppState,
) -> Result<AuthContext, sprout_auth::AuthError> {
    // Step 1: verify NIP-42 signature, challenge, relay URL, timestamp.
    let event_clone = event.clone();
    let challenge_owned = challenge.to_string();
    let relay_owned = relay_url.to_string();
    tokio::task::spawn_blocking(move || {
        verify_nip42_event(&event_clone, &challenge_owned, &relay_owned)
    })
    .await
    .map_err(|_| sprout_auth::AuthError::Internal("spawn_blocking panicked".into()))??;

    // Step 2: look up the token in the database by its SHA-256 hash.
    let token_hash = hash_token(raw_token);
    let record = state
        .db
        .get_api_token_by_hash(&token_hash)
        .await
        .map_err(|_| sprout_auth::AuthError::TokenInvalid)?;

    // Step 3: constant-time verify + expiry + pubkey + scope resolution.
    let owner_pubkey = nostr::PublicKey::from_slice(&record.owner_pubkey)
        .map_err(|_| sprout_auth::AuthError::TokenInvalid)?;

    let (pubkey, scopes) = state.auth.verify_api_token_against_hash(
        raw_token,
        &record.token_hash,
        &owner_pubkey,
        &event.pubkey,
        record.expires_at,
        &record.scopes,
    )?;

    // Step 4: update last_used_at (best-effort, don't fail auth on this).
    let _ = state.db.update_token_last_used(&token_hash).await;

    Ok(AuthContext {
        pubkey,
        scopes,
        auth_method: AuthMethod::Nip42ApiToken,
    })
}
