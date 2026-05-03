//! Relay membership enforcement and read endpoints.
//!
//! ## Enforcement
//! [`enforce_relay_membership`] is the single gate — called at every authenticated
//! entry point. When `require_relay_membership` is disabled, it's a no-op.
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

/// Enforce relay membership for a pubkey, with optional NIP-AA fallback.
///
/// - If `config.require_relay_membership` is false → always `Ok(None)` (no-op).
/// - If enabled → checks `relay_members` table.
///   - Direct member → `Ok(None)`.
///   - Not a direct member and `auth_tag_json` + `event_created_at` provided →
///     attempts NIP-AA: verifies the auth tag, checks `created_at` conditions,
///     and confirms the owner is a relay member. Returns `Ok(Some(owner_pubkey))`
///     on success.
///   - Otherwise → `Err(403)`.
///
/// `pubkey_bytes` is the 32-byte compressed pubkey; it is hex-encoded before
/// the DB lookup (the `relay_members` table stores 64-char hex strings).
///
/// `auth_tag_json` is the JSON-serialised NIP-OA `auth` tag array (e.g.
/// `["auth","<owner_hex>","<conditions>","<sig>"]`). Pass `None` to skip NIP-AA.
///
/// # NIP-AA callers MUST pre-verify the NIP-42 AUTH event
///
/// This function only performs NIP-OA credential verification (Steps 3–5 of
/// NIP-AA). It does NOT verify the NIP-42 `kind:22242` event signature,
/// challenge binding, relay URL, or freshness window (Step 1). Callers that
/// pass `auth_tag_json` MUST have already verified the enclosing NIP-42 AUTH
/// event before calling this function, or they bypass NIP-AA Step 1 entirely.
///
/// REST paths (NIP-98, Blossom, git HTTP) MUST pass `None` — NIP-AA is
/// defined only for NIP-42 WebSocket AUTH flows.
pub async fn enforce_relay_membership(
    state: &AppState,
    pubkey_bytes: &[u8],
    auth_tag_json: Option<&str>,
    event_created_at: Option<u64>,
) -> Result<Option<nostr::PublicKey>, (StatusCode, Json<serde_json::Value>)> {
    if !state.config.require_relay_membership {
        return Ok(None);
    }

    let pubkey_hex = hex::encode(pubkey_bytes);
    let is_member = state
        .db
        .is_relay_member(&pubkey_hex)
        .await
        .map_err(|e| internal_error(&format!("relay membership check failed: {e}")))?;

    if is_member {
        return Ok(None);
    }

    // Not a direct member — attempt NIP-AA if an auth tag was supplied.
    if let (Some(tag_json), Some(created_at)) = (auth_tag_json, event_created_at) {
        let agent_pubkey = nostr::PublicKey::from_slice(pubkey_bytes)
            .map_err(|e| internal_error(&format!("invalid agent pubkey bytes: {e}")))?;

        // Step 4: Pre-validate lowercase hex requirements before calling verify_auth_tag.
        // NIP-OA requires 64-char lowercase hex owner pubkey and 128-char lowercase hex sig.
        // secp256k1's from_hex accepts uppercase, so we enforce the spec constraint here.
        {
            let tag_arr: serde_json::Value = serde_json::from_str(tag_json)
                .map_err(|e| internal_error(&format!("failed to parse auth tag JSON: {e}")))?;
            if let (Some(owner_hex), Some(sig_hex)) = (
                tag_arr.get(1).and_then(|v| v.as_str()),
                tag_arr.get(3).and_then(|v| v.as_str()),
            ) {
                let is_lowercase_hex = |s: &str| {
                    s.chars()
                        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
                };
                if owner_hex.len() != 64 || !is_lowercase_hex(owner_hex) {
                    return Err((
                        StatusCode::FORBIDDEN,
                        Json(serde_json::json!({
                            "error": "nip_aa_invalid",
                            "message": "restricted: owner pubkey must be 64 lowercase hex chars"
                        })),
                    ));
                }
                if sig_hex.len() != 128 || !is_lowercase_hex(sig_hex) {
                    return Err((
                        StatusCode::FORBIDDEN,
                        Json(serde_json::json!({
                            "error": "nip_aa_invalid",
                            "message": "restricted: signature must be 128 lowercase hex chars"
                        })),
                    ));
                }
            }
        }

        // Step 4: Verify the auth tag cryptographically.
        let owner_pubkey =
            sprout_sdk::nip_oa::verify_auth_tag(tag_json, &agent_pubkey).map_err(|e| {
                (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({
                        "error": "nip_aa_invalid",
                        "message": format!("restricted: invalid auth tag: {e}")
                    })),
                )
            })?;

        // Step 4b: Evaluate created_at conditions embedded in the tag.
        // Parse the tag array to extract the conditions string (element [2]).
        let tag_arr: serde_json::Value = serde_json::from_str(tag_json)
            .map_err(|e| internal_error(&format!("failed to parse auth tag JSON: {e}")))?;
        if let Some(conditions) = tag_arr.get(2).and_then(|v| v.as_str()) {
            if !conditions.is_empty() {
                evaluate_rest_created_at_conditions(conditions, created_at).map_err(|reason| {
                    (
                        StatusCode::FORBIDDEN,
                        Json(serde_json::json!({
                            "error": "nip_aa_condition_failed",
                            "message": format!("restricted: {reason}")
                        })),
                    )
                })?;
            }
        }

        // Step 5: Check the owner is an active relay member.
        let owner_hex = owner_pubkey.to_hex();
        let owner_is_member = state
            .db
            .is_relay_member(&owner_hex)
            .await
            .map_err(|e| internal_error(&format!("owner membership check failed: {e}")))?;

        if owner_is_member {
            tracing::info!(
                agent = %pubkey_hex,
                owner = %owner_hex,
                "NIP-AA: virtual membership granted (REST)"
            );
            return Ok(Some(owner_pubkey));
        }

        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "nip_aa_owner_not_member",
                "message": "restricted: owner is not a relay member"
            })),
        ));
    }

    Err((
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({
            "error": "relay_membership_required",
            "message": "You must be a relay member to access this relay"
        })),
    ))
}

/// Evaluate `created_at<t` and `created_at>t` conditions for REST NIP-AA checks.
/// Returns `Ok(())` if all conditions pass, `Err(reason)` if any fail.
/// `kind=` conditions are intentionally skipped per NIP-AA spec.
fn evaluate_rest_created_at_conditions(
    conditions: &str,
    event_created_at: u64,
) -> Result<(), String> {
    for clause in conditions.split('&') {
        if clause.is_empty() {
            continue;
        }
        if let Some(val_str) = clause.strip_prefix("created_at<") {
            let threshold: u64 = val_str
                .parse()
                .map_err(|_| format!("malformed created_at< condition: {clause}"))?;
            if event_created_at >= threshold {
                return Err(format!(
                    "created_at condition not satisfied: {event_created_at} >= {threshold}"
                ));
            }
        } else if let Some(val_str) = clause.strip_prefix("created_at>") {
            let threshold: u64 = val_str
                .parse()
                .map_err(|_| format!("malformed created_at> condition: {clause}"))?;
            if event_created_at <= threshold {
                return Err(format!(
                    "created_at condition not satisfied: {event_created_at} <= {threshold}"
                ));
            }
        }
        // kind= clauses are intentionally skipped at admission per NIP-AA §Kind Conditions
    }
    Ok(())
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
