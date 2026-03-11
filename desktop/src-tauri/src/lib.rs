use std::sync::Mutex;

use nostr::{EventBuilder, JsonUtil, Keys, Kind, Tag, ToBech32};
use reqwest::Method;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tauri_plugin_window_state::StateFlags;

pub struct AppState {
    pub keys: Mutex<Keys>,
    pub http_client: reqwest::Client,
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
pub struct ChannelInfo {
    pub id: String,
    pub name: String,
    pub channel_type: String,
    pub visibility: String,
    pub description: String,
    pub topic: Option<String>,
    pub purpose: Option<String>,
    pub member_count: i64,
    pub last_message_at: Option<String>,
    pub archived_at: Option<String>,
    pub participants: Vec<String>,
    pub participant_pubkeys: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ChannelDetailInfo {
    pub id: String,
    pub name: String,
    pub channel_type: String,
    pub visibility: String,
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
    let pubkey_hex = auth_pubkey_header(state)?;
    let url = format!("{}{}", relay_api_base_url(), path);

    Ok(client.request(method, url).header("X-Pubkey", pubkey_hex))
}

fn auth_pubkey_header(state: &AppState) -> Result<String, String> {
    let keys = state.keys.lock().map_err(|e| e.to_string())?;
    Ok(keys.public_key().to_hex())
}

async fn relay_error_message(response: reqwest::Response) -> String {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&body) {
        if let Some(message) = value.get("error").and_then(serde_json::Value::as_str) {
            return format!("relay returned {status}: {message}");
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

    let tags = vec![
        Tag::parse(vec!["relay", &relay_url]).map_err(|e| format!("relay tag failed: {e}"))?,
        Tag::parse(vec!["challenge", &challenge])
            .map_err(|e| format!("challenge tag failed: {e}"))?,
    ];

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = AppState {
        keys: Mutex::new(Keys::generate()),
        http_client: reqwest::Client::new(),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED,
                )
                .build(),
        )
        .plugin(tauri_plugin_websocket::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            get_identity,
            get_profile,
            update_profile,
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
            get_event,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
