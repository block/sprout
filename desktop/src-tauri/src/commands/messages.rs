use nostr::EventId;
use reqwest::Method;
use tauri::State;

use crate::{
    app_state::AppState,
    events,
    models::{
        FeedResponse, ForumPostsResponse, ForumThreadResponse, GetFeedQuery, GetForumPostsQuery,
        GetForumThreadQuery, SearchQueryParams, SearchResponse, SendChannelMessageResponse,
    },
    relay::{build_authed_request, relay_error_message, send_json_request, submit_event},
    util::percent_encode,
};

// ── Reads (unchanged) ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_feed(
    since: Option<i64>,
    limit: Option<u32>,
    types: Option<String>,
    state: State<'_, AppState>,
) -> Result<FeedResponse, String> {
    let request = build_authed_request(&state.http_client, Method::GET, "/api/feed", &state)?
        .query(&GetFeedQuery {
            since,
            limit,
            types: types.as_deref(),
        });

    send_json_request(request).await
}

#[tauri::command]
pub async fn search_messages(
    q: String,
    limit: Option<u32>,
    state: State<'_, AppState>,
) -> Result<SearchResponse, String> {
    let request = build_authed_request(&state.http_client, Method::GET, "/api/search", &state)?
        .query(&SearchQueryParams { q: q.trim(), limit });

    send_json_request(request).await
}

#[tauri::command]
pub async fn get_forum_posts(
    channel_id: String,
    limit: Option<u32>,
    before: Option<i64>,
    state: State<'_, AppState>,
) -> Result<ForumPostsResponse, String> {
    let path = format!("/api/channels/{channel_id}/messages");
    let request = build_authed_request(&state.http_client, Method::GET, &path, &state)?.query(
        &GetForumPostsQuery {
            limit,
            before,
            with_threads: true,
        },
    );

    send_json_request(request).await
}

#[tauri::command]
pub async fn get_forum_thread(
    channel_id: String,
    event_id: String,
    limit: Option<u32>,
    cursor: Option<String>,
    state: State<'_, AppState>,
) -> Result<ForumThreadResponse, String> {
    let path = format!("/api/channels/{channel_id}/threads/{event_id}");
    let request = build_authed_request(&state.http_client, Method::GET, &path, &state)?
        .query(&GetForumThreadQuery { limit, cursor });

    send_json_request(request).await
}

#[tauri::command]
pub async fn get_event(event_id: String, state: State<'_, AppState>) -> Result<String, String> {
    let request = build_authed_request(
        &state.http_client,
        Method::GET,
        &format!("/api/events/{event_id}"),
        &state,
    )?;
    let response = request
        .send()
        .await
        .map_err(|error| format!("request failed: {error}"))?;

    if !response.status().is_success() {
        return Err(relay_error_message(response).await);
    }

    response
        .text()
        .await
        .map_err(|error| format!("parse failed: {error}"))
}

// ── Writes (migrated to signed events via POST /api/events) ──────────────────

/// Fetch a parent event and extract the thread root from its NIP-10 e-tags.
/// Same logic as MCP's `resolve_thread_ref`.
async fn resolve_thread_ref(
    parent_event_id: &str,
    state: &AppState,
) -> Result<events::ThreadRef, String> {
    let parent_eid =
        EventId::from_hex(parent_event_id).map_err(|e| format!("invalid parent event ID: {e}"))?;

    let path = format!("/api/events/{parent_event_id}");
    let request = build_authed_request(&state.http_client, Method::GET, &path, state)?;
    let response = request
        .send()
        .await
        .map_err(|e| format!("failed to fetch parent event: {e}"))?;

    if !response.status().is_success() {
        return Err(relay_error_message(response).await);
    }

    let event_json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("failed to parse parent event: {e}"))?;

    // Walk tags looking for NIP-10 root/reply markers — same as MCP's find_root_from_tags.
    let root_hex = event_json
        .get("tags")
        .and_then(|t| t.as_array())
        .and_then(|tags| {
            let mut root = None;
            let mut reply = None;
            for tag in tags {
                let parts = tag.as_array()?;
                if parts.len() >= 4 && parts[0].as_str() == Some("e") {
                    match parts[3].as_str() {
                        Some("root") => root = parts[1].as_str().map(|s| s.to_string()),
                        Some("reply") => reply = parts[1].as_str().map(|s| s.to_string()),
                        _ => {}
                    }
                }
            }
            root.or(reply)
        });

    let root_eid = match root_hex {
        Some(hex) if hex != parent_event_id => {
            EventId::from_hex(&hex).map_err(|e| format!("invalid root event ID: {e}"))?
        }
        _ => parent_eid,
    };

    Ok(events::ThreadRef {
        root_event_id: root_eid,
        parent_event_id: parent_eid,
    })
}

#[tauri::command]
pub async fn send_channel_message(
    channel_id: String,
    content: String,
    parent_event_id: Option<String>,
    media_tags: Option<Vec<Vec<String>>>,
    mention_pubkeys: Option<Vec<String>>,
    kind: Option<u32>,
    state: State<'_, AppState>,
) -> Result<SendChannelMessageResponse, String> {
    let channel_uuid = uuid::Uuid::parse_str(&channel_id)
        .map_err(|_| format!("invalid channel UUID: {channel_id}"))?;
    let mentions = mention_pubkeys.unwrap_or_default();
    let mention_refs: Vec<&str> = mentions.iter().map(|s| s.as_str()).collect();
    let media = media_tags.unwrap_or_default();
    let kind_num = kind.unwrap_or(sprout_core::kind::KIND_STREAM_MESSAGE);

    // Track the resolved thread ref so we can return accurate metadata.
    let mut resolved_root: Option<String> = None;

    let builder = match kind_num {
        sprout_core::kind::KIND_FORUM_POST => {
            events::build_forum_post(channel_uuid, content.trim(), &mention_refs, &media)?
        }
        sprout_core::kind::KIND_FORUM_COMMENT => {
            let parent_id = parent_event_id
                .as_deref()
                .ok_or("forum comment requires parent_event_id")?;
            let thread_ref = resolve_thread_ref(parent_id, &state).await?;
            resolved_root = Some(thread_ref.root_event_id.to_hex());
            events::build_forum_comment(
                channel_uuid,
                content.trim(),
                &thread_ref,
                &mention_refs,
                &media,
            )?
        }
        _ => {
            // Stream message (kind 9) — with optional thread ref for replies.
            let thread_ref = match parent_event_id.as_deref() {
                Some(pid) => {
                    let tr = resolve_thread_ref(pid, &state).await?;
                    resolved_root = Some(tr.root_event_id.to_hex());
                    Some(tr)
                }
                None => None,
            };
            events::build_message(
                channel_uuid,
                content.trim(),
                thread_ref.as_ref(),
                &mention_refs,
                &media,
            )?
        }
    };

    let result = submit_event(builder, &state).await?;

    // Derive depth: 0 = top-level, 1 = direct reply, 2+ = nested.
    let depth = match (&parent_event_id, &resolved_root) {
        (None, _) => 0,
        (Some(pid), Some(root)) if pid == root => 1,
        (Some(_), Some(_)) => 2,
        (Some(_), None) => 1,
    };

    Ok(SendChannelMessageResponse {
        event_id: result.event_id,
        root_event_id: resolved_root,
        parent_event_id,
        depth,
        created_at: chrono::Utc::now().timestamp(),
    })
}

#[tauri::command]
pub async fn add_reaction(
    event_id: String,
    emoji: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let target_eid = EventId::from_hex(&event_id).map_err(|e| format!("invalid event ID: {e}"))?;
    let builder = events::build_reaction(target_eid, emoji.trim())?;
    submit_event(builder, &state).await?;
    Ok(())
}

#[tauri::command]
pub async fn remove_reaction(
    event_id: String,
    emoji: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Fetch reactions to find our reaction event ID — same pattern as MCP.
    let encoded_event_id = percent_encode(event_id.trim());
    let path = format!("/api/messages/{encoded_event_id}/reactions");
    let request = build_authed_request(&state.http_client, Method::GET, &path, &state)?;
    let reactions: serde_json::Value = send_json_request(request).await?;

    let my_pubkey = state
        .keys
        .lock()
        .map_err(|e| e.to_string())?
        .public_key()
        .to_hex();

    let reaction_event_id_hex = reactions
        .get("reactions")
        .and_then(|r| r.as_array())
        .and_then(|groups| {
            groups.iter().find_map(|group| {
                if group.get("emoji")?.as_str()? != emoji.trim() {
                    return None;
                }
                group.get("users")?.as_array()?.iter().find_map(|user| {
                    if user.get("pubkey")?.as_str()? != my_pubkey {
                        return None;
                    }
                    user.get("reaction_event_id")?.as_str().map(String::from)
                })
            })
        })
        .ok_or("could not find your reaction event for this emoji")?;

    let reaction_eid = EventId::from_hex(&reaction_event_id_hex)
        .map_err(|e| format!("invalid reaction event ID: {e}"))?;
    let builder = events::build_remove_reaction(reaction_eid)?;
    submit_event(builder, &state).await?;
    Ok(())
}

#[tauri::command]
pub async fn edit_message(
    channel_id: String,
    event_id: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let channel_uuid = uuid::Uuid::parse_str(&channel_id)
        .map_err(|_| format!("invalid channel UUID: {channel_id}"))?;
    let target_eid = EventId::from_hex(&event_id).map_err(|e| format!("invalid event ID: {e}"))?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err("edit content must not be empty".into());
    }
    let builder = events::build_message_edit(channel_uuid, target_eid, trimmed)?;
    submit_event(builder, &state).await?;
    Ok(())
}

#[tauri::command]
pub async fn delete_message(event_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let target_eid = EventId::from_hex(&event_id).map_err(|e| format!("invalid event ID: {e}"))?;
    let builder = events::build_delete_compat(target_eid)?;
    submit_event(builder, &state).await?;
    Ok(())
}
