use reqwest::Method;
use tauri::State;

use crate::{
    app_state::AppState,
    models::{
        AddMembersBody, AddMembersResponse, ChannelDetailInfo, ChannelInfo, ChannelMembersResponse,
        CreateChannelBody, SetPurposeBody, SetTopicBody, UpdateChannelBody,
    },
    relay::{build_authed_request, send_empty_request, send_json_request},
};

#[tauri::command]
pub async fn get_channels(state: State<'_, AppState>) -> Result<Vec<ChannelInfo>, String> {
    let request = build_authed_request(&state.http_client, Method::GET, "/api/channels", &state)?;
    send_json_request(request).await
}

#[tauri::command]
pub async fn create_channel(
    name: String,
    channel_type: String,
    visibility: String,
    description: Option<String>,
    state: State<'_, AppState>,
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
pub async fn get_channel_details(
    channel_id: String,
    state: State<'_, AppState>,
) -> Result<ChannelDetailInfo, String> {
    let path = format!("/api/channels/{channel_id}");
    let request = build_authed_request(&state.http_client, Method::GET, &path, &state)?;
    send_json_request(request).await
}

#[tauri::command]
pub async fn get_channel_members(
    channel_id: String,
    state: State<'_, AppState>,
) -> Result<ChannelMembersResponse, String> {
    let path = format!("/api/channels/{channel_id}/members");
    let request = build_authed_request(&state.http_client, Method::GET, &path, &state)?;
    send_json_request(request).await
}

#[tauri::command]
pub async fn update_channel(
    channel_id: String,
    name: Option<String>,
    description: Option<String>,
    state: State<'_, AppState>,
) -> Result<ChannelDetailInfo, String> {
    let path = format!("/api/channels/{channel_id}");
    let request = build_authed_request(&state.http_client, Method::PUT, &path, &state)?.json(
        &UpdateChannelBody {
            name: name.as_deref(),
            description: description.as_deref(),
        },
    );

    send_json_request(request).await
}

#[tauri::command]
pub async fn set_channel_topic(
    channel_id: String,
    topic: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let path = format!("/api/channels/{channel_id}/topic");
    let request = build_authed_request(&state.http_client, Method::PUT, &path, &state)?
        .json(&SetTopicBody { topic: &topic });
    send_empty_request(request).await
}

#[tauri::command]
pub async fn set_channel_purpose(
    channel_id: String,
    purpose: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let path = format!("/api/channels/{channel_id}/purpose");
    let request = build_authed_request(&state.http_client, Method::PUT, &path, &state)?
        .json(&SetPurposeBody { purpose: &purpose });
    send_empty_request(request).await
}

#[tauri::command]
pub async fn archive_channel(channel_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let path = format!("/api/channels/{channel_id}/archive");
    let request = build_authed_request(&state.http_client, Method::POST, &path, &state)?;
    send_empty_request(request).await
}

#[tauri::command]
pub async fn unarchive_channel(
    channel_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let path = format!("/api/channels/{channel_id}/unarchive");
    let request = build_authed_request(&state.http_client, Method::POST, &path, &state)?;
    send_empty_request(request).await
}

#[tauri::command]
pub async fn delete_channel(channel_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let path = format!("/api/channels/{channel_id}");
    let request = build_authed_request(&state.http_client, Method::DELETE, &path, &state)?;
    send_empty_request(request).await
}

#[tauri::command]
pub async fn add_channel_members(
    channel_id: String,
    pubkeys: Vec<String>,
    role: Option<String>,
    state: State<'_, AppState>,
) -> Result<AddMembersResponse, String> {
    let path = format!("/api/channels/{channel_id}/members");
    let request = build_authed_request(&state.http_client, Method::POST, &path, &state)?.json(
        &AddMembersBody {
            pubkeys: &pubkeys,
            role: role.as_deref(),
        },
    );

    send_json_request(request).await
}

#[tauri::command]
pub async fn remove_channel_member(
    channel_id: String,
    pubkey: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let path = format!("/api/channels/{channel_id}/members/{pubkey}");
    let request = build_authed_request(&state.http_client, Method::DELETE, &path, &state)?;
    send_empty_request(request).await
}

#[tauri::command]
pub async fn join_channel(channel_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let path = format!("/api/channels/{channel_id}/join");
    let request = build_authed_request(&state.http_client, Method::POST, &path, &state)?;
    send_empty_request(request).await
}

#[tauri::command]
pub async fn leave_channel(channel_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let path = format!("/api/channels/{channel_id}/leave");
    let request = build_authed_request(&state.http_client, Method::POST, &path, &state)?;
    send_empty_request(request).await
}
