//! User profile REST API.
//!
//! Endpoints:
//!   GET /api/users/me/profile      — get own profile
//!   GET /api/users/{pubkey}/profile — get any user's profile by pubkey hex
//!   POST /api/users/batch          — resolve display names for multiple pubkeys
//!   GET /api/users/search          — search users by display name, NIP-05, or pubkey
//!   PUT /api/users/me/channel-add-policy — set channel add policy (DB-native setting)

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

use sprout_core::kind::{KIND_CONTACT_LIST, KIND_TEXT_NOTE};
use sprout_db::event::EventQuery;

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
        Some(p) => {
            let (_, owner_pk) = state
                .db
                .get_agent_channel_policy(&pubkey_bytes)
                .await
                .map_err(|e| internal_error(&format!("db error: {e}")))?
                .unwrap_or_else(|| ("anyone".to_string(), None));

            Ok(Json(serde_json::json!({
                "pubkey": nostr_hex::encode(&p.pubkey),
                "display_name": p.display_name,
                "avatar_url": p.avatar_url,
                "about": p.about,
                "nip05_handle": p.nip05_handle,
                "agent_owner_pubkey": owner_pk.map(|b| nostr_hex::encode(&b)),
            })))
        }
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

    let (_, owner_pk) = state
        .db
        .get_agent_channel_policy(&pubkey_bytes)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?
        .unwrap_or_else(|| ("anyone".to_string(), None));

    Ok(Json(serde_json::json!({
        "pubkey": nostr_hex::encode(&profile.pubkey),
        "display_name": profile.display_name,
        "avatar_url": profile.avatar_url,
        "about": profile.about,
        "nip05_handle": profile.nip05_handle,
        "agent_owner_pubkey": owner_pk.map(|b| nostr_hex::encode(&b)),
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
    if q.len() > 200 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "search query too long (max 200 characters)",
        ));
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

// ── Social endpoints ─────────────────────────────────────────────────────────

/// Query params for [`get_user_notes`].
#[derive(Debug, Deserialize)]
pub struct NotesQuery {
    /// Maximum number of notes to return (capped at 100, default 50).
    pub limit: Option<u32>,
    /// Unix timestamp cursor. When used alone (backward compat), events strictly
    /// before this timestamp are returned (subtracts 1 second). When used together
    /// with `before_id`, enables composite keyset pagination that correctly handles
    /// same-second events.
    pub before: Option<i64>,
    /// Hex event ID cursor. When provided with `before`, enables composite keyset
    /// pagination: returns events where `created_at < before` OR
    /// `(created_at = before AND id > before_id)`. This prevents same-second events
    /// from being skipped during pagination.
    pub before_id: Option<String>,
}

/// `GET /api/users/{pubkey}/notes` — list kind:1 text notes by a user.
pub async fn get_user_notes(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(pubkey): Path<String>,
    Query(params): Query<NotesQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::MessagesRead)
        .map_err(scope_error)?;

    let pubkey_bytes = nostr_hex::decode(&pubkey)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid pubkey hex"))?;
    if pubkey_bytes.len() != 32 {
        return Err(api_error(StatusCode::BAD_REQUEST, "invalid pubkey hex"));
    }

    let limit = params.limit.unwrap_or(50).clamp(1, 100) as i64;

    // Composite cursor: when both `before` and `before_id` are provided, use exact
    // keyset pagination (no ±1 adjustment needed — the DB clause is strictly exclusive).
    // Simple cursor (backward compat): subtract 1 second so the last event on the
    // previous page is not repeated (DB uses `<=` for the simple case).
    let (until, before_id_bytes) = match (params.before, params.before_id.as_deref()) {
        (Some(ts), Some(id_hex)) => {
            // Composite cursor — validate and decode the event ID.
            let id_bytes = hex::decode(id_hex)
                .ok()
                .filter(|b| b.len() == 32)
                .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "invalid before_id hex"))?;
            let dt = chrono::DateTime::from_timestamp(ts, 0)
                .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "invalid before timestamp"))?;
            (Some(dt), Some(id_bytes))
        }
        (Some(ts), None) => {
            // Simple cursor: subtract 1 second for exclusivity (DB uses <=).
            let dt = chrono::DateTime::from_timestamp(ts.saturating_sub(1), 0)
                .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "invalid before timestamp"))?;
            (Some(dt), None)
        }
        (None, Some(_)) => {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "before_id requires before",
            ));
        }
        (None, None) => (None, None),
    };

    let q = EventQuery {
        pubkey: Some(pubkey_bytes),
        kinds: Some(vec![KIND_TEXT_NOTE as i32]),
        limit: Some(limit),
        until,
        before_id: before_id_bytes,
        ..Default::default()
    };

    let events = state
        .db
        .query_events(&q)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    // Composite cursor: include both timestamp and event ID so the next page
    // can use exact keyset pagination without same-second skipping.
    let next_cursor = if events.len() == limit as usize {
        events.last().map(|e| {
            serde_json::json!({
                "before": e.event.created_at.as_u64() as i64,
                "before_id": e.event.id.to_hex(),
            })
        })
    } else {
        None
    };
    let notes: Vec<_> = events
        .into_iter()
        .map(|e| {
            serde_json::json!({
                "id":         e.event.id.to_hex(),
                "pubkey":     e.event.pubkey.to_hex(),
                "created_at": e.event.created_at.as_u64(),
                "content":    e.event.content,
            })
        })
        .collect();

    Ok(Json(
        serde_json::json!({ "notes": notes, "next_cursor": next_cursor }),
    ))
}

/// `GET /api/users/{pubkey}/contact-list` — get a user's kind:3 contact list.
pub async fn get_contact_list(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(pubkey): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::UsersRead).map_err(scope_error)?;

    let pubkey_bytes = nostr_hex::decode(&pubkey)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid pubkey hex"))?;
    if pubkey_bytes.len() != 32 {
        return Err(api_error(StatusCode::BAD_REQUEST, "invalid pubkey hex"));
    }

    let event = state
        .db
        .get_latest_global_replaceable(KIND_CONTACT_LIST as i32, &pubkey_bytes)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    match event {
        Some(e) => {
            let tags = serde_json::to_value(&e.event.tags)
                .map_err(|err| internal_error(&format!("tag serialization error: {err}")))?;
            Ok(Json(serde_json::json!({
                "id":         e.event.id.to_hex(),
                "pubkey":     e.event.pubkey.to_hex(),
                "created_at": e.event.created_at.as_u64(),
                "tags":       tags,
                "content":    e.event.content,
            })))
        }
        None => Err(api_error(StatusCode::NOT_FOUND, "contact list not found")),
    }
}
