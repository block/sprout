//! Event lookup endpoints.
//!
//! Endpoints:
//!   GET /api/events/:id — fetch a single stored event by ID

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};

use crate::state::AppState;

use super::{
    api_error, check_channel_access, check_token_channel_access, extract_auth_context,
    internal_error, not_found,
};

/// Fetch a single stored event by its 64-char hex ID.
pub async fn get_event(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(event_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::MessagesRead)
        .map_err(super::scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();

    let id_bytes = hex::decode(&event_id)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid event ID"))?;
    if id_bytes.len() != 32 {
        return Err(api_error(StatusCode::BAD_REQUEST, "invalid event ID"));
    }

    let stored_event = state
        .db
        .get_event_by_id(&id_bytes)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?
        .ok_or_else(|| not_found("event not found"))?;

    if let Some(channel_id) = stored_event.channel_id {
        // Token-level channel restriction check (in addition to membership check).
        // channel_id is obtained from the event's stored metadata — no extra lookup needed.
        check_token_channel_access(&ctx, &channel_id)?;
        check_channel_access(&state, channel_id, &pubkey_bytes).await?;
    } else {
        return Err(not_found("event not found"));
    }

    let tags = serde_json::to_value(&stored_event.event.tags)
        .map_err(|e| internal_error(&format!("tag serialization error: {e}")))?;

    Ok(Json(serde_json::json!({
        "id": stored_event.event.id.to_hex(),
        "pubkey": stored_event.event.pubkey.to_hex(),
        "created_at": stored_event.event.created_at.as_u64(),
        "kind": stored_event.event.kind.as_u16(),
        "tags": tags,
        "content": stored_event.event.content,
        "sig": stored_event.event.sig.to_string(),
    })))
}
