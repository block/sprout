//! User profile REST API.
//!
//! Endpoints:
//!   GET /api/users/me/profile      — get own profile
//!   PUT /api/users/me/profile      — update own profile (display_name, avatar_url, about)
//!   GET /api/users/{pubkey}/profile — get any user's profile by pubkey hex
//!   POST /api/users/batch          — resolve display names for multiple pubkeys

use std::sync::Arc;

use axum::{
    extract::{Json as ExtractJson, Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use nostr::util::hex as nostr_hex;
use serde::Deserialize;

use crate::state::AppState;

use super::{api_error, extract_auth_pubkey, internal_error};

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
/// Body: `{ "display_name": "Alice", "avatar_url": "https://...", "about": "..." }` (all optional, at least one required)
/// Returns: `{ "updated": true }`
pub async fn update_profile(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    ExtractJson(body): ExtractJson<UpdateProfileBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (_pubkey, pubkey_bytes) = extract_auth_pubkey(&headers, &state).await?;

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
    let nip05_handle = body
        .nip05_handle
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

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
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    Ok(Json(serde_json::json!({ "updated": true })))
}

/// `GET /api/users/me/profile` — get the authenticated user's profile.
///
/// Returns: `{ "pubkey": "<hex>", "display_name": "...", "avatar_url": "...", "about": "...", "nip05_handle": "..." }`
pub async fn get_profile(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (_pubkey, pubkey_bytes) = extract_auth_pubkey(&headers, &state).await?;

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
    let _ = extract_auth_pubkey(&headers, &state).await?;

    let pubkey_bytes = nostr_hex::decode(&pubkey_hex)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid pubkey hex"))?;
    if pubkey_bytes.len() != 32 {
        return Err(api_error(StatusCode::BAD_REQUEST, "pubkey must be 32 bytes"));
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

/// `POST /api/users/batch` — resolve display names for multiple pubkeys.
pub async fn get_users_batch(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    ExtractJson(body): ExtractJson<BatchProfilesRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _ = extract_auth_pubkey(&headers, &state).await?;

    if body.pubkeys.len() > 200 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "max 200 pubkeys per request",
        ));
    }

    let valid_inputs: Vec<&str> = body.pubkeys
        .iter()
        .filter(|p| p.len() == 64)
        .map(|p| p.as_str())
        .collect();

    let normalized: std::collections::HashSet<String> = valid_inputs
        .iter()
        .map(|p| p.to_lowercase())
        .collect();
    let mut normalized: Vec<String> = normalized.into_iter().collect();
    normalized.sort();

    let pubkey_bytes: Vec<Vec<u8>> = normalized.iter()
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
        profiles.insert(hex, serde_json::json!({
            "display_name": r.display_name,
            "nip05_handle": r.nip05_handle,
        }));
    }

    let mut missing: Vec<String> = normalized
        .iter()
        .filter(|p| !found_pubkeys.contains(p.as_str()))
        .cloned()
        .collect();
    missing.extend(
        body.pubkeys
            .iter()
            .filter(|p| p.len() != 64)
            .cloned(),
    );

    Ok(Json(serde_json::json!({
        "profiles": profiles,
        "missing": missing,
    })))
}
