//! Identity registration endpoint for proxy/hybrid identity mode.
//!
//! In proxy mode, the desktop client generates its own Nostr keypair locally.
//! This endpoint binds the client's public key to its corporate identity
//! (UID + device) so the relay can resolve identity on subsequent requests.
//!
//! The endpoint is only available when `SPROUT_IDENTITY_MODE=proxy` or `hybrid`.
//!
//! # Trusted-proxy assumption
//!
//! The relay trusts the `x-forwarded-identity-token` and
//! `x-block-client-cert-subject-cn` headers unconditionally.
//! It MUST be deployed behind a trusted reverse proxy (cf-doorman) that is the
//! sole source of these headers.

use std::sync::Arc;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Json,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

use crate::state::AppState;

/// `POST /api/identity/register`
///
/// Binds the caller's Nostr public key to their corporate identity (UID + device).
/// The caller proves key ownership via a NIP-98 signed event in the `Authorization`
/// header.
///
/// # Headers
///
/// - `x-forwarded-identity-token`: Corporate identity JWT (injected by cf-doorman)
/// - `x-block-client-cert-subject-cn`: Device identifier from client certificate
/// - `Authorization: Nostr <base64>`: NIP-98 signed event proving pubkey ownership
///
/// # Binding semantics
///
/// - First request from a (UID, device) pair: creates a new binding.
/// - Subsequent requests with the same pubkey: succeeds (idempotent).
/// - Request with a different pubkey for an already-bound (UID, device): returns
///   409 Conflict with `identity_binding_mismatch`.
///
/// # Response
///
/// ```json
/// {
///   "pubkey": "abcd1234…",
///   "username": "alice",
///   "binding_status": "created"
/// }
/// ```
pub async fn identity_register(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if !state.auth.identity_config().mode.is_proxy() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "not_available",
                "message": "identity registration is only available in proxy identity mode"
            })),
        ));
    }

    // 1. Validate proxy identity JWT → extract uid + username
    let identity_jwt = headers
        .get("x-forwarded-identity-token")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "identity_token_required",
                    "message": "x-forwarded-identity-token header is required"
                })),
            )
        })?;

    let (identity_claims, _scopes) = state
        .auth
        .validate_identity_jwt(identity_jwt)
        .await
        .map_err(|e| {
            tracing::warn!("identity register: JWT validation failed: {e}");
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "identity_token_invalid" })),
            )
        })?;

    // 2. Extract device identifier from client certificate CN (fallback to "default")
    let device_cn = super::extract_device_cn(&headers);

    // 3. Verify NIP-98 auth to prove pubkey ownership
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "authorization_required",
                    "message": "Authorization: Nostr <base64> header is required for identity registration"
                })),
            )
        })?;

    let nostr_b64 = auth_header.strip_prefix("Nostr ").ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "nip98_required",
                "message": "identity registration requires NIP-98 auth (Authorization: Nostr <base64>)"
            })),
        )
    })?;

    let decoded_bytes = BASE64.decode(nostr_b64).map_err(|_| {
        tracing::warn!("identity register: NIP-98 base64 decode failed");
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "nip98_invalid" })),
        )
    })?;

    let event_json = String::from_utf8(decoded_bytes).map_err(|_| {
        tracing::warn!("identity register: NIP-98 decoded bytes are not valid UTF-8");
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "nip98_invalid" })),
        )
    })?;

    let canonical_url = reconstruct_canonical_url(&state);

    let pubkey = sprout_auth::verify_nip98_event(&event_json, &canonical_url, "POST", None)
        .map_err(|e| {
            tracing::warn!("identity register: NIP-98 verification failed: {e}");
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "nip98_invalid" })),
            )
        })?;

    let pubkey_bytes = pubkey.serialize().to_vec();

    // 4. Bind or validate the identity
    let result = state
        .db
        .bind_or_validate_identity(
            &identity_claims.uid,
            device_cn,
            &pubkey_bytes,
            &identity_claims.username,
        )
        .await
        .map_err(|e| {
            tracing::error!("identity register: DB error: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal_error" })),
            )
        })?;

    match result {
        sprout_db::BindingResult::Created => {
            // Invalidate cached `false` so the identity-bound guard takes
            // effect immediately on this relay instance.
            state.identity_bound_cache.invalidate(&pubkey_bytes);
            tracing::info!(
                uid = %identity_claims.uid,
                device_cn = %device_cn,
                pubkey = %pubkey.to_hex(),
                "identity binding created"
            );
        }
        sprout_db::BindingResult::Matched => {
            tracing::info!(
                uid = %identity_claims.uid,
                device_cn = %device_cn,
                pubkey = %pubkey.to_hex(),
                "identity binding matched"
            );
        }
        sprout_db::BindingResult::Mismatch { .. } => {
            tracing::warn!(
                uid = %identity_claims.uid,
                device_cn = %device_cn,
                presented = %pubkey.to_hex(),
                "identity binding mismatch"
            );
            return Err((
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "error": "identity_binding_mismatch",
                    "message": "this device is already bound to a different pubkey"
                })),
            ));
        }
    }

    // 5. Ensure user record exists with verified name
    if let Err(e) = state
        .db
        .ensure_user_with_verified_name(&pubkey_bytes, &identity_claims.username)
        .await
    {
        tracing::warn!("identity register: ensure_user_with_verified_name failed: {e}");
    }

    let binding_status = match result {
        sprout_db::BindingResult::Created => "created",
        sprout_db::BindingResult::Matched => "existing",
        sprout_db::BindingResult::Mismatch { .. } => unreachable!(),
    };

    Ok(Json(serde_json::json!({
        "pubkey": pubkey.to_hex(),
        "username": identity_claims.username,
        "binding_status": binding_status,
    })))
}

fn reconstruct_canonical_url(state: &AppState) -> String {
    let base = state
        .config
        .relay_url
        .replace("wss://", "https://")
        .replace("ws://", "http://");
    format!("{base}/api/identity/register")
}
