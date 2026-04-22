use reqwest::Method;
use tauri::State;

use crate::{
    app_state::AppState,
    models::{ListTokensResponse, MintTokenBody, MintTokenResponse, RevokeAllTokensResponse},
    relay::{
        api_path, build_authed_request, build_nip98_auth_header, build_token_management_request,
        relay_api_base_url, send_empty_request, send_json_request,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MintTokenAuthMode {
    Auto,
    BootstrapNip98,
}

#[tauri::command]
pub async fn list_tokens(state: State<'_, AppState>) -> Result<ListTokensResponse, String> {
    let request =
        build_token_management_request(&state.http_client, Method::GET, "/api/tokens", &state)?;
    send_json_request(request).await
}

fn build_mint_token_request(
    state: &AppState,
    body: &MintTokenBody<'_>,
    auth_mode: MintTokenAuthMode,
) -> Result<reqwest::RequestBuilder, String> {
    if matches!(auth_mode, MintTokenAuthMode::Auto) && state.configured_api_token.is_some() {
        return Ok(
            build_authed_request(&state.http_client, Method::POST, "/api/tokens", state)?
                .json(body),
        );
    }

    let url = format!("{}{}", relay_api_base_url(), "/api/tokens");
    let body_bytes =
        serde_json::to_vec(body).map_err(|error| format!("serialize failed: {error}"))?;
    let auth_header = build_nip98_auth_header(&Method::POST, &url, &body_bytes, state)?;

    Ok(state
        .http_client
        .request(Method::POST, url)
        .header("Authorization", auth_header)
        .header("Content-Type", "application/json")
        .body(body_bytes))
}

/// Internal token minting logic, callable from other modules (e.g. pairing).
pub async fn mint_token_internal(
    state: &AppState,
    name: &str,
    scopes: &[String],
    channel_ids: Option<&[String]>,
    expires_in_days: Option<u32>,
) -> Result<MintTokenResponse, String> {
    mint_token_internal_with_auth_mode(
        state,
        name,
        scopes,
        channel_ids,
        expires_in_days,
        MintTokenAuthMode::Auto,
    )
    .await
}

pub async fn mint_token_internal_with_auth_mode(
    state: &AppState,
    name: &str,
    scopes: &[String],
    channel_ids: Option<&[String]>,
    expires_in_days: Option<u32>,
    auth_mode: MintTokenAuthMode,
) -> Result<MintTokenResponse, String> {
    let body = MintTokenBody {
        name,
        scopes,
        channel_ids,
        expires_in_days,
        owner_pubkey: None,
    };
    let request = build_mint_token_request(state, &body, auth_mode)?;
    let response: MintTokenResponse = send_json_request(request).await?;

    if matches!(auth_mode, MintTokenAuthMode::Auto) && state.configured_api_token.is_none() {
        let mut token = state
            .session_token
            .lock()
            .map_err(|error| error.to_string())?;
        *token = Some(response.token.clone());
    }

    Ok(response)
}

#[tauri::command]
pub async fn mint_token(
    name: String,
    scopes: Vec<String>,
    channel_ids: Option<Vec<String>>,
    expires_in_days: Option<u32>,
    state: State<'_, AppState>,
) -> Result<MintTokenResponse, String> {
    mint_token_internal(
        &state,
        &name,
        &scopes,
        channel_ids.as_deref(),
        expires_in_days,
    )
    .await
}

#[tauri::command]
pub async fn revoke_token(token_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let path = api_path(&["tokens", &token_id]);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::build_app_state;

    fn test_body<'a>(scopes: &'a [String]) -> MintTokenBody<'a> {
        MintTokenBody {
            name: "test-token",
            scopes,
            channel_ids: None,
            expires_in_days: Some(7),
            owner_pubkey: None,
        }
    }

    #[test]
    fn auto_mint_uses_configured_bearer_when_present() {
        let mut state = build_app_state();
        state.configured_api_token = Some("desktop-token".to_string());
        let scopes = vec!["messages:read".to_string()];

        let request =
            build_mint_token_request(&state, &test_body(&scopes), MintTokenAuthMode::Auto)
                .expect("request should build")
                .build()
                .expect("request should finalize");

        assert_eq!(
            request
                .headers()
                .get("Authorization")
                .expect("auth header")
                .to_str()
                .expect("auth header should be valid utf-8"),
            "Bearer desktop-token"
        );
    }

    #[test]
    fn bootstrap_mint_ignores_configured_bearer_token() {
        let mut state = build_app_state();
        state.configured_api_token = Some("desktop-token".to_string());
        let scopes = vec!["messages:read".to_string(), "files:write".to_string()];

        let request = build_mint_token_request(
            &state,
            &test_body(&scopes),
            MintTokenAuthMode::BootstrapNip98,
        )
        .expect("request should build")
        .build()
        .expect("request should finalize");

        let auth_header = request
            .headers()
            .get("Authorization")
            .expect("auth header")
            .to_str()
            .expect("auth header should be valid utf-8");

        assert!(auth_header.starts_with("Nostr "));
        assert_ne!(auth_header, "Bearer desktop-token");
    }
}
