use std::collections::HashMap;

use reqwest::Method;
use sprout_core::PresenceStatus;
use tauri::State;

use crate::{
    app_state::AppState,
    events,
    models::{
        GetUsersBatchBody, ProfileInfo, SearchUsersResponse, SetPresenceBody, SetPresenceResponse,
        UsersBatchResponse,
    },
    relay::{build_authed_request, send_json_request, submit_event},
};

#[tauri::command]
pub async fn get_profile(state: State<'_, AppState>) -> Result<ProfileInfo, String> {
    let request = build_authed_request(
        &state.http_client,
        Method::GET,
        "/api/users/me/profile",
        &state,
    )?;
    send_json_request(request).await
}

#[tauri::command]
pub async fn update_profile(
    display_name: Option<String>,
    avatar_url: Option<String>,
    about: Option<String>,
    nip05_handle: Option<String>,
    state: State<'_, AppState>,
) -> Result<ProfileInfo, String> {
    // Read-merge-write: kind 0 is a full profile snapshot, so we must fetch
    // the current profile, merge the caller's changes, then sign the complete
    // profile as a Nostr event. Same pattern as MCP's set_profile.
    let current: serde_json::Value = {
        let request = build_authed_request(
            &state.http_client,
            Method::GET,
            "/api/users/me/profile",
            &state,
        )?;
        send_json_request(request).await.unwrap_or_default()
    };

    let dn = display_name
        .as_deref()
        .or_else(|| current.get("display_name").and_then(|v| v.as_str()));
    let name = current.get("name").and_then(|v| v.as_str());
    let picture = avatar_url
        .as_deref()
        .or_else(|| current.get("avatar_url").and_then(|v| v.as_str()));
    let ab = about
        .as_deref()
        .or_else(|| current.get("about").and_then(|v| v.as_str()));
    let nip05 = nip05_handle
        .as_deref()
        .or_else(|| current.get("nip05_handle").and_then(|v| v.as_str()));

    let builder = events::build_profile(dn, name, picture, ab, nip05)?;
    submit_event(builder, &state).await?;

    // Re-fetch to return the canonical profile the frontend expects.
    let request = build_authed_request(
        &state.http_client,
        Method::GET,
        "/api/users/me/profile",
        &state,
    )?;
    send_json_request(request).await
}

// ── Unchanged reads below ────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_user_profile(
    pubkey: Option<String>,
    state: State<'_, AppState>,
) -> Result<ProfileInfo, String> {
    let path = match pubkey {
        Some(pubkey) => format!("/api/users/{pubkey}/profile"),
        None => "/api/users/me/profile".to_string(),
    };
    let request = build_authed_request(&state.http_client, Method::GET, &path, &state)?;
    send_json_request(request).await
}

#[tauri::command]
pub async fn get_users_batch(
    pubkeys: Vec<String>,
    state: State<'_, AppState>,
) -> Result<UsersBatchResponse, String> {
    let request =
        build_authed_request(&state.http_client, Method::POST, "/api/users/batch", &state)?.json(
            &GetUsersBatchBody {
                pubkeys: pubkeys.as_slice(),
            },
        );
    send_json_request(request).await
}

#[tauri::command]
pub async fn search_users(
    query: String,
    limit: Option<u32>,
    state: State<'_, AppState>,
) -> Result<SearchUsersResponse, String> {
    let limit = limit.unwrap_or(8);
    let limit_param = limit.to_string();
    let request =
        build_authed_request(&state.http_client, Method::GET, "/api/users/search", &state)?
            .query(&[("q", query.as_str()), ("limit", limit_param.as_str())]);

    send_json_request(request).await
}

#[tauri::command]
pub async fn get_presence(
    pubkeys: Vec<String>,
    state: State<'_, AppState>,
) -> Result<HashMap<String, PresenceStatus>, String> {
    if pubkeys.is_empty() {
        return Ok(HashMap::new());
    }

    let request = build_authed_request(&state.http_client, Method::GET, "/api/presence", &state)?
        .query(&[("pubkeys", pubkeys.join(","))]);
    send_json_request(request).await
}

#[tauri::command]
pub async fn set_presence(
    status: PresenceStatus,
    state: State<'_, AppState>,
) -> Result<SetPresenceResponse, String> {
    let request = build_authed_request(&state.http_client, Method::PUT, "/api/presence", &state)?
        .json(&SetPresenceBody { status });
    send_json_request(request).await
}
