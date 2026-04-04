//! Transport-neutral event ingestion pipeline.
//!
//! Both WebSocket `["EVENT", ...]` and `POST /api/events` feed into
//! [`ingest_event`] — two doors, one room.

use std::sync::Arc;

use chrono::Utc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use nostr::Event;
use sprout_auth::Scope;
use sprout_core::kind::{
    event_kind_u32, KIND_AUTH, KIND_CANVAS, KIND_DELETION, KIND_FORUM_COMMENT, KIND_FORUM_POST,
    KIND_FORUM_VOTE, KIND_GIFT_WRAP, KIND_MEMBER_ADDED_NOTIFICATION,
    KIND_MEMBER_REMOVED_NOTIFICATION, KIND_NIP29_CREATE_GROUP, KIND_NIP29_DELETE_EVENT,
    KIND_NIP29_DELETE_GROUP, KIND_NIP29_EDIT_METADATA, KIND_NIP29_JOIN_REQUEST,
    KIND_NIP29_LEAVE_REQUEST, KIND_NIP29_PUT_USER, KIND_NIP29_REMOVE_USER, KIND_PRESENCE_UPDATE,
    KIND_PROFILE, KIND_REACTION, KIND_STREAM_MESSAGE, KIND_STREAM_MESSAGE_BOOKMARKED,
    KIND_STREAM_MESSAGE_DIFF, KIND_STREAM_MESSAGE_EDIT, KIND_STREAM_MESSAGE_PINNED,
    KIND_STREAM_MESSAGE_SCHEDULED, KIND_STREAM_MESSAGE_V2, KIND_STREAM_REMINDER,
};
use sprout_core::verification::verify_event;

use crate::state::AppState;

use super::event::dispatch_persistent_event;

// ── Public types ─────────────────────────────────────────────────────────────

/// How the HTTP caller authenticated (for [`IngestAuth::Http`]).
#[derive(Debug, Clone)]
pub enum HttpAuthMethod {
    /// `Authorization: Bearer sprout_*` API token.
    ApiToken,
    /// `Authorization: Bearer eyJ*` Okta JWT.
    OktaJwt,
    /// `X-Pubkey: <hex>` dev-mode header.
    DevPubkey,
}

/// Authentication context for event ingestion — transport-neutral.
#[derive(Debug, Clone)]
pub enum IngestAuth {
    /// WebSocket NIP-42 authenticated connection.
    Nip42 {
        /// The authenticated Nostr public key.
        pubkey: nostr::PublicKey,
        /// Permission scopes granted to this connection.
        scopes: Vec<Scope>,
        /// WebSocket connection identifier.
        conn_id: Uuid,
    },
    /// HTTP REST authenticated request.
    Http {
        /// The authenticated Nostr public key.
        pubkey: nostr::PublicKey,
        /// Permission scopes granted to this request.
        scopes: Vec<Scope>,
        /// How the HTTP request was authenticated.
        auth_method: HttpAuthMethod,
        /// API token UUID, if auth_method is `ApiToken`.
        token_id: Option<Uuid>,
        /// Token-level channel restriction, if any.
        channel_ids: Option<Vec<Uuid>>,
    },
}

impl IngestAuth {
    /// The authenticated public key.
    pub fn pubkey(&self) -> &nostr::PublicKey {
        match self {
            Self::Nip42 { pubkey, .. } | Self::Http { pubkey, .. } => pubkey,
        }
    }

    /// Permission scopes for this auth context.
    pub fn scopes(&self) -> &[Scope] {
        match self {
            Self::Nip42 { scopes, .. } | Self::Http { scopes, .. } => scopes,
        }
    }

    /// Whether this auth context includes the `ProxySubmit` scope.
    pub fn has_proxy_scope(&self) -> bool {
        self.scopes().contains(&Scope::ProxySubmit)
    }

    /// WebSocket connection ID (Nip42 only).
    pub fn conn_id(&self) -> Option<Uuid> {
        match self {
            Self::Nip42 { conn_id, .. } => Some(*conn_id),
            Self::Http { .. } => None,
        }
    }

    /// Token-level channel restriction (Http/ApiToken only).
    pub fn channel_ids(&self) -> Option<&[Uuid]> {
        match self {
            Self::Http {
                channel_ids: Some(ids),
                ..
            } => Some(ids),
            _ => None,
        }
    }

    /// Whether this auth context is an HTTP request (not WebSocket).
    pub fn is_http(&self) -> bool {
        matches!(self, Self::Http { .. })
    }
}

/// Successful ingestion result.
pub struct IngestResult {
    /// Hex-encoded event ID.
    pub event_id: String,
    /// Whether the event was accepted.
    pub accepted: bool,
    /// Optional message (e.g. "duplicate:" for dedup).
    pub message: String,
}

/// Ingestion error — the caller maps this to their transport's error format.
#[derive(Debug)]
pub enum IngestError {
    /// Client error (bad event) — WS: OK false, HTTP: 400.
    Rejected(String),
    /// Auth/scope error — WS: OK false, HTTP: 401/403.
    AuthFailed(String),
    /// Server error — WS: OK false, HTTP: 500.
    Internal(String),
}

// ── Per-kind scope allowlist ─────────────────────────────────────────────────

/// Determine the required scope for a given event kind.
///
/// Returns `Err` for unknown kinds — the relay rejects them.
fn required_scope_for_kind(kind: u32, event: &Event) -> Result<Scope, &'static str> {
    match kind {
        KIND_PROFILE => Ok(Scope::UsersWrite),
        KIND_DELETION
        | KIND_REACTION
        | KIND_GIFT_WRAP
        | KIND_STREAM_MESSAGE
        | KIND_STREAM_MESSAGE_V2
        | KIND_NIP29_DELETE_EVENT
        | KIND_STREAM_MESSAGE_EDIT
        | KIND_STREAM_MESSAGE_PINNED
        | KIND_STREAM_MESSAGE_BOOKMARKED
        | KIND_STREAM_MESSAGE_SCHEDULED
        | KIND_STREAM_REMINDER
        | KIND_STREAM_MESSAGE_DIFF
        | KIND_FORUM_POST
        | KIND_FORUM_VOTE
        | KIND_FORUM_COMMENT => Ok(Scope::MessagesWrite),
        KIND_NIP29_PUT_USER | KIND_NIP29_REMOVE_USER | KIND_NIP29_DELETE_GROUP => {
            Ok(Scope::AdminChannels)
        }
        KIND_NIP29_EDIT_METADATA => {
            // kind:9002 scope split: archived tag → AdminChannels, else ChannelsWrite
            let has_archived = event
                .tags
                .iter()
                .any(|t| t.kind().to_string() == "archived");
            if has_archived {
                Ok(Scope::AdminChannels)
            } else {
                Ok(Scope::ChannelsWrite)
            }
        }
        KIND_NIP29_CREATE_GROUP | KIND_CANVAS => Ok(Scope::ChannelsWrite),
        KIND_NIP29_JOIN_REQUEST | KIND_NIP29_LEAVE_REQUEST => Ok(Scope::ChannelsRead),
        _ => Err("restricted: unknown event kind"),
    }
}

// ── Channel resolution helpers ───────────────────────────────────────────────

/// Extract a channel UUID from the `"h"` NIP-29 group tag.
pub(crate) fn extract_channel_id(event: &Event) -> Option<Uuid> {
    for tag in event.tags.iter() {
        if tag.kind().to_string() == "h" {
            if let Some(val) = tag.content() {
                if let Ok(id) = val.parse::<Uuid>() {
                    return Some(id);
                }
            }
        }
    }
    None
}

/// Result of resolving a reaction's target channel.
pub(crate) enum ReactionChannelResult {
    Channel(Uuid),
    NoChannel,
    NotFound,
    NoTarget,
    DbError(String),
}

/// Derive channel_id from the target event for NIP-25 reactions.
pub(crate) async fn derive_reaction_channel(
    db: &sprout_db::Db,
    event: &Event,
) -> ReactionChannelResult {
    let target_hex = match event.tags.iter().rev().find_map(|tag| {
        if tag.kind().to_string() == "e" {
            tag.content().and_then(|v| {
                if v.len() == 64 && v.chars().all(|c| c.is_ascii_hexdigit()) {
                    Some(v.to_string())
                } else {
                    None
                }
            })
        } else {
            None
        }
    }) {
        Some(h) => h,
        None => return ReactionChannelResult::NoTarget,
    };

    let id_bytes = match hex::decode(&target_hex) {
        Ok(b) if b.len() == 32 => b,
        _ => return ReactionChannelResult::NoTarget,
    };

    match db.get_event_by_id(&id_bytes).await {
        Ok(Some(target)) => match target.channel_id {
            Some(ch_id) => ReactionChannelResult::Channel(ch_id),
            None => ReactionChannelResult::NoChannel,
        },
        Ok(None) => ReactionChannelResult::NotFound,
        Err(e) => ReactionChannelResult::DbError(e.to_string()),
    }
}

/// Kinds that require an `h` tag for channel scoping.
pub(crate) fn requires_h_channel_scope(kind: u32) -> bool {
    matches!(
        kind,
        KIND_STREAM_MESSAGE
            | KIND_STREAM_MESSAGE_V2
            | KIND_STREAM_MESSAGE_EDIT
            | KIND_STREAM_MESSAGE_PINNED
            | KIND_STREAM_MESSAGE_BOOKMARKED
            | KIND_STREAM_MESSAGE_SCHEDULED
            | KIND_STREAM_REMINDER
            | KIND_STREAM_MESSAGE_DIFF
            | KIND_CANVAS
            | KIND_FORUM_POST
            | KIND_FORUM_VOTE
            | KIND_FORUM_COMMENT
            // NIP-29 admin kinds (except CREATE_GROUP which creates the channel)
            | KIND_NIP29_PUT_USER
            | KIND_NIP29_REMOVE_USER
            | KIND_NIP29_EDIT_METADATA
            | KIND_NIP29_DELETE_EVENT
            | KIND_NIP29_DELETE_GROUP
            | KIND_NIP29_LEAVE_REQUEST
    )
}

/// Check channel membership: member OR open-visibility channel.
///
/// Returns `Ok(())` if allowed, `Err(reason)` if denied.
pub(crate) async fn check_channel_membership(
    state: &AppState,
    ch_id: Uuid,
    pubkey_bytes: &[u8],
) -> Result<(), String> {
    match state.db.is_member(ch_id, pubkey_bytes).await {
        Ok(true) => return Ok(()),
        Ok(false) => {}
        Err(e) => return Err(format!("error: database error: {e}")),
    }
    // Not a member — check if channel is open.
    let is_open = state
        .db
        .get_channel(ch_id)
        .await
        .map(|ch| ch.visibility == "open")
        .unwrap_or(false);
    if is_open {
        Ok(())
    } else {
        Err("restricted: not a channel member".to_string())
    }
}

// ── Token channel access ─────────────────────────────────────────────────────

fn check_token_channel_access(auth: &IngestAuth, channel_id: Uuid) -> Result<(), String> {
    if let Some(allowed) = auth.channel_ids() {
        if !allowed.contains(&channel_id) {
            return Err("restricted: token does not have access to this channel".to_string());
        }
    }
    Ok(())
}

// ── NIP-10 thread resolution ─────────────────────────────────────────────────

/// Owned thread metadata for the DB insert.
pub(crate) struct ThreadMetadataOwned {
    pub event_id: Vec<u8>,
    pub event_created_at: chrono::DateTime<Utc>,
    pub channel_id: Uuid,
    pub parent_event_id: Vec<u8>,
    pub parent_event_created_at: chrono::DateTime<Utc>,
    pub root_event_id: Vec<u8>,
    pub root_event_created_at: chrono::DateTime<Utc>,
    pub depth: i32,
    pub broadcast: bool,
}

impl ThreadMetadataOwned {
    pub fn as_params(&self) -> sprout_db::event::ThreadMetadataParams<'_> {
        sprout_db::event::ThreadMetadataParams {
            event_id: &self.event_id,
            event_created_at: self.event_created_at,
            channel_id: self.channel_id,
            parent_event_id: Some(&self.parent_event_id),
            parent_event_created_at: Some(self.parent_event_created_at),
            root_event_id: Some(&self.root_event_id),
            root_event_created_at: Some(self.root_event_created_at),
            depth: self.depth,
            broadcast: self.broadcast,
        }
    }
}

/// Resolve NIP-10 thread ancestry from e-tags.
pub(crate) async fn resolve_nip10_thread_meta(
    event: &Event,
    channel_id: Uuid,
    state: &AppState,
) -> Result<Option<ThreadMetadataOwned>, String> {
    let mut root_hex: Option<String> = None;
    let mut reply_hex: Option<String> = None;

    for tag in event.tags.iter() {
        let parts = tag.as_slice();
        if parts.len() >= 4 && parts[0] == "e" {
            let hex_val = &parts[1];
            let marker = &parts[3];
            if hex_val.len() == 64 && hex_val.chars().all(|c| c.is_ascii_hexdigit()) {
                match marker.as_str() {
                    "root" => root_hex = Some(hex_val.to_string()),
                    "reply" => reply_hex = Some(hex_val.to_string()),
                    _ => {}
                }
            }
        }
    }

    if root_hex.is_none() && reply_hex.is_none() {
        return Ok(None);
    }

    let (root_hex, parent_hex) = match (root_hex, reply_hex) {
        (Some(r), Some(p)) => (r, p),
        (None, Some(p)) => (p.clone(), p),
        (Some(_), None) | (None, None) => return Ok(None),
    };

    let parent_bytes =
        hex::decode(&parent_hex).map_err(|_| "invalid parent event ID hex".to_string())?;

    let (parent_event_result, parent_meta_result) = tokio::join!(
        state.db.get_event_by_id(&parent_bytes),
        state.db.get_thread_metadata_by_event(&parent_bytes),
    );

    let parent_event = parent_event_result
        .map_err(|e| format!("db error looking up parent: {e}"))?
        .ok_or_else(|| "reply parent not found".to_string())?;

    match parent_event.channel_id {
        Some(parent_ch) if parent_ch != channel_id => {
            return Err("parent event belongs to a different channel".to_string());
        }
        None => return Err("parent event has no channel association".to_string()),
        _ => {}
    }

    let parent_created =
        chrono::DateTime::from_timestamp(parent_event.event.created_at.as_u64() as i64, 0)
            .unwrap_or_else(Utc::now);

    let client_root_bytes =
        hex::decode(&root_hex).map_err(|_| "invalid root event ID hex".to_string())?;

    let parent_meta =
        parent_meta_result.map_err(|e| format!("db error looking up thread metadata: {e}"))?;

    let (final_root_bytes, root_created, depth) = match parent_meta {
        Some(meta) => {
            let effective_root = meta.root_event_id.unwrap_or_else(|| parent_bytes.clone());
            if client_root_bytes != effective_root {
                return Err("root tag does not match thread ancestry".to_string());
            }
            let root_ts = if let Ok(Some(root_ev)) = state.db.get_event_by_id(&effective_root).await
            {
                chrono::DateTime::from_timestamp(root_ev.event.created_at.as_u64() as i64, 0)
                    .unwrap_or(parent_created)
            } else {
                parent_created
            };
            let depth = meta.depth + 1;
            if depth > 100 {
                return Err("thread depth limit exceeded".to_string());
            }
            (effective_root, root_ts, depth)
        }
        None => {
            let parent_root = parent_event
                .event
                .tags
                .iter()
                .find_map(|t| {
                    let parts = t.as_slice();
                    if parts.len() >= 4 && parts[0] == "e" && parts[3] == "root" {
                        hex::decode(&parts[1]).ok().filter(|b| b.len() == 32)
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    parent_event.event.tags.iter().find_map(|t| {
                        let parts = t.as_slice();
                        if parts.len() >= 4 && parts[0] == "e" && parts[3] == "reply" {
                            hex::decode(&parts[1]).ok().filter(|b| b.len() == 32)
                        } else {
                            None
                        }
                    })
                })
                .unwrap_or_else(|| parent_bytes.clone());

            if client_root_bytes != parent_root {
                return Err("root tag does not match thread ancestry".to_string());
            }
            let depth = if parent_root == parent_bytes { 1 } else { 2 };
            let root_created = if parent_root != parent_bytes {
                if let Ok(Some(root_ev)) = state.db.get_event_by_id(&parent_root).await {
                    chrono::DateTime::from_timestamp(root_ev.event.created_at.as_u64() as i64, 0)
                        .unwrap_or(parent_created)
                } else {
                    parent_created
                }
            } else {
                parent_created
            };
            (parent_root, root_created, depth)
        }
    };

    let broadcast = event.tags.iter().any(|t| {
        let parts = t.as_slice();
        parts.len() >= 2 && parts[0] == "broadcast" && parts[1] == "1"
    });

    let event_created_at = chrono::DateTime::from_timestamp(event.created_at.as_u64() as i64, 0)
        .unwrap_or_else(Utc::now);

    Ok(Some(ThreadMetadataOwned {
        event_id: event.id.as_bytes().to_vec(),
        event_created_at,
        channel_id,
        parent_event_id: parent_bytes,
        parent_event_created_at: parent_created,
        root_event_id: final_root_bytes,
        root_event_created_at: root_created,
        depth,
        broadcast,
    }))
}

// ── New validations (Phase 0a additions) ─────────────────────────────────────

/// Count all `e` tags regardless of content validity.
fn count_e_tags(event: &Event) -> usize {
    event
        .tags
        .iter()
        .filter(|t| t.kind().to_string() == "e")
        .count()
}

/// Extract the effective author of a stored event (handles relay-signed REST events).
pub(crate) fn effective_message_author(event: &Event, relay_pubkey: &nostr::PublicKey) -> Vec<u8> {
    if event.pubkey == *relay_pubkey {
        // Relay-signed REST event — real author in "actor" or "p" tag.
        if let Some(hex) = event.tags.iter().find_map(|t| {
            if t.kind().to_string() == "actor" {
                t.content().map(|s| s.to_string())
            } else {
                None
            }
        }) {
            if let Ok(bytes) = hex::decode(&hex) {
                if bytes.len() == 32 {
                    return bytes;
                }
            }
        }
        for tag in event.tags.iter() {
            if tag.kind().to_string() == "p" {
                if let Some(hex) = tag.content() {
                    if let Ok(bytes) = hex::decode(hex) {
                        if bytes.len() == 32 {
                            return bytes;
                        }
                    }
                }
            }
        }
    }
    event.pubkey.serialize().to_vec()
}

/// Validate kind:40003 edit ownership — event.pubkey must match target's effective author.
async fn validate_edit_ownership(event: &Event, state: &AppState) -> Result<(), String> {
    let target_hex = event
        .tags
        .iter()
        .find_map(|t| {
            if t.kind().to_string() == "e" {
                t.content().and_then(|v| {
                    if v.len() == 64 && v.chars().all(|c| c.is_ascii_hexdigit()) {
                        Some(v.to_string())
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        })
        .ok_or_else(|| "missing e tag for edit target".to_string())?;

    let target_bytes =
        hex::decode(&target_hex).map_err(|_| "invalid target event ID".to_string())?;
    let target_event = state
        .db
        .get_event_by_id(&target_bytes)
        .await
        .map_err(|e| format!("db error: {e}"))?
        .ok_or_else(|| "edit target event not found".to_string())?;

    // Verify target belongs to the same channel as the edit event.
    let edit_channel_id = extract_channel_id(event);
    match (edit_channel_id, target_event.channel_id) {
        (Some(edit_ch), Some(target_ch)) if edit_ch != target_ch => {
            return Err("target event belongs to a different channel".to_string());
        }
        (Some(_), None) => {
            return Err("target event has no channel".to_string());
        }
        _ => {} // Same channel or no channel context — OK
    }

    let author = effective_message_author(&target_event.event, &state.relay_keypair.public_key());
    let actor = event.pubkey.serialize().to_vec();
    if author != actor {
        return Err("must be event author to edit".to_string());
    }
    Ok(())
}

/// Validate kind:45002 vote targets a forum post (45001) or comment (45003).
async fn validate_forum_vote_target(event: &Event, state: &AppState) -> Result<(), String> {
    let target_hex = event
        .tags
        .iter()
        .find_map(|t| {
            if t.kind().to_string() == "e" {
                t.content().and_then(|v| {
                    if v.len() == 64 && v.chars().all(|c| c.is_ascii_hexdigit()) {
                        Some(v.to_string())
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        })
        .ok_or_else(|| "missing e tag for vote target".to_string())?;

    let target_bytes =
        hex::decode(&target_hex).map_err(|_| "invalid target event ID".to_string())?;
    let target_event = state
        .db
        .get_event_by_id(&target_bytes)
        .await
        .map_err(|e| format!("db error: {e}"))?
        .ok_or_else(|| "vote target event not found".to_string())?;

    let target_kind = event_kind_u32(&target_event.event);
    if target_kind != KIND_FORUM_POST && target_kind != KIND_FORUM_COMMENT {
        return Err("vote target must be a forum post or comment".to_string());
    }

    // Verify target belongs to the same channel as the vote event.
    let vote_channel_id = extract_channel_id(event);
    match (vote_channel_id, target_event.channel_id) {
        (Some(vote_ch), Some(target_ch)) if vote_ch != target_ch => {
            return Err("target event belongs to a different channel".to_string());
        }
        (Some(_), None) => {
            return Err("target event has no channel".to_string());
        }
        _ => {}
    }
    Ok(())
}

/// Validate kind:40008 diff event metadata tags.
fn validate_diff_event(event: &Event) -> Result<(), String> {
    // Content max 60KB
    if event.content.len() > 61_440 {
        return Err(format!(
            "diff content exceeds 60KB limit (got {} bytes)",
            event.content.len()
        ));
    }

    let mut has_repo = false;
    let mut has_commit = false;

    for tag in event.tags.iter() {
        let parts = tag.as_slice();
        if parts.len() < 2 {
            continue;
        }
        match parts[0].as_str() {
            "repo" => {
                let url = &parts[1];
                if !url.starts_with("http://") && !url.starts_with("https://") {
                    return Err("repo URL must be http or https".to_string());
                }
                has_repo = true;
            }
            "commit" => {
                let sha = &parts[1];
                if sha.len() < 7 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Err("commit SHA must be at least 7 hex characters".to_string());
                }
                has_commit = true;
            }
            "parent-commit" => {
                let sha = &parts[1];
                if sha.len() < 7 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Err("parent-commit SHA must be at least 7 hex characters".to_string());
                }
            }
            "branch" => {
                if parts.len() < 3 || parts[1].is_empty() || parts[2].is_empty() {
                    return Err("branch tag requires both source and target".to_string());
                }
            }
            "pr" => {
                if parts[1].parse::<u32>().map(|n| n == 0).unwrap_or(true) {
                    return Err("pr number must be a positive integer".to_string());
                }
            }
            _ => {}
        }
    }

    if !has_repo {
        return Err("diff event requires a repo tag".to_string());
    }
    if !has_commit {
        return Err("diff event requires a commit tag".to_string());
    }
    Ok(())
}

// ── Pipeline step functions ───────────────────────────────────────────────────

/// Step 1 + 1b: Reject relay-internal kinds and transport-restricted kinds.
///
/// AUTH and membership notification events are relay-signed only.
/// Gift-wrap and presence updates are WebSocket-only.
pub(crate) fn check_blocked_kinds(kind_u32: u32, is_http: bool) -> Result<(), IngestError> {
    if kind_u32 == KIND_AUTH {
        return Err(IngestError::Rejected(
            "invalid: AUTH events cannot be submitted".into(),
        ));
    }
    if kind_u32 == KIND_MEMBER_ADDED_NOTIFICATION || kind_u32 == KIND_MEMBER_REMOVED_NOTIFICATION {
        return Err(IngestError::Rejected(
            "invalid: membership notifications are relay-signed only".into(),
        ));
    }
    if is_http && (kind_u32 == KIND_GIFT_WRAP || kind_u32 == KIND_PRESENCE_UPDATE) {
        return Err(IngestError::Rejected(format!(
            "invalid: kind {kind_u32} is only accepted via WebSocket"
        )));
    }
    Ok(())
}

/// Step 2: Verify the event's cryptographic signature (runs in a blocking thread).
pub(crate) async fn verify_signature(event: &Event) -> Result<(), IngestError> {
    let event_clone = event.clone();
    match tokio::task::spawn_blocking(move || verify_event(&event_clone)).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(IngestError::Rejected(format!("invalid: {e}"))),
        Err(e) => {
            error!("spawn_blocking panicked: {e}");
            Err(IngestError::Internal(
                "error: internal verification error".into(),
            ))
        }
    }
}

/// Step 2b: Reject events whose timestamp drifts more than ±15 minutes from server time.
///
/// Skipped for `proxy:submit` — proxied events may carry historical timestamps.
pub(crate) fn check_timestamp(event: &Event, has_proxy_scope: bool) -> Result<(), IngestError> {
    if has_proxy_scope {
        return Ok(());
    }
    const MAX_TIMESTAMP_DRIFT_SECS: i64 = 900;
    let now = chrono::Utc::now().timestamp();
    let event_ts = event.created_at.as_u64() as i64;
    if (event_ts - now).abs() > MAX_TIMESTAMP_DRIFT_SECS {
        return Err(IngestError::Rejected(
            "invalid: event timestamp too far from server time".into(),
        ));
    }
    Ok(())
}

/// Step 2c: Reject events whose content exceeds 256 KB.
pub(crate) fn check_content_size(event: &Event) -> Result<(), IngestError> {
    const MAX_EVENT_CONTENT_BYTES: usize = 256 * 1024;
    if event.content.len() > MAX_EVENT_CONTENT_BYTES {
        return Err(IngestError::Rejected(format!(
            "invalid: content exceeds maximum size of {} bytes (got {})",
            MAX_EVENT_CONTENT_BYTES,
            event.content.len()
        )));
    }
    Ok(())
}

/// Step 3: Confirm the event's pubkey matches the authenticated identity.
///
/// Skipped for gift-wrap (sealed-sender) and proxy submissions.
pub(crate) fn check_pubkey_match(
    event: &Event,
    auth: &IngestAuth,
    kind_u32: u32,
) -> Result<(), IngestError> {
    let is_gift_wrap = kind_u32 == KIND_GIFT_WRAP;
    if event.pubkey != *auth.pubkey() && !auth.has_proxy_scope() && !is_gift_wrap {
        return Err(IngestError::AuthFailed(
            "invalid: event pubkey does not match authenticated identity".into(),
        ));
    }
    Ok(())
}

/// Step 5: Resolve the channel UUID for this event.
///
/// Reactions derive their channel from the target event; deletions look up the
/// target's channel; gift-wraps are channel-less; everything else reads the `h` tag.
pub(crate) async fn resolve_channel_id(
    db: &sprout_db::Db,
    event: &Event,
    kind_u32: u32,
) -> Result<Option<Uuid>, IngestError> {
    if kind_u32 == KIND_REACTION {
        return match derive_reaction_channel(db, event).await {
            ReactionChannelResult::Channel(ch_id) => Ok(Some(ch_id)),
            ReactionChannelResult::NoChannel => Ok(None),
            ReactionChannelResult::NotFound => Err(IngestError::Rejected(
                "invalid: reaction target event not found".into(),
            )),
            ReactionChannelResult::NoTarget => Err(IngestError::Rejected(
                "invalid: reaction must reference a target event via e tag".into(),
            )),
            ReactionChannelResult::DbError(e) => Err(IngestError::Internal(format!(
                "error: internal error looking up reaction target: {e}"
            ))),
        };
    }
    if kind_u32 == KIND_GIFT_WRAP {
        return Ok(None);
    }
    if kind_u32 == KIND_DELETION {
        // kind:5 has no h-tag; derive channel from the target event.
        let target_hex = event.tags.iter().find_map(|t| {
            if t.kind().to_string() == "e" {
                t.content().and_then(|v| {
                    if v.len() == 64 && v.chars().all(|c| c.is_ascii_hexdigit()) {
                        Some(v.to_string())
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        });
        return match target_hex {
            Some(hex) => {
                let target_bytes = hex::decode(&hex).map_err(|_| {
                    IngestError::Rejected("invalid: malformed deletion target id".into())
                })?;
                match db.get_event_by_id(&target_bytes).await {
                    Ok(Some(target)) => Ok(target.channel_id),
                    Ok(None) => Ok(None), // validate_standard_deletion will catch missing target
                    Err(e) => Err(IngestError::Internal(format!(
                        "error: looking up deletion target: {e}"
                    ))),
                }
            }
            None => Ok(None), // no e-tag — caught by single-target enforcement (step 12)
        };
    }
    Ok(extract_channel_id(event))
}

/// Step 6: Require an `h` tag for channel-scoped event kinds.
pub(crate) fn check_h_tag_required(
    kind_u32: u32,
    channel_id: Option<Uuid>,
) -> Result<(), IngestError> {
    if requires_h_channel_scope(kind_u32) && channel_id.is_none() {
        return Err(IngestError::Rejected(
            "invalid: channel-scoped events must include an h tag".into(),
        ));
    }
    Ok(())
}

/// Step 7: Enforce token-level channel restrictions.
///
/// Channel-scoped tokens may only submit events to their allowed channels.
pub(crate) fn check_token_channel_scope(
    auth: &IngestAuth,
    kind_u32: u32,
    channel_id: Option<Uuid>,
) -> Result<(), IngestError> {
    if let Some(ch_id) = channel_id {
        check_token_channel_access(auth, ch_id).map_err(IngestError::AuthFailed)?;
    } else if kind_u32 == KIND_NIP29_CREATE_GROUP && auth.channel_ids().is_some() {
        return Err(IngestError::AuthFailed(
            "restricted: channel-scoped tokens must include an h tag for create-group".into(),
        ));
    }
    Ok(())
}

/// Step 8: Verify the submitter is a member of the target channel (or the channel is open).
///
/// Skipped for join requests, channel creation, and proxy submissions.
pub(crate) async fn check_membership(
    state: &AppState,
    auth: &IngestAuth,
    kind_u32: u32,
    channel_id: Option<Uuid>,
) -> Result<(), IngestError> {
    if let Some(ch_id) = channel_id {
        let skip = kind_u32 == KIND_NIP29_JOIN_REQUEST
            || kind_u32 == KIND_NIP29_CREATE_GROUP
            || auth.has_proxy_scope();
        if !skip {
            let pubkey_bytes = auth.pubkey().serialize().to_vec();
            check_channel_membership(state, ch_id, &pubkey_bytes)
                .await
                .map_err(IngestError::Rejected)?;
        }
    }
    Ok(())
}

/// Step 9: Run admin-kind validation (kinds 9000–9022).
pub(crate) async fn check_admin_event(
    state: &Arc<AppState>,
    event: &Event,
    kind_u32: u32,
) -> Result<(), IngestError> {
    if crate::handlers::side_effects::is_admin_kind(kind_u32) {
        crate::handlers::side_effects::validate_admin_event(kind_u32, event, state)
            .await
            .map_err(|e| match e {
                crate::error::ValidationError::Rejected(msg) => {
                    IngestError::Rejected(format!("invalid: {msg}"))
                }
                crate::error::ValidationError::Infra(msg) => {
                    IngestError::Internal(format!("error: {msg}"))
                }
            })?;
    }
    Ok(())
}

/// Step 10: Validate kind:5 standard deletion events.
pub(crate) async fn check_standard_deletion(
    state: &Arc<AppState>,
    event: &Event,
    kind_u32: u32,
) -> Result<(), IngestError> {
    if kind_u32 == KIND_DELETION {
        crate::handlers::side_effects::validate_standard_deletion_event(event, state)
            .await
            .map_err(|e| match e {
                crate::error::ValidationError::Rejected(msg) => {
                    IngestError::Rejected(format!("invalid: {msg}"))
                }
                crate::error::ValidationError::Infra(msg) => {
                    IngestError::Internal(format!("error: {msg}"))
                }
            })?;
    }
    Ok(())
}

/// Step 11: Reject events targeting an archived channel.
///
/// Exception: kind:9002 with `archived=false` is the unarchive operation itself.
pub(crate) async fn check_channel_not_archived(
    state: &AppState,
    event: &Event,
    kind_u32: u32,
    channel_id: Option<Uuid>,
) -> Result<(), IngestError> {
    if let Some(ch_id) = channel_id {
        let is_unarchive = kind_u32 == KIND_NIP29_EDIT_METADATA
            && event.tags.iter().any(|t| {
                let parts = t.as_slice();
                parts.len() >= 2 && parts[0] == "archived" && parts[1] == "false"
            });
        if !is_unarchive {
            match state.db.get_channel(ch_id).await {
                Ok(channel) => {
                    if channel.archived_at.is_some() {
                        return Err(IngestError::Rejected("invalid: channel is archived".into()));
                    }
                }
                Err(e) => {
                    // Fail-closed: DB errors are infrastructure failures, not client errors.
                    // Letting events through on DB failure could violate archive invariants.
                    tracing::warn!(channel_id = %ch_id, error = %e, "DB error checking archived status");
                    return Err(IngestError::Internal(format!("error: database error: {e}")));
                }
            }
        }
    }
    Ok(())
}

/// Step 12: Enforce exactly-one-target rule for deletion events (kind:9005 and kind:5).
pub(crate) fn check_single_deletion_target(
    event: &Event,
    kind_u32: u32,
) -> Result<(), IngestError> {
    if kind_u32 == KIND_NIP29_DELETE_EVENT || kind_u32 == KIND_DELETION {
        let e_count = count_e_tags(event);
        if e_count != 1 {
            return Err(IngestError::Rejected(format!(
                "invalid: deletion events must reference exactly one target (got {e_count})"
            )));
        }
    }
    Ok(())
}

/// Step 13: Verify the submitter is the original author of the edit target (kind:40003).
pub(crate) async fn check_edit_ownership(
    state: &AppState,
    event: &Event,
    kind_u32: u32,
) -> Result<(), IngestError> {
    if kind_u32 == KIND_STREAM_MESSAGE_EDIT {
        validate_edit_ownership(event, state)
            .await
            .map_err(|e| IngestError::Rejected(format!("invalid: {e}")))?;
    }
    Ok(())
}

/// Step 14: Verify a forum vote targets a post or comment (kind:45002).
pub(crate) async fn check_forum_vote_target(
    state: &AppState,
    event: &Event,
    kind_u32: u32,
) -> Result<(), IngestError> {
    if kind_u32 == KIND_FORUM_VOTE {
        validate_forum_vote_target(event, state)
            .await
            .map_err(|e| IngestError::Rejected(format!("invalid: {e}")))?;
    }
    Ok(())
}

/// Step 15: Validate diff event metadata tags (kind:40008).
pub(crate) fn check_diff_event(event: &Event, kind_u32: u32) -> Result<(), IngestError> {
    if kind_u32 == KIND_STREAM_MESSAGE_DIFF {
        validate_diff_event(event).map_err(|e| IngestError::Rejected(format!("invalid: {e}")))?;
    }
    Ok(())
}

/// Outcome of the create-group step (step 16).
pub(crate) enum CreateGroupOutcome {
    /// Channel was created; UUID is tracked for compensation on later failure.
    Created(Uuid),
    /// No h-tag — server will assign UUID at insert time; nothing to track.
    NoClientUuid,
    /// Channel already exists — return a duplicate result immediately.
    Duplicate,
}

/// Step 16: For kind:9007, validate channel metadata and pre-create the channel row.
///
/// Returns `CreateGroupOutcome` so the caller can short-circuit on `Duplicate`
/// and track the pre-created UUID for compensation on DB insert failure.
pub(crate) async fn create_group_if_needed(
    state: &AppState,
    event: &Event,
    kind_u32: u32,
    channel_id: Option<Uuid>,
) -> Result<CreateGroupOutcome, IngestError> {
    if kind_u32 != KIND_NIP29_CREATE_GROUP {
        return Ok(CreateGroupOutcome::NoClientUuid);
    }

    // Name tag must be present and non-empty.
    let create_name = event.tags.iter().find_map(|t| {
        if t.kind().to_string() == "name" {
            t.content().map(|s| s.to_string())
        } else {
            None
        }
    });
    if create_name
        .as_ref()
        .map(|n| n.trim().is_empty())
        .unwrap_or(true)
    {
        return Err(IngestError::Rejected(
            "invalid: channel name is required".into(),
        ));
    }

    // Validate visibility and channel_type enums before any DB work.
    let visibility_str = event
        .tags
        .iter()
        .find_map(|t| {
            if t.kind().to_string() == "visibility" {
                t.content().map(|s| s.to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "open".to_string());
    let channel_type_str = event
        .tags
        .iter()
        .find_map(|t| {
            if t.kind().to_string() == "channel_type" {
                t.content().map(|s| s.to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "stream".to_string());

    let visibility: sprout_db::channel::ChannelVisibility = visibility_str
        .parse()
        .map_err(|_| IngestError::Rejected(format!("invalid visibility: {visibility_str}")))?;
    let channel_type: sprout_db::channel::ChannelType = channel_type_str
        .parse()
        .map_err(|_| IngestError::Rejected(format!("invalid channel_type: {channel_type_str}")))?;

    let Some(client_uuid) = channel_id else {
        return Ok(CreateGroupOutcome::NoClientUuid);
    };

    let name = create_name.unwrap_or_default();
    let description = event.tags.iter().find_map(|t| {
        if t.kind().to_string() == "about" {
            t.content().map(|s| s.to_string())
        } else {
            None
        }
    });
    let actor_bytes = event.pubkey.serialize().to_vec();

    let (_, was_created) = state
        .db
        .create_channel_with_id(
            client_uuid,
            &name,
            channel_type,
            visibility,
            description.as_deref(),
            &actor_bytes,
        )
        .await
        .map_err(|e| IngestError::Internal(format!("error: {e}")))?;

    if !was_created {
        return Ok(CreateGroupOutcome::Duplicate);
    }
    Ok(CreateGroupOutcome::Created(client_uuid))
}

/// Step 17: Reject join requests (kind:9021) to private channels.
pub(crate) async fn check_join_open_channel(
    state: &AppState,
    kind_u32: u32,
    channel_id: Option<Uuid>,
) -> Result<(), IngestError> {
    if kind_u32 != KIND_NIP29_JOIN_REQUEST {
        return Ok(());
    }
    let Some(ch_id) = channel_id else {
        return Err(IngestError::Rejected(
            "invalid: join request must include an h tag".into(),
        ));
    };
    match state.db.get_channel(ch_id).await {
        Ok(ch) if ch.visibility == "private" => Err(IngestError::Rejected(
            "restricted: channel is private".into(),
        )),
        Err(_) => Err(IngestError::Rejected("invalid: channel not found".into())),
        _ => Ok(()),
    }
}

/// Step 18: Validate `imeta` tags and verify referenced blobs exist in media storage.
pub(crate) async fn check_imeta_tags(state: &AppState, event: &Event) -> Result<(), IngestError> {
    let imeta_tags: Vec<Vec<String>> = event
        .tags
        .iter()
        .filter(|t| t.kind().to_string() == "imeta")
        .map(|t| t.as_slice().iter().map(|s| s.to_string()).collect())
        .collect();
    if imeta_tags.is_empty() {
        return Ok(());
    }
    crate::api::validate_imeta_tags(&imeta_tags, &state.config.media.public_base_url)
        .map_err(|e| IngestError::Rejected(format!("invalid: {e}")))?;
    crate::api::verify_imeta_blobs(&imeta_tags, &state.media_storage)
        .await
        .map_err(|e| IngestError::Rejected(format!("invalid: {e}")))?;
    Ok(())
}

/// Step 19: Resolve NIP-10 thread ancestry for channel-scoped events.
pub(crate) async fn resolve_thread_metadata(
    state: &AppState,
    event: &Event,
    kind_u32: u32,
    channel_id: Option<Uuid>,
) -> Result<Option<ThreadMetadataOwned>, IngestError> {
    if !requires_h_channel_scope(kind_u32) {
        return Ok(None);
    }
    let Some(ch_id) = channel_id else {
        return Ok(None);
    };
    resolve_nip10_thread_meta(event, ch_id, state)
        .await
        .map_err(|msg| IngestError::Rejected(format!("invalid: {msg}")))
}

/// Step 20 pre-check: Reject kind:0 events with non-JSON content before storage.
pub(crate) fn check_profile_json(event: &Event, kind_u32: u32) -> Result<(), IngestError> {
    if kind_u32 == KIND_PROFILE
        && serde_json::from_str::<serde_json::Value>(&event.content).is_err()
    {
        return Err(IngestError::Rejected(
            "invalid: kind:0 content must be valid JSON".into(),
        ));
    }
    Ok(())
}

/// Step 20a: Reaction-specific insert path (kind:7).
///
/// Inserts the reaction row first (dedup via ON CONFLICT), then stores the event.
/// On event insert failure, compensates by removing the reaction row.
/// Returns `Some(IngestResult)` to short-circuit the pipeline, or `None` if this
/// event is not a reaction (caller should continue to the normal insert path).
pub(crate) async fn insert_reaction_event(
    state: &Arc<AppState>,
    event: &Event,
    event_id_hex: &str,
    kind_u32: u32,
    channel_id: Option<Uuid>,
    thread_meta: &Option<ThreadMetadataOwned>,
    auth_pubkey_hex: &str,
) -> Result<Option<IngestResult>, IngestError> {
    if kind_u32 != KIND_REACTION {
        return Ok(None);
    }

    let target_hex = event
        .tags
        .iter()
        .rev()
        .find_map(|tag| {
            if tag.kind().to_string() == "e" {
                tag.content().and_then(|v| {
                    if v.len() == 64 && v.chars().all(|c| c.is_ascii_hexdigit()) {
                        Some(v.to_string())
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        })
        .ok_or_else(|| {
            IngestError::Rejected(
                "invalid: reaction must reference a target event via e tag".into(),
            )
        })?;

    let target_id = hex::decode(&target_hex)
        .map_err(|_| IngestError::Rejected("invalid: malformed reaction target id".into()))?;

    let target_event = state
        .db
        .get_event_by_id(&target_id)
        .await
        .map_err(|e| IngestError::Internal(format!("error: {e}")))?
        .ok_or_else(|| IngestError::Rejected("invalid: reaction target event not found".into()))?;

    let target_created_at =
        chrono::DateTime::from_timestamp(target_event.event.created_at.as_u64() as i64, 0)
            .unwrap_or_else(chrono::Utc::now);

    let actor_bytes = effective_message_author(event, &state.relay_keypair.public_key());
    let emoji = if event.content.is_empty() {
        "+"
    } else {
        &event.content
    };

    const MAX_REACTION_EMOJI_CHARS: usize = 64;
    let emoji_char_count = emoji.chars().count();
    if emoji_char_count > MAX_REACTION_EMOJI_CHARS {
        return Err(IngestError::Rejected(format!(
            "invalid: reaction emoji exceeds {} characters (got {})",
            MAX_REACTION_EMOJI_CHARS, emoji_char_count
        )));
    }

    let inserted = state
        .db
        .add_reaction(&target_id, target_created_at, &actor_bytes, emoji, None)
        .await
        .map_err(|e| IngestError::Internal(format!("error: {e}")))?;

    if !inserted {
        return Ok(Some(IngestResult {
            event_id: event_id_hex.to_string(),
            accepted: false,
            message: "duplicate: reaction already exists".into(),
        }));
    }

    let thread_params = thread_meta.as_ref().map(|m| m.as_params());
    let (stored_event, was_inserted) = match state
        .db
        .insert_event_with_thread_metadata(event, channel_id, thread_params)
        .await
    {
        Ok(result) => result,
        Err(e) => {
            if let Err(re) = state
                .db
                .remove_reaction(&target_id, target_created_at, &actor_bytes, emoji)
                .await
            {
                warn!(event_id = %event_id_hex, "reaction compensation failed: {re}");
            }
            return Err(IngestError::Internal(format!("error: database error: {e}")));
        }
    };

    if was_inserted {
        if let Err(e) = state
            .db
            .set_reaction_event_id(
                &target_id,
                target_created_at,
                &actor_bytes,
                emoji,
                event.id.as_bytes(),
            )
            .await
        {
            warn!(event_id = %event_id_hex, "set_reaction_event_id failed: {e}");
        }
    }

    dispatch_persistent_event(state, &stored_event, kind_u32, auth_pubkey_hex).await;
    info!(event_id = %event_id_hex, kind = kind_u32, "Event ingested via pipeline");
    Ok(Some(IngestResult {
        event_id: event_id_hex.to_string(),
        accepted: true,
        message: String::new(),
    }))
}

/// Step 20 (normal path): Store the event in the database.
///
/// kind:0 uses addressable (replaceable) storage; all others use insert-with-thread-metadata.
/// On failure, compensates by soft-deleting any pre-created channel row.
pub(crate) async fn store_event(
    state: &Arc<AppState>,
    event: &Event,
    event_id_hex: &str,
    kind_u32: u32,
    channel_id: Option<Uuid>,
    thread_meta: &Option<ThreadMetadataOwned>,
    pre_created_channel: Option<Uuid>,
) -> Result<(sprout_core::StoredEvent, bool), IngestError> {
    if kind_u32 == KIND_PROFILE {
        return state
            .db
            .replace_addressable_event(event, None)
            .await
            .map_err(|e| IngestError::Internal(format!("error: {e}")));
    }

    let thread_params = thread_meta.as_ref().map(|m| m.as_params());
    match state
        .db
        .insert_event_with_thread_metadata(event, channel_id, thread_params)
        .await
    {
        Ok(result) => Ok(result),
        Err(e) => {
            if let Some(ch_id) = pre_created_channel {
                if let Err(re) = state.db.soft_delete_channel(ch_id).await {
                    warn!(event_id = %event_id_hex, "channel compensation failed: {re}");
                }
            }
            Err(match e {
                sprout_db::DbError::AuthEventRejected => {
                    IngestError::Rejected("invalid: AUTH events cannot be stored".into())
                }
                other => IngestError::Internal(format!("error: database error: {other}")),
            })
        }
    }
}

/// Step 21: Run post-storage side effects (membership updates, profile sync, etc.).
pub(crate) async fn run_side_effects(
    state: &Arc<AppState>,
    event: &Event,
    event_id_hex: &str,
    kind_u32: u32,
) {
    if crate::handlers::side_effects::is_side_effect_kind(kind_u32) {
        if let Err(e) =
            crate::handlers::side_effects::handle_side_effects(kind_u32, event, state).await
        {
            warn!(event_id = %event_id_hex, kind = kind_u32, "Side effect failed: {e}");
        }
    }
}

/// Step 22: Fan out the stored event to all matching WebSocket subscribers.
pub(crate) async fn fan_out_event(
    state: &Arc<AppState>,
    stored_event: &sprout_core::StoredEvent,
    kind_u32: u32,
    pubkey_hex: &str,
) {
    dispatch_persistent_event(state, stored_event, kind_u32, pubkey_hex).await;
}

/// Step 4: Confirm the auth context holds the required scope for this event kind.
pub(crate) fn check_kind_scope(
    event: &Event,
    auth: &IngestAuth,
    kind_u32: u32,
) -> Result<(), IngestError> {
    let required = match required_scope_for_kind(kind_u32, event) {
        Ok(scope) => scope,
        Err(msg) => return Err(IngestError::Rejected(msg.into())),
    };
    if !auth.has_proxy_scope() && !auth.scopes().contains(&required) {
        return Err(IngestError::AuthFailed(format!(
            "restricted: insufficient scope (need {})",
            required
        )));
    }
    Ok(())
}

// ── The pipeline ─────────────────────────────────────────────────────────────

/// Ingest a signed Nostr event through the full validation pipeline.
///
/// Shared by WebSocket and HTTP transports. The caller constructs [`IngestAuth`]
/// from their transport-specific auth mechanism and maps the result to their
/// transport-specific response format.
pub async fn ingest_event(
    state: &Arc<AppState>,
    event: Event,
    auth: IngestAuth,
) -> Result<IngestResult, IngestError> {
    let event_id_hex = event.id.to_hex();
    let kind_u32 = event_kind_u32(&event);
    debug!(event_id = %event_id_hex, kind = kind_u32, "ingest_event");

    // ── 1 + 1b. Blocked kinds ────────────────────────────────────────────
    check_blocked_kinds(kind_u32, auth.is_http())?;

    // ── 2. Signature verification ────────────────────────────────────────
    verify_signature(&event).await?;

    // ── 2b. Timestamp sanity ─────────────────────────────────────────────
    check_timestamp(&event, auth.has_proxy_scope())?;

    // ── 2c. Content size guard ───────────────────────────────────────────
    check_content_size(&event)?;

    // ── 3. Pubkey match ──────────────────────────────────────────────────
    check_pubkey_match(&event, &auth, kind_u32)?;

    // ── 4. Per-kind scope allowlist ──────────────────────────────────────
    check_kind_scope(&event, &auth, kind_u32)?;

    // ── 5. Channel resolution ────────────────────────────────────────────
    let channel_id = resolve_channel_id(&state.db, &event, kind_u32).await?;

    // ── 6. h-tag requirement ─────────────────────────────────────────────
    check_h_tag_required(kind_u32, channel_id)?;

    // ── 7. Token channel access ──────────────────────────────────────────
    check_token_channel_scope(&auth, kind_u32, channel_id)?;

    // ── 8. Membership check ──────────────────────────────────────────────
    check_membership(state, &auth, kind_u32, channel_id).await?;

    // ── 9. Admin validation (kinds 9000–9022) ────────────────────────────
    check_admin_event(state, &event, kind_u32).await?;

    // ── 10. Standard deletion validation (kind:5) ────────────────────────
    check_standard_deletion(state, &event, kind_u32).await?;

    // ── 11. Archived channel check ───────────────────────────────────────
    check_channel_not_archived(state, &event, kind_u32, channel_id).await?;

    // ── 12. Single-target enforcement (kind:9005, kind:5) ────────────────
    check_single_deletion_target(&event, kind_u32)?;

    // ── 13. Edit ownership (kind:40003) ──────────────────────────────────
    check_edit_ownership(state, &event, kind_u32).await?;

    // ── 14. Forum vote target-kind (kind:45002) ──────────────────────────
    check_forum_vote_target(state, &event, kind_u32).await?;

    // ── 15. Diff validation (kind:40008) ─────────────────────────────────
    check_diff_event(&event, kind_u32)?;

    // ── 16. kind:9007 UUID dedup (create channel with client UUID) ───────
    // Track pre-created channel UUID for compensation on insert failure.
    let pre_created_channel =
        match create_group_if_needed(state, &event, kind_u32, channel_id).await? {
            CreateGroupOutcome::Duplicate => {
                return Ok(IngestResult {
                    event_id: event_id_hex,
                    accepted: false,
                    message: "duplicate: channel already exists".into(),
                });
            }
            CreateGroupOutcome::Created(uuid) => Some(uuid),
            CreateGroupOutcome::NoClientUuid => None,
        };

    // ── 17. kind:9021 open-only check ────────────────────────────────────
    check_join_open_channel(state, kind_u32, channel_id).await?;

    // ── 18. imeta tag validation ─────────────────────────────────────────
    check_imeta_tags(state, &event).await?;

    // ── 19. NIP-10 thread resolution ─────────────────────────────────────
    let thread_meta = resolve_thread_metadata(state, &event, kind_u32, channel_id).await?;

    // ── 20. DB insert ────────────────────────────────────────────────────
    check_profile_json(&event, kind_u32)?;

    // ── 20a. Reaction insert (kind:7) ────────────────────────────────────
    let pubkey_hex = auth.pubkey().to_hex();
    if let Some(result) = insert_reaction_event(
        state,
        &event,
        &event_id_hex,
        kind_u32,
        channel_id,
        &thread_meta,
        &pubkey_hex,
    )
    .await?
    {
        return Ok(result);
    }

    // ── 20. DB insert (non-reaction path) ────────────────────────────────
    let (stored_event, was_inserted) = store_event(
        state,
        &event,
        &event_id_hex,
        kind_u32,
        channel_id,
        &thread_meta,
        pre_created_channel,
    )
    .await?;

    if !was_inserted {
        return Ok(IngestResult {
            event_id: event_id_hex,
            accepted: true,
            message: "duplicate: event already exists".into(),
        });
    }

    // ── 21. Side effects ─────────────────────────────────────────────────
    run_side_effects(state, &event, &event_id_hex, kind_u32).await;

    // ── 22. Fan-out ──────────────────────────────────────────────────────
    fan_out_event(state, &stored_event, kind_u32, &pubkey_hex).await;

    info!(event_id = %event_id_hex, kind = kind_u32, "Event ingested via pipeline");

    Ok(IngestResult {
        event_id: event_id_hex,
        accepted: true,
        message: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sprout_core::kind::{
        KIND_CANVAS, KIND_FORUM_COMMENT, KIND_FORUM_POST, KIND_FORUM_VOTE, KIND_PRESENCE_UPDATE,
        KIND_STREAM_MESSAGE, KIND_STREAM_MESSAGE_DIFF,
    };

    #[test]
    fn channel_scoped_content_kinds_require_h_tags() {
        for kind in [
            KIND_STREAM_MESSAGE,
            KIND_STREAM_MESSAGE_DIFF,
            KIND_CANVAS,
            KIND_FORUM_POST,
            KIND_FORUM_VOTE,
            KIND_FORUM_COMMENT,
        ] {
            assert!(
                requires_h_channel_scope(kind),
                "kind {kind} should require h"
            );
        }
    }

    #[test]
    fn nip29_admin_kinds_require_h_tags() {
        for kind in [
            KIND_NIP29_PUT_USER,
            KIND_NIP29_REMOVE_USER,
            KIND_NIP29_EDIT_METADATA,
            KIND_NIP29_DELETE_EVENT,
            KIND_NIP29_DELETE_GROUP,
            KIND_NIP29_LEAVE_REQUEST,
        ] {
            assert!(
                requires_h_channel_scope(kind),
                "kind {kind} should require h"
            );
        }
    }

    #[test]
    fn create_group_does_not_require_h_tag() {
        // kind:9007 creates the channel — h-tag is optional (client-chosen UUID)
        assert!(!requires_h_channel_scope(KIND_NIP29_CREATE_GROUP));
    }

    #[test]
    fn join_request_does_not_require_h_tag_via_requires_h() {
        // kind:9021 uses h-tag for channel reference but doesn't go through
        // requires_h_channel_scope — it's handled separately in the pipeline
        // because it needs special "open-only" validation
        assert!(!requires_h_channel_scope(KIND_NIP29_JOIN_REQUEST));
    }

    #[test]
    fn reactions_do_not_require_h_tag() {
        assert!(!requires_h_channel_scope(KIND_REACTION));
    }

    #[test]
    fn ephemeral_kinds_not_in_scope_allowlist() {
        assert!(required_scope_for_kind(KIND_PRESENCE_UPDATE, &make_dummy_event()).is_err());
    }

    #[test]
    fn per_kind_scope_allowlist_covers_all_18_migrated_kinds() {
        let dummy = make_dummy_event();
        let migrated = [
            KIND_PROFILE,
            KIND_DELETION,
            KIND_REACTION,
            KIND_STREAM_MESSAGE,
            KIND_NIP29_PUT_USER,
            KIND_NIP29_REMOVE_USER,
            KIND_NIP29_EDIT_METADATA,
            KIND_NIP29_DELETE_EVENT,
            KIND_NIP29_CREATE_GROUP,
            KIND_NIP29_DELETE_GROUP,
            KIND_NIP29_JOIN_REQUEST,
            KIND_NIP29_LEAVE_REQUEST,
            KIND_STREAM_MESSAGE_EDIT,
            KIND_STREAM_MESSAGE_DIFF,
            KIND_CANVAS,
            KIND_FORUM_POST,
            KIND_FORUM_VOTE,
            KIND_FORUM_COMMENT,
        ];
        for kind in migrated {
            assert!(
                required_scope_for_kind(kind, &dummy).is_ok(),
                "kind {kind} should be in the allowlist"
            );
        }
    }

    #[test]
    fn unknown_kind_rejected() {
        let dummy = make_dummy_event();
        assert!(required_scope_for_kind(99999, &dummy).is_err());
    }

    #[test]
    fn gift_wrap_is_in_scope_allowlist() {
        // KIND_GIFT_WRAP is still in the per-kind scope allowlist.
        // The HTTP block is transport-level (is_http gate), not scope-level.
        let dummy = make_dummy_event();
        assert!(
            required_scope_for_kind(KIND_GIFT_WRAP, &dummy).is_ok(),
            "KIND_GIFT_WRAP should be in the scope allowlist"
        );
    }

    #[test]
    fn ingest_auth_is_http_returns_true_for_http_variant() {
        use crate::handlers::ingest::{HttpAuthMethod, IngestAuth};
        let keys = nostr::Keys::generate();
        let http_auth = IngestAuth::Http {
            pubkey: keys.public_key(),
            scopes: vec![],
            auth_method: HttpAuthMethod::ApiToken,
            token_id: None,
            channel_ids: None,
        };
        assert!(
            http_auth.is_http(),
            "Http variant should return true for is_http()"
        );
    }

    #[test]
    fn ingest_auth_is_http_returns_false_for_nip42_variant() {
        use crate::handlers::ingest::IngestAuth;
        let keys = nostr::Keys::generate();
        let ws_auth = IngestAuth::Nip42 {
            pubkey: keys.public_key(),
            scopes: vec![],
            conn_id: uuid::Uuid::new_v4(),
        };
        assert!(
            !ws_auth.is_http(),
            "Nip42 variant should return false for is_http()"
        );
    }

    #[test]
    fn presence_update_not_in_scope_allowlist() {
        // KIND_PRESENCE_UPDATE is ephemeral — not in the allowlist regardless of transport.
        let dummy = make_dummy_event();
        assert!(
            required_scope_for_kind(KIND_PRESENCE_UPDATE, &dummy).is_err(),
            "KIND_PRESENCE_UPDATE should not be in the scope allowlist"
        );
    }

    #[test]
    fn diff_validation_rejects_missing_repo() {
        let event = make_event_with_tags(
            KIND_STREAM_MESSAGE_DIFF,
            "diff content",
            &[&["commit", "abc1234"]],
        );
        assert!(validate_diff_event(&event).is_err());
    }

    #[test]
    fn diff_validation_rejects_missing_commit() {
        let event = make_event_with_tags(
            KIND_STREAM_MESSAGE_DIFF,
            "diff content",
            &[&["repo", "https://github.com/example/repo"]],
        );
        assert!(validate_diff_event(&event).is_err());
    }

    #[test]
    fn diff_validation_accepts_valid() {
        let event = make_event_with_tags(
            KIND_STREAM_MESSAGE_DIFF,
            "diff content",
            &[
                &["repo", "https://github.com/example/repo"],
                &["commit", "abc1234"],
            ],
        );
        assert!(validate_diff_event(&event).is_ok());
    }

    #[test]
    fn diff_validation_rejects_oversized_content() {
        let big = "x".repeat(61_441);
        let event = make_event_with_tags(
            KIND_STREAM_MESSAGE_DIFF,
            &big,
            &[
                &["repo", "https://github.com/example/repo"],
                &["commit", "abc1234"],
            ],
        );
        assert!(validate_diff_event(&event).is_err());
    }

    // ── Test helpers ─────────────────────────────────────────────────────

    fn make_dummy_event() -> Event {
        let keys = nostr::Keys::generate();
        nostr::EventBuilder::new(nostr::Kind::Custom(9), "", [])
            .sign_with_keys(&keys)
            .unwrap()
    }

    fn make_event_with_tags(kind: u32, content: &str, tags: &[&[&str]]) -> Event {
        let keys = nostr::Keys::generate();
        let nostr_tags: Vec<nostr::Tag> =
            tags.iter().map(|t| nostr::Tag::parse(t).unwrap()).collect();
        nostr::EventBuilder::new(nostr::Kind::Custom(kind as u16), content, nostr_tags)
            .sign_with_keys(&keys)
            .unwrap()
    }

    #[test]
    fn count_e_tags_includes_malformed() {
        // A deletion event with one valid e-tag and one malformed e-tag
        // should count as 2 e-tags (and be rejected by the "exactly 1" check).
        let event = make_event_with_tags(
            5, // kind:5 deletion
            "",
            &[&["e", "a".repeat(64).as_str()], &["e", "not-valid-hex"]],
        );
        assert_eq!(count_e_tags(&event), 2);
    }

    #[test]
    fn count_e_tags_single_valid() {
        let event = make_event_with_tags(5, "", &[&["e", "a".repeat(64).as_str()]]);
        assert_eq!(count_e_tags(&event), 1);
    }
}
