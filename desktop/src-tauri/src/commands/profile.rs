use std::collections::HashMap;

use reqwest::Method;
use sprout_core::PresenceStatus;
use tauri::State;

use crate::{
    app_state::AppState,
    models::{
        GetUsersBatchBody, ProfileInfo, SetPresenceBody, SetPresenceResponse, UpdateProfileBody,
        UsersBatchResponse,
    },
    relay::{build_authed_request, send_empty_request, send_json_request},
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
    let request = build_authed_request(
        &state.http_client,
        Method::PUT,
        "/api/users/me/profile",
        &state,
    )?
    .json(&UpdateProfileBody {
        display_name: display_name.as_deref(),
        avatar_url: avatar_url.as_deref(),
        about: about.as_deref(),
        nip05_handle: nip05_handle.as_deref(),
    });
    send_empty_request(request).await?;

    let request = build_authed_request(
        &state.http_client,
        Method::GET,
        "/api/users/me/profile",
        &state,
    )?;
    send_json_request(request).await
}

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
