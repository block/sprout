use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use nostr::{EventBuilder, JsonUtil, Keys, Kind, Tag};
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::app_state::AppState;

const DEFAULT_RELAY_WS_URL: &str = "ws://localhost:3000";

fn configured_env_var(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub fn relay_ws_url() -> String {
    configured_env_var("SPROUT_RELAY_URL")
        .or_else(|| option_env!("SPROUT_DESKTOP_BUILD_RELAY_URL").map(str::to_string))
        .unwrap_or_else(|| DEFAULT_RELAY_WS_URL.to_string())
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
    if let Some(base) = configured_env_var("SPROUT_RELAY_HTTP") {
        return base.trim_end_matches('/').to_string();
    }

    if let Some(base) = option_env!("SPROUT_DESKTOP_BUILD_RELAY_HTTP") {
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
    agent_keys: &nostr::Keys,
    api_token: Option<&str>,
    token_scopes: &[String],
    display_name: &str,
    avatar_url: Option<&str>,
) -> Result<(), String> {
    // Build a kind:0 profile event signed by the agent's keys.
    let builder = crate::events::build_profile(Some(display_name), None, avatar_url, None, None)?;

    // Sign with the agent's keys (not the desktop user's).
    let event = builder
        .sign_with_keys(agent_keys)
        .map_err(|e| format!("failed to sign profile event: {e}"))?;
    let event_json = event.as_json();

    // POST to the relay's /api/events endpoint.
    let url = format!("{}/api/events", relay_http_base_url(relay_url));
    let use_bearer_token = api_token.is_some() && token_supports_scope(token_scopes, "users:write");
    let mut request = state.http_client.post(&url);

    if let Some(token) = api_token.filter(|_| use_bearer_token) {
        request = request.header("Authorization", format!("Bearer {token}"));
    } else {
        request = request.header("X-Pubkey", agent_keys.public_key().to_hex());
    }

    request = request
        .header("Content-Type", "application/json")
        .body(event_json);

    let response = request
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !response.status().is_success() {
        let msg = relay_error_message(response).await;
        return Err(if api_token.is_some() && !use_bearer_token {
            format!(
                "Created the agent, but could not sync its profile metadata. The minted token does not include `users:write`, and the relay rejected dev-mode pubkey auth: {msg}"
            )
        } else if api_token.is_some() {
            format!("Created the agent, but could not sync its profile metadata: {msg}")
        } else {
            format!(
                "Created the agent, but could not sync its profile metadata without a token: {msg}"
            )
        });
    }

    Ok(())
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
        .sign_with_keys(keys)
        .map_err(|error| format!("sign failed: {error}"))?;

    Ok(format!(
        "Nostr {}",
        BASE64.encode(event.as_json().as_bytes())
    ))
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

// ── Signed-event submission ──────────────────────────────────────────────────

/// Response from `POST /api/events`.
#[derive(Debug, Deserialize)]
pub struct SubmitEventResponse {
    pub event_id: String,
    pub accepted: bool,
    pub message: String,
}

/// Build an `EventBuilder` from the events module, sign it with the user's keys,
/// and POST the signed event to `/api/events`.
pub async fn submit_event(
    builder: nostr::EventBuilder,
    state: &AppState,
) -> Result<SubmitEventResponse, String> {
    // All synchronous work (signing) must complete before any .await
    // so the MutexGuard is dropped and the future remains Send.
    let (event_json, auth_header) = {
        let keys = state.keys.lock().map_err(|e| e.to_string())?;
        let event = builder
            .sign_with_keys(&keys)
            .map_err(|e| format!("failed to sign event: {e}"))?;
        let json = event.as_json();
        let auth = match state.configured_api_token.as_deref() {
            Some(token) => format!("Bearer {token}"),
            None => format!("X-Pubkey {}", keys.public_key().to_hex()),
        };
        (json, auth)
    }; // keys lock dropped here

    let url = format!("{}/api/events", relay_api_base_url());
    let request = if auth_header.starts_with("Bearer ") {
        state
            .http_client
            .post(&url)
            .header("Authorization", &auth_header)
    } else {
        let pubkey = auth_header.strip_prefix("X-Pubkey ").unwrap_or("");
        state.http_client.post(&url).header("X-Pubkey", pubkey)
    }
    .header("Content-Type", "application/json")
    .body(event_json);

    let response = request
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(relay_error_message(response).await);
    }

    let result: SubmitEventResponse = response
        .json()
        .await
        .map_err(|e| format!("failed to parse response: {e}"))?;

    if !result.accepted {
        return Err(format!("relay rejected event: {}", result.message));
    }

    Ok(result)
}
