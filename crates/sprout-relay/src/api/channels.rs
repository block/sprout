//! Channel REST API.
//!
//! Endpoints:
//!   GET  /api/channels — list accessible channels for the authenticated user
//!   POST /api/channels — create a new channel for the authenticated user

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::Json as ExtractJson,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Json,
};
use nostr::util::hex as nostr_hex;
use serde::Deserialize;
use sprout_db::channel::{ChannelRecord, ChannelType, ChannelVisibility};

use crate::state::AppState;

use super::{api_error, extract_auth_pubkey, internal_error};

/// Returns all channels accessible to the authenticated user.
///
/// For DM channels, resolves participant display names and pubkeys.
pub async fn channels_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (_pubkey, pubkey_bytes) = extract_auth_pubkey(&headers, &state).await?;

    let channels = state
        .db
        .get_accessible_channels(&pubkey_bytes)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    // Bulk-fetch member counts and last-message timestamps in two queries
    // instead of 2N queries (one per channel per metric).
    let channel_ids: Vec<uuid::Uuid> = channels.iter().map(|ch| ch.id).collect();
    let member_counts = state
        .db
        .get_member_counts_bulk(&channel_ids)
        .await
        .unwrap_or_default();
    let last_messages = state
        .db
        .get_last_message_at_bulk(&channel_ids)
        .await
        .unwrap_or_default();

    let mut result = Vec::with_capacity(channels.len());

    for ch in &channels {
        let (participants, participant_pubkeys) = if ch.channel_type == "dm" {
            resolve_dm_participants(&state, ch.id).await
        } else {
            (vec![], vec![])
        };

        let member_count = member_counts.get(&ch.id).copied().unwrap_or(0);
        let last_message_at = last_messages.get(&ch.id).copied();

        result.push(channel_record_to_json(
            ch,
            participants,
            participant_pubkeys,
            member_count,
            last_message_at,
        ));
    }

    Ok(Json(serde_json::json!(result)))
}

/// Request body for creating a new channel.
#[derive(Debug, Deserialize)]
pub struct CreateChannelBody {
    /// Human-readable channel name.
    pub name: String,
    /// Requested channel type (`stream` or `forum`).
    pub channel_type: String,
    /// Channel visibility (`open` or `private`).
    pub visibility: String,
    /// Optional channel description.
    pub description: Option<String>,
}

/// Creates a new stream or forum channel for the authenticated user.
pub async fn create_channel(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    ExtractJson(body): ExtractJson<CreateChannelBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let (_pubkey, pubkey_bytes) = extract_auth_pubkey(&headers, &state).await?;

    let name = body.name.trim();
    if name.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "channel name is required",
        ));
    }

    let channel_type = match body.channel_type.as_str() {
        "stream" => ChannelType::Stream,
        "forum" => ChannelType::Forum,
        _ => {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "channel_type must be 'stream' or 'forum'",
            ))
        }
    };

    let visibility = match body.visibility.as_str() {
        "open" => ChannelVisibility::Open,
        "private" => ChannelVisibility::Private,
        _ => {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "visibility must be 'open' or 'private'",
            ))
        }
    };

    let description = body
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let channel = state
        .db
        .create_channel(name, channel_type, visibility, description, &pubkey_bytes)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    Ok((
        StatusCode::CREATED,
        Json(channel_record_to_json(&channel, vec![], vec![], 1, None)),
    ))
}

fn channel_record_to_json(
    channel: &ChannelRecord,
    participants: Vec<String>,
    participant_pubkeys: Vec<String>,
    member_count: i64,
    last_message_at: Option<chrono::DateTime<chrono::Utc>>,
) -> serde_json::Value {
    serde_json::json!({
        "id": channel.id.to_string(),
        "name": &channel.name,
        "channel_type": &channel.channel_type,
        "visibility": &channel.visibility,
        "description": channel.description.clone().unwrap_or_default(),
        "topic": channel.topic,
        "purpose": channel.purpose,
        "created_by": nostr_hex::encode(&channel.created_by),
        "created_at": channel.created_at.to_rfc3339(),
        "updated_at": channel.updated_at.to_rfc3339(),
        "archived_at": channel.archived_at.map(|t| t.to_rfc3339()),
        "member_count": member_count,
        "last_message_at": last_message_at.map(|t| t.to_rfc3339()),
        "participants": participants,
        "participant_pubkeys": participant_pubkeys,
    })
}

/// Fetch DM participants and resolve their display names.
async fn resolve_dm_participants(
    state: &AppState,
    channel_id: uuid::Uuid,
) -> (Vec<String>, Vec<String>) {
    let members = state.db.get_members(channel_id).await.unwrap_or_else(|e| {
        tracing::error!("channels: failed to load members for channel {channel_id}: {e}");
        vec![]
    });

    let member_pubkeys: Vec<Vec<u8>> = members.iter().map(|m| m.pubkey.clone()).collect();

    let user_records = state
        .db
        .get_users_bulk(&member_pubkeys)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("channels: failed to load user records for DM participants: {e}");
            vec![]
        });

    let user_map: HashMap<String, String> = user_records
        .into_iter()
        .filter_map(|u| {
            let hex = nostr_hex::encode(&u.pubkey);
            u.display_name.map(|name| (hex, name))
        })
        .collect();

    let mut names = Vec::new();
    let mut pk_hexes = Vec::new();
    for m in &members {
        let hex = nostr_hex::encode(&m.pubkey);
        let name = user_map
            .get(&hex)
            .cloned()
            .unwrap_or_else(|| hex[..8.min(hex.len())].to_string());
        names.push(name);
        pk_hexes.push(hex);
    }
    (names, pk_hexes)
}
