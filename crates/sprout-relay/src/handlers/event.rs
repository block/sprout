//! EVENT handler — auth → verify → store → fan-out → index → audit.

use std::sync::Arc;

use hex;
use tracing::{debug, error, info, warn};

use nostr::Event;
use sprout_audit::{AuditAction, NewAuditEntry};
use sprout_core::event::StoredEvent;
use sprout_core::kind::{
    event_kind_u32, is_ephemeral, is_workflow_execution_kind, KIND_AUTH, KIND_CANVAS,
    KIND_FORUM_COMMENT, KIND_FORUM_POST, KIND_FORUM_VOTE, KIND_PRESENCE_UPDATE,
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
        if let Err(e) = state.pubsub.publish_event(ch_id, &stored_event.event).await {
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
    for (target_conn_id, sub_id) in &matches {
        let msg = format!(r#"["EVENT","{}",{}]"#, sub_id, event_json);
        state.conn_manager.send_to(*target_conn_id, msg);
    }

    let search = Arc::clone(&state.search);
    let stored_for_search = stored_event.clone();
    tokio::spawn(async move {
        if let Err(e) = search.index_event(&stored_for_search).await {
            error!(event_id = %stored_for_search.event.id.to_hex(), "Search index failed: {e}");
        }
    });

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

    if !is_workflow_execution_kind(kind_u32) {
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
    // Exception: proxy:submit scope allows submitting events on behalf of shadow pubkeys.
    if event.pubkey != auth_pubkey && !has_proxy_scope {
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

    let (stored_event, was_inserted) = match state.db.insert_event(&event, channel_id).await {
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
        for (target_conn_id, sub_id) in &matches {
            let msg = format!(r#"["EVENT","{}",{}]"#, sub_id, event_json);
            state.conn_manager.send_to(*target_conn_id, msg);
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
            let is_open = state
                .db
                .get_channel(ch_id)
                .await
                .map(|ch| ch.visibility == "open")
                .unwrap_or(false);
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
