use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use nostr::{EventBuilder, JsonUtil, Keys, Kind, Tag};
use reqwest::Method;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use sha2::{Digest, Sha256};

// nostr 0.36 alias — required for cross-version bridging with sprout-sdk.
use nostr_compat;

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

/// Read the workspace relay URL override, if set. Returns `None` when no
/// override is active or when the mutex is poisoned (best-effort).
fn workspace_relay_override(state: &AppState) -> Option<String> {
    state
        .relay_url_override
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
}

/// Returns the relay WebSocket URL, checking the workspace override first.
/// Precedence: workspace override > env vars > build-time vars > default.
pub fn relay_ws_url_with_override(state: &AppState) -> String {
    workspace_relay_override(state).unwrap_or_else(relay_ws_url)
}

/// Returns the relay HTTP API base URL, checking the workspace override first.
/// Precedence: workspace override > env vars > build-time vars > default.
pub fn relay_api_base_url_with_override(state: &AppState) -> String {
    match workspace_relay_override(state) {
        Some(url) => relay_http_base_url(&url),
        None => relay_api_base_url(),
    }
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
    let url = format!("{}{}", relay_api_base_url_with_override(state), path);
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

pub fn auth_pubkey_header(state: &AppState) -> Result<String, String> {
    let keys = state.keys.lock().map_err(|error| error.to_string())?;
    Ok(keys.public_key().to_hex())
}

fn token_supports_scope(scopes: &[String], required_scope: &str) -> bool {
    scopes.iter().any(|scope| scope == required_scope)
}

/// Build a signed kind:0 profile event, optionally injecting a verified NIP-OA auth tag.
///
/// This is a pure function (no I/O) extracted from `sync_managed_agent_profile` so that
/// the event-building and auth-tag-injection logic can be unit tested without HTTP calls.
///
/// `sprout-sdk` uses `nostr 0.36` while the desktop crate uses `nostr 0.37`. Cross-version
/// bridging is done via hex-encoded public keys and raw tag slices — both versions share the
/// same wire format.
fn build_profile_event(
    agent_keys: &nostr::Keys,
    display_name: &str,
    avatar_url: Option<&str>,
    auth_tag_json: Option<&str>,
) -> Result<nostr::Event, String> {
    let builder = crate::events::build_profile(Some(display_name), None, avatar_url, None, None)?;

    let builder = if let Some(tag_json) = auth_tag_json {
        // Bridge nostr 0.37 PublicKey → nostr 0.36 PublicKey via hex encoding.
        let agent_pubkey_hex = agent_keys.public_key().to_hex();
        let compat_pubkey = nostr_compat::PublicKey::from_hex(&agent_pubkey_hex)
            .map_err(|e| format!("failed to convert agent pubkey for auth verification: {e}"))?;

        // Verify Schnorr signature before injecting into profile event.
        sprout_sdk::nip_oa::verify_auth_tag(tag_json, &compat_pubkey)
            .map_err(|e| format!("auth tag verification failed for profile event: {e}"))?;

        // parse_auth_tag returns a nostr 0.36 Tag; bridge to nostr 0.37 via raw slice.
        let compat_tag = sprout_sdk::nip_oa::parse_auth_tag(tag_json)
            .map_err(|e| format!("failed to parse verified auth tag: {e}"))?;
        let tag = nostr::Tag::parse(compat_tag.as_slice())
            .map_err(|e| format!("failed to convert auth tag to nostr 0.37: {e}"))?;
        builder.tags([tag])
    } else {
        builder
    };

    builder
        .sign_with_keys(agent_keys)
        .map_err(|e| format!("failed to sign profile event: {e}"))
}

pub async fn sync_managed_agent_profile(
    state: &AppState,
    relay_url: &str,
    agent_keys: &nostr::Keys,
    api_token: Option<&str>,
    token_scopes: &[String],
    display_name: &str,
    avatar_url: Option<&str>,
    auth_tag: Option<&str>, // NIP-OA auth tag JSON
) -> Result<(), String> {
    // Build a signed kind:0 profile event (with optional NIP-OA auth tag).
    let event = build_profile_event(agent_keys, display_name, avatar_url, auth_tag)?;
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
    let url = format!("{}{}", relay_api_base_url_with_override(state), path);
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

#[cfg(test)]
mod tests {
    use super::{api_path, build_profile_event, validate_api_path};

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

    // ── build_profile_event ──────────────────────────────────────────────────

    /// Generate a valid NIP-OA auth tag JSON string signed by a fresh owner key
    /// and addressed to `agent_keys`.
    ///
    /// Uses `nostr_compat` (nostr 0.36) for the owner keys because
    /// `sprout_sdk::nip_oa::compute_auth_tag` expects nostr 0.36 types.
    /// The agent pubkey is bridged via hex encoding.
    fn make_valid_auth_tag(agent_keys: &nostr::Keys) -> String {
        let owner_keys = nostr_compat::Keys::generate();
        let agent_pubkey_hex = agent_keys.public_key().to_hex();
        let agent_compat_pubkey = nostr_compat::PublicKey::from_hex(&agent_pubkey_hex)
            .expect("valid hex pubkey should parse");
        sprout_sdk::nip_oa::compute_auth_tag(&owner_keys, &agent_compat_pubkey, "")
            .expect("compute_auth_tag should not fail with distinct keys")
    }

    #[test]
    fn profile_event_with_valid_auth_tag() {
        let agent_keys = nostr::Keys::generate();
        let tag_json = make_valid_auth_tag(&agent_keys);
        let event = build_profile_event(&agent_keys, "TestBot", None, Some(&tag_json))
            .expect("should succeed with a valid auth tag");

        // Exactly one "auth" tag must be present.
        let auth_tags: Vec<_> = event
            .tags
            .iter()
            .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("auth"))
            .collect();
        assert_eq!(auth_tags.len(), 1, "expected exactly 1 auth tag");

        // Must be a kind:0 (Metadata) event.
        assert_eq!(event.kind, nostr::Kind::Metadata);
    }

    #[test]
    fn profile_event_without_auth_tag() {
        let agent_keys = nostr::Keys::generate();
        let event = build_profile_event(&agent_keys, "TestBot", None, None)
            .expect("should succeed without an auth tag");

        // No "auth" tags should be present.
        let auth_tags: Vec<_> = event
            .tags
            .iter()
            .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("auth"))
            .collect();
        assert_eq!(auth_tags.len(), 0, "expected no auth tags");

        assert_eq!(event.kind, nostr::Kind::Metadata);
    }

    #[test]
    fn profile_event_rejects_invalid_auth_tag() {
        let agent_keys = nostr::Keys::generate();
        // Structurally valid JSON array but with a bogus signature — verification must fail.
        let bad_json = format!(r#"["auth","{}","","{}"]"#, "a".repeat(64), "b".repeat(128));
        let result = build_profile_event(&agent_keys, "TestBot", None, Some(&bad_json));
        assert!(result.is_err(), "should reject an invalid auth tag");
        assert!(
            result.unwrap_err().contains("verification failed"),
            "error message should mention verification failure"
        );
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
        let auth = if let Some(token) = state.configured_api_token.as_deref() {
            format!("Bearer {token}")
        } else if let Some(token) = session_api_token(state)? {
            format!("Bearer {token}")
        } else {
            format!("X-Pubkey {}", keys.public_key().to_hex())
        };
        (json, auth)
    }; // keys lock dropped here

    let url = format!("{}/api/events", relay_api_base_url_with_override(state));
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
