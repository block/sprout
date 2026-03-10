//! GET /api/feed — personalized home feed.
//!
//! Returns a structured feed with four categories:
//!   - `mentions`       — messages that mention the authenticated user
//!   - `needs_action`   — items requiring the user's attention
//!   - `activity`       — recent channel activity
//!   - `agent_activity` — agent/bot job events

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;

use sprout_core::kind::{self, event_kind_u32};

use crate::state::AppState;

use super::{extract_auth_pubkey, internal_error};

/// Agent activity kind set — used to partition activity into agent vs channel activity.
const AGENT_KINDS: &[u32] = &[
    kind::KIND_JOB_REQUEST,
    kind::KIND_JOB_ACCEPTED,
    kind::KIND_JOB_PROGRESS,
    kind::KIND_JOB_RESULT,
    kind::KIND_JOB_CANCEL,
    kind::KIND_JOB_ERROR,
];

/// Query parameters for the feed endpoint.
#[derive(Debug, Deserialize)]
pub struct FeedParams {
    /// Unix timestamp — only return events after this time. Default: now - 7 days.
    pub since: Option<i64>,
    /// Max items per category. Default: 20. Max: 50.
    pub limit: Option<u32>,
    /// Comma-separated category filter: "mentions,needs_action,activity,agent_activity"
    /// Default: all categories.
    pub types: Option<String>,
}

/// Returns a personalized home feed for the authenticated user.
///
/// Runs mention, needs-action, and activity queries in parallel. Partitions
/// activity into agent vs channel activity by event kind.
pub async fn feed_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<FeedParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (_pubkey, pubkey_bytes) = extract_auth_pubkey(&headers, &state).await?;

    let limit = params.limit.unwrap_or(20).min(50) as i64;
    let since: DateTime<Utc> = params
        .since
        .and_then(|ts| DateTime::from_timestamp(ts, 0))
        .unwrap_or_else(|| Utc::now() - Duration::days(7));

    // Parse optional type filter.
    let type_filter: Option<std::collections::HashSet<&str>> = params
        .types
        .as_deref()
        .map(|t| t.split(',').map(|s| s.trim()).collect());
    let wants = |cat: &str| -> bool { type_filter.as_ref().is_none_or(|f| f.contains(cat)) };

    // 1. Get accessible channel IDs for this user.
    let accessible_ids = state
        .db
        .get_accessible_channel_ids(&pubkey_bytes)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    if accessible_ids.is_empty() {
        let generated_at = Utc::now().timestamp();
        return Ok(Json(serde_json::json!({
            "feed": {
                "mentions": [],
                "needs_action": [],
                "activity": [],
                "agent_activity": [],
            },
            "meta": {
                "since": since.timestamp(),
                "total": 0,
                "generated_at": generated_at,
            }
        })));
    }

    // 2. Run queries in parallel.
    let (mentions_res, needs_action_res, activity_res) = tokio::join!(
        state
            .db
            .query_feed_mentions(&pubkey_bytes, &accessible_ids, Some(since), limit),
        state
            .db
            .query_feed_needs_action(&pubkey_bytes, &accessible_ids, Some(since), limit),
        state
            .db
            .query_feed_activity(&accessible_ids, Some(since), limit),
    );

    // I10: Return 500 for critical feed query failures instead of masking with empty.
    let mentions = mentions_res.map_err(|e| internal_error(&format!("db error: {e}")))?;
    let needs_action = needs_action_res.map_err(|e| internal_error(&format!("db error: {e}")))?;
    let activity_all = activity_res.map_err(|e| internal_error(&format!("db error: {e}")))?;

    // 3. Partition activity into agent activity vs channel activity.
    let (agent_activity, channel_activity): (Vec<_>, Vec<_>) = activity_all
        .into_iter()
        .partition(|e| AGENT_KINDS.contains(&event_kind_u32(&e.event)));

    // 4. Enrich events with channel names (batch lookup).
    let all_channels = state.db.list_channels(None).await.unwrap_or_else(|e| {
        tracing::warn!("feed: failed to load channel names for enrichment: {e}");
        vec![]
    });
    let channel_name_map: HashMap<uuid::Uuid, String> =
        all_channels.into_iter().map(|c| (c.id, c.name)).collect();

    // Helper: convert a StoredEvent to a FeedItem JSON value.
    let to_feed_item = |event: &sprout_core::StoredEvent, category: &str| -> serde_json::Value {
        let channel_name = event
            .channel_id
            .and_then(|id| channel_name_map.get(&id))
            .cloned()
            .unwrap_or_default();

        let tags: Vec<serde_json::Value> = event
            .event
            .tags
            .iter()
            .map(|t| {
                let tag_vec: Vec<String> = t.as_slice().iter().map(|s| s.to_string()).collect();
                serde_json::json!(tag_vec)
            })
            .collect();

        serde_json::json!({
            "id": event.event.id.to_hex(),
            "kind": event_kind_u32(&event.event),
            "pubkey": event.event.pubkey.to_hex(),
            "content": event.event.content,
            "created_at": event.event.created_at.as_u64(),
            "channel_id": event.channel_id.map(|id| id.to_string()),
            "channel_name": channel_name,
            "tags": tags,
            "category": category,
        })
    };

    // 5. Build feed sections (apply type filter).
    let mentions_items: Vec<serde_json::Value> = if wants("mentions") {
        mentions
            .iter()
            .map(|e| to_feed_item(e, "mention"))
            .collect()
    } else {
        vec![]
    };

    let needs_action_items: Vec<serde_json::Value> = if wants("needs_action") {
        needs_action
            .iter()
            .map(|e| to_feed_item(e, "needs_action"))
            .collect()
    } else {
        vec![]
    };

    let activity_items: Vec<serde_json::Value> = if wants("activity") {
        channel_activity
            .iter()
            .map(|e| to_feed_item(e, "activity"))
            .collect()
    } else {
        vec![]
    };

    let agent_activity_items: Vec<serde_json::Value> = if wants("agent_activity") {
        agent_activity
            .iter()
            .map(|e| to_feed_item(e, "agent_activity"))
            .collect()
    } else {
        vec![]
    };

    let total = mentions_items.len()
        + needs_action_items.len()
        + activity_items.len()
        + agent_activity_items.len();

    let generated_at = Utc::now().timestamp();

    Ok(Json(serde_json::json!({
        "feed": {
            "mentions": mentions_items,
            "needs_action": needs_action_items,
            "activity": activity_items,
            "agent_activity": agent_activity_items,
        },
        "meta": {
            "since": since.timestamp(),
            "total": total,
            "generated_at": generated_at,
        }
    })))
}
