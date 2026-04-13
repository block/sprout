use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use nostr::{EventBuilder, JsonUtil, Keys, Kind, Tag};
use reqwest::{header::CONTENT_TYPE, Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::app_state::AppState;
use crate::util::percent_encode;

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

/// Build a relay API path from untrusted path segments by percent-encoding each segment.
pub fn api_path(segments: &[&str]) -> String {
    let mut path = String::from("/api");
    for segment in segments {
        path.push('/');
        path.push_str(&percent_encode(segment));
    }
    path
}

fn validate_api_path(path: &str) -> Result<(), String> {
    let path_only = path
        .split_once('?')
        .map(|(prefix, _)| prefix)
        .unwrap_or(path);

    if !path_only.starts_with('/') {
        return Err("API paths must start with '/'".to_string());
    }

    if path_only
        .split('/')
        .any(|segment| matches!(segment, "." | ".."))
    {
        return Err("API path contains unsafe traversal segments".to_string());
    }

    Ok(())
}

pub fn build_authed_request(
    client: &reqwest::Client,
    method: Method,
    path: &str,
    state: &AppState,
) -> Result<reqwest::RequestBuilder, String> {
    validate_api_path(path)?;
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
    validate_api_path(path)?;
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
    let payload_hash = hex::encode(Sha256::digest(body));
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

fn summarize_response_body(body: &str) -> String {
    let trimmed = body.trim();
    let preview: String = trimmed.chars().take(160).collect();
    if trimmed.chars().count() > 160 {
        format!("{preview}...")
    } else {
        preview
    }
}

fn cloudflare_access_error(final_url: &str, body: &str) -> Option<String> {
    let lower_body = body.to_ascii_lowercase();
    if final_url.contains("cloudflareaccess.com")
        || lower_body.contains("warp client")
        || lower_body.contains("cdn-cgi/access/login")
    {
        return Some(
            "Relay returned a Cloudflare Access login page instead of JSON. Connect via the Warp client or use a local relay (`just relay` + `just dev`).".to_string(),
        );
    }
    None
}

fn decode_json_body<T>(
    status: StatusCode,
    content_type: Option<&str>,
    final_url: &str,
    body: &str,
    operation: &str,
) -> Result<T, String>
where
    T: DeserializeOwned,
{
    if let Some(message) = cloudflare_access_error(final_url, body) {
        return Err(message);
    }

    serde_json::from_str::<T>(body).map_err(|error| {
        let content_type = content_type.unwrap_or("unknown");
        let preview = summarize_response_body(body);
        format!(
            "{operation} failed to decode JSON (status {status}, content-type {content_type}). Response preview: {preview}. Error: {error}"
        )
    })
}

async fn parse_json_response<T>(response: reqwest::Response, operation: &str) -> Result<T, String>
where
    T: DeserializeOwned,
{
    let status = response.status();
    let final_url = response.url().to_string();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let body = response
        .text()
        .await
        .map_err(|error| format!("{operation} failed to read response body: {error}"))?;

    decode_json_body(
        status,
        content_type.as_deref(),
        &final_url,
        &body,
        operation,
    )
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

    parse_json_response(response, "Relay response").await
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

#[cfg(test)]
mod tests {
    use super::{api_path, cloudflare_access_error, decode_json_body, validate_api_path};
    use reqwest::StatusCode;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct TestPayload {
        ok: bool,
    }

    #[test]
    fn api_path_encodes_path_segments() {
        let path = api_path(&["tokens", "../../etc/passwd"]);
        assert_eq!(path, "/api/tokens/..%2F..%2Fetc%2Fpasswd");
    }

    #[test]
    fn validate_api_path_rejects_traversal_segments() {
        assert!(validate_api_path("/api/tokens/../admin").is_err());
        assert!(validate_api_path("/api/tokens/./admin").is_err());
    }

    #[test]
    fn validate_api_path_allows_encoded_segments() {
        assert!(validate_api_path("/api/tokens/..%2Fadmin").is_ok());
    }

    #[test]
    fn cloudflare_access_error_detects_login_redirect() {
        let message = cloudflare_access_error(
            "https://sqprod.cloudflareaccess.com/cdn-cgi/access/login/example",
            "Please authenticate via the warp client",
        );
        assert!(message.is_some());
    }

    #[test]
    fn decode_json_body_parses_valid_json() {
        let parsed: TestPayload = decode_json_body(
            StatusCode::OK,
            Some("application/json"),
            "https://sprout.example/api/channels",
            r#"{"ok":true}"#,
            "Relay response",
        )
        .expect("valid JSON should parse");

        assert_eq!(parsed, TestPayload { ok: true });
    }

    #[test]
    fn decode_json_body_surfaces_cloudflare_hint() {
        let error = decode_json_body::<TestPayload>(
            StatusCode::OK,
            Some("text/html"),
            "https://sqprod.cloudflareaccess.com/cdn-cgi/access/login/example",
            "<html>Please authenticate via the warp client</html>",
            "Relay response",
        )
        .expect_err("Cloudflare Access HTML should not parse");

        assert!(error.contains("Cloudflare Access"));
        assert!(error.contains("Warp client"));
    }

    #[test]
    fn decode_json_body_includes_preview_for_non_json() {
        let error = decode_json_body::<TestPayload>(
            StatusCode::OK,
            Some("text/plain"),
            "https://sprout.example/api/channels",
            "not json",
            "Relay response",
        )
        .expect_err("plain text should fail");

        assert!(error.contains("failed to decode JSON"));
        assert!(error.contains("content-type text/plain"));
        assert!(error.contains("Response preview: not json"));
    }
}

// ── Signed-event submission ──────────────────────────────────────────────────

/// Response from `POST /api/events`.
#[derive(Debug, Deserialize, serde::Serialize)]
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

    let result: SubmitEventResponse =
        parse_json_response(response, "Relay submit response").await?;

    if !result.accepted {
        return Err(format!("relay rejected event: {}", result.message));
    }

    Ok(result)
}
