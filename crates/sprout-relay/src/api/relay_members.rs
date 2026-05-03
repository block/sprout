//! Relay membership enforcement and read endpoints.
//!
//! ## Enforcement
//! [`enforce_relay_membership`] is the single gate for REST paths — called at every
//! authenticated REST entry point. When `require_relay_membership` is disabled, it's
//! a no-op. NIP-AA (WebSocket/NIP-42 only) is handled exclusively in
//! `handlers::auth` via `enforce_ws_relay_membership`.
//!
//! ## Routes
//! - `GET /api/relay/members`    — list all relay members (any authenticated member)
//! - `GET /api/relay/members/me` — get own membership record (or 404)

use std::sync::Arc;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Json,
};

use sprout_auth::Scope;

use super::{extract_auth_context, extract_auth_context_inner, internal_error};
use crate::state::AppState;

// ── Enforcement ───────────────────────────────────────────────────────────────

/// Enforce relay membership for a pubkey on REST paths.
///
/// - If `config.require_relay_membership` is false → always `Ok(())` (no-op).
/// - If enabled → checks `relay_members` table.
///   - Direct member → `Ok(())`.
///   - Not a direct member → `Err(403)`.
///
/// `pubkey_bytes` is the 32-byte compressed pubkey; it is hex-encoded before
/// the DB lookup (the `relay_members` table stores 64-char hex strings).
///
/// # NIP-AA is WebSocket-only
///
/// NIP-AA (NIP-42 agent auth) is handled exclusively in `handlers::auth` via
/// `enforce_ws_relay_membership`. REST paths (NIP-98, Blossom, git HTTP) perform
/// direct membership checks only — never NIP-AA. Passing NIP-AA credentials to
/// a REST endpoint is a client error.
pub async fn enforce_relay_membership(
    state: &AppState,
    pubkey_bytes: &[u8],
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if !state.config.require_relay_membership {
        return Ok(());
    }

    let pubkey_hex = hex::encode(pubkey_bytes);
    let is_member = state
        .db
        .is_relay_member(&pubkey_hex)
        .await
        .map_err(|e| internal_error(&format!("relay membership check failed: {e}")))?;

    if is_member {
        return Ok(());
    }

    Err((
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({
            "error": "relay_membership_required",
            "message": "You must be a relay member to access this relay"
        })),
    ))
}

// ── REST read handlers ────────────────────────────────────────────────────────

/// `GET /api/relay/members` — list all relay members.
///
/// Any authenticated relay member can call this. The membership gate is
/// enforced by `extract_auth_context` (which wraps the inner extractor).
pub async fn list_relay_members(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // extract_auth_context enforces relay membership
    let ctx = extract_auth_context(&headers, &state).await?;

    // Require at least UsersRead scope to enumerate relay members.
    // Empty scopes means NIP-98 auth (implicit full access) — skip the check.
    if !ctx.scopes.is_empty()
        && !ctx.scopes.contains(&Scope::UsersRead)
        && !ctx.scopes.contains(&Scope::AdminUsers)
    {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "insufficient_scope",
                "message": "Requires users:read or admin:users scope"
            })),
        ));
    }

    let members = state
        .db
        .list_relay_members()
        .await
        .map_err(|e| internal_error(&format!("list relay members: {e}")))?;

    let items: Vec<serde_json::Value> = members
        .into_iter()
        .map(|m| {
            serde_json::json!({
                "pubkey": m.pubkey,
                "role": m.role,
                "added_by": m.added_by,
                "created_at": m.created_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "members": items })))
}

/// `GET /api/relay/members/me` — get own membership record.
///
/// Uses the inner auth extractor (no membership gate) so non-members
/// get a proper 404 instead of 403.
pub async fn get_my_relay_membership(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context_inner(&headers, &state).await?;
    let pubkey_hex = hex::encode(&ctx.pubkey_bytes);

    let member = state
        .db
        .get_relay_member(&pubkey_hex)
        .await
        .map_err(|e| internal_error(&format!("get relay member: {e}")))?;

    match member {
        Some(m) => Ok(Json(serde_json::json!({
            "pubkey": m.pubkey,
            "role": m.role,
            "added_by": m.added_by,
            "created_at": m.created_at.to_rfc3339(),
        }))),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "not_a_member",
                "message": "You are not a relay member"
            })),
        )),
    }
}
