//! Identity bootstrap endpoint for proxy/hybrid identity mode.
//!
//! In proxy mode, the desktop client cannot derive its own Nostr keypair because
//! the derivation secret is held only by the relay. This endpoint validates the
//! client's identity JWT (injected by cf-doorman) and returns the derived secret
//! key so the client can sign events locally.
//!
//! The endpoint is only available when `SPROUT_IDENTITY_MODE=proxy` or `hybrid`.
//!
//! # Trusted-proxy assumption
//!
//! The relay trusts the `x-forwarded-identity-token` header unconditionally.
//! It MUST be deployed behind a trusted reverse proxy (cf-doorman) that is the
//! sole source of this header. If the relay port is directly reachable, an
//! attacker could inject arbitrary identity headers.

use std::sync::Arc;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Json,
};

use crate::state::AppState;

/// `POST /api/identity/bootstrap`
///
/// Validates the caller's `x-forwarded-identity-token` JWT and returns the
/// derived Nostr secret key (hex-encoded) for that user.
///
/// Uses POST (not GET) because the response contains a secret key that must
/// never be cached by intermediaries.
///
/// # Security
///
/// - Only available when `SPROUT_IDENTITY_MODE=proxy` or `hybrid`.
/// - The identity JWT is validated via JWKS (signature, issuer, audience, expiry).
/// - Each caller only receives **their own** derived key — never another user's.
/// - The derivation secret (`SPROUT_IDENTITY_SECRET`) never leaves the relay.
/// - Transport is TLS behind cf-doorman; the secret key travels only over the
///   authenticated, encrypted channel.
/// - Response includes `Cache-Control: no-store` to prevent intermediary caching.
///
/// # Response
///
/// ```json
/// {
///   "pubkey": "abcd1234…",
///   "secret_key": "ef012345…",
///   "username": "alice"
/// }
/// ```
pub async fn identity_bootstrap(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<(StatusCode, HeaderMap, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)>
{
    if !state.auth.identity_config().mode.is_proxy() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "not_available",
                "message": "identity bootstrap is only available in proxy identity mode"
            })),
        ));
    }

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

    let (keys, _scopes, username) = state
        .auth
        .validate_identity_jwt_keys(identity_jwt)
        .await
        .map_err(|e| {
            tracing::warn!("identity bootstrap: JWT validation failed: {e}");
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "identity_token_invalid" })),
            )
        })?;

    let pubkey_bytes = keys.public_key().serialize().to_vec();
    if let Err(e) = state
        .db
        .ensure_user_with_verified_name(&pubkey_bytes, &username)
        .await
    {
        tracing::warn!("identity bootstrap: ensure_user_with_verified_name failed: {e}");
    }

    // ⚠️ SECURITY: secret_key is logged at no level — it is sensitive material.
    // The response travels over TLS behind cf-doorman.
    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(
        axum::http::header::CACHE_CONTROL,
        "no-store, private, max-age=0".parse().unwrap(),
    );
    resp_headers.insert(axum::http::header::PRAGMA, "no-cache".parse().unwrap());

    Ok((
        StatusCode::OK,
        resp_headers,
        Json(serde_json::json!({
            "pubkey": keys.public_key().to_hex(),
            "secret_key": keys.secret_key().to_secret_hex(),
            "username": username,
        })),
    ))
}
