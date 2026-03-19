use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use nostr::{EventBuilder, JsonUtil, Keys, Kind, Tag};
use reqwest::Method;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

use crate::{
    app_state::AppState,
    models::{MintTokenBody, MintTokenResponse, UpdateProfileBody},
};

pub fn relay_ws_url() -> String {
    std::env::var("SPROUT_RELAY_URL").unwrap_or_else(|_| "ws://localhost:3000".to_string())
}

pub fn relay_http_base_url(relay_url: &str) -> String {
    let trimmed = relay_url.trim().trim_end_matches('/');

    if let Some(suffix) = trimmed.strip_prefix("wss://") {
        return format!("https://{suffix}");
    }

    if let Some(suffix) = trimmed.strip_prefix("ws://") {
        return format!("http://{suffix}");
    }

    trimmed.to_string()
}

pub fn relay_api_base_url() -> String {
    if let Ok(base) = std::env::var("SPROUT_RELAY_HTTP") {
        return base.trim().trim_end_matches('/').to_string();
    }

    relay_http_base_url(&relay_ws_url())
}

pub fn build_authed_request(
    client: &reqwest::Client,
    method: Method,
    path: &str,
    state: &AppState,
) -> Result<reqwest::RequestBuilder, String> {
    let url = format!("{}{}", relay_api_base_url(), path);
    let request = client.request(method, url);

    if let Some(token) = state.configured_api_token.as_deref() {
        return Ok(request.header("Authorization", format!("Bearer {token}")));
    }

    let pubkey_hex = auth_pubkey_header(state)?;
    Ok(request.header("X-Pubkey", pubkey_hex))
}

pub fn auth_pubkey_header(state: &AppState) -> Result<String, String> {
    let keys = state.keys.lock().map_err(|error| error.to_string())?;
    Ok(keys.public_key().to_hex())
}

fn token_supports_scope(scopes: &[String], required_scope: &str) -> bool {
    scopes.iter().any(|scope| scope == required_scope)
}

pub async fn sync_managed_agent_profile(
    state: &AppState,
    relay_url: &str,
    pubkey: &str,
    api_token: Option<&str>,
    token_scopes: &[String],
    display_name: &str,
    avatar_url: Option<&str>,
) -> Result<(), String> {
    let url = format!(
        "{}{}",
        relay_http_base_url(relay_url),
        "/api/users/me/profile"
    );
    let use_bearer_token = api_token.is_some() && token_supports_scope(token_scopes, "users:write");
    let mut request = state.http_client.request(Method::PUT, url);

    if let Some(token) = api_token.filter(|_| use_bearer_token) {
        request = request.header("Authorization", format!("Bearer {token}"));
    } else {
        request = request.header("X-Pubkey", pubkey);
    }

    let request = request.json(&UpdateProfileBody {
        display_name: Some(display_name),
        avatar_url,
        about: None,
        nip05_handle: None,
    });

    send_empty_request(request).await.map_err(|error| {
        if api_token.is_some() && !use_bearer_token {
            format!(
                "Created the agent, but could not sync its profile metadata. The minted token does not include `users:write`, and the relay rejected dev-mode pubkey auth: {error}"
            )
        } else if api_token.is_some() {
            format!("Created the agent, but could not sync its profile metadata: {error}")
        } else {
            format!(
                "Created the agent, but could not sync its profile metadata without a token: {error}"
            )
        }
    })
}

fn session_api_token(state: &AppState) -> Result<Option<String>, String> {
    let token = state
        .session_token
        .lock()
        .map_err(|error| error.to_string())?;
    Ok(token.clone())
}

pub fn build_token_management_request(
    client: &reqwest::Client,
    method: Method,
    path: &str,
    state: &AppState,
) -> Result<reqwest::RequestBuilder, String> {
    let url = format!("{}{}", relay_api_base_url(), path);
    let request = client.request(method, url);

    if let Some(token) = state.configured_api_token.as_deref() {
        return Ok(request.header("Authorization", format!("Bearer {token}")));
    }

    if let Some(token) = session_api_token(state)? {
        return Ok(request.header("Authorization", format!("Bearer {token}")));
    }

    let pubkey_hex = auth_pubkey_header(state)?;
    Ok(request.header("X-Pubkey", pubkey_hex))
}

pub fn build_nip98_auth_header(
    method: &Method,
    url: &str,
    body: &[u8],
    state: &AppState,
) -> Result<String, String> {
    let keys = state.keys.lock().map_err(|error| error.to_string())?;
    build_nip98_auth_header_for_keys(&keys, method, url, body)
}

pub fn build_nip98_auth_header_for_keys(
    keys: &Keys,
    method: &Method,
    url: &str,
    body: &[u8],
) -> Result<String, String> {
    let payload_hash = format!("{:x}", Sha256::digest(body));
    let tags = vec![
        Tag::parse(vec!["u", url]).map_err(|error| format!("url tag failed: {error}"))?,
        Tag::parse(vec!["method", method.as_str()])
            .map_err(|error| format!("method tag failed: {error}"))?,
        Tag::parse(vec!["payload", &payload_hash])
            .map_err(|error| format!("payload tag failed: {error}"))?,
    ];

    let event = EventBuilder::new(Kind::HttpAuth, "")
        .tags(tags)
        .sign_with_keys(&keys)
        .map_err(|error| format!("sign failed: {error}"))?;

    Ok(format!(
        "Nostr {}",
        BASE64.encode(event.as_json().as_bytes())
    ))
}

pub async fn mint_managed_agent_api_token(
    client: &reqwest::Client,
    relay_url: &str,
    keys: &Keys,
    name: &str,
    scopes: &[String],
) -> Result<MintTokenResponse, String> {
    let url = format!("{}{}", relay_http_base_url(relay_url), "/api/tokens");
    let body = MintTokenBody {
        name,
        scopes,
        channel_ids: None,
        expires_in_days: None,
    };
    let body_bytes =
        serde_json::to_vec(&body).map_err(|error| format!("serialize failed: {error}"))?;
    let auth_header = build_nip98_auth_header_for_keys(keys, &Method::POST, &url, &body_bytes)?;
    let forwarded_proto = if url.starts_with("http://") {
        "http"
    } else {
        "https"
    };
    let request = client
        .request(Method::POST, url)
        .header("Authorization", auth_header)
        .header("Content-Type", "application/json")
        .header("X-Forwarded-Proto", forwarded_proto)
        .body(body_bytes);

    send_json_request(request).await
}

pub async fn relay_error_message(response: reqwest::Response) -> String {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&body) {
        if let Some(message) = value.get("message").and_then(serde_json::Value::as_str) {
            return format!("relay returned {status}: {message}");
        }

        if let Some(error) = value.get("error").and_then(serde_json::Value::as_str) {
            return format!("relay returned {status}: {error}");
        }
    }

    format!("relay returned {status}: {body}")
}

pub async fn send_json_request<T>(request: reqwest::RequestBuilder) -> Result<T, String>
where
    T: DeserializeOwned,
{
    let response = request
        .send()
        .await
        .map_err(|error| format!("request failed: {error}"))?;

    if !response.status().is_success() {
        return Err(relay_error_message(response).await);
    }

    response
        .json::<T>()
        .await
        .map_err(|error| format!("parse failed: {error}"))
}

pub async fn send_empty_request(request: reqwest::RequestBuilder) -> Result<(), String> {
    let response = request
        .send()
        .await
        .map_err(|error| format!("request failed: {error}"))?;

    if !response.status().is_success() {
        return Err(relay_error_message(response).await);
    }

    Ok(())
}
