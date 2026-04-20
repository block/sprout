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

use super::{constrain_accessible_channels, extract_auth_context, internal_error};

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

    let channels = constrain_accessible_channels(
        state
            .db
            .get_accessible_channels(&pubkey_bytes, params.visibility.as_deref(), params.member)
            .await
            .map_err(|e| internal_error(&format!("db error: {e}")))?,
        ctx.channel_ids.as_deref(),
    );

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
        .get_last_message_at_bulk(&channel_ids)
        .await
        .unwrap_or_default();

    // ── Batch DM participant resolution (2 queries total, not 2×N_DMs) ──
    let dm_channel_ids: Vec<uuid::Uuid> = channels
        .iter()
        .filter(|ac| ac.channel.channel_type == "dm")
        .map(|ac| ac.channel.id)
        .collect();

    // 1. One query: all members for all DM channels.
    let all_dm_members = state
        .db
        .get_members_bulk(&dm_channel_ids)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("channels: failed to bulk-load DM members: {e}");
            vec![]
        });

    // 2. Collect unique pubkeys across all DM members.
    let unique_pubkeys: Vec<Vec<u8>> = {
        let mut seen = std::collections::HashSet::new();
        all_dm_members
            .iter()
            .filter(|m| seen.insert(m.pubkey.clone()))
            .map(|m| m.pubkey.clone())
            .collect()
    };

    // 3. One query: resolve display names for all unique pubkeys.
    let user_records = state
        .db
        .get_users_bulk(&unique_pubkeys)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("channels: failed to bulk-load DM participant profiles: {e}");
            vec![]
        });
    let user_map: HashMap<String, String> = user_records
        .into_iter()
        .filter_map(|u| {
            let hex = nostr_hex::encode(&u.pubkey);
            u.display_name.map(|name| (hex, name))
        })
        .collect();

    // 4. Group members by channel_id for O(1) lookup.
    let mut members_by_channel: HashMap<uuid::Uuid, Vec<&sprout_db::channel::MemberRecord>> =
        HashMap::new();
    for m in &all_dm_members {
        members_by_channel.entry(m.channel_id).or_default().push(m);
    }

    let mut result = Vec::with_capacity(channels.len());

    for ac in &channels {
        let ch = &ac.channel;
        let (participants, participant_pubkeys) = if ch.channel_type == "dm" {
            let members = members_by_channel.get(&ch.id);
            let mut names = Vec::new();
            let mut pk_hexes = Vec::new();
            if let Some(members) = members {
                for m in members {
                    let hex = nostr_hex::encode(&m.pubkey);
                    let name = user_map
                        .get(&hex)
                        .cloned()
                        .unwrap_or_else(|| hex[..8.min(hex.len())].to_string());
                    names.push(name);
                    pk_hexes.push(hex);
                }
            }
            (names, pk_hexes)
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
