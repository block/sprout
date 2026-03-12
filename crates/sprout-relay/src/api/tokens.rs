//! Self-service API token minting, listing, and revocation endpoints.
//!
//! ## Routes
//! - `POST   /api/tokens`      — mint a new token (NIP-98 bootstrap or Bearer)
//! - `GET    /api/tokens`      — list own tokens (Bearer required)
//! - `DELETE /api/tokens/{id}` — revoke one token by UUID (Bearer required)
//! - `DELETE /api/tokens`      — revoke all tokens / panic button (Bearer required)
//!
//! ## Rate Limiting
//! [`MintRateLimiter`] enforces a configurable per-pubkey-per-hour limit
//! (default 50, override with `SPROUT_MINT_RATE_LIMIT`) using a bounded
//! in-memory rolling-window cache (moka). Resets on restart — acceptable since
//! the limit is a DoS guard, not a hard security cap.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use chrono::Utc;
use moka::sync::Cache;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use nostr::JsonUtil;
use sprout_auth::{is_self_mintable, Scope};

use super::{api_error, extract_auth_context, internal_error, RestAuthMethod};
use crate::state::AppState;

// ── Rate limiter ──────────────────────────────────────────────────────────────

/// Default: 50 mints per pubkey per hour. Override with `SPROUT_MINT_RATE_LIMIT`.
const DEFAULT_MINT_LIMIT: usize = 50;
const MINT_WINDOW: Duration = Duration::from_secs(3600);
const RATE_LIMITER_MAX_ENTRIES: u64 = 100_000;

/// Per-pubkey rolling-window rate limiter for `POST /api/tokens`.
///
/// Uses a bounded moka cache (max 100,000 entries) to prevent OOM from an
/// attacker flooding with unique pubkeys. Evicted entries lose their window
/// history — worst case is a few extra mints, not a security bypass.
pub struct MintRateLimiter {
    cache: Cache<[u8; 32], Arc<std::sync::Mutex<VecDeque<Instant>>>>,
    limit: usize,
}

impl MintRateLimiter {
    /// Create a new rate limiter.
    ///
    /// `limit` is the max mints per pubkey per hour. Reads `SPROUT_MINT_RATE_LIMIT`
    /// env var at construction; falls back to [`DEFAULT_MINT_LIMIT`] (50).
    pub fn new() -> Self {
        let limit = std::env::var("SPROUT_MINT_RATE_LIMIT")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(DEFAULT_MINT_LIMIT);
        tracing::info!("mint rate limiter: {limit} mints/hr/pubkey");
        Self {
            cache: Cache::builder()
                .max_capacity(RATE_LIMITER_MAX_ENTRIES)
                .time_to_idle(Duration::from_secs(3600))
                .build(),
            limit,
        }
    }

    /// Check whether `pubkey_bytes` is within the rate limit and record the attempt.
    ///
    /// Returns `Ok(())` if the mint is allowed, or `Err(retry_after)` with the
    /// duration until the oldest entry in the window expires.
    pub fn check_and_record(&self, pubkey_bytes: &[u8; 32]) -> Result<(), Duration> {
        let now = Instant::now();
        let entry = self
            .cache
            .entry(*pubkey_bytes)
            .or_insert_with(|| Arc::new(std::sync::Mutex::new(VecDeque::new())));
        let mut timestamps = entry.value().lock().unwrap();

        // Evict timestamps that have fallen outside the rolling window.
        while timestamps
            .front()
            .map(|t| now.duration_since(*t) > MINT_WINDOW)
            .unwrap_or(false)
        {
            timestamps.pop_front();
        }

        if timestamps.len() >= self.limit {
            let retry_after = MINT_WINDOW - now.duration_since(*timestamps.front().unwrap());
            return Err(retry_after);
        }

        timestamps.push_back(now);
        Ok(())
    }

    /// The configured limit (for error messages).
    pub fn limit(&self) -> usize {
        self.limit
    }
}

impl Default for MintRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

// ── Request / response types ──────────────────────────────────────────────────

/// Request body for `POST /api/tokens`.
#[derive(Debug, Deserialize)]
pub struct MintTokenRequest {
    /// Human-readable label for the token (1–100 chars).
    pub name: String,
    /// Scope strings to grant (e.g. `["messages:read", "channels:read"]`).
    pub scopes: Vec<String>,
    /// Optional channel UUIDs to restrict the token to.
    /// Absent means unrestricted (unless caller's token is channel-restricted, in which case
    /// channel_ids is required). Empty array is rejected for restricted callers.
    pub channel_ids: Option<Vec<String>>,
    /// Optional expiry in days (1–365). Omit for no expiry.
    pub expires_in_days: Option<u32>,
}

/// Response body for `POST /api/tokens` (token shown once only).
#[derive(Debug, Serialize)]
pub struct MintTokenResponse {
    /// Unique token identifier (UUID).
    pub id: Uuid,
    /// Raw token value — shown **once only**. Only the SHA-256 hash is stored.
    pub token: String,
    /// Human-readable label.
    pub name: String,
    /// Scope strings granted to this token.
    pub scopes: Vec<String>,
    /// Channel UUIDs this token is restricted to (empty = unrestricted).
    pub channel_ids: Vec<String>,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 expiry timestamp, or `null` if no expiry.
    pub expires_at: Option<String>,
}

/// A single token entry in the `GET /api/tokens` response.
#[derive(Debug, Serialize)]
pub struct TokenListItem {
    /// Unique token identifier (UUID).
    pub id: Uuid,
    /// Human-readable label.
    pub name: String,
    /// Scope strings granted to this token.
    pub scopes: Vec<String>,
    /// Channel UUIDs this token is restricted to (empty = unrestricted).
    pub channel_ids: Vec<String>,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 expiry timestamp, or `null` if no expiry.
    pub expires_at: Option<String>,
    /// ISO 8601 timestamp of last use, or `null` if never used.
    pub last_used_at: Option<String>,
    /// ISO 8601 revocation timestamp, or `null` if not revoked.
    pub revoked_at: Option<String>,
}

/// Response body for `GET /api/tokens`.
#[derive(Debug, Serialize)]
pub struct TokenListResponse {
    /// All tokens owned by the authenticated pubkey (including revoked).
    pub tokens: Vec<TokenListItem>,
}

/// Response body for `DELETE /api/tokens` (panic button).
#[derive(Debug, Serialize)]
pub struct RevokeAllResponse {
    /// Number of tokens that were newly revoked (0 if all already revoked).
    pub revoked_count: u64,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `POST /api/tokens` — mint a new API token.
///
/// Accepts NIP-98 HTTP Auth (bootstrap — no existing token required) or an
/// existing Bearer API token. Validates scopes, rate-limits, checks channel
/// membership if `channel_ids` is provided, then inserts via the conditional
/// INSERT that enforces the 10-token-per-pubkey limit atomically.
pub async fn post_tokens(
    State(state): State<std::sync::Arc<AppState>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    // Parse the request body as JSON.
    let req: MintTokenRequest = serde_json::from_slice(&body).map_err(|e| {
        api_error(
            StatusCode::BAD_REQUEST,
            &format!("invalid request body: {e}"),
        )
    })?;

    // Extract auth context — NIP-98 or Bearer.
    // For NIP-98 we re-verify here with the raw body for payload hash checking.
    let ctx = if let Some(auth_header) = headers.get("authorization").and_then(|v| v.to_str().ok())
    {
        if let Some(encoded) = auth_header.strip_prefix("Nostr ") {
            // Reconstruct canonical URL for NIP-98 verification.
            let canonical_url = reconstruct_canonical_url_for_tokens(&headers, &state);

            // The Authorization: Nostr header value is base64-encoded JSON.
            // Decode it before passing to verify_nip98_event which expects JSON.
            let decoded_bytes = {
                use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
                BASE64.decode(encoded).map_err(|_| {
                    tracing::warn!("post_tokens: NIP-98 base64 decode failed");
                    (
                        StatusCode::UNAUTHORIZED,
                        Json(serde_json::json!({
                            "error": "invalid_auth",
                            "message": "NIP-98 verification failed"
                        })),
                    )
                })?
            };
            let event_json = String::from_utf8(decoded_bytes).map_err(|_| {
                tracing::warn!("post_tokens: NIP-98 decoded bytes are not valid UTF-8");
                (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({
                        "error": "invalid_auth",
                        "message": "NIP-98 verification failed"
                    })),
                )
            })?;

            match sprout_auth::verify_nip98_event(&event_json, &canonical_url, "POST", Some(&body))
            {
                Ok(pubkey) => {
                    // POST /api/tokens requires the payload tag — body must be
                    // cryptographically bound to the signed event.
                    let event: nostr::Event =
                        nostr::Event::from_json(&event_json).expect("already verified");
                    let has_payload = event.tags.find(nostr::TagKind::Payload).is_some();
                    if !has_payload {
                        tracing::warn!("post_tokens: NIP-98 event missing required payload tag");
                        return Err((
                            StatusCode::UNAUTHORIZED,
                            Json(serde_json::json!({
                                "error": "invalid_auth",
                                "message": "NIP-98 payload tag required for POST /api/tokens"
                            })),
                        ));
                    }
                    let pubkey_bytes = pubkey.to_bytes().to_vec();
                    if let Err(e) = state.db.ensure_user(&pubkey_bytes).await {
                        tracing::warn!("ensure_user failed for NIP-98 pubkey: {e}");
                    }
                    super::RestAuthContext {
                        pubkey,
                        pubkey_bytes,
                        scopes: vec![],
                        auth_method: RestAuthMethod::Nip98,
                        token_id: None,
                        channel_ids: None,
                    }
                }
                Err(e) => {
                    tracing::warn!("post_tokens: NIP-98 verification failed: {e}");
                    return Err((
                        StatusCode::UNAUTHORIZED,
                        Json(serde_json::json!({
                            "error": "invalid_auth",
                            "message": "NIP-98 verification failed"
                        })),
                    ));
                }
            }
        } else {
            extract_auth_context(&headers, &state).await?
        }
    } else {
        extract_auth_context(&headers, &state).await?
    };

    // ── Validate name ─────────────────────────────────────────────────────────
    if req.name.is_empty() || req.name.len() > 100 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "invalid_name: name must be 1–100 characters",
        ));
    }

    // ── Validate scopes ───────────────────────────────────────────────────────
    if req.scopes.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "invalid_scopes: scopes must not be empty",
        ));
    }

    let mut parsed_scopes: Vec<Scope> = Vec::with_capacity(req.scopes.len());
    for s in &req.scopes {
        let scope: Scope = s.parse().expect("infallible");
        match &scope {
            Scope::Unknown(_) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "invalid_scopes",
                        "message": format!("unknown scope: {s}")
                    })),
                ));
            }
            _ => {
                if !is_self_mintable(&scope) {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({
                            "error": "invalid_scopes",
                            "message": format!("scope requires admin: {s}")
                        })),
                    ));
                }
            }
        }
        parsed_scopes.push(scope);
    }

    // Deduplicate scopes (order-preserving).
    let mut seen = std::collections::HashSet::new();
    parsed_scopes.retain(|s| seen.insert(s.clone()));

    // ── Scope escalation prevention (Bearer-authenticated callers) ───────────
    // If the caller authenticated via an API token, the requested scopes must be
    // a subset of the caller's own scopes, and channel_ids must be a subset too.
    // NIP-98 and Okta JWT callers are unrestricted (they authenticate the identity
    // directly, not via a scoped token).
    if matches!(ctx.auth_method, RestAuthMethod::ApiToken) {
        for scope in &parsed_scopes {
            if !ctx.scopes.contains(scope) {
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({
                        "error": "scope_escalation",
                        "message": format!("Cannot mint scope '{}' — not in your token's scopes", scope)
                    })),
                ));
            }
        }

        // If caller has channel_ids restriction, child must also be restricted
        // to a subset of those channels.
        if let Some(ref caller_channels) = ctx.channel_ids {
            match &req.channel_ids {
                None => {
                    return Err((
                        StatusCode::FORBIDDEN,
                        Json(serde_json::json!({
                            "error": "channel_escalation",
                            "message": "Your token is channel-restricted; minted tokens must also specify channel_ids (subset of yours)"
                        })),
                    ));
                }
                Some(requested_raw) => {
                    // Parse requested channel IDs (already validated above, but we need UUIDs here).
                    for raw in requested_raw {
                        if let Ok(cid) = raw.parse::<uuid::Uuid>() {
                            if !caller_channels.contains(&cid) {
                                return Err((
                                    StatusCode::FORBIDDEN,
                                    Json(serde_json::json!({
                                        "error": "channel_escalation",
                                        "message": format!("Cannot mint access to channel {} — not in your token's channel_ids", cid)
                                    })),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    // ── Rate limit ────────────────────────────────────────────────────────────
    let pubkey_bytes_arr: [u8; 32] = ctx
        .pubkey_bytes
        .as_slice()
        .try_into()
        .map_err(|_| internal_error("pubkey bytes length mismatch"))?;

    if let Err(retry_after) = state.mint_rate_limiter.check_and_record(&pubkey_bytes_arr) {
        let retry_secs = retry_after.as_secs();
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": "rate_limited",
                "message": format!(
                    "Mint limit exceeded: {} per hour. Try again in {} seconds.",
                    state.mint_rate_limiter.limit(), retry_secs
                ),
                "retry_after_seconds": retry_secs
            })),
        ));
    }

    // ── Validate and verify channel_ids ──────────────────────────────────────
    let validated_channel_ids: Option<Vec<Uuid>> = if let Some(ref raw_ids) = req.channel_ids {
        if raw_ids.is_empty() {
            // Empty array is treated as "no restriction" — but if the caller's
            // token is channel-restricted, this would be an escalation. The subset
            // check above already rejects `None` for restricted callers, so we
            // must also reject empty arrays here.
            if matches!(ctx.auth_method, RestAuthMethod::ApiToken) && ctx.channel_ids.is_some() {
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({
                        "error": "channel_escalation",
                        "message": "Your token is channel-restricted; minted tokens must specify non-empty channel_ids (subset of yours)"
                    })),
                ));
            }
            None
        } else {
            let mut uuids = Vec::with_capacity(raw_ids.len());
            for raw in raw_ids {
                let cid: Uuid = raw.parse().map_err(|_| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({
                            "error": "invalid_channel_ids",
                            "message": format!("malformed UUID: {raw}")
                        })),
                    )
                })?;

                // Verify channel exists.
                let channel = state.db.get_channel(cid).await.map_err(|_| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({
                            "error": "invalid_channel_ids",
                            "message": format!("channel not found: {cid}")
                        })),
                    )
                })?;
                let _ = channel; // existence confirmed

                // Verify caller is a member of the channel.
                let is_member = state
                    .db
                    .is_member(cid, &ctx.pubkey_bytes)
                    .await
                    .map_err(|e| internal_error(&format!("db error: {e}")))?;
                if !is_member {
                    return Err((
                        StatusCode::FORBIDDEN,
                        Json(serde_json::json!({
                            "error": "not_channel_member",
                            "message": format!("not a member of channel: {cid}")
                        })),
                    ));
                }

                uuids.push(cid);
            }
            // Deduplicate channel_ids (sorted; UUIDs have no meaningful order).
            uuids.sort();
            uuids.dedup();
            Some(uuids)
        }
    } else {
        None
    };

    // ── Validate expires_in_days ──────────────────────────────────────────────
    if let Some(days) = req.expires_in_days {
        if days == 0 || days > 365 {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "invalid expires_in_days: must be 1–365",
            ));
        }
    }

    let expires_at = req
        .expires_in_days
        .map(|days| Utc::now() + chrono::Duration::days(days as i64));

    // ── Generate token ────────────────────────────────────────────────────────
    let raw_token = sprout_auth::generate_token();
    let token_hash: Vec<u8> = Sha256::digest(raw_token.as_bytes()).to_vec();
    let scope_strings: Vec<String> = parsed_scopes.iter().map(|s| s.to_string()).collect();

    // ── Conditional INSERT (enforces 10-token limit atomically) ──────────────
    let channel_ids_slice = validated_channel_ids.as_deref();
    let token_id = state
        .db
        .create_api_token_if_under_limit(
            &token_hash,
            &ctx.pubkey_bytes,
            &req.name,
            &scope_strings,
            channel_ids_slice,
            expires_at,
        )
        .await
        .map_err(|e| internal_error(&format!("db error creating token: {e}")))?;

    let token_id = match token_id {
        Some(id) => id,
        None => {
            return Err((
                StatusCode::CONFLICT,
                Json(serde_json::json!({
                    "error": "token_limit_exceeded",
                    "message": "Maximum of 10 active tokens per pubkey"
                })),
            ));
        }
    };

    // ── Set agent owner ─────────────────────────────────────────────────────
    // Self-minted tokens always set the caller as the agent owner. This is the
    // same field that `sprout-admin mint-token --owner-pubkey` sets, ensuring
    // self-minted agents have the same ownership semantics as admin-minted ones.
    if let Err(e) = state
        .db
        .set_agent_owner(&ctx.pubkey_bytes, &ctx.pubkey_bytes)
        .await
    {
        tracing::warn!("set_agent_owner failed for self-mint: {e}");
        // Non-fatal — token was already created. Owner field is a convenience,
        // not a security gate. Log and continue.
    }

    // ── Build response ────────────────────────────────────────────────────────
    let channel_ids_response: Vec<String> = validated_channel_ids
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(|id| id.to_string())
        .collect();

    let resp = MintTokenResponse {
        id: token_id,
        token: raw_token,
        name: req.name,
        scopes: scope_strings,
        channel_ids: channel_ids_response,
        created_at: Utc::now().to_rfc3339(),
        expires_at: expires_at.map(|t| t.to_rfc3339()),
    };

    Ok((
        StatusCode::CREATED,
        Json(serde_json::to_value(resp).unwrap()),
    ))
}

/// `GET /api/tokens` — list all tokens owned by the authenticated pubkey.
///
/// Returns all tokens including revoked ones (for audit). Token values are
/// **never** returned. Requires Bearer token or Okta JWT (not NIP-98 bootstrap).
pub async fn get_tokens(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;

    // NIP-98 bootstrap is not allowed for listing — caller must have a real token.
    if ctx.auth_method == RestAuthMethod::Nip98 {
        return Err(api_error(
            StatusCode::UNAUTHORIZED,
            "Bearer token required to list tokens",
        ));
    }

    let records = state
        .db
        .list_tokens_by_owner(&ctx.pubkey_bytes)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    let tokens: Vec<TokenListItem> = records
        .into_iter()
        .map(|r| {
            let channel_ids: Vec<String> = r
                .channel_ids
                .as_deref()
                .unwrap_or(&[])
                .iter()
                .map(|id| id.to_string())
                .collect();
            TokenListItem {
                id: r.id,
                name: r.name,
                scopes: r.scopes,
                channel_ids,
                created_at: r.created_at.to_rfc3339(),
                expires_at: r.expires_at.map(|t| t.to_rfc3339()),
                last_used_at: r.last_used_at.map(|t| t.to_rfc3339()),
                revoked_at: r.revoked_at.map(|t| t.to_rfc3339()),
            }
        })
        .collect();

    Ok(Json(serde_json::json!({ "tokens": tokens })))
}

/// `DELETE /api/tokens/{id}` — revoke a single token by UUID.
///
/// The caller must own the token. Returns 204 on success, 404 if not found
/// or not owned by the caller, 409 if already revoked.
pub async fn delete_token(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;

    if ctx.auth_method == RestAuthMethod::Nip98 {
        return Err(api_error(
            StatusCode::UNAUTHORIZED,
            "Bearer token required to revoke tokens",
        ));
    }

    // First, check if the token exists and is owned by the caller — to distinguish
    // "not found / not owned" (404) from "already revoked" (409).
    // We use get_api_token_by_hash_including_revoked is not applicable here (we have
    // the UUID, not the hash). Instead, we attempt the revoke and then check existence.
    let revoked = state
        .db
        .revoke_token(id, &ctx.pubkey_bytes, &ctx.pubkey_bytes)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    if revoked {
        return Ok(StatusCode::NO_CONTENT);
    }

    // rows_affected == 0: either not found, not owned, or already revoked.
    // Check if the token exists at all (including revoked) to distinguish the cases.
    let all_tokens = state
        .db
        .list_tokens_by_owner(&ctx.pubkey_bytes)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    let found = all_tokens.iter().find(|t| t.id == id);
    match found {
        Some(t) if t.revoked_at.is_some() => Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "already_revoked",
                "message": "Token is already revoked"
            })),
        )),
        _ => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "not_found",
                "message": "Token not found or not owned by caller"
            })),
        )),
    }
}

/// `DELETE /api/tokens` — revoke all tokens (panic button).
///
/// Revokes all active tokens for the authenticated pubkey, including the token
/// used to make this call. Skips already-revoked tokens (idempotent).
/// Returns `{ "revoked_count": N }` with 200.
pub async fn delete_all_tokens(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;

    if ctx.auth_method == RestAuthMethod::Nip98 {
        return Err(api_error(
            StatusCode::UNAUTHORIZED,
            "Bearer token required to revoke tokens",
        ));
    }

    let revoked_count = state
        .db
        .revoke_all_tokens(&ctx.pubkey_bytes, &ctx.pubkey_bytes)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    Ok(Json(serde_json::json!({ "revoked_count": revoked_count })))
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Reconstruct the canonical URL for NIP-98 verification on the token mint endpoint.
fn reconstruct_canonical_url_for_tokens(headers: &HeaderMap, state: &AppState) -> String {
    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("https");
    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get("host"))
        .and_then(|v| v.to_str().ok());

    if let Some(host) = host {
        format!("{proto}://{host}/api/tokens")
    } else {
        let base = state
            .config
            .relay_url
            .replace("wss://", "https://")
            .replace("ws://", "http://");
        format!("{base}/api/tokens")
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limiter_allows_up_to_limit() {
        let limiter = MintRateLimiter::new();
        let key = [0u8; 32];
        for _ in 0..DEFAULT_MINT_LIMIT {
            assert!(limiter.check_and_record(&key).is_ok());
        }
        // 6th call should be denied.
        assert!(limiter.check_and_record(&key).is_err());
    }

    #[test]
    fn rate_limiter_different_pubkeys_are_independent() {
        let limiter = MintRateLimiter::new();
        let key_a = [1u8; 32];
        let key_b = [2u8; 32];
        for _ in 0..DEFAULT_MINT_LIMIT {
            assert!(limiter.check_and_record(&key_a).is_ok());
        }
        // key_b should still be allowed.
        assert!(limiter.check_and_record(&key_b).is_ok());
    }

    #[test]
    fn rate_limiter_returns_retry_after_duration() {
        let limiter = MintRateLimiter::new();
        let key = [3u8; 32];
        for _ in 0..DEFAULT_MINT_LIMIT {
            let _ = limiter.check_and_record(&key);
        }
        let err = limiter.check_and_record(&key).unwrap_err();
        // retry_after should be ≤ MINT_WINDOW and > 0.
        assert!(err > Duration::ZERO);
        assert!(err <= MINT_WINDOW);
    }

    #[test]
    fn mint_token_request_deserializes() {
        let json = r#"{
            "name": "my-agent",
            "scopes": ["messages:read", "messages:write"],
            "expires_in_days": 30
        }"#;
        let req: MintTokenRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "my-agent");
        assert_eq!(req.scopes.len(), 2);
        assert_eq!(req.expires_in_days, Some(30));
        assert!(req.channel_ids.is_none());
    }

    #[test]
    fn mint_token_request_with_channel_ids() {
        let json = r#"{
            "name": "channel-agent",
            "scopes": ["messages:read"],
            "channel_ids": ["01950a3b-c000-7000-8000-000000000001"]
        }"#;
        let req: MintTokenRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.channel_ids.as_ref().unwrap().len(), 1);
    }
}
