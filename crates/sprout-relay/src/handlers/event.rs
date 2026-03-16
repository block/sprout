//! EVENT handler — auth → verify → store → fan-out → index → audit.

use std::sync::Arc;

use chrono::Utc;
use hex;
use tracing::{debug, error, info, warn};

use nostr::Event;
use sprout_audit::{AuditAction, NewAuditEntry};
use sprout_core::event::StoredEvent;
use sprout_core::kind::{
    event_kind_u32, is_ephemeral, is_workflow_execution_kind, KIND_AUTH, KIND_CANVAS,
    KIND_DELETION, KIND_FORUM_COMMENT, KIND_FORUM_POST, KIND_FORUM_VOTE, KIND_GIFT_WRAP,
    KIND_MEMBER_ADDED_NOTIFICATION, KIND_MEMBER_REMOVED_NOTIFICATION, KIND_PRESENCE_UPDATE,
    KIND_STREAM_MESSAGE, KIND_STREAM_MESSAGE_BOOKMARKED, KIND_STREAM_MESSAGE_DIFF,
    KIND_STREAM_MESSAGE_EDIT, KIND_STREAM_MESSAGE_PINNED, KIND_STREAM_MESSAGE_SCHEDULED,
    KIND_STREAM_MESSAGE_V2, KIND_STREAM_REMINDER,
};
use sprout_core::verification::verify_event;

use sprout_auth::Scope;

use crate::connection::{AuthState, ConnectionState};
use crate::protocol::RelayMessage;
use crate::state::AppState;

/// Publish a stored event to subscribers and kick off async side effects.
pub(crate) async fn dispatch_persistent_event(
    state: &Arc<AppState>,
    stored_event: &StoredEvent,
    kind_u32: u32,
    actor_pubkey_hex: &str,
) -> usize {
    let event_id_hex = stored_event.event.id.to_hex();

    if let Some(ch_id) = stored_event.channel_id {
        state.mark_local_event(&stored_event.event.id);
        if let Err(e) = state.pubsub.publish_event(ch_id, &stored_event.event).await {
            // Publish failed — remove from dedup cache so the ID doesn't leak.
            state
                .local_event_ids
                .invalidate(&stored_event.event.id.to_bytes());
            warn!(event_id = %event_id_hex, "Redis publish failed: {e}");
        }
    }

    let matches = state.sub_registry.fan_out(stored_event);
    debug!(
        event_id = %event_id_hex,
        channel_id = ?stored_event.channel_id,
        match_count = matches.len(),
        "Fan-out"
    );

    let event_json = serde_json::to_string(&stored_event.event)
        .expect("nostr::Event serialization is infallible for well-formed events");
    let mut drop_count = 0u32;
    for (target_conn_id, sub_id) in &matches {
        let msg = format!(r#"["EVENT","{}",{}]"#, sub_id, event_json);
        if !state.conn_manager.send_to(*target_conn_id, msg) {
            drop_count += 1;
        }
    }
    if drop_count > 0 {
        tracing::warn!(
            event_id = %event_id_hex,
            drop_count,
            "fan-out: {drop_count} connection(s) cancelled due to full/closed buffers"
        );
    }

    // Skip search indexing for NIP-17 gift wraps — content is ciphertext,
    // and indexing would leak #p tag metadata into the search index.
    if kind_u32 != KIND_GIFT_WRAP {
        let search = Arc::clone(&state.search);
        let stored_for_search = stored_event.clone();
        tokio::spawn(async move {
            if let Err(e) = search.index_event(&stored_for_search).await {
                error!(event_id = %stored_for_search.event.id.to_hex(), "Search index failed: {e}");
            }
        });
    }

    let audit = Arc::clone(&state.audit);
    let audit_event_id = event_id_hex.clone();
    let audit_actor_pubkey = actor_pubkey_hex.to_string();
    let audit_channel_id = stored_event.channel_id;
    tokio::spawn(async move {
        let entry = NewAuditEntry {
            event_id: audit_event_id.clone(),
            event_kind: kind_u32,
            actor_pubkey: audit_actor_pubkey,
            action: AuditAction::EventCreated,
            channel_id: audit_channel_id,
            metadata: serde_json::Value::Null,
        };
        if let Err(e) = audit.log(entry).await {
            error!(event_id = %audit_event_id, "Audit log failed: {e}");
        }
    });

    // Skip workflow triggering for workflow-execution kinds (46001+) and for
    // relay-signed workflow-generated messages (sprout:workflow tag). The tag
    // check is combined with a pubkey check so that user-submitted events
    // carrying the same tag cannot bypass workflow triggers.
    let is_relay_workflow_msg = stored_event.event.pubkey == state.relay_keypair.public_key()
        && stored_event
            .event
            .tags
            .iter()
            .any(|t| t.as_slice().first().map(|s| s.as_str()) == Some("sprout:workflow"));

    if !is_workflow_execution_kind(kind_u32) && !is_relay_workflow_msg && kind_u32 != KIND_GIFT_WRAP
    {
        let workflow_engine = Arc::clone(&state.workflow_engine);
        let workflow_event = stored_event.clone();
        tokio::spawn(async move {
            if let Err(e) = workflow_engine.on_event(&workflow_event).await {
                tracing::error!(event_id = ?workflow_event.event.id, "Workflow trigger failed: {e}");
            }
        });
    }

    matches.len()
}

/// Handle an EVENT message: authenticate, verify, store, fan-out, index, and audit the event.
pub async fn handle_event(event: Event, conn: Arc<ConnectionState>, state: Arc<AppState>) {
    let event_id_hex = event.id.to_hex();
    let kind_u32 = event_kind_u32(&event);
    debug!(event_id = %event_id_hex, kind = kind_u32, "EVENT");

    let (conn_id, pubkey_hex, pubkey_bytes, auth_pubkey, has_proxy_scope) = {
        let auth = conn.auth_state.read().await;
        match &*auth {
            AuthState::Authenticated(ctx) => {
                if !ctx.scopes.is_empty()
                    && !ctx.scopes.contains(&Scope::MessagesWrite)
                    && !ctx.scopes.contains(&Scope::ProxySubmit)
                {
                    conn.send(RelayMessage::ok(
                        &event_id_hex,
                        false,
                        "restricted: insufficient scope",
                    ));
                    return;
                }
                (
                    conn.conn_id,
                    ctx.pubkey.to_hex(),
                    ctx.pubkey.serialize().to_vec(),
                    ctx.pubkey,
                    ctx.scopes.contains(&Scope::ProxySubmit),
                )
            }
            _ => {
                conn.send(RelayMessage::ok(
                    &event_id_hex,
                    false,
                    "auth-required: not authenticated",
                ));
                return;
            }
        }
    };

    // Enforce that the event's pubkey matches the authenticated identity.
    // Without this, a user authenticated as key A could submit events signed by key B.
    // Exceptions: proxy:submit scope, and NIP-17 gift wraps (kind:1059) which use
    // ephemeral one-time signing keys by design. The relay still knows the submitter
    // via NIP-42 auth for rate limiting and abuse prevention.
    let is_gift_wrap = kind_u32 == KIND_GIFT_WRAP;
    if event.pubkey != auth_pubkey && !has_proxy_scope && !is_gift_wrap {
        conn.send(RelayMessage::ok(
            &event_id_hex,
            false,
            "invalid: event pubkey does not match authenticated identity",
        ));
        return;
    }
    if has_proxy_scope && event.pubkey != auth_pubkey {
        tracing::info!(
            proxy_pubkey = %auth_pubkey,
            event_pubkey = %event.pubkey,
            event_id = %event_id_hex,
            "proxy:submit scope used — event submitted on behalf of shadow pubkey"
        );
    }

    if kind_u32 == KIND_AUTH {
        conn.send(RelayMessage::ok(
            &event_id_hex,
            false,
            "invalid: AUTH events cannot be submitted",
        ));
        return;
    }

    // Membership notification events are relay-signed only — reject client submissions.
    if kind_u32 == KIND_MEMBER_ADDED_NOTIFICATION || kind_u32 == KIND_MEMBER_REMOVED_NOTIFICATION {
        conn.send(RelayMessage::ok(
            &event_id_hex,
            false,
            "invalid: membership notifications are relay-signed only",
        ));
        return;
    }

    if is_ephemeral(kind_u32) {
        handle_ephemeral_event(
            event,
            conn_id,
            &event_id_hex,
            pubkey_bytes,
            auth_pubkey,
            conn,
            state,
        )
        .await;
        return;
    }

    let event_clone = event.clone();
    let verify_result = tokio::task::spawn_blocking(move || verify_event(&event_clone)).await;

    match verify_result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            warn!(conn_id = %conn_id, event_id = %event_id_hex, "Verification failed: {e}");
            conn.send(RelayMessage::ok(
                &event_id_hex,
                false,
                &format!("invalid: {e}"),
            ));
            return;
        }
        Err(e) => {
            error!(conn_id = %conn_id, "spawn_blocking panicked: {e}");
            conn.send(RelayMessage::ok(
                &event_id_hex,
                false,
                "error: internal verification error",
            ));
            return;
        }
    }

    let channel_id = if event.kind == nostr::Kind::Reaction {
        // For NIP-25 reactions, always derive channel from the target event.
        // Client-supplied channel tags are ignored to prevent spoofing.
        match derive_reaction_channel(&state.db, &event).await {
            ReactionChannelResult::Channel(ch_id) => Some(ch_id),
            ReactionChannelResult::NoChannel => {
                // Target event exists but has no channel (global/DM message).
                // Allow the reaction to proceed without channel scoping.
                None
            }
            ReactionChannelResult::NotFound => {
                // Fail closed: reject reactions to events we don't know about.
                warn!(
                    event_id = %event_id_hex,
                    "Rejecting reaction: target event not found in DB"
                );
                conn.send(RelayMessage::ok(
                    &event_id_hex,
                    false,
                    "invalid: reaction target event not found",
                ));
                return;
            }
            ReactionChannelResult::NoTarget => {
                // Malformed reaction: no valid `e` tag.
                warn!(
                    event_id = %event_id_hex,
                    "Rejecting reaction: no valid e tag referencing target event"
                );
                conn.send(RelayMessage::ok(
                    &event_id_hex,
                    false,
                    "invalid: reaction must reference a target event via e tag",
                ));
                return;
            }
            ReactionChannelResult::DbError(ref err) => {
                // Fail closed on transient DB errors — don't allow reactions
                // through when we can't verify the target.
                error!(
                    event_id = %event_id_hex,
                    "Rejecting reaction: database error looking up target: {err}"
                );
                conn.send(RelayMessage::ok(
                    &event_id_hex,
                    false,
                    "error: internal error looking up reaction target",
                ));
                return;
            }
        }
    } else if is_gift_wrap {
        // NIP-17 gift wraps are always global (channel_id = None).
        // A client-supplied h-tag is ignored — storing gift wraps channel-scoped
        // would let any channel subscriber read them, bypassing #p gating.
        None
    } else {
        extract_channel_id(&event)
    };

    if requires_h_channel_scope(kind_u32) && channel_id.is_none() {
        conn.send(RelayMessage::ok(
            &event_id_hex,
            false,
            "invalid: channel-scoped events must include an h tag",
        ));
        return;
    }

    if let Some(ch_id) = channel_id {
        if !has_proxy_scope {
            if let Err(msg) =
                check_channel_membership(&state, ch_id, &pubkey_bytes, conn_id, &event_id_hex).await
            {
                conn.send(msg);
                return;
            }
        }
    }

    // Admin kind validation (9000-9022) must happen BEFORE storage.
    if crate::handlers::side_effects::is_admin_kind(kind_u32) {
        if let Err(e) =
            crate::handlers::side_effects::validate_admin_event(kind_u32, &event, &state).await
        {
            conn.send(RelayMessage::ok(
                &event_id_hex,
                false,
                &format!("invalid: {e}"),
            ));
            return;
        }
    }

    if kind_u32 == KIND_DELETION {
        if let Err(e) =
            crate::handlers::side_effects::validate_standard_deletion_event(&event, &state).await
        {
            conn.send(RelayMessage::ok(
                &event_id_hex,
                false,
                &format!("invalid: {e}"),
            ));
            return;
        }
    }

    // Reject reactions (kind 7) targeting archived channels before storage.
    // This prevents invalid events from being stored and fanned out.
    if kind_u32 == 7 {
        if let Some(ch_id) = channel_id {
            match state.db.get_channel(ch_id).await {
                Ok(channel) if channel.archived_at.is_some() => {
                    conn.send(RelayMessage::ok(
                        &event_id_hex,
                        false,
                        "invalid: channel is archived",
                    ));
                    return;
                }
                Err(_) => {
                    // Channel not found — let it through; the event may still be valid
                }
                _ => {} // Channel exists and not archived — OK
            }
        }
    }

    // Validate imeta tags on WebSocket-submitted events (same rules as REST path).
    // Extract imeta tags from the event and validate them.
    let imeta_tags: Vec<Vec<String>> = event
        .tags
        .iter()
        .filter(|t| t.kind().to_string() == "imeta")
        .map(|t| {
            // Use as_slice() to capture ALL tag elements — no index cap.
            // The first element is the tag kind ("imeta"), already included.
            t.as_slice().iter().map(|s| s.to_string()).collect()
        })
        .collect();
    if !imeta_tags.is_empty() {
        if let Err(e) =
            crate::api::validate_imeta_tags(&imeta_tags, &state.config.media.public_base_url)
        {
            conn.send(RelayMessage::ok(
                &event_id_hex,
                false,
                &format!("invalid: {e}"),
            ));
            return;
        }
        // Verify referenced blobs exist in storage and metadata matches.
        if let Err(e) = crate::api::verify_imeta_blobs(&imeta_tags, &state.media_storage).await {
            conn.send(RelayMessage::ok(
                &event_id_hex,
                false,
                &format!("invalid: {e}"),
            ));
            return;
        }
    }

    // ── NIP-10 thread resolution for channel-scoped content kinds ─────────
    // Resolve ancestry from e-tags before storage so thread_metadata is populated
    // atomically with the event insert (prevents race with concurrent deletes).
    let thread_meta = if requires_h_channel_scope(kind_u32) {
        if let Some(ch_id) = channel_id {
            match resolve_nip10_thread_meta(&event, ch_id, &state).await {
                Ok(meta) => meta,
                Err(msg) => {
                    conn.send(RelayMessage::ok(
                        &event_id_hex,
                        false,
                        &format!("invalid: {msg}"),
                    ));
                    return;
                }
            }
        } else {
            None
        }
    } else {
        None
    };

    let thread_params = thread_meta.as_ref().map(|m| m.as_params());
    let (stored_event, was_inserted) = match state
        .db
        .insert_event_with_thread_metadata(&event, channel_id, thread_params)
        .await
    {
        Ok(result) => result,
        Err(sprout_db::DbError::AuthEventRejected) => {
            conn.send(RelayMessage::ok(
                &event_id_hex,
                false,
                "invalid: AUTH events cannot be stored",
            ));
            return;
        }
        Err(e) => {
            error!(conn_id = %conn_id, event_id = %event_id_hex, "DB insert failed: {e}");
            conn.send(RelayMessage::ok(
                &event_id_hex,
                false,
                "error: database error",
            ));
            return;
        }
    };

    if !was_inserted {
        conn.send(RelayMessage::ok(&event_id_hex, true, "duplicate:"));
        return;
    }

    // Side effects (reactions, thread metadata, NIP-29 membership changes) run after storage.
    if crate::handlers::side_effects::is_side_effect_kind(kind_u32) {
        if let Err(e) =
            crate::handlers::side_effects::handle_side_effects(kind_u32, &event, &state).await
        {
            tracing::warn!(event_id = %event_id_hex, kind = kind_u32, "Side effect failed: {e}");
        }
    }

    let fan_out = dispatch_persistent_event(&state, &stored_event, kind_u32, &pubkey_hex).await;

    conn.send(RelayMessage::ok(&event_id_hex, true, ""));

    info!(
        event_id = %event_id_hex,
        kind = kind_u32,
        conn_id = %conn_id,
        fan_out,
        "Event ingested"
    );
}

async fn handle_ephemeral_event(
    event: Event,
    conn_id: uuid::Uuid,
    event_id_hex: &str,
    pubkey_bytes: Vec<u8>,
    auth_pubkey: nostr::PublicKey,
    conn: Arc<ConnectionState>,
    state: Arc<AppState>,
) {
    let event_clone = event.clone();
    let verify_result = tokio::task::spawn_blocking(move || verify_event(&event_clone)).await;

    match verify_result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            conn.send(RelayMessage::ok(
                event_id_hex,
                false,
                &format!("invalid: {e}"),
            ));
            return;
        }
        Err(_) => {
            conn.send(RelayMessage::ok(
                event_id_hex,
                false,
                "error: internal error",
            ));
            return;
        }
    }

    // Special handling for presence events (kind:20001).
    // Presence fan-out is local-only. Multi-node would need Redis pub/sub.
    if event_kind_u32(&event) == KIND_PRESENCE_UPDATE {
        let status = event.content.to_string();
        let status = if status.len() > 128 {
            let mut end = 128;
            while !status.is_char_boundary(end) {
                end -= 1;
            }
            status[..end].to_string()
        } else {
            status
        };

        // Store presence in Redis (write the presence key that was previously missing).
        if status == "offline" {
            let _ = state.pubsub.clear_presence(&auth_pubkey).await;
        } else {
            let _ = state.pubsub.set_presence(&auth_pubkey, &status).await;
        }

        // Fan-out to all local subscribers with matching kind:20001 filter.
        let stored_event = StoredEvent::new(event.clone(), None);
        let matches = state.sub_registry.fan_out(&stored_event);
        let event_json = serde_json::to_string(&event)
            .expect("nostr::Event serialization is infallible for well-formed events");
        let mut drop_count = 0u32;
        for (target_conn_id, sub_id) in &matches {
            let msg = format!(r#"["EVENT","{}",{}]"#, sub_id, event_json);
            if !state.conn_manager.send_to(*target_conn_id, msg) {
                drop_count += 1;
            }
        }
        if drop_count > 0 {
            tracing::warn!(
                event_id = %event_id_hex,
                drop_count,
                "fan-out: {drop_count} connection(s) cancelled due to full/closed buffers"
            );
        }

        conn.send(RelayMessage::ok(event_id_hex, true, ""));
        return;
    }

    // Check channel membership before publishing ephemeral events.
    // Any authenticated user could otherwise publish typing indicators / presence
    // to channels they don't belong to.
    if let Some(ch_id) = extract_channel_id(&event) {
        if let Err(msg) =
            check_channel_membership(&state, ch_id, &pubkey_bytes, conn_id, event_id_hex).await
        {
            conn.send(msg);
            return;
        }

        if let Err(e) = state.pubsub.publish_event(ch_id, &event).await {
            warn!(conn_id = %conn_id, event_id = %event_id_hex, "Ephemeral publish failed: {e}");
        }
    }

    conn.send(RelayMessage::ok(event_id_hex, true, ""));
}

/// Check whether `pubkey_bytes` is allowed to post to `ch_id`.
///
/// Returns `Ok(())` if the user is a member or the channel is open.
/// Returns `Err(relay_message)` with the rejection notice to send back to the client.
///
/// Shared by `handle_event` and `handle_ephemeral_event` to avoid duplicating the
/// is_member + open-channel fallback logic.
async fn check_channel_membership(
    state: &AppState,
    ch_id: uuid::Uuid,
    pubkey_bytes: &[u8],
    conn_id: uuid::Uuid,
    event_id_hex: &str,
) -> Result<(), String> {
    match state.db.is_member(ch_id, pubkey_bytes).await {
        Ok(true) => Ok(()),
        Ok(false) => {
            let is_open = match state.db.get_channel(ch_id).await {
                Ok(ch) => ch.visibility == "open",
                Err(e) => {
                    tracing::warn!(conn_id = %conn_id, channel = %ch_id, error = %e,
                                   "channel lookup failed during membership fallback, denying");
                    false
                }
            };
            if is_open {
                Ok(())
            } else {
                Err(RelayMessage::ok(
                    event_id_hex,
                    false,
                    "restricted: not a channel member",
                ))
            }
        }
        Err(e) => {
            error!(conn_id = %conn_id, "Membership check failed: {e}");
            Err(RelayMessage::ok(
                event_id_hex,
                false,
                "error: database error",
            ))
        }
    }
}

/// Result of resolving a reaction's target channel.
enum ReactionChannelResult {
    /// Target event found and has a channel_id.
    Channel(uuid::Uuid),
    /// Target event found but has no channel (global/DM message) — allow as global.
    NoChannel,
    /// Target event not found in DB — reject (fail closed).
    NotFound,
    /// No valid `e` tag on the reaction — reject (malformed).
    NoTarget,
    /// DB error during lookup — reject (fail closed on transient errors).
    DbError(String),
}

/// For NIP-25 reactions, derive the channel_id from the target event.
///
/// Reactions reference their target via an `e` tag containing a 64-hex event ID.
/// We look up that event in the DB to find its channel_id.
///
/// Returns a [`ReactionChannelResult`] so the caller can distinguish between
/// "target exists but is global" (allow) and "target not found" (reject).
async fn derive_reaction_channel(
    db: &sprout_db::Db,
    event: &nostr::Event,
) -> ReactionChannelResult {
    // Find the target event ID from NIP-25 `e` tags.
    // Per NIP-25, the last `e` tag is the target (in case of threading).
    // Filter for 64-char hex event IDs inside find_map to skip UUID channel refs,
    // consistent with build_trigger_context() in sprout-workflow/src/lib.rs.
    let target_hex = match event.tags.iter().rev().find_map(|tag| {
        let key = tag.kind().to_string();
        if key == "e" {
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
        Ok(Some(target_event)) => {
            if let Some(ch_id) = target_event.channel_id {
                tracing::debug!(
                    reaction_id = %event.id.to_hex(),
                    target_id = %target_hex,
                    channel_id = %ch_id,
                    "Derived reaction channel from target event"
                );
                ReactionChannelResult::Channel(ch_id)
            } else {
                tracing::debug!(
                    reaction_id = %event.id.to_hex(),
                    target_id = %target_hex,
                    "Target event has no channel — allowing as global reaction"
                );
                ReactionChannelResult::NoChannel
            }
        }
        Ok(None) => {
            tracing::debug!(
                reaction_id = %event.id.to_hex(),
                target_id = %target_hex,
                "Target event not found in DB"
            );
            ReactionChannelResult::NotFound
        }
        Err(e) => {
            tracing::warn!(
                reaction_id = %event.id.to_hex(),
                target_id = %target_hex,
                "Failed to look up target event: {e}"
            );
            ReactionChannelResult::DbError(e.to_string())
        }
    }
}

/// Extract a channel UUID from event tags.
///
/// Checks the `"h"` NIP-29 group tag for a channel UUID.
/// The `"e"` tag is intentionally NOT checked — it is reserved for event references only.
fn extract_channel_id(event: &Event) -> Option<uuid::Uuid> {
    for tag in event.tags.iter() {
        let key = tag.kind().to_string();
        if key == "h" {
            if let Some(val) = tag.content() {
                if let Ok(id) = val.parse::<uuid::Uuid>() {
                    return Some(id);
                }
            }
        }
    }
    None
}

// NOTE: This function only validates that channel-scoped kinds include an `h` tag.
// Kind-specific metadata validation (e.g., diff_repo_url for kind:40008) is NOT
// enforced on the WebSocket path — it is handled by the REST API layer (api/messages.rs).
// This follows the Nostr protocol model where the relay is kind-agnostic for content events.
fn requires_h_channel_scope(kind: u32) -> bool {
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
    )
}

/// Owned thread metadata — bridges the async resolve step and the borrowed
/// [`sprout_db::event::ThreadMetadataParams`] expected by the DB layer.
struct ThreadMetadataOwned {
    event_id: Vec<u8>,
    event_created_at: chrono::DateTime<Utc>,
    channel_id: uuid::Uuid,
    parent_event_id: Vec<u8>,
    parent_event_created_at: chrono::DateTime<Utc>,
    root_event_id: Vec<u8>,
    root_event_created_at: chrono::DateTime<Utc>,
    depth: i32,
    broadcast: bool,
}

impl ThreadMetadataOwned {
    fn as_params(&self) -> sprout_db::event::ThreadMetadataParams<'_> {
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

/// Resolve NIP-10 thread ancestry from e-tags on a WebSocket-submitted event.
///
/// Returns:
///   `Ok(None)`        — not a reply (no NIP-10 markers), use plain insert
///   `Ok(Some(meta))`  — reply with resolved ancestry, use insert_event_with_thread_metadata
///   `Err(msg)`        — validation failure, reject event with OK false
async fn resolve_nip10_thread_meta(
    event: &nostr::Event,
    channel_id: uuid::Uuid,
    state: &AppState,
) -> Result<Option<ThreadMetadataOwned>, String> {
    // Scan e-tags for NIP-10 positional markers.
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

    // No NIP-10 markers → not a reply, proceed with plain insert.
    if root_hex.is_none() && reply_hex.is_none() {
        return Ok(None);
    }

    // NIP-10: "reply" marker is required to indicate a reply. "root"-only
    // is a thread-context reference, not a reply — skip thread metadata.
    let (root_hex, parent_hex) = match (root_hex, reply_hex) {
        (Some(r), Some(p)) => (r, p),      // nested reply: root + parent
        (None, Some(p)) => (p.clone(), p), // direct reply: single "reply" = both
        (Some(_), None) | (None, None) => return Ok(None), // not a reply
    };

    // Decode and look up parent event.
    let parent_bytes =
        hex::decode(&parent_hex).map_err(|_| "invalid parent event ID hex".to_string())?;

    let parent_event = state
        .db
        .get_event_by_id(&parent_bytes)
        .await
        .map_err(|e| format!("db error looking up parent: {e}"))?
        .ok_or_else(|| "reply parent not found".to_string())?;

    // Verify parent belongs to the same channel.
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

    let parent_meta = state
        .db
        .get_thread_metadata_by_event(&parent_bytes)
        .await
        .map_err(|e| format!("db error looking up thread metadata: {e}"))?;

    let (final_root_bytes, root_created, depth) = match parent_meta {
        Some(meta) => {
            let effective_root = meta.root_event_id.unwrap_or_else(|| parent_bytes.clone());
            // Reject if client-supplied root diverges from server-resolved root.
            // This prevents stored thread_metadata from contradicting wire-visible e-tags.
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
            // Parent has no thread metadata — either it's a top-level message (the root)
            // or a legacy WS reply created before NIP-10 thread resolution was added.
            // Fall back to parsing the parent's own e-tags to find its root.
            //
            // Check "root" marker first, then "reply" — Sprout's REST emitter uses a
            // single ["e", <root>, "", "reply"] tag for direct replies (no separate
            // "root" tag), so we must handle both forms.
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
                    // Direct-reply form: single "reply" tag = root reference
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
            // depth=1 if parent is root, depth=2 if parent is a legacy reply.
            // depth=2 is an approximation for deeper legacy chains (no metadata to
            // determine true depth), but correct for the common direct-reply case.
            let depth = if parent_root == parent_bytes { 1 } else { 2 };
            // Look up actual root event for its timestamp (don't use parent_created).
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

#[cfg(test)]
mod tests {
    use super::requires_h_channel_scope;
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
    fn non_channel_kinds_do_not_require_h_tags() {
        assert!(
            !requires_h_channel_scope(nostr::Kind::Reaction.as_u16().into()),
            "reactions derive channel from the target event"
        );
        assert!(
            !requires_h_channel_scope(KIND_PRESENCE_UPDATE),
            "presence updates are global/ephemeral"
        );
    }
}
