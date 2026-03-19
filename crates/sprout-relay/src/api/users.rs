//! User profile REST API.
//!
//! Endpoints:
//!   GET /api/users/me/profile      — get own profile
//!   PUT /api/users/me/profile      — update own profile (display_name, avatar_url, about, nip05_handle)
//!   GET /api/users/{pubkey}/profile — get any user's profile by pubkey hex
//!   POST /api/users/batch          — resolve display names for multiple pubkeys
//!   GET /api/users/search          — search users by display name, NIP-05, or pubkey

use std::sync::Arc;

use axum::{
    extract::{Json as ExtractJson, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use nostr::util::hex as nostr_hex;
use serde::Deserialize;

use crate::state::AppState;

use super::{api_error, extract_auth_context, internal_error, scope_error};

/// Request body for updating a user's profile.
/// All fields are optional — at least one must be present.
#[derive(Debug, Deserialize)]
pub struct UpdateProfileBody {
    /// New display name for the user, or `None` to leave unchanged.
    pub display_name: Option<String>,
    /// New avatar URL for the user, or `None` to leave unchanged.
    pub avatar_url: Option<String>,
    /// Short bio or description, or `None` to leave unchanged.
    pub about: Option<String>,
    /// NIP-05 identifier (e.g. "alice@example.com"), or `None` to leave unchanged.
    pub nip05_handle: Option<String>,
}

/// `PUT /api/users/me/profile` — update the authenticated user's profile.
///
/// Body: `{ "display_name": "Alice", "avatar_url": "https://...", "about": "...", "nip05_handle": "alice@relay.example" }` (all optional, at least one required)
/// Returns: `{ "updated": true }`
pub async fn update_profile(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    ExtractJson(body): ExtractJson<UpdateProfileBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::UsersWrite).map_err(scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();

    let display_name = body
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let avatar_url = body
        .avatar_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let about = body
        .about
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    // nip05_handle: empty string means "clear", None means "leave unchanged"
    let nip05_handle_raw = body.nip05_handle.as_deref().map(str::trim);
    // Don't filter empty — empty string means "clear to NULL" via empty_to_none in DB layer

    // Validate and canonicalize NIP-05 handle (if non-empty)
    let canonical_nip05: Option<String> = match nip05_handle_raw {
        Some("") => Some(String::new()), // empty = clear to NULL
        Some(h) => Some(
            super::nip05::canonicalize_nip05(h, &state.config.relay_url)
                .map_err(|msg| api_error(StatusCode::BAD_REQUEST, &msg))?,
        ),
        None => None,
    };
    let nip05_handle = canonical_nip05.as_deref();

    if display_name.is_none() && avatar_url.is_none() && about.is_none() && nip05_handle.is_none() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "at least one of display_name, avatar_url, about, or nip05_handle is required",
        ));
    }

    state
        .db
        .update_user_profile(&pubkey_bytes, display_name, avatar_url, about, nip05_handle)
        .await
        .map_err(|e| {
            let msg = format!("{e}");
            if msg.contains("duplicate key value") || msg.contains("23505") {
                api_error(
                    StatusCode::CONFLICT,
                    "nip05_handle is already claimed by another user",
                )
            } else {
                internal_error(&format!("db error: {e}"))
            }
        })?;

    Ok(Json(serde_json::json!({ "updated": true })))
}

/// `GET /api/users/me/profile` — get the authenticated user's profile.
///
/// Returns: `{ "pubkey": "<hex>", "display_name": "...", "avatar_url": "...", "about": "...", "nip05_handle": "..." }`
pub async fn get_profile(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::UsersRead).map_err(scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();

    let profile = state
        .db
        .get_user(&pubkey_bytes)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    match profile {
        Some(p) => Ok(Json(serde_json::json!({
            "pubkey": nostr_hex::encode(&p.pubkey),
            "display_name": p.display_name,
            "avatar_url": p.avatar_url,
            "about": p.about,
            "nip05_handle": p.nip05_handle,
        }))),
        None => Err(api_error(StatusCode::NOT_FOUND, "user not found")),
    }
}

/// `GET /api/users/{pubkey}/profile` — get any user's profile by pubkey hex.
pub async fn get_user_profile(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(pubkey_hex): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::UsersRead).map_err(scope_error)?;

    let pubkey_bytes = nostr_hex::decode(&pubkey_hex)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid pubkey hex"))?;
    if pubkey_bytes.len() != 32 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "pubkey must be 32 bytes",
        ));
    }

    let profile = state
        .db
        .get_user(&pubkey_bytes)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "user not found"))?;

    Ok(Json(serde_json::json!({
        "pubkey": nostr_hex::encode(&profile.pubkey),
        "display_name": profile.display_name,
        "avatar_url": profile.avatar_url,
        "about": profile.about,
        "nip05_handle": profile.nip05_handle,
    })))
}

/// Request body for the batch profile resolution endpoint.
#[derive(Debug, Deserialize)]
pub struct BatchProfilesRequest {
    /// List of pubkey hex strings to resolve (max 200).
    pub pubkeys: Vec<String>,
}

/// Query string for user search.
#[derive(Debug, Deserialize)]
pub struct SearchUsersQuery {
    /// Case-insensitive search query.
    pub q: String,
    /// Maximum number of results to return.
    pub limit: Option<u32>,
}

/// `POST /api/users/batch` — resolve profile summaries for multiple pubkeys.
pub async fn get_users_batch(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    ExtractJson(body): ExtractJson<BatchProfilesRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::UsersRead).map_err(scope_error)?;

    if body.pubkeys.len() > 200 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "max 200 pubkeys per request",
        ));
    }

    // Partition inputs: valid hex (64 chars, valid hex) vs invalid (wrong length or bad hex).
    // Both wrong-length and 64-char-non-hex inputs go to the missing list.
    let mut invalid_inputs: Vec<String> = Vec::new();
    let mut valid_hex_set: std::collections::HashSet<String> = std::collections::HashSet::new();

    for p in &body.pubkeys {
        if p.len() != 64 {
            invalid_inputs.push(p.clone());
        } else {
            let lower = p.to_lowercase();
            if nostr_hex::decode(&lower)
                .map(|b| b.len() == 32)
                .unwrap_or(false)
            {
                valid_hex_set.insert(lower);
            } else {
                invalid_inputs.push(p.clone());
            }
        }
    }

    let mut normalized: Vec<String> = valid_hex_set.into_iter().collect();
    normalized.sort();

    let pubkey_bytes: Vec<Vec<u8>> = normalized
        .iter()
        .filter_map(|h| nostr_hex::decode(h).ok())
        .filter(|b| b.len() == 32)
        .collect();

    let records = state
        .db
        .get_users_bulk(&pubkey_bytes)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    let found_pubkeys: std::collections::HashSet<String> = records
        .iter()
        .map(|r| nostr_hex::encode(&r.pubkey))
        .collect();

    let mut profiles = serde_json::Map::new();
    for r in records {
        let hex = nostr_hex::encode(&r.pubkey);
        profiles.insert(
            hex,
            serde_json::json!({
                "display_name": r.display_name,
                "avatar_url": r.avatar_url,
                "nip05_handle": r.nip05_handle,
            }),
        );
    }

    let mut missing: Vec<String> = normalized
        .iter()
        .filter(|p| !found_pubkeys.contains(p.as_str()))
        .cloned()
        .collect();
    missing.extend(invalid_inputs);

    Ok(Json(serde_json::json!({
        "profiles": profiles,
        "missing": missing,
    })))
}

/// `GET /api/users/search` — search users by display name, NIP-05, or pubkey prefix.
pub async fn search_users(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<SearchUsersQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::UsersRead).map_err(scope_error)?;

    let q = query.q.trim();
    if q.is_empty() {
        return Ok(Json(serde_json::json!({ "users": [] })));
    }

    let results = state
        .db
        .search_users(q, query.limit.unwrap_or(8))
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    Ok(Json(serde_json::json!({
        "users": results.into_iter().map(|user| {
            serde_json::json!({
                "pubkey": nostr_hex::encode(&user.pubkey),
                "display_name": user.display_name,
                "avatar_url": user.avatar_url,
                "nip05_handle": user.nip05_handle,
            })
        }).collect::<Vec<_>>(),
    })))
}

/// Request body for updating channel add policy.
#[derive(Debug, Deserialize)]
pub struct UpdateChannelAddPolicyBody {
    /// Policy value: `"anyone"`, `"owner_only"`, or `"nobody"`.
    pub channel_add_policy: String,
}

/// `PUT /api/users/me/channel-add-policy` — set the caller's channel add policy.
pub async fn put_channel_add_policy(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    ExtractJson(body): ExtractJson<UpdateChannelAddPolicyBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::UsersWrite).map_err(scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();

    let policy = body.channel_add_policy.as_str();
    if !matches!(policy, "anyone" | "owner_only" | "nobody") {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "channel_add_policy must be 'anyone', 'owner_only', or 'nobody'",
        ));
    }

    state
        .db
        .set_channel_add_policy(&pubkey_bytes, policy)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    // Return updated state
    let (current_policy, owner_pk) = state
        .db
        .get_agent_channel_policy(&pubkey_bytes)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?
        .unwrap_or_else(|| ("anyone".to_string(), None));

    Ok(Json(serde_json::json!({
        "channel_add_policy": current_policy,
        "agent_owner_pubkey": owner_pk.map(|b| nostr_hex::encode(&b)),
    })))
}
