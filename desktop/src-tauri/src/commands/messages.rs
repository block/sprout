use reqwest::Method;
use tauri::State;

use crate::{
    app_state::AppState,
    models::{
        AddReactionBody, FeedResponse, GetFeedQuery, SearchQueryParams, SearchResponse,
        SendChannelMessageBody, SendChannelMessageResponse,
    },
    relay::{build_authed_request, relay_error_message, send_empty_request, send_json_request},
    util::percent_encode,
};

#[tauri::command]
pub async fn get_feed(
    since: Option<i64>,
    limit: Option<u32>,
    types: Option<String>,
    state: State<'_, AppState>,
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
pub async fn search_messages(
    q: String,
    limit: Option<u32>,
    state: State<'_, AppState>,
) -> Result<SearchResponse, String> {
    let request = build_authed_request(&state.http_client, Method::GET, "/api/search", &state)?
        .query(&SearchQueryParams { q: q.trim(), limit });

    send_json_request(request).await
}

#[tauri::command]
pub async fn send_channel_message(
    channel_id: String,
    content: String,
    parent_event_id: Option<String>,
    mention_pubkeys: Option<Vec<String>>,
    state: State<'_, AppState>,
) -> Result<SendChannelMessageResponse, String> {
    let path = format!("/api/channels/{channel_id}/messages");
    let mentions = mention_pubkeys.unwrap_or_default();
    let mention_refs: Vec<&str> = mentions.iter().map(|s| s.as_str()).collect();
    let request = build_authed_request(&state.http_client, Method::POST, &path, &state)?.json(
        &SendChannelMessageBody {
            content: content.trim(),
            parent_event_id: parent_event_id.as_deref(),
            broadcast_to_channel: false,
            mention_pubkeys: mention_refs,
        },
    );

    send_json_request(request).await
}

#[tauri::command]
pub async fn add_reaction(
    event_id: String,
    emoji: String,
    state: State<'_, AppState>,
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
pub async fn remove_reaction(
    event_id: String,
    emoji: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let path = format!(
        "/api/messages/{event_id}/reactions/{}",
        percent_encode(emoji.trim())
    );
    let request = build_authed_request(&state.http_client, Method::DELETE, &path, &state)?;

    send_empty_request(request).await
}

#[tauri::command]
pub async fn get_event(event_id: String, state: State<'_, AppState>) -> Result<String, String> {
    let request = build_authed_request(
        &state.http_client,
        Method::GET,
        &format!("/api/events/{event_id}"),
        &state,
    )?;
    let response = request
        .send()
        .await
        .map_err(|error| format!("request failed: {error}"))?;

    if !response.status().is_success() {
        return Err(relay_error_message(response).await);
    }

    response
        .text()
        .await
        .map_err(|error| format!("parse failed: {error}"))
}
