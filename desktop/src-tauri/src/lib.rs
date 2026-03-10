use std::sync::Mutex;

use nostr::{EventBuilder, JsonUtil, Keys, Kind, Tag, ToBech32};
use serde::{Deserialize, Serialize};
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
pub struct ChannelInfo {
    pub id: String,
    pub name: String,
    pub channel_type: String,
    pub description: String,
    pub participants: Vec<String>,
    pub participant_pubkeys: Vec<String>,
}

#[derive(Serialize)]
struct CreateChannelBody<'a> {
    name: &'a str,
    channel_type: &'a str,
    visibility: &'a str,
    description: Option<&'a str>,
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

async fn build_authed_request(
    client: &reqwest::Client,
    path: &str,
    state: &AppState,
) -> Result<reqwest::RequestBuilder, String> {
    let pubkey_hex = auth_pubkey_header(state)?;
    let url = format!("{}{}", relay_api_base_url(), path);

    Ok(client.get(url).header("X-Pubkey", pubkey_hex))
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
    let request = build_authed_request(&state.http_client, "/api/channels", &state).await?;
    let response = request
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(relay_error_message(response).await);
    }

    response
        .json::<Vec<ChannelInfo>>()
        .await
        .map_err(|e| format!("parse failed: {e}"))
}

#[tauri::command]
async fn create_channel(
    name: String,
    channel_type: String,
    visibility: String,
    description: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<ChannelInfo, String> {
    let pubkey_hex = auth_pubkey_header(&state)?;
    let url = format!("{}{}", relay_api_base_url(), "/api/channels");
    let response = state
        .http_client
        .post(url)
        .header("X-Pubkey", pubkey_hex)
        .json(&CreateChannelBody {
            name: &name,
            channel_type: &channel_type,
            visibility: &visibility,
            description: description.as_deref(),
        })
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(relay_error_message(response).await);
    }

    response
        .json::<ChannelInfo>()
        .await
        .map_err(|e| format!("parse failed: {e}"))
}

#[tauri::command]
async fn get_feed(
    since: Option<i64>,
    limit: Option<u32>,
    types: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<FeedResponse, String> {
    let pubkey_hex = auth_pubkey_header(&state)?;
    let url = format!("{}{}", relay_api_base_url(), "/api/feed");
    let response = state
        .http_client
        .get(url)
        .header("X-Pubkey", pubkey_hex)
        .query(&GetFeedQuery {
            since,
            limit,
            types: types.as_deref(),
        })
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(relay_error_message(response).await);
    }

    response
        .json::<FeedResponse>()
        .await
        .map_err(|e| format!("parse failed: {e}"))
}

#[tauri::command]
async fn search_messages(
    q: String,
    limit: Option<u32>,
    state: tauri::State<'_, AppState>,
) -> Result<SearchResponse, String> {
    let pubkey_hex = auth_pubkey_header(&state)?;
    let url = format!("{}{}", relay_api_base_url(), "/api/search");
    let response = state
        .http_client
        .get(url)
        .header("X-Pubkey", pubkey_hex)
        .query(&SearchQueryParams {
            q: q.trim(),
            limit,
        })
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(relay_error_message(response).await);
    }

    response
        .json::<SearchResponse>()
        .await
        .map_err(|e| format!("parse failed: {e}"))
}

#[tauri::command]
async fn get_event(event_id: String, state: tauri::State<'_, AppState>) -> Result<String, String> {
    let request = build_authed_request(
        &state.http_client,
        &format!("/api/events/{event_id}"),
        &state,
    )
    .await?;
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
            get_relay_ws_url,
            sign_event,
            create_auth_event,
            get_channels,
            create_channel,
            get_feed,
            search_messages,
            get_event,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
