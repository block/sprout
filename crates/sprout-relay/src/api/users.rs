//! User profile REST API.
//!
//! Endpoints:
//!   GET /api/users/me/profile  — get own profile
//!   PUT /api/users/me/profile  — update own profile (display_name, avatar_url, about)

use std::sync::Arc;

use axum::{
    extract::{Json as ExtractJson, State},
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

    if display_name.is_none() && avatar_url.is_none() && about.is_none() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "at least one of display_name, avatar_url, or about is required",
        ));
    }

    state
        .db
        .update_user_profile(&pubkey_bytes, display_name, avatar_url, about)
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
