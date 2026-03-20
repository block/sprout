use reqwest::Method;
use tauri::State;

use crate::{
    app_state::AppState,
    models::{ListTokensResponse, MintTokenBody, MintTokenResponse, RevokeAllTokensResponse},
    relay::{
        build_authed_request, build_nip98_auth_header, build_token_management_request,
        relay_api_base_url, send_empty_request, send_json_request,
    },
};

#[tauri::command]
pub async fn list_tokens(state: State<'_, AppState>) -> Result<ListTokensResponse, String> {
    let request =
        build_token_management_request(&state.http_client, Method::GET, "/api/tokens", &state)?;
    send_json_request(request).await
}

#[tauri::command]
pub async fn mint_token(
    name: String,
    scopes: Vec<String>,
    channel_ids: Option<Vec<String>>,
    expires_in_days: Option<u32>,
    state: State<'_, AppState>,
) -> Result<MintTokenResponse, String> {
    let body = MintTokenBody {
        name: &name,
        scopes: &scopes,
        channel_ids: channel_ids.as_deref(),
        expires_in_days,
        owner_pubkey: None, // User-minted tokens don't set agent owner
    };
    let request = if state.configured_api_token.is_some() {
        build_authed_request(&state.http_client, Method::POST, "/api/tokens", &state)?.json(&body)
    } else {
        let url = format!("{}{}", relay_api_base_url(), "/api/tokens");
        let body_bytes =
            serde_json::to_vec(&body).map_err(|error| format!("serialize failed: {error}"))?;
        let auth_header = build_nip98_auth_header(&Method::POST, &url, &body_bytes, &state)?;
        let forwarded_proto = if url.starts_with("http://") {
            "http"
        } else {
            "https"
        };

        state
            .http_client
            .request(Method::POST, url)
            .header("Authorization", auth_header)
            .header("Content-Type", "application/json")
            .header("X-Forwarded-Proto", forwarded_proto)
            .body(body_bytes)
    };
    let response: MintTokenResponse = send_json_request(request).await?;

    if state.configured_api_token.is_none() {
        let mut token = state
            .session_token
            .lock()
            .map_err(|error| error.to_string())?;
        *token = Some(response.token.clone());
    }

    Ok(response)
}

#[tauri::command]
pub async fn revoke_token(token_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let path = format!("/api/tokens/{token_id}");
    let request =
        build_token_management_request(&state.http_client, Method::DELETE, &path, &state)?;
    send_empty_request(request).await
}

#[tauri::command]
pub async fn revoke_all_tokens(
    state: State<'_, AppState>,
) -> Result<RevokeAllTokensResponse, String> {
    let request =
        build_token_management_request(&state.http_client, Method::DELETE, "/api/tokens", &state)?;
    send_json_request(request).await
}
