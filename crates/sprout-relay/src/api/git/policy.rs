//! Internal policy endpoint — pre-receive hook callback.
//!
//! The pre-receive hook POSTs here with HMAC-signed payload containing
//! the pusher's pubkey, repo ID, and ref updates. This endpoint:
//!
//! 1. Validates HMAC signature + 30s TTL (fail-closed)
//! 2. Resolves kind:30617 → protection rules
//! 3. Resolves pusher's channel role via sprout-channel binding
//! 4. Calls `sprout_core::git_perms::evaluate_push()`
//! 5. Returns 200 (allow) or 403 (deny with reasons)
//!
//! Security invariants:
//! - Endpoint binds to 127.0.0.1 only (enforced at router level)
//! - HMAC binds callback to the specific push operation
//! - Fail-closed: any error → 403

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tracing::{error, warn};

use uuid::Uuid;

use sprout_core::channel::MemberRole;
use sprout_core::git_perms::{evaluate_push, parse_protection_tags, Denial, RefUpdate, UpdateKind};
use sprout_db::EventQuery;

use crate::state::AppState;

// ── Types ────────────────────────────────────────────────────────────────────

/// Maximum age of a hook callback (seconds). Push is synchronous so 30s is generous.
const MAX_CALLBACK_AGE_SECS: u64 = 30;

/// Request payload from the pre-receive hook.
#[derive(Debug, Deserialize)]
pub struct HookCallbackRequest {
    /// Repo identifier (d-tag from kind:30617).
    pub repo_id: String,
    /// Hex-encoded pusher pubkey.
    pub pusher_pubkey: String,
    /// Ref updates from git stdin (old_oid, new_oid, ref_name, is_ancestor).
    pub ref_updates: Vec<HookRefUpdate>,
    /// Unix timestamp when the hook was invoked.
    pub timestamp: u64,
    /// HMAC-SHA256 signature over the canonical payload.
    pub signature: String,
}

/// A single ref update as reported by the pre-receive hook.
#[derive(Debug, Clone, Deserialize)]
pub struct HookRefUpdate {
    /// Old object ID (40 hex chars, zero OID for creates).
    pub old_oid: String,
    /// New object ID (40 hex chars, zero OID for deletes).
    pub new_oid: String,
    /// Full ref name (e.g., "refs/heads/main").
    pub ref_name: String,
    /// Result of `git merge-base --is-ancestor old new`.
    /// For creates/deletes this is false (ignored by classifier).
    pub is_ancestor: bool,
}

/// Response to the hook — either allow or deny.
#[derive(Debug, Serialize)]
pub struct HookCallbackResponse {
    /// Whether the push is allowed.
    pub allowed: bool,
    /// Denial reasons (empty if allowed).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub denials: Vec<DenialResponse>,
}

/// A single denial reason in the hook response.
#[derive(Debug, Serialize)]
pub struct DenialResponse {
    /// The ref that was denied.
    pub ref_name: String,
    /// Human-readable reason for denial.
    pub reason: String,
}

impl From<Denial> for DenialResponse {
    fn from(d: Denial) -> Self {
        Self {
            ref_name: d.ref_name,
            reason: d.reason,
        }
    }
}

// ── HMAC Verification ────────────────────────────────────────────────────────

/// Compute the canonical HMAC payload.
///
/// Format: `repo_id || pusher_pubkey || ref_updates_json || timestamp`
/// where ref_updates_json is the sorted, deterministic JSON of ref updates.
fn compute_hmac(secret: &[u8], req: &HookCallbackRequest) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC can take key of any size");

    mac.update(req.repo_id.as_bytes());
    mac.update(b"|");
    mac.update(req.pusher_pubkey.as_bytes());
    mac.update(b"|");
    // Deterministic ref update representation: sorted by ref_name.
    let mut refs_sorted: Vec<&HookRefUpdate> = req.ref_updates.iter().collect();
    refs_sorted.sort_by(|a, b| a.ref_name.cmp(&b.ref_name));
    for r in &refs_sorted {
        mac.update(r.old_oid.as_bytes());
        mac.update(r.new_oid.as_bytes());
        mac.update(r.ref_name.as_bytes());
    }
    mac.update(b"|");
    mac.update(req.timestamp.to_string().as_bytes());

    mac.finalize().into_bytes().to_vec()
}

/// Verify the HMAC signature on a hook callback.
fn verify_hmac(secret: &[u8], req: &HookCallbackRequest) -> bool {
    let expected = compute_hmac(secret, req);
    let provided = match hex::decode(&req.signature) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    // Constant-time comparison.
    use subtle::ConstantTimeEq;
    expected.ct_eq(&provided).into()
}

// ── Handler ──────────────────────────────────────────────────────────────────

/// `POST /internal/git/policy` — pre-receive hook callback.
///
/// Fail-closed: ANY error returns 403. The hook script treats non-200 as deny.
pub async fn hook_policy_check(
    State(state): State<Arc<AppState>>,
    Json(req): Json<HookCallbackRequest>,
) -> Response {
    // 1. Validate HMAC signature.
    let secret = state.config.git_hook_hmac_secret.as_bytes();
    if !verify_hmac(secret, &req) {
        warn!(repo = %req.repo_id, "hook callback: HMAC verification failed");
        return (StatusCode::FORBIDDEN, "signature verification failed").into_response();
    }

    // 2. Validate timestamp (30s TTL).
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if now.saturating_sub(req.timestamp) > MAX_CALLBACK_AGE_SECS {
        warn!(repo = %req.repo_id, age = now.saturating_sub(req.timestamp), "hook callback: expired");
        return (StatusCode::FORBIDDEN, "callback expired").into_response();
    }

    // 3. Resolve kind:30617 for this repo.
    // Query by d_tag (repo_id) + kind 30617. The d_tag is globally unique per relay.
    let query = EventQuery {
        kinds: Some(vec![30617]),
        d_tag: Some(req.repo_id.clone()),
        global_only: true,
        limit: Some(1),
        ..Default::default()
    };
    let repo_event = match state.db.query_events(&query).await {
        Ok(mut events) => {
            if let Some(event) = events.pop() {
                event
            } else {
                warn!(repo = %req.repo_id, "hook callback: kind:30617 not found");
                return (StatusCode::FORBIDDEN, "repository not found").into_response();
            }
        }
        Err(e) => {
            error!(repo = %req.repo_id, error = %e, "hook callback: DB error");
            return (StatusCode::FORBIDDEN, "internal error").into_response();
        }
    };

    // 4. Parse protection rules from kind:30617 tags.
    let tags: Vec<Vec<String>> = repo_event
        .event
        .tags
        .iter()
        .map(|t| t.as_slice().to_vec())
        .collect();

    let rules = match parse_protection_tags(&tags) {
        Ok(rules) => rules,
        Err(e) => {
            warn!(repo = %req.repo_id, error = %e, "hook callback: malformed protection tags");
            // Fail-closed: malformed rules = deny.
            return (StatusCode::FORBIDDEN, "malformed protection rules").into_response();
        }
    };

    // 5. Resolve pusher's role.
    let repo_owner_hex = hex::encode(repo_event.event.pubkey.to_bytes());
    let role = if req.pusher_pubkey == repo_owner_hex {
        MemberRole::Owner
    } else {
        // Look up sprout-channel tag to find the bound channel.
        let channel_id = tags
            .iter()
            .find(|t| t.first().map(|s| s.as_str()) == Some("sprout-channel"))
            .and_then(|t| t.get(1))
            .and_then(|id| Uuid::parse_str(id).ok());

        match channel_id {
            None => {
                warn!(repo = %req.repo_id, "hook callback: no sprout-channel binding");
                return (StatusCode::FORBIDDEN, "no channel binding").into_response();
            }
            Some(ch_id) => {
                let pusher_bytes = match hex::decode(&req.pusher_pubkey) {
                    Ok(b) if b.len() == 32 => b,
                    _ => {
                        return (StatusCode::FORBIDDEN, "invalid pusher pubkey").into_response();
                    }
                };
                match state.db.get_member_role(ch_id, &pusher_bytes).await {
                    Ok(Some(role_str)) => match role_str.parse::<MemberRole>() {
                        Ok(role) => role,
                        Err(_) => {
                            error!(role = %role_str, "hook callback: unknown role");
                            return (StatusCode::FORBIDDEN, "internal error").into_response();
                        }
                    },
                    Ok(None) => {
                        return (StatusCode::FORBIDDEN, "not a channel member").into_response();
                    }
                    Err(e) => {
                        error!(error = %e, "hook callback: role lookup failed");
                        return (StatusCode::FORBIDDEN, "internal error").into_response();
                    }
                }
            }
        }
    };

    // 6. Classify ref updates and evaluate policy.
    let updates: Vec<RefUpdate> = req
        .ref_updates
        .iter()
        .map(|r| RefUpdate {
            ref_name: r.ref_name.clone(),
            kind: UpdateKind::classify(&r.old_oid, &r.new_oid, r.is_ancestor),
            old_oid: r.old_oid.clone(),
            new_oid: r.new_oid.clone(),
        })
        .collect();

    match evaluate_push(&updates, role, &rules) {
        Ok(()) => Json(HookCallbackResponse {
            allowed: true,
            denials: vec![],
        })
        .into_response(),
        Err(denials) => {
            let response = HookCallbackResponse {
                allowed: false,
                denials: denials.into_iter().map(DenialResponse::from).collect(),
            };
            (StatusCode::FORBIDDEN, Json(response)).into_response()
        }
    }
}

// ── HMAC Generation (for the relay to pass to the hook) ──────────────────────

/// Generate the HMAC signature for a hook callback payload.
///
/// Called by the relay when setting up the pre-receive hook environment.
pub fn generate_hook_hmac(
    secret: &[u8],
    repo_id: &str,
    pusher_pubkey: &str,
    ref_updates: &[HookRefUpdate],
    timestamp: u64,
) -> String {
    let req = HookCallbackRequest {
        repo_id: repo_id.to_string(),
        pusher_pubkey: pusher_pubkey.to_string(),
        ref_updates: ref_updates.to_vec(),
        timestamp,
        signature: String::new(), // Not used in computation.
    };
    let mac_bytes = compute_hmac(secret, &req);
    hex::encode(mac_bytes)
}
