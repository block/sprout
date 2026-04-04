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

use super::{api_error, extract_auth_context, internal_error, ApiError, RestAuthMethod};
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
        let mut timestamps = entry.value().lock().unwrap_or_else(|e| e.into_inner());

        // Evict timestamps that have fallen outside the rolling window.
        while timestamps
            .front()
            .map(|t| now.duration_since(*t) > MINT_WINDOW)
            .unwrap_or(false)
        {
            timestamps.pop_front();
        }

        if timestamps.len() >= self.limit {
            // SAFETY: vec is guaranteed non-empty by prior check (len >= limit > 0)
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
    /// Optional owner pubkey (hex). Only accepted via NIP-98 auth (bootstrap mint).
    /// Sets `agent_owner_pubkey` on the agent's user record. This proves the caller
    /// holds the agent's private key and is designating another pubkey as the owner.
    /// Rejected if auth is Bearer (child token minting cannot reassign ownership).
    pub owner_pubkey: Option<String>,
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
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
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
                    ApiError::Coded {
                        status: StatusCode::UNAUTHORIZED,
                        code: "invalid_auth",
                        message: "NIP-98 verification failed".into(),
                        extra: None,
                    }
                })?
            };
            let event_json = String::from_utf8(decoded_bytes).map_err(|_| {
                tracing::warn!("post_tokens: NIP-98 decoded bytes are not valid UTF-8");
                ApiError::Coded {
                    status: StatusCode::UNAUTHORIZED,
                    code: "invalid_auth",
                    message: "NIP-98 verification failed".into(),
                    extra: None,
                }
            })?;

            match sprout_auth::verify_nip98_event(&event_json, &canonical_url, "POST", Some(&body))
            {
                Ok(pubkey) => {
                    // POST /api/tokens requires the payload tag — body must be
                    // cryptographically bound to the signed event.
                    let event: nostr::Event =
                        // SAFETY: event_json was already parsed and verified by verify_nip98_event above
                        nostr::Event::from_json(&event_json).expect("SAFETY: already verified by verify_nip98_event");
                    let has_payload = event.tags.find(nostr::TagKind::Payload).is_some();
                    if !has_payload {
                        tracing::warn!("post_tokens: NIP-98 event missing required payload tag");
                        return Err(ApiError::Coded {
                            status: StatusCode::UNAUTHORIZED,
                            code: "invalid_auth",
                            message: "NIP-98 payload tag required for POST /api/tokens".into(),
                            extra: None,
                        });
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
                    return Err(ApiError::Coded {
                        status: StatusCode::UNAUTHORIZED,
                        code: "invalid_auth",
                        message: "NIP-98 verification failed".into(),
                        extra: None,
                    });
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
        return Err(ApiError::Coded {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_name",
            message: "name must be 1–100 characters".into(),
            extra: None,
        });
    }

    // ── Validate scopes ───────────────────────────────────────────────────────
    if req.scopes.is_empty() {
        return Err(ApiError::Coded {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_scopes",
            message: "scopes must not be empty".into(),
            extra: None,
        });
    }

    let mut parsed_scopes: Vec<Scope> = Vec::with_capacity(req.scopes.len());
    for s in &req.scopes {
        // Scope::from_str is infallible — unknown values map to Scope::Unknown(_).
        // Use unwrap_or to avoid panicking in production even though this can't fail.
        let scope: Scope = s.parse().unwrap_or(Scope::Unknown(s.clone()));
        match &scope {
            Scope::Unknown(_) => {
                return Err(ApiError::Coded {
                    status: StatusCode::BAD_REQUEST,
                    code: "invalid_scopes",
                    message: format!("unknown scope: {s}"),
                    extra: None,
                });
            }
            _ => {
                if !is_self_mintable(&scope) {
                    return Err(ApiError::Coded {
                        status: StatusCode::BAD_REQUEST,
                        code: "invalid_scopes",
                        message: format!("scope requires admin: {s}"),
                        extra: None,
                    });
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
                return Err(ApiError::Coded {
                    status: StatusCode::FORBIDDEN,
                    code: "scope_escalation",
                    message: format!("Cannot mint scope '{}' — not in your token's scopes", scope),
                    extra: None,
                });
            }
        }

        // If caller has channel_ids restriction, child must also be restricted
        // to a subset of those channels.
        if let Some(ref caller_channels) = ctx.channel_ids {
            match &req.channel_ids {
                None => {
                    return Err(ApiError::Coded {
                        status: StatusCode::FORBIDDEN,
                        code: "channel_escalation",
                        message: "Your token is channel-restricted; minted tokens must also specify channel_ids (subset of yours)".into(),
                        extra: None,
                    });
                }
                Some(requested_raw) => {
                    // Parse requested channel IDs (already validated above, but we need UUIDs here).
                    for raw in requested_raw {
                        if let Ok(cid) = raw.parse::<uuid::Uuid>() {
                            if !caller_channels.contains(&cid) {
                                return Err(ApiError::Coded {
                                    status: StatusCode::FORBIDDEN,
                                    code: "channel_escalation",
                                    message: format!("Cannot mint access to channel {} — not in your token's channel_ids", cid),
                                    extra: None,
                                });
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
        return Err(ApiError::Coded {
            status: StatusCode::TOO_MANY_REQUESTS,
            code: "rate_limited",
            message: format!(
                "Mint limit exceeded: {} per hour. Try again in {} seconds.",
                state.mint_rate_limiter.limit(),
                retry_secs
            ),
            extra: Some(serde_json::json!({ "retry_after_seconds": retry_secs })),
        });
    }

    // ── Validate and verify channel_ids ──────────────────────────────────────
    let validated_channel_ids: Option<Vec<Uuid>> = if let Some(ref raw_ids) = req.channel_ids {
        if raw_ids.is_empty() {
            // Empty array is treated as "no restriction" — but if the caller's
            // token is channel-restricted, this would be an escalation. The subset
            // check above already rejects `None` for restricted callers, so we
            // must also reject empty arrays here.
            if matches!(ctx.auth_method, RestAuthMethod::ApiToken) && ctx.channel_ids.is_some() {
                return Err(ApiError::Coded {
                    status: StatusCode::FORBIDDEN,
                    code: "channel_escalation",
                    message: "Your token is channel-restricted; minted tokens must specify non-empty channel_ids (subset of yours)".into(),
                    extra: None,
                });
            }
            None
        } else {
            let mut uuids = Vec::with_capacity(raw_ids.len());
            for raw in raw_ids {
                let cid: Uuid = raw.parse().map_err(|_| ApiError::Coded {
                    status: StatusCode::BAD_REQUEST,
                    code: "invalid_channel_ids",
                    message: format!("malformed UUID: {raw}"),
                    extra: None,
                })?;

                // Verify channel exists. Distinguish "not found" from real DB errors.
                match state.db.get_channel(cid).await {
                    Ok(_) => {}
                    Err(sprout_db::DbError::ChannelNotFound(_)) => {
                        return Err(ApiError::Coded {
                            status: StatusCode::BAD_REQUEST,
                            code: "invalid_channel_ids",
                            message: format!("channel not found: {cid}"),
                            extra: None,
                        });
                    }
                    Err(e) => {
                        return Err(internal_error(&format!("db error: {e}")));
                    }
                }

                // Verify caller is a member of the channel.
                let is_member = state
                    .db
                    .is_member(cid, &ctx.pubkey_bytes)
                    .await
                    .map_err(|e| internal_error(&format!("db error: {e}")))?;
                if !is_member {
                    return Err(ApiError::Coded {
                        status: StatusCode::FORBIDDEN,
                        code: "not_channel_member",
                        message: format!("not a member of channel: {cid}"),
                        extra: None,
                    });
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

    // ── Validate owner_pubkey (before token insert) ─────────────────────────
    let validated_owner_bytes: Option<Vec<u8>> = if let Some(ref owner_hex) = req.owner_pubkey {
        if ctx.auth_method != super::RestAuthMethod::Nip98 {
            return Err(ApiError::Forbidden(
                "owner_pubkey can only be set via NIP-98 auth (bootstrap mint)".into(),
            ));
        }
        let bytes = nostr::util::hex::decode(owner_hex)
            .map_err(|_| ApiError::BadRequest("invalid owner_pubkey hex".into()))?;
        if bytes.len() != 32 {
            return Err(ApiError::BadRequest(
                "owner_pubkey must be 32 bytes (64 hex chars)".into(),
            ));
        }
        Some(bytes)
    } else {
        None
    };

    // ── Validate expires_in_days ──────────────────────────────────────────────
    if let Some(days) = req.expires_in_days {
        if days == 0 || days > 365 {
            return Err(ApiError::BadRequest(
                "invalid expires_in_days: must be 1–365".into(),
            ));
        }
    }

    let expires_at = req
        .expires_in_days
        .map(|days| Utc::now() + chrono::Duration::days(days as i64));

    // ── Set agent owner BEFORE token creation ────────────────────────────────
    // Ownership must be settled before the token is inserted. This eliminates
    // the orphaned-token failure mode: if ownership fails, no token exists to
    // revoke. set_agent_owner is atomic (UPDATE ... WHERE agent_owner_pubkey
    // IS NULL) — concurrent bootstrap mints are serialized by the DB.
    let owner_bytes = validated_owner_bytes.unwrap_or_else(|| ctx.pubkey_bytes.clone());

    // ── Enforce shutdown-required scopes ─────────────────────────────────────
    // Check BEFORE any side effects (ownership, token creation). Two triggers:
    // 1. Explicit owner_pubkey in request (bootstrap mint)
    // 2. Agent already has an owner in the DB (re-mint must preserve controllability)
    // Fail closed: if the DB lookup errors, assume owned and enforce scopes.
    // A transient DB error must not open a bypass for stripping shutdown scopes.
    let has_existing_owner = match state.db.get_agent_channel_policy(&ctx.pubkey_bytes).await {
        Ok(Some((_, Some(_)))) => true,
        Ok(_) => false,
        Err(e) => {
            tracing::warn!("owner lookup failed (assuming owned, enforcing scopes): {e}");
            true // fail closed
        }
    };
    let needs_scope_check = req.owner_pubkey.is_some() || has_existing_owner;
    if needs_scope_check {
        let required = [
            "users:read",
            "messages:read",
            "messages:write",
            "channels:read",
        ];
        let scope_strs: Vec<String> = parsed_scopes.iter().map(|s| s.to_string()).collect();
        for r in &required {
            if !scope_strs.iter().any(|s| s == r) {
                return Err(ApiError::Coded {
                    status: StatusCode::BAD_REQUEST,
                    code: "missing_required_scope",
                    message: format!("owned agents require the '{r}' scope for controllability"),
                    extra: None,
                });
            }
        }
    }

    // ── Set agent owner (only when explicitly requested) ─────────────────────
    // Self-mints without owner_pubkey do NOT assign ownership. Only bootstrap
    // mints with an explicit owner_pubkey write the ownership relationship.
    // This preserves the semantics that omitting owner_pubkey means "don't
    // set agent owner" — important because self-ownership would force
    // controllability scopes on all future re-mints.
    if req.owner_pubkey.is_some() {
        state
            .db
            .ensure_user(&owner_bytes)
            .await
            .map_err(|e| internal_error(&format!("ensure_user for owner failed: {e}")))?;

        match state
            .db
            .set_agent_owner(&ctx.pubkey_bytes, &owner_bytes)
            .await
        {
            Ok(true) => {
                tracing::debug!("agent owner set successfully");
            }
            Ok(false) => {
                let existing = state
                    .db
                    .get_agent_channel_policy(&ctx.pubkey_bytes)
                    .await
                    .map_err(|e| internal_error(&format!("db error checking owner: {e}")))?
                    .and_then(|(_, owner)| owner);
                if existing.as_deref() != Some(owner_bytes.as_slice()) {
                    return Err(ApiError::Conflict(
                        "agent already has a different owner".into(),
                    ));
                }
                tracing::debug!("agent already owned by the requested pubkey — no change needed");
            }
            Err(e) => {
                return Err(internal_error(&format!("failed to set agent owner: {e}")));
            }
        }
    }

    // ── Generate token (after scope validation + ownership settled) ───────────
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
            return Err(ApiError::Coded {
                status: StatusCode::CONFLICT,
                code: "token_limit_exceeded",
                message: "Maximum of 10 active tokens per pubkey".into(),
                extra: None,
            });
        }
    };

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

    // SAFETY: MintTokenResponse contains only String/Uuid/Vec fields — serialization is infallible
    Ok((
        StatusCode::CREATED,
        Json(
            serde_json::to_value(resp)
                .expect("SAFETY: MintTokenResponse serialization is infallible"),
        ),
    ))
}

/// `GET /api/tokens` — list all tokens owned by the authenticated pubkey.
///
/// Returns all tokens including revoked ones (for audit). Token values are
/// **never** returned. Requires Bearer token or Okta JWT (not NIP-98 bootstrap).
pub async fn get_tokens(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ctx = extract_auth_context(&headers, &state).await?;

    // NIP-98 bootstrap is not allowed for listing — caller must have a real token.
    if ctx.auth_method == RestAuthMethod::Nip98 {
        return Err(ApiError::Unauthorized(
            "Bearer token required to list tokens".into(),
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
) -> Result<StatusCode, ApiError> {
    let ctx = extract_auth_context(&headers, &state).await?;

    if ctx.auth_method == RestAuthMethod::Nip98 {
        return Err(ApiError::Unauthorized(
            "Bearer token required to revoke tokens".into(),
        ));
    }

    // First, check if the token exists and is owned by the caller — to distinguish
    // "not found / not owned" (404) from "already revoked" (409).
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
        Some(t) if t.revoked_at.is_some() => Err(ApiError::Coded {
            status: StatusCode::CONFLICT,
            code: "already_revoked",
            message: "Token is already revoked".into(),
            extra: None,
        }),
        _ => Err(ApiError::Coded {
            status: StatusCode::NOT_FOUND,
            code: "not_found",
            message: "Token not found or not owned by caller".into(),
            extra: None,
        }),
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
) -> Result<Json<serde_json::Value>, ApiError> {
    let ctx = extract_auth_context(&headers, &state).await?;

    if ctx.auth_method == RestAuthMethod::Nip98 {
        return Err(ApiError::Unauthorized(
            "Bearer token required to revoke tokens".into(),
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
