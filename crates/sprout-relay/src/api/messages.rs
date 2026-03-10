//! Channel messages and thread REST API.
//!
//! Endpoints:
//!   POST /api/channels/:channel_id/messages          — send a message or reply
//!   GET  /api/channels/:channel_id/messages          — list top-level messages
//!   GET  /api/channels/:channel_id/threads/:event_id — full thread tree
//!
//! NOTE: These handlers call `state.db.*` methods that are wired through
//! `sprout-db/src/lib.rs` by the orchestrator:
//!   - `state.db.insert_thread_metadata(...)` → thread::insert_thread_metadata
//!   - `state.db.get_thread_replies(root_id, depth_limit, limit, cursor)` → thread::get_thread_replies
//!   - `state.db.get_thread_summary(event_id)` → thread::get_thread_summary
//!   - `state.db.get_channel_messages_top_level(channel_id, limit, before)` → thread::get_channel_messages_top_level
//!   - `state.db.get_thread_metadata_by_event(event_id)` → thread::get_thread_metadata_by_event
//!   - `state.db.get_event_by_id(id_bytes)` → event::get_event_by_id  (already exists)
//!   - `state.db.insert_event(event, channel_id)` → event::insert_event  (already exists)

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use chrono::Utc;
use nostr::util::hex as nostr_hex;
use nostr::{EventBuilder, Kind, Tag};
use serde::Deserialize;

use crate::state::AppState;

use super::{
    api_error, check_channel_access, extract_auth_pubkey, forbidden, internal_error, not_found,
};

/// Extract the effective message author from a stored event.
///
/// REST-created messages are signed by the relay keypair and attribute the real
/// sender via a `p` tag. For user-signed events (WebSocket), `event.pubkey` is
/// the author. This helper returns the correct author bytes in both cases.
fn effective_author(event: &nostr::Event, relay_pubkey: &nostr::PublicKey) -> Vec<u8> {
    if event.pubkey == *relay_pubkey {
        // Relay-signed: real author is in the first p tag.
        for tag in event.tags.iter() {
            if tag.kind().to_string() == "p" {
                if let Some(hex) = tag.content() {
                    if let Ok(bytes) = nostr_hex::decode(hex) {
                        if bytes.len() == 32 {
                            return bytes;
                        }
                    }
                }
            }
        }
    }
    // User-signed or no p tag found: pubkey is the author.
    event.pubkey.serialize().to_vec()
}

/// Serialize a slice of reaction summaries to JSON.
fn reactions_to_json(reactions: &[sprout_db::reaction::ReactionSummary]) -> serde_json::Value {
    serde_json::json!(reactions
        .iter()
        .map(|r| serde_json::json!({
            "emoji": r.emoji,
            "count": r.count,
        }))
        .collect::<Vec<_>>())
}

// ── POST /api/channels/:channel_id/messages ───────────────────────────────────

/// Request body for sending a channel message or thread reply.
#[derive(Debug, Deserialize)]
pub struct SendMessageBody {
    /// Message text content.
    pub content: String,
    /// Hex-encoded event ID of the parent message (for replies).
    pub parent_event_id: Option<String>,
    /// When `true`, a reply is also surfaced in the channel feed (broadcast).
    #[serde(default)]
    pub broadcast_to_channel: bool,
    /// Nostr kind for this message. Defaults to `KIND_STREAM_MESSAGE` (40001).
    pub kind: Option<u32>,
}

/// Send a new channel message or reply to an existing thread.
///
/// The event is signed with the relay keypair and attributed to the
/// authenticated user via a `p` tag. This is a REST convenience — clients
/// that want user-signed events should use the WebSocket protocol instead.
pub async fn send_message(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel_id_str): Path<String>,
    Json(body): Json<SendMessageBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (_pubkey, pubkey_bytes) = extract_auth_pubkey(&headers, &state).await?;

    let channel_id = uuid::Uuid::parse_str(&channel_id_str)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid channel UUID"))?;

    check_channel_access(&state, channel_id, &pubkey_bytes).await?;

    let channel = state
        .db
        .get_channel(channel_id)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;
    if channel.archived_at.is_some() {
        return Err(api_error(StatusCode::FORBIDDEN, "channel is archived"));
    }

    if body.content.trim().is_empty() {
        return Err(api_error(StatusCode::BAD_REQUEST, "content is required"));
    }

    // Resolve kind — default to KIND_STREAM_MESSAGE (40001).
    let kind_u32 = body.kind.unwrap_or(sprout_core::kind::KIND_STREAM_MESSAGE);
    let kind = Kind::from(kind_u32 as u16);

    // ── Resolve thread ancestry ───────────────────────────────────────────────

    let (parent_id_bytes, parent_created_at, root_id_bytes, root_created_at, depth) =
        if let Some(ref parent_hex) = body.parent_event_id {
            let pid = nostr_hex::decode(parent_hex)
                .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid parent_event_id hex"))?;

            // Look up the parent's thread metadata to find the root and depth.
            let parent_meta = state
                .db
                .get_thread_metadata_by_event(&pid)
                .await
                .map_err(|e| internal_error(&format!("db error: {e}")))?;

            // Also need the parent event's created_at for the FK join.
            let parent_event = state
                .db
                .get_event_by_id(&pid)
                .await
                .map_err(|e| internal_error(&format!("db error: {e}")))?
                .ok_or_else(|| not_found("parent event not found"))?;

            // Verify the parent event belongs to the same channel.
            // Explicitly reject None — a parent with no channel association must not
            // be used as a thread anchor (F6: silently skipped check allowed cross-channel
            // or non-channel parents through).
            match parent_event.channel_id {
                Some(parent_channel) if parent_channel != channel_id => {
                    return Err(api_error(
                        StatusCode::BAD_REQUEST,
                        "parent event belongs to a different channel",
                    ));
                }
                None => {
                    return Err(api_error(
                        StatusCode::BAD_REQUEST,
                        "parent event has no channel association",
                    ));
                }
                _ => {} // Same channel — OK
            }

            let parent_ts = parent_event.event.created_at;
            let parent_created = chrono::DateTime::from_timestamp(parent_ts.as_u64() as i64, 0)
                .unwrap_or_else(Utc::now);

            let (root_bytes, root_ts, depth) = match parent_meta {
                Some(meta) => {
                    // Parent is already in a thread — root propagates, depth increases.
                    let root = meta.root_event_id.unwrap_or_else(|| pid.clone());
                    // Look up the actual root event to get its real created_at.
                    let root_ts =
                        if let Ok(Some(root_event)) = state.db.get_event_by_id(&root).await {
                            let ts = root_event.event.created_at.as_u64() as i64;
                            chrono::DateTime::from_timestamp(ts, 0).unwrap_or(parent_created)
                        } else {
                            // Fallback: use parent_created as a safe approximation.
                            parent_created
                        };
                    (root, root_ts, meta.depth + 1)
                }
                None => {
                    // Parent has no thread metadata yet — it becomes the root.
                    (pid.clone(), parent_created, 1)
                }
            };

            (
                Some(pid),
                Some(parent_created),
                Some(root_bytes),
                Some(root_ts),
                depth,
            )
        } else {
            (None, None, None, None, 0)
        };

    // ── Build Nostr event ─────────────────────────────────────────────────────

    // Attribute to the authenticated user via a `p` tag.
    let user_pubkey_hex = nostr_hex::encode(&pubkey_bytes);

    let mut tags: Vec<Tag> = vec![
        // Attribution to the actual sender.
        Tag::parse(&["p", &user_pubkey_hex])
            .map_err(|e| internal_error(&format!("tag build error: {e}")))?,
        // Channel tag so Nostr clients can find this event by channel.
        Tag::custom(nostr::TagKind::custom("channel"), [channel_id.to_string()]),
    ];

    // Thread reply tags (NIP-10 style).
    if let (Some(ref root_bytes), Some(ref parent_bytes)) = (&root_id_bytes, &parent_id_bytes) {
        let root_hex = nostr_hex::encode(root_bytes);
        let parent_hex = nostr_hex::encode(parent_bytes);

        if root_hex == parent_hex {
            // Direct reply to root — single `e` tag with "reply" marker.
            tags.push(
                Tag::parse(&["e", &root_hex, "", "reply"])
                    .map_err(|e| internal_error(&format!("tag build error: {e}")))?,
            );
        } else {
            // Nested reply — root tag + reply tag.
            tags.push(
                Tag::parse(&["e", &root_hex, "", "root"])
                    .map_err(|e| internal_error(&format!("tag build error: {e}")))?,
            );
            tags.push(
                Tag::parse(&["e", &parent_hex, "", "reply"])
                    .map_err(|e| internal_error(&format!("tag build error: {e}")))?,
            );
        }
    }

    if body.broadcast_to_channel {
        tags.push(
            Tag::parse(&["broadcast", "1"])
                .map_err(|e| internal_error(&format!("tag build error: {e}")))?,
        );
    }

    let event = EventBuilder::new(kind, &body.content, tags)
        .sign_with_keys(&state.relay_keypair)
        .map_err(|e| internal_error(&format!("event signing error: {e}")))?;

    let event_id_hex = event.id.to_hex();
    let event_id_bytes = event.id.as_bytes().to_vec();
    let event_created_at = {
        let ts = event.created_at.as_u64() as i64;
        chrono::DateTime::from_timestamp(ts, 0).unwrap_or_else(Utc::now)
    };

    // ── Persist event + thread metadata atomically ──────────────────────────

    let thread_meta = Some(sprout_db::event::ThreadMetadataParams {
        event_id: &event_id_bytes,
        event_created_at,
        channel_id,
        parent_event_id: parent_id_bytes.as_deref(),
        parent_event_created_at: parent_created_at,
        root_event_id: root_id_bytes.as_deref(),
        root_event_created_at: root_created_at,
        depth,
        broadcast: body.broadcast_to_channel,
    });

    state
        .db
        .insert_event_with_thread_metadata(&event, Some(channel_id), thread_meta)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    // ── Response ──────────────────────────────────────────────────────────────

    Ok(Json(serde_json::json!({
        "event_id":        event_id_hex,
        "parent_event_id": body.parent_event_id,
        "root_event_id":   root_id_bytes.as_ref().map(nostr_hex::encode),
        "depth":           depth,
        "created_at":      event_created_at.timestamp(),
    })))
}

// ── DELETE /api/messages/:event_id ─────────────────────────────────────────────

/// Soft-delete a message by event ID.
///
/// Authorization: the caller must be the message author, or an owner/admin of
/// the channel the message belongs to. If the deleted event is a thread reply,
/// parent/root reply counts are decremented.
pub async fn delete_message(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(event_id_hex): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (_pubkey, pubkey_bytes) = extract_auth_pubkey(&headers, &state).await?;

    let event_id_bytes = nostr_hex::decode(&event_id_hex)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid event_id hex"))?;

    // Look up the event to check ownership and channel.
    let stored = state
        .db
        .get_event_by_id(&event_id_bytes)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?
        .ok_or_else(|| not_found("event not found"))?;

    let channel_id = stored
        .channel_id
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "event has no channel"))?;

    // Auth: must be the message author OR an owner/admin of the channel.
    // For relay-signed REST messages, the real author is in the p tag.
    let author_bytes = effective_author(&stored.event, &state.relay_keypair.public_key());
    let is_author = author_bytes == pubkey_bytes;

    if !is_author {
        let role = state
            .db
            .get_member_role(channel_id, &pubkey_bytes)
            .await
            .map_err(|e| internal_error(&format!("db error: {e}")))?;
        match role.as_deref() {
            Some("owner") | Some("admin") => {} // authorized
            _ => return Err(forbidden("must be message author or channel owner/admin")),
        }
    }

    // Look up thread metadata before deleting so we can pass parent/root IDs
    // to the transactional delete function. Fail hard on DB errors to avoid
    // deleting the event without decrementing counters.
    let meta = state
        .db
        .get_thread_metadata_by_event(&event_id_bytes)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    let parent_id = meta.as_ref().and_then(|m| m.parent_event_id.clone());
    let root_id = meta.as_ref().and_then(|m| m.root_event_id.clone());

    // Atomically soft-delete the event and decrement thread counters in one transaction.
    let deleted = state
        .db
        .soft_delete_event_and_update_thread(
            &event_id_bytes,
            parent_id.as_deref(),
            root_id.as_deref(),
        )
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    if !deleted {
        return Err(api_error(StatusCode::CONFLICT, "event already deleted"));
    }

    Ok(Json(serde_json::json!({ "ok": true, "deleted": true })))
}

// ── GET /api/channels/:channel_id/messages ────────────────────────────────────

/// Query parameters for listing top-level channel messages.
#[derive(Debug, Deserialize)]
pub struct ListMessagesParams {
    /// Maximum messages to return. Default: 50, max: 200.
    pub limit: Option<u32>,
    /// Pagination cursor — Unix timestamp (seconds). Returns messages created
    /// strictly before this time.
    pub before: Option<i64>,
    /// When `true`, include thread summaries for each message.
    #[serde(default)]
    pub with_threads: bool,
}

/// List top-level messages in a channel (newest first).
///
/// Returns root messages and broadcast replies. Thread replies are excluded
/// unless `with_threads=true`, in which case each message includes a
/// `thread_summary` with reply counts and participant pubkeys.
pub async fn list_messages(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel_id_str): Path<String>,
    Query(params): Query<ListMessagesParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (_pubkey, pubkey_bytes) = extract_auth_pubkey(&headers, &state).await?;

    let channel_id = uuid::Uuid::parse_str(&channel_id_str)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid channel UUID"))?;

    check_channel_access(&state, channel_id, &pubkey_bytes).await?;

    let limit = params.limit.unwrap_or(50).min(200);

    let before_cursor: Option<chrono::DateTime<Utc>> = params
        .before
        .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0));

    let mut messages = state
        .db
        .get_channel_messages_top_level(channel_id, limit, before_cursor)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    // Always enrich with thread summaries for messages that have replies.
    // The `with_threads` param is kept for backward compatibility but summaries
    // are now included by default.
    for msg in &mut messages {
        if let Ok(summary) = state.db.get_thread_summary(&msg.event_id).await {
            msg.thread_summary = summary;
        }
    }

    // Bulk-fetch reaction counts for all messages in this page.
    let event_pairs: Vec<(&[u8], chrono::DateTime<Utc>)> = messages
        .iter()
        .map(|m| (m.event_id.as_slice(), m.created_at))
        .collect();
    let bulk_reactions = state
        .db
        .get_reactions_bulk(&event_pairs)
        .await
        .unwrap_or_default();

    // Index reactions by event_id for O(1) lookup during serialization.
    let reaction_map: std::collections::HashMap<Vec<u8>, &[sprout_db::reaction::ReactionSummary]> =
        bulk_reactions
            .iter()
            .map(|entry| (entry.event_id.clone(), entry.reactions.as_slice()))
            .collect();

    // Determine next_cursor from the oldest message in this page.
    let next_cursor = messages.last().map(|m| m.created_at.timestamp());

    let result: Vec<serde_json::Value> = messages
        .iter()
        .map(|m| {
            let mut obj = serde_json::json!({
                "event_id":   nostr_hex::encode(&m.event_id),
                "pubkey":     nostr_hex::encode(&m.pubkey),
                "content":    m.content,
                "kind":       m.kind,
                "created_at": m.created_at.timestamp(),
                "channel_id": m.channel_id.to_string(),
            });

            if let Some(ref ts) = m.thread_summary {
                obj["thread_summary"] = serde_json::json!({
                    "reply_count":      ts.reply_count,
                    "descendant_count": ts.descendant_count,
                    "last_reply_at":    ts.last_reply_at.map(|t| t.timestamp()),
                    "participants":     ts.participants.iter()
                        .map(nostr_hex::encode)
                        .collect::<Vec<_>>(),
                });
            }

            // Embed reaction counts if any exist for this message.
            if let Some(reactions) = reaction_map.get(&m.event_id) {
                obj["reactions"] = reactions_to_json(reactions);
            }

            obj
        })
        .collect();

    Ok(Json(serde_json::json!({
        "messages":    result,
        "next_cursor": next_cursor,
    })))
}

// ── GET /api/channels/:channel_id/threads/:event_id ──────────────────────────

/// Query parameters for fetching a thread tree.
#[derive(Debug, Deserialize)]
pub struct GetThreadParams {
    /// Maximum reply depth to include. Omit for unlimited.
    pub depth_limit: Option<u32>,
    /// Maximum replies to return. Default: 100, max: 500.
    pub limit: Option<u32>,
    /// Keyset pagination cursor — hex-encoded event_id of the last seen reply.
    pub cursor: Option<String>,
}

/// Fetch the full reply tree for a thread rooted at `event_id`.
///
/// Returns the root event details, all replies (optionally depth-limited),
/// and pagination info.
pub async fn get_thread(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((channel_id_str, event_id_hex)): Path<(String, String)>,
    Query(params): Query<GetThreadParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (_pubkey, pubkey_bytes) = extract_auth_pubkey(&headers, &state).await?;

    let channel_id = uuid::Uuid::parse_str(&channel_id_str)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid channel UUID"))?;

    check_channel_access(&state, channel_id, &pubkey_bytes).await?;

    let root_id_bytes = nostr_hex::decode(&event_id_hex)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid event_id hex"))?;

    // Fetch the root event.
    let root_event = state
        .db
        .get_event_by_id(&root_id_bytes)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?
        .ok_or_else(|| not_found("event not found"))?;

    // Verify the root event belongs to the requested channel.
    if let Some(root_channel) = root_event.channel_id {
        if root_channel != channel_id {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "event belongs to a different channel",
            ));
        }
    }

    // Fetch thread summary for the root.
    let summary = state
        .db
        .get_thread_summary(&root_id_bytes)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    let limit = params.limit.unwrap_or(100).min(500);

    // Decode optional cursor.
    // The cursor is a hex-encoded 8-byte big-endian i64 Unix timestamp (seconds),
    // matching the encoding produced when building next_cursor below (F8).
    let cursor_bytes: Option<Vec<u8>> = match params.cursor {
        Some(ref hex) => {
            let bytes = nostr_hex::decode(hex)
                .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid cursor hex"))?;
            if bytes.len() != 8 {
                return Err(api_error(
                    StatusCode::BAD_REQUEST,
                    "cursor must be 8 bytes (timestamp)",
                ));
            }
            Some(bytes)
        }
        None => None,
    };

    let replies = state
        .db
        .get_thread_replies(
            &root_id_bytes,
            params.depth_limit,
            limit,
            cursor_bytes.as_deref(),
        )
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    // Encode next_cursor as hex of the last reply's created_at timestamp (8-byte big-endian i64).
    // Using created_at (not event_id) because the ORDER BY is on event_created_at and binary
    // event IDs do not correlate with chronological order (F8).
    let next_cursor = replies.last().map(|r| {
        let secs: i64 = r.created_at.timestamp();
        nostr_hex::encode(secs.to_be_bytes())
    });

    let total_replies = summary.as_ref().map(|s| s.descendant_count).unwrap_or(0);

    // Serialize root event.
    let root_created_at = root_event.event.created_at.as_u64() as i64;
    let mut root_obj = serde_json::json!({
        "event_id":   root_event.event.id.to_hex(),
        "pubkey":     root_event.event.pubkey.to_hex(),
        "content":    root_event.event.content,
        "kind":       root_event.event.kind.as_u16(),
        "created_at": root_created_at,
        "channel_id": channel_id.to_string(),
        "thread_summary": summary.as_ref().map(|s| serde_json::json!({
            "reply_count":      s.reply_count,
            "descendant_count": s.descendant_count,
            "last_reply_at":    s.last_reply_at.map(|t| t.timestamp()),
            "participants":     s.participants.iter()
                .map(nostr_hex::encode)
                .collect::<Vec<_>>(),
        })),
    });

    // Bulk-fetch reaction counts for root + all replies.
    let root_created_at_dt =
        chrono::DateTime::from_timestamp(root_created_at, 0).unwrap_or_else(Utc::now);
    let mut thread_event_pairs: Vec<(&[u8], chrono::DateTime<Utc>)> =
        vec![(root_id_bytes.as_slice(), root_created_at_dt)];
    for r in &replies {
        thread_event_pairs.push((r.event_id.as_slice(), r.created_at));
    }
    let thread_bulk_reactions = state
        .db
        .get_reactions_bulk(&thread_event_pairs)
        .await
        .unwrap_or_default();
    let thread_reaction_map: std::collections::HashMap<
        Vec<u8>,
        &[sprout_db::reaction::ReactionSummary],
    > = thread_bulk_reactions
        .iter()
        .map(|entry| (entry.event_id.clone(), entry.reactions.as_slice()))
        .collect();

    // Attach reactions to root event.
    if let Some(reactions) = thread_reaction_map.get(&root_id_bytes) {
        root_obj["reactions"] = reactions_to_json(reactions);
    }

    // Serialize replies.
    let reply_objs: Vec<serde_json::Value> = replies
        .iter()
        .map(|r| {
            let mut obj = serde_json::json!({
                "event_id":        nostr_hex::encode(&r.event_id),
                "parent_event_id": r.parent_event_id.as_ref().map(nostr_hex::encode),
                "root_event_id":   r.root_event_id.as_ref().map(nostr_hex::encode),
                "channel_id":      r.channel_id.to_string(),
                "pubkey":          nostr_hex::encode(&r.pubkey),
                "content":         r.content,
                "kind":            r.kind,
                "depth":           r.depth,
                "created_at":      r.created_at.timestamp(),
                "broadcast":       r.broadcast,
            });

            if let Some(reactions) = thread_reaction_map.get(&r.event_id) {
                obj["reactions"] = reactions_to_json(reactions);
            }

            obj
        })
        .collect();

    Ok(Json(serde_json::json!({
        "root":          root_obj,
        "replies":       reply_objs,
        "total_replies": total_replies,
        "next_cursor":   next_cursor,
    })))
}
