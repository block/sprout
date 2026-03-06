//! GET /api/agents — list bot/agent members with presence status.

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

/// Returns all bot/agent members visible to the authenticated user, with presence status.
///
/// Filters channel visibility to only channels the requester can access.
pub async fn agents_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (_pubkey, pubkey_bytes) = extract_auth_pubkey(&headers, &state).await?;

    // Get requester's accessible channels to filter bot channel visibility.
    let accessible_channels = state
        .db
        .get_accessible_channels(&pubkey_bytes)
        .await
        .map_err(|e| {
            tracing::error!("agents: failed to load accessible channels: {e}");
            internal_error("presence lookup failed")
        })?;
    let accessible_names: std::collections::HashSet<String> =
        accessible_channels.iter().map(|c| c.name.clone()).collect();

    let bots = state
        .db
        .get_bot_members()
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    // Collect pubkeys for bulk presence lookup.
    let mut pubkeys_for_presence: Vec<nostr::PublicKey> = Vec::new();
    let mut bot_pubkey_hexes: Vec<String> = Vec::new();

    for bot in &bots {
        let hex = nostr_hex::encode(&bot.pubkey);
        bot_pubkey_hexes.push(hex);
        if let Ok(pk) = nostr::PublicKey::from_slice(&bot.pubkey) {
            pubkeys_for_presence.push(pk);
        }
    }

    // Bulk presence lookup (non-critical — degrade gracefully on failure).
    let presence_map = state
        .pubsub
        .get_presence_bulk(&pubkeys_for_presence)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("agents: presence lookup failed, returning empty map: {e}");
            Default::default()
        });

    // Fetch user records for name resolution.
    let user_records = state
        .db
        .get_users_bulk(&bots.iter().map(|b| b.pubkey.clone()).collect::<Vec<_>>())
        .await
        .map_err(|e| {
            tracing::error!("agents: failed to load user records: {e}");
            internal_error("presence lookup failed")
        })?;

    let user_name_map: HashMap<String, String> = user_records
        .into_iter()
        .filter_map(|u| {
            let hex = nostr_hex::encode(&u.pubkey);
            u.display_name.map(|name| (hex, name))
        })
        .collect();

    let mut result = Vec::with_capacity(bots.len());

    for (bot, hex) in bots.iter().zip(bot_pubkey_hexes.iter()) {
        // Resolve display name: users table → bot record → test mapping → fallback.
        let name = user_name_map
            .get(hex.as_str())
            .cloned()
            .or_else(|| bot.display_name.clone())
            .unwrap_or_else(|| {
                let end = hex.len().min(8);
                format!("agent-{}", &hex[..end])
            });

        // Parse channel names from comma-separated string, filtered to requester's access.
        let channels: Vec<&str> = bot
            .channel_names
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty() && accessible_names.contains(*s))
            .collect();

        // Parse capabilities from JSON value.
        let capabilities: Vec<String> = bot
            .capabilities
            .as_ref()
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        // Presence status.
        let status = presence_map
            .get(hex.as_str())
            .map(|s| s.as_str())
            .unwrap_or("offline")
            .to_string();

        result.push(serde_json::json!({
            "pubkey": hex,
            "name": name,
            "agent_type": bot.agent_type.clone().unwrap_or_default(),
            "channels": channels,
            "capabilities": capabilities,
            "status": status,
        }));
    }

    Ok(Json(serde_json::json!(result)))
}
