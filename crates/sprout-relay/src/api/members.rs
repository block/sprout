//! Channel membership REST API.
//!
//! Endpoints:
//!   GET    /api/channels/{channel_id}/members          — List members

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use nostr::util::hex as nostr_hex;
use uuid::Uuid;

use crate::state::AppState;

use super::{
    api_error, check_channel_access, check_token_channel_access, extract_auth_context,
    internal_error, scope_error,
};

/// `GET /api/channels/{channel_id}/members` — List members of a channel.
///
/// Requires channel membership or open visibility.
pub async fn list_members(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::ChannelsRead)
        .map_err(scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();

    let channel_id = Uuid::parse_str(&channel_id)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid channel_id"))?;
    check_token_channel_access(&ctx, &channel_id)?;

    check_channel_access(&state, channel_id, &pubkey_bytes).await?;

    let members = state
        .db
        .get_members(channel_id)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    // Resolve display names in bulk.
    let member_pubkeys: Vec<Vec<u8>> = members.iter().map(|m| m.pubkey.clone()).collect();
    let user_records = state
        .db
        .get_users_bulk(&member_pubkeys)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("list_members: failed to load user records: {e}");
            vec![]
        });

    let display_name_map: HashMap<String, String> = user_records
        .into_iter()
        .filter_map(|u| {
            let hex = nostr_hex::encode(&u.pubkey);
            u.display_name.map(|name| (hex, name))
        })
        .collect();

    let result: Vec<serde_json::Value> = members
        .iter()
        .map(|m| {
            let hex = nostr_hex::encode(&m.pubkey);
            let display_name = display_name_map.get(&hex).cloned();
            serde_json::json!({
                "pubkey": hex,
                "role": m.role,
                "joined_at": m.joined_at.to_rfc3339(),
                "display_name": display_name,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "members": result,
        "next_cursor": serde_json::Value::Null,
    })))
}
