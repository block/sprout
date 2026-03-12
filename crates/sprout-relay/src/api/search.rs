//! GET /api/search — full-text search (Typesense-backed).

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use serde::Deserialize;

use sprout_search::SearchQuery;

use crate::state::AppState;

use super::extract_auth_context;

/// Query parameters for the search endpoint.
#[derive(Debug, Deserialize)]
pub struct SearchParams {
    /// Full-text search query string. Defaults to `"*"` (match all) when absent.
    pub q: Option<String>,
    /// Maximum number of results to return. Defaults to 20, capped at 100.
    pub limit: Option<u32>,
}

/// Full-text search over messages accessible to the authenticated user.
///
/// Scopes results to channels the requester can access. Degrades gracefully
/// if the search backend is unavailable (returns empty results).
pub async fn search_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<SearchParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::MessagesRead)
        .map_err(super::scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();

    let query_str = params.q.unwrap_or_default();
    let per_page = params.limit.unwrap_or(20).min(100);

    let channel_ids = state
        .db
        .get_accessible_channel_ids(&pubkey_bytes)
        .await
        .unwrap_or_default();

    // Build Typesense filter_by: channel_id:=[id1,id2,...]
    let filter_by = if channel_ids.is_empty() {
        // No accessible channels — return empty results immediately.
        return Ok(Json(serde_json::json!({ "hits": [], "found": 0 })));
    } else {
        let ids: Vec<String> = channel_ids.iter().map(|id| id.to_string()).collect();
        Some(format!("channel_id:=[{}]", ids.join(",")))
    };

    let search_query = SearchQuery {
        q: if query_str.is_empty() {
            "*".into()
        } else {
            query_str
        },
        filter_by,
        per_page,
        ..Default::default()
    };

    // Execute search — gracefully degrade on failure.
    let search_result = match state.search.search(&search_query).await {
        Ok(r) => r,
        Err(_) => {
            return Ok(Json(serde_json::json!({ "hits": [], "found": 0 })));
        }
    };

    let all_channels = state.db.list_channels(None).await.unwrap_or_default();
    let channel_name_map: HashMap<String, String> = all_channels
        .into_iter()
        .map(|c| (c.id.to_string(), c.name))
        .collect();

    // Filter out hits with no channel_id (spec requirement: "Exclude hits with channel_id: None").
    // This also prevents a deserialization mismatch — the desktop expects channel_id: String.
    let hits: Vec<serde_json::Value> = search_result
        .hits
        .into_iter()
        .filter(|hit| hit.channel_id.is_some())
        .map(|hit| {
            let channel_name = hit
                .channel_id
                .as_deref()
                .and_then(|id| channel_name_map.get(id))
                .cloned()
                .unwrap_or_default();
            serde_json::json!({
                "event_id": hit.event_id,
                "content": hit.content,
                "kind": hit.kind,
                "pubkey": hit.pubkey,
                "channel_id": hit.channel_id,
                "channel_name": channel_name,
                "created_at": hit.created_at,
                "score": hit.score,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "hits": hits,
        "found": hits.len(),
    })))
}
