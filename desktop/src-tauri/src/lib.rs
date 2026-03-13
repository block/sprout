use std::{collections::HashMap, sync::Mutex};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use nostr::{EventBuilder, JsonUtil, Keys, Kind, Tag, ToBech32};
use reqwest::Method;
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use sprout_core::PresenceStatus;
use tauri_plugin_window_state::StateFlags;

pub struct AppState {
    pub keys: Mutex<Keys>,
    pub http_client: reqwest::Client,
    pub configured_api_token: Option<String>,
    pub session_token: Mutex<Option<String>>,
}

#[derive(Serialize)]
pub struct IdentityInfo {
    pub pubkey: String,
    pub display_name: String,
}

#[derive(Serialize, Deserialize)]
pub struct ProfileInfo {
    pub pubkey: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub about: Option<String>,
    pub nip05_handle: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct UserProfileSummaryInfo {
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub nip05_handle: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct UsersBatchResponse {
    pub profiles: HashMap<String, UserProfileSummaryInfo>,
    pub missing: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct SetPresenceResponse {
    pub status: PresenceStatus,
    pub ttl_seconds: u64,
}

#[derive(Serialize, Deserialize)]
pub struct ChannelInfo {
    pub id: String,
    pub name: String,
    pub channel_type: String,
    pub visibility: String,
    #[serde(deserialize_with = "deserialize_null_string_as_empty")]
    pub description: String,
    pub topic: Option<String>,
    pub purpose: Option<String>,
    pub member_count: i64,
    pub last_message_at: Option<String>,
    pub archived_at: Option<String>,
    pub participants: Vec<String>,
    pub participant_pubkeys: Vec<String>,
    #[serde(default = "default_true")]
    pub is_member: bool,
}

#[derive(Serialize, Deserialize)]
pub struct ChannelDetailInfo {
    pub id: String,
    pub name: String,
    pub channel_type: String,
    pub visibility: String,
    #[serde(deserialize_with = "deserialize_null_string_as_empty")]
    pub description: String,
    pub topic: Option<String>,
    pub topic_set_by: Option<String>,
    pub topic_set_at: Option<String>,
    pub purpose: Option<String>,
    pub purpose_set_by: Option<String>,
    pub purpose_set_at: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
    pub member_count: i64,
    pub topic_required: bool,
    pub max_members: Option<i32>,
    pub nip29_group_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ChannelMemberInfo {
    pub pubkey: String,
    pub role: String,
    pub joined_at: String,
    pub display_name: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ChannelMembersResponse {
    pub members: Vec<ChannelMemberInfo>,
    pub next_cursor: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct AddMembersResponse {
    pub added: Vec<String>,
    pub errors: Vec<serde_json::Value>,
}

#[derive(Serialize)]
struct CreateChannelBody<'a> {
    name: &'a str,
    channel_type: &'a str,
    visibility: &'a str,
    description: Option<&'a str>,
}

#[derive(Serialize)]
struct UpdateChannelBody<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
}

#[derive(Serialize)]
struct SetTopicBody<'a> {
    topic: &'a str,
}

#[derive(Serialize)]
struct SetPurposeBody<'a> {
    purpose: &'a str,
}

#[derive(Serialize)]
struct AddMembersBody<'a> {
    pubkeys: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<&'a str>,
}

#[derive(Serialize)]
struct UpdateProfileBody<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    avatar_url: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    about: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    nip05_handle: Option<&'a str>,
}

#[derive(Serialize)]
struct SetPresenceBody {
    status: PresenceStatus,
}

#[derive(Serialize)]
struct GetFeedQuery<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    since: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    types: Option<&'a str>,
}

#[derive(Serialize)]
struct SearchQueryParams<'a> {
    q: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u32>,
}

#[derive(Serialize)]
struct SendChannelMessageBody<'a> {
    content: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_event_id: Option<&'a str>,
    broadcast_to_channel: bool,
}

#[derive(Serialize)]
struct AddReactionBody<'a> {
    emoji: &'a str,
}

#[derive(Serialize)]
struct MintTokenBody<'a> {
    name: &'a str,
    scopes: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    channel_ids: Option<&'a [String]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_in_days: Option<u32>,
}

#[derive(Serialize, Deserialize)]
pub struct MintTokenResponse {
    pub id: String,
    pub token: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub channel_ids: Vec<String>,
    pub created_at: String,
    pub expires_at: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct TokenInfo {
    pub id: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub channel_ids: Vec<String>,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
    pub revoked_at: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ListTokensResponse {
    pub tokens: Vec<TokenInfo>,
}

#[derive(Serialize, Deserialize)]
pub struct RevokeAllTokensResponse {
    pub revoked_count: u64,
}

#[derive(Serialize, Deserialize)]
pub struct FeedItemInfo {
    pub id: String,
    pub kind: u32,
    pub pubkey: String,
    pub content: String,
    pub created_at: u64,
    pub channel_id: Option<String>,
    pub channel_name: String,
    pub tags: Vec<Vec<String>>,
    pub category: String,
}

#[derive(Serialize, Deserialize)]
pub struct FeedSections {
    pub mentions: Vec<FeedItemInfo>,
    pub needs_action: Vec<FeedItemInfo>,
    pub activity: Vec<FeedItemInfo>,
    pub agent_activity: Vec<FeedItemInfo>,
}

#[derive(Serialize, Deserialize)]
pub struct FeedMeta {
    pub since: i64,
    pub total: u64,
    pub generated_at: i64,
}

#[derive(Serialize, Deserialize)]
pub struct FeedResponse {
    pub feed: FeedSections,
    pub meta: FeedMeta,
}

#[derive(Serialize, Deserialize)]
pub struct SearchHitInfo {
    pub event_id: String,
    pub content: String,
    pub kind: u32,
    pub pubkey: String,
    pub channel_id: String,
    pub channel_name: String,
    pub created_at: u64,
    pub score: f64,
}

#[derive(Serialize, Deserialize)]
pub struct SearchResponse {
    pub hits: Vec<SearchHitInfo>,
    pub found: u64,
}

#[derive(Serialize, Deserialize)]
pub struct SendChannelMessageResponse {
    pub event_id: String,
    pub parent_event_id: Option<String>,
    pub root_event_id: Option<String>,
    pub depth: u32,
    pub created_at: i64,
}

fn deserialize_null_string_as_empty<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

fn default_true() -> bool {
    true
}

fn percent_encode(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len());

    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                let high = char::from_digit((byte >> 4) as u32, 16)
                    .expect("nibble 0-15 is always a valid hex digit")
                    .to_ascii_uppercase();
                let low = char::from_digit((byte & 0x0f) as u32, 16)
                    .expect("nibble 0-15 is always a valid hex digit")
                    .to_ascii_uppercase();
                encoded.push('%');
                encoded.push(high);
                encoded.push(low);
            }
        }
    }

    encoded
}

fn relay_ws_url() -> String {
    std::env::var("SPROUT_RELAY_URL").unwrap_or_else(|_| "ws://localhost:3000".to_string())
}

fn relay_api_base_url() -> String {
    if let Ok(base) = std::env::var("SPROUT_RELAY_HTTP") {
        return base;
    }

    relay_ws_url()
        .replace("wss://", "https://")
        .replace("ws://", "http://")
}

fn build_authed_request(
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

fn auth_pubkey_header(state: &AppState) -> Result<String, String> {
    let keys = state.keys.lock().map_err(|e| e.to_string())?;
    Ok(keys.public_key().to_hex())
}

fn session_api_token(state: &AppState) -> Result<Option<String>, String> {
    let token = state.session_token.lock().map_err(|e| e.to_string())?;
    Ok(token.clone())
}

fn build_token_management_request(
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

fn build_nip98_auth_header(
    method: &Method,
    url: &str,
    body: &[u8],
    state: &AppState,
) -> Result<String, String> {
    let keys = state.keys.lock().map_err(|e| e.to_string())?;
    let payload_hash = format!("{:x}", Sha256::digest(body));
    let tags = vec![
        Tag::parse(vec!["u", url]).map_err(|e| format!("url tag failed: {e}"))?,
        Tag::parse(vec!["method", method.as_str()])
            .map_err(|e| format!("method tag failed: {e}"))?,
        Tag::parse(vec!["payload", &payload_hash])
            .map_err(|e| format!("payload tag failed: {e}"))?,
    ];

    let event = EventBuilder::new(Kind::HttpAuth, "")
        .tags(tags)
        .sign_with_keys(&keys)
        .map_err(|e| format!("sign failed: {e}"))?;

    Ok(format!("Nostr {}", BASE64.encode(event.as_json().as_bytes())))
}

async fn relay_error_message(response: reqwest::Response) -> String {
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

async fn send_json_request<T>(request: reqwest::RequestBuilder) -> Result<T, String>
where
    T: DeserializeOwned,
{
    let response = request
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(relay_error_message(response).await);
    }

    response
        .json::<T>()
        .await
        .map_err(|e| format!("parse failed: {e}"))
}

async fn send_empty_request(request: reqwest::RequestBuilder) -> Result<(), String> {
    let response = request
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(relay_error_message(response).await);
    }

    Ok(())
}

#[tauri::command]
fn get_identity(state: tauri::State<'_, AppState>) -> Result<IdentityInfo, String> {
    let keys = state.keys.lock().map_err(|e| e.to_string())?;
    let pubkey = keys.public_key();
    let pubkey_hex = pubkey.to_hex();
    let bech32 = pubkey
        .to_bech32()
        .map_err(|e| format!("bech32 encode failed: {e}"))?;
    let display_name = if bech32.len() > 16 {
        format!("{}…{}", &bech32[..10], &bech32[bech32.len() - 4..])
    } else {
        bech32
    };

    Ok(IdentityInfo {
        pubkey: pubkey_hex,
        display_name,
    })
}

#[tauri::command]
fn get_relay_ws_url() -> String {
    relay_ws_url()
}

#[tauri::command]
async fn get_profile(state: tauri::State<'_, AppState>) -> Result<ProfileInfo, String> {
    let request = build_authed_request(
        &state.http_client,
        Method::GET,
        "/api/users/me/profile",
        &state,
    )?;
    send_json_request(request).await
}

#[tauri::command]
async fn update_profile(
    display_name: Option<String>,
    avatar_url: Option<String>,
    about: Option<String>,
    nip05_handle: Option<String>,
    state: tauri::State<'_, AppState>,
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
async fn get_user_profile(
    pubkey: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<ProfileInfo, String> {
    let path = match pubkey {
        Some(pubkey) => format!("/api/users/{pubkey}/profile"),
        None => "/api/users/me/profile".to_string(),
    };
    let request = build_authed_request(&state.http_client, Method::GET, &path, &state)?;
    send_json_request(request).await
}

#[derive(Serialize)]
struct GetUsersBatchBody<'a> {
    pubkeys: &'a [String],
}

#[tauri::command]
async fn get_users_batch(
    pubkeys: Vec<String>,
    state: tauri::State<'_, AppState>,
) -> Result<UsersBatchResponse, String> {
    let request = build_authed_request(
        &state.http_client,
        Method::POST,
        "/api/users/batch",
        &state,
    )?
    .json(&GetUsersBatchBody {
        pubkeys: pubkeys.as_slice(),
    });
    send_json_request(request).await
}

#[tauri::command]
async fn get_presence(
    pubkeys: Vec<String>,
    state: tauri::State<'_, AppState>,
) -> Result<HashMap<String, PresenceStatus>, String> {
    if pubkeys.is_empty() {
        return Ok(HashMap::new());
    }

    let request = build_authed_request(&state.http_client, Method::GET, "/api/presence", &state)?
        .query(&[("pubkeys", pubkeys.join(","))]);
    send_json_request(request).await
}

#[tauri::command]
async fn set_presence(
    status: PresenceStatus,
    state: tauri::State<'_, AppState>,
) -> Result<SetPresenceResponse, String> {
    let request = build_authed_request(&state.http_client, Method::PUT, "/api/presence", &state)?
        .json(&SetPresenceBody { status });
    send_json_request(request).await
}

#[tauri::command]
fn sign_event(
    kind: u16,
    content: String,
    tags: Vec<Vec<String>>,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let keys = state.keys.lock().map_err(|e| e.to_string())?;

    let nostr_tags = tags
        .into_iter()
        .map(|tag| Tag::parse(tag).map_err(|e| format!("invalid tag: {e}")))
        .collect::<Result<Vec<_>, _>>()?;

    let event = EventBuilder::new(Kind::Custom(kind), content)
        .tags(nostr_tags)
        .sign_with_keys(&keys)
        .map_err(|e| format!("sign failed: {e}"))?;

    Ok(event.as_json())
}

#[tauri::command]
fn create_auth_event(
    challenge: String,
    relay_url: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let keys = state.keys.lock().map_err(|e| e.to_string())?;

    let mut tags = vec![
        Tag::parse(vec!["relay", &relay_url]).map_err(|e| format!("relay tag failed: {e}"))?,
        Tag::parse(vec!["challenge", &challenge])
            .map_err(|e| format!("challenge tag failed: {e}"))?,
    ];

    if let Some(token) = state.configured_api_token.as_deref() {
        tags.push(
            Tag::parse(vec!["auth_token", token])
                .map_err(|e| format!("auth token tag failed: {e}"))?,
        );
    }

    let event = EventBuilder::new(Kind::Custom(22242), "")
        .tags(tags)
        .sign_with_keys(&keys)
        .map_err(|e| format!("sign failed: {e}"))?;

    Ok(event.as_json())
}

#[tauri::command]
async fn get_channels(state: tauri::State<'_, AppState>) -> Result<Vec<ChannelInfo>, String> {
    let request = build_authed_request(&state.http_client, Method::GET, "/api/channels", &state)?;
    send_json_request(request).await
}

#[tauri::command]
async fn create_channel(
    name: String,
    channel_type: String,
    visibility: String,
    description: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<ChannelInfo, String> {
    let request = build_authed_request(&state.http_client, Method::POST, "/api/channels", &state)?
        .json(&CreateChannelBody {
            name: &name,
            channel_type: &channel_type,
            visibility: &visibility,
            description: description.as_deref(),
        });

    send_json_request(request).await
}

#[tauri::command]
async fn get_channel_details(
    channel_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ChannelDetailInfo, String> {
    let path = format!("/api/channels/{channel_id}");
    let request = build_authed_request(&state.http_client, Method::GET, &path, &state)?;
    send_json_request(request).await
}

#[tauri::command]
async fn get_channel_members(
    channel_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ChannelMembersResponse, String> {
    let path = format!("/api/channels/{channel_id}/members");
    let request = build_authed_request(&state.http_client, Method::GET, &path, &state)?;
    send_json_request(request).await
}

#[tauri::command]
async fn update_channel(
    channel_id: String,
    name: Option<String>,
    description: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<ChannelDetailInfo, String> {
    let path = format!("/api/channels/{channel_id}");
    let request = build_authed_request(&state.http_client, Method::PUT, &path, &state)?
        .json(&UpdateChannelBody {
            name: name.as_deref(),
            description: description.as_deref(),
        });

    send_json_request(request).await
}

#[tauri::command]
async fn set_channel_topic(
    channel_id: String,
    topic: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let path = format!("/api/channels/{channel_id}/topic");
    let request = build_authed_request(&state.http_client, Method::PUT, &path, &state)?
        .json(&SetTopicBody { topic: &topic });
    send_empty_request(request).await
}

#[tauri::command]
async fn set_channel_purpose(
    channel_id: String,
    purpose: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let path = format!("/api/channels/{channel_id}/purpose");
    let request = build_authed_request(&state.http_client, Method::PUT, &path, &state)?
        .json(&SetPurposeBody { purpose: &purpose });
    send_empty_request(request).await
}

#[tauri::command]
async fn archive_channel(
    channel_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let path = format!("/api/channels/{channel_id}/archive");
    let request = build_authed_request(&state.http_client, Method::POST, &path, &state)?;
    send_empty_request(request).await
}

#[tauri::command]
async fn unarchive_channel(
    channel_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let path = format!("/api/channels/{channel_id}/unarchive");
    let request = build_authed_request(&state.http_client, Method::POST, &path, &state)?;
    send_empty_request(request).await
}

#[tauri::command]
async fn delete_channel(
    channel_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let path = format!("/api/channels/{channel_id}");
    let request = build_authed_request(&state.http_client, Method::DELETE, &path, &state)?;
    send_empty_request(request).await
}

#[tauri::command]
async fn add_channel_members(
    channel_id: String,
    pubkeys: Vec<String>,
    role: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<AddMembersResponse, String> {
    let path = format!("/api/channels/{channel_id}/members");
    let request = build_authed_request(&state.http_client, Method::POST, &path, &state)?
        .json(&AddMembersBody {
            pubkeys: &pubkeys,
            role: role.as_deref(),
        });

    send_json_request(request).await
}

#[tauri::command]
async fn remove_channel_member(
    channel_id: String,
    pubkey: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let path = format!("/api/channels/{channel_id}/members/{pubkey}");
    let request = build_authed_request(&state.http_client, Method::DELETE, &path, &state)?;
    send_empty_request(request).await
}

#[tauri::command]
async fn join_channel(
    channel_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let path = format!("/api/channels/{channel_id}/join");
    let request = build_authed_request(&state.http_client, Method::POST, &path, &state)?;
    send_empty_request(request).await
}

#[tauri::command]
async fn leave_channel(
    channel_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let path = format!("/api/channels/{channel_id}/leave");
    let request = build_authed_request(&state.http_client, Method::POST, &path, &state)?;
    send_empty_request(request).await
}

#[tauri::command]
async fn get_feed(
    since: Option<i64>,
    limit: Option<u32>,
    types: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<FeedResponse, String> {
    let request = build_authed_request(&state.http_client, Method::GET, "/api/feed", &state)?
        .query(&GetFeedQuery {
            since,
            limit,
            types: types.as_deref(),
        });

    send_json_request(request).await
}

#[tauri::command]
async fn search_messages(
    q: String,
    limit: Option<u32>,
    state: tauri::State<'_, AppState>,
) -> Result<SearchResponse, String> {
    let request = build_authed_request(&state.http_client, Method::GET, "/api/search", &state)?
        .query(&SearchQueryParams {
            q: q.trim(),
            limit,
        });

    send_json_request(request).await
}

#[tauri::command]
async fn send_channel_message(
    channel_id: String,
    content: String,
    parent_event_id: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<SendChannelMessageResponse, String> {
    let path = format!("/api/channels/{channel_id}/messages");
    let request = build_authed_request(&state.http_client, Method::POST, &path, &state)?.json(
        &SendChannelMessageBody {
            content: content.trim(),
            parent_event_id: parent_event_id.as_deref(),
            broadcast_to_channel: false,
        },
    );

    send_json_request(request).await
}

#[tauri::command]
async fn add_reaction(
    event_id: String,
    emoji: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let path = format!("/api/messages/{event_id}/reactions");
    let request = build_authed_request(&state.http_client, Method::POST, &path, &state)?.json(
        &AddReactionBody {
            emoji: emoji.trim(),
        },
    );

    send_empty_request(request).await
}

#[tauri::command]
async fn remove_reaction(
    event_id: String,
    emoji: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let path = format!(
        "/api/messages/{event_id}/reactions/{}",
        percent_encode(emoji.trim())
    );
    let request = build_authed_request(&state.http_client, Method::DELETE, &path, &state)?;

    send_empty_request(request).await
}

#[tauri::command]
async fn get_event(event_id: String, state: tauri::State<'_, AppState>) -> Result<String, String> {
    let request = build_authed_request(
        &state.http_client,
        Method::GET,
        &format!("/api/events/{event_id}"),
        &state,
    )?;
    let response = request
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(relay_error_message(response).await);
    }

    response.text().await.map_err(|e| format!("parse failed: {e}"))
}

#[tauri::command]
async fn list_tokens(state: tauri::State<'_, AppState>) -> Result<ListTokensResponse, String> {
    let request =
        build_token_management_request(&state.http_client, Method::GET, "/api/tokens", &state)?;
    send_json_request(request).await
}

#[tauri::command]
async fn mint_token(
    name: String,
    scopes: Vec<String>,
    channel_ids: Option<Vec<String>>,
    expires_in_days: Option<u32>,
    state: tauri::State<'_, AppState>,
) -> Result<MintTokenResponse, String> {
    let body = MintTokenBody {
        name: &name,
        scopes: &scopes,
        channel_ids: channel_ids.as_deref(),
        expires_in_days,
    };
    let request = if state.configured_api_token.is_some() {
        build_authed_request(&state.http_client, Method::POST, "/api/tokens", &state)?.json(&body)
    } else {
        let url = format!("{}{}", relay_api_base_url(), "/api/tokens");
        let body_bytes =
            serde_json::to_vec(&body).map_err(|e| format!("serialize failed: {e}"))?;
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
        let mut token = state.session_token.lock().map_err(|e| e.to_string())?;
        *token = Some(response.token.clone());
    }

    Ok(response)
}

#[tauri::command]
async fn revoke_token(
    token_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let path = format!("/api/tokens/{token_id}");
    let request =
        build_token_management_request(&state.http_client, Method::DELETE, &path, &state)?;
    send_empty_request(request).await
}

#[tauri::command]
async fn revoke_all_tokens(
    state: tauri::State<'_, AppState>,
) -> Result<RevokeAllTokensResponse, String> {
    let request =
        build_token_management_request(&state.http_client, Method::DELETE, "/api/tokens", &state)?;
    send_json_request(request).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // GUI app: warn on bad key but don't crash — fall back to ephemeral.
    // CLI crates (sprout-mcp, sprout-test-client) use fatal errors instead.
    let (keys, source) = match std::env::var("SPROUT_PRIVATE_KEY") {
        Ok(nsec) => match Keys::parse(nsec.trim()) {
            Ok(k) => (k, "configured"),
            Err(e) => {
                eprintln!("sprout-desktop: invalid SPROUT_PRIVATE_KEY: {e}");
                (Keys::generate(), "ephemeral")
            }
        },
        Err(std::env::VarError::NotUnicode(_)) => {
            eprintln!("sprout-desktop: SPROUT_PRIVATE_KEY contains invalid UTF-8");
            (Keys::generate(), "ephemeral")
        }
        Err(std::env::VarError::NotPresent) => (Keys::generate(), "ephemeral"),
    };

    eprintln!(
        "sprout-desktop: {source} identity pubkey {}",
        keys.public_key().to_hex()
    );

    let api_token = match std::env::var("SPROUT_API_TOKEN") {
        Ok(token) if !token.trim().is_empty() => Some(token),
        Ok(_) | Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(_)) => {
            eprintln!("sprout-desktop: SPROUT_API_TOKEN contains invalid UTF-8");
            None
        }
    };

    let app_state = AppState {
        keys: Mutex::new(keys),
        http_client: reqwest::Client::new(),
        configured_api_token: api_token,
        session_token: Mutex::new(None),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    StateFlags::all() & !StateFlags::VISIBLE,
                )
                .build(),
        )
        .plugin(tauri_plugin_websocket::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            get_identity,
            get_profile,
            update_profile,
            get_user_profile,
            get_users_batch,
            get_presence,
            set_presence,
            get_relay_ws_url,
            sign_event,
            create_auth_event,
            get_channels,
            create_channel,
            get_channel_details,
            get_channel_members,
            update_channel,
            set_channel_topic,
            set_channel_purpose,
            archive_channel,
            unarchive_channel,
            delete_channel,
            add_channel_members,
            remove_channel_member,
            join_channel,
            leave_channel,
            get_feed,
            search_messages,
            send_channel_message,
            add_reaction,
            remove_reaction,
            get_event,
            list_tokens,
            mint_token,
            revoke_token,
            revoke_all_tokens,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{percent_encode, ChannelInfo};

    #[test]
    fn channel_info_defaults_is_member_for_legacy_payloads() {
        let channel: ChannelInfo = serde_json::from_value(json!({
            "id": "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50",
            "name": "general",
            "channel_type": "stream",
            "visibility": "open",
            "description": "General discussion",
            "topic": null,
            "purpose": null,
            "member_count": 3,
            "last_message_at": null,
            "archived_at": null,
            "participants": [],
            "participant_pubkeys": []
        }))
        .expect("legacy payload should deserialize");

        assert!(channel.is_member);
    }

    #[test]
    fn percent_encode_leaves_unreserved_chars() {
        assert_eq!(
            percent_encode("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.~"),
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.~"
        );
    }

    #[test]
    fn percent_encode_escapes_unicode_and_reserved_chars() {
        assert_eq!(percent_encode("👍"), "%F0%9F%91%8D");
        assert_eq!(percent_encode("a/b?c"), "a%2Fb%3Fc");
    }
}
