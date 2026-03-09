//! GET /api/channels — list accessible channels for the authenticated user.

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Json,
};

use nostr::util::hex as nostr_hex;

use crate::state::AppState;

use super::{extract_auth_pubkey, internal_error};

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

    let mut result = Vec::with_capacity(channels.len());

    for ch in &channels {
        let (participants, participant_pubkeys) = if ch.channel_type == "dm" {
            resolve_dm_participants(&state, ch.id).await
        } else {
            (vec![], vec![])
        };

        result.push(serde_json::json!({
            "id": ch.id.to_string(),
            "name": ch.name,
            "channel_type": ch.channel_type,
            "description": ch.description.clone().unwrap_or_default(),
            "participants": participants,
            "participant_pubkeys": participant_pubkeys,
        }));
    }

    Ok(Json(serde_json::json!(result)))
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

    // Bulk-fetch user records for name resolution.
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
