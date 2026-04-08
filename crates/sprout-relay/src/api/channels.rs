//! Channel REST API.
//!
//! Endpoints:
//!   GET  /api/channels — list accessible channels for the authenticated user

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use nostr::util::hex as nostr_hex;
use serde::Deserialize;
use sprout_db::channel::ChannelRecord;

use crate::state::AppState;

use super::{extract_auth_context, internal_error};

/// Query parameters for `GET /api/channels`.
#[derive(Debug, Deserialize)]
pub struct ListChannelsParams {
    /// Optional visibility filter: `"open"` or `"private"`.
    pub visibility: Option<String>,
    /// When `true`, return only channels the user is a member of.
    pub member: Option<bool>,
}

/// Returns all channels accessible to the authenticated user.
///
/// For DM channels, resolves participant display names and pubkeys.
pub async fn channels_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<ListChannelsParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::ChannelsRead)
        .map_err(super::scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();

    let channels = state
        .db
        .get_accessible_channels(&pubkey_bytes, params.visibility.as_deref(), params.member)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    // Bulk-fetch member counts and last-message timestamps in two queries
    // instead of 2N queries (one per channel per metric).
    let channel_ids: Vec<uuid::Uuid> = channels.iter().map(|ac| ac.channel.id).collect();
    let member_counts = state
        .db
        .get_member_counts_bulk(&channel_ids)
        .await
        .unwrap_or_default();
    let last_messages = state
        .db
        .get_last_channel_activity_at_bulk(&channel_ids)
        .await
        .unwrap_or_default();

    let mut result = Vec::with_capacity(channels.len());

    for ac in &channels {
        let ch = &ac.channel;
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
            ac.is_member,
        ));
    }

    Ok(Json(serde_json::json!(result)))
}

fn channel_record_to_json(
    channel: &ChannelRecord,
    participants: Vec<String>,
    participant_pubkeys: Vec<String>,
    member_count: i64,
    last_message_at: Option<chrono::DateTime<chrono::Utc>>,
    is_member: bool,
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
        "is_member": is_member,
        "ttl_seconds": channel.ttl_seconds,
        "ttl_deadline": channel.ttl_deadline.map(|t| t.to_rfc3339()),
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
