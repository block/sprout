//! Reaction REST API.
//!
//! Endpoints:
//!   GET    /api/messages/:event_id/reactions          — list reactions
//!
use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use chrono::{TimeZone, Utc};
use nostr::util::hex as nostr_hex;
use serde::Deserialize;

use crate::state::AppState;

use super::{
    api_error, check_channel_access, check_token_channel_access, extract_auth_context,
    internal_error, not_found,
};

// ── Request / query types ─────────────────────────────────────────────────────

/// Query parameters for listing reactions.
#[derive(Debug, Deserialize)]
pub struct ListReactionsParams {
    /// Opaque pagination cursor (reserved for future use).
    pub cursor: Option<String>,
    /// Maximum number of emoji groups to return. Default: 50. Max: 200.
    pub limit: Option<u32>,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Decode a hex event_id path segment into 32 bytes.
///
/// Returns a 400 error if the string is not valid hex or not exactly 32 bytes.
fn decode_event_id(hex: &str) -> Result<Vec<u8>, (StatusCode, Json<serde_json::Value>)> {
    hex::decode(hex)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid event_id: not valid hex"))
        .and_then(|bytes| {
            if bytes.len() == 32 {
                Ok(bytes)
            } else {
                Err(api_error(
                    StatusCode::BAD_REQUEST,
                    "invalid event_id: must be 32 bytes (64 hex chars)",
                ))
            }
        })
}

// ── GET /api/messages/:event_id/reactions ────────────────────────────────────

/// List all active reactions for a message, grouped by emoji.
///
/// Resolves display names for reacting users where available.
/// Supports optional `cursor` and `limit` query parameters.
pub async fn list_reactions_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(event_id_hex): Path<String>,
    Query(params): Query<ListReactionsParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::MessagesRead)
        .map_err(super::scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();

    let limit = params.limit.unwrap_or(50).min(200);
    let cursor = params.cursor.as_deref();

    let event_id_bytes = decode_event_id(&event_id_hex)?;

    // Look up the event to get its created_at and channel_id.
    let stored = state
        .db
        .get_event_by_id(&event_id_bytes)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?
        .ok_or_else(|| not_found("event not found"))?;

    // Verify channel access if the event belongs to a channel.
    if let Some(channel_id) = stored.channel_id {
        // Token-level channel restriction check (channel_id from event lookup).
        check_token_channel_access(&ctx, &channel_id)?;
        check_channel_access(&state, channel_id, &pubkey_bytes).await?;
    }

    let event_created_at = Utc
        .timestamp_opt(stored.event.created_at.as_u64() as i64, 0)
        .single()
        .unwrap_or_default();

    let groups = state
        .db
        .get_reactions(&event_id_bytes, event_created_at, limit, cursor)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    // Collect all unique pubkeys across all groups for bulk display-name resolution.
    let all_pubkeys: Vec<Vec<u8>> = {
        let mut seen = std::collections::HashSet::new();
        let mut pks = Vec::new();
        for g in &groups {
            for u in &g.users {
                if seen.insert(u.pubkey.clone()) {
                    pks.push(u.pubkey.clone());
                }
            }
        }
        pks
    };

    // Resolve display names via bulk user lookup.
    let display_names: HashMap<String, String> = if all_pubkeys.is_empty() {
        HashMap::new()
    } else {
        state
            .db
            .get_users_bulk(&all_pubkeys)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!("reactions: failed to resolve display names: {e}");
                vec![]
            })
            .into_iter()
            .filter_map(|u| {
                let hex = nostr_hex::encode(&u.pubkey);
                u.display_name.map(|name| (hex, name))
            })
            .collect()
    };

    // Build the response, enriching each user with their display name.
    let reaction_list: Vec<serde_json::Value> = groups
        .into_iter()
        .map(|g| {
            let users: Vec<serde_json::Value> = g
                .users
                .into_iter()
                .map(|u| {
                    let hex = nostr_hex::encode(&u.pubkey);
                    let name = display_names
                        .get(&hex)
                        .cloned()
                        .unwrap_or_else(|| hex[..8.min(hex.len())].to_string());
                    serde_json::json!({
                        "pubkey": hex,
                        "display_name": name,
                    })
                })
                .collect();

            serde_json::json!({
                "emoji": g.emoji,
                "count": g.count,
                "users": users,
            })
        })
        .collect();

    // next_cursor is reserved for future keyset pagination.
    Ok(Json(serde_json::json!({
        "reactions": reaction_list,
        "next_cursor": serde_json::Value::Null,
    })))
}
