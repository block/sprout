//! EVENT handler — auth → verify → store → fan-out → index → audit.

use std::sync::Arc;

use hex;
use tracing::{debug, error, info, warn};

use nostr::Event;
use sprout_audit::{AuditAction, NewAuditEntry};
use sprout_core::event::StoredEvent;
use sprout_core::kind::{
    event_kind_u32, is_ephemeral, is_workflow_execution_kind, KIND_AUTH, KIND_PRESENCE_UPDATE,
};
use sprout_core::verification::verify_event;

use sprout_auth::Scope;

use crate::connection::{AuthState, ConnectionState};
use crate::protocol::RelayMessage;
use crate::state::AppState;

/// Handle an EVENT message: authenticate, verify, store, fan-out, index, and audit the event.
pub async fn handle_event(event: Event, conn: Arc<ConnectionState>, state: Arc<AppState>) {
    let event_id_hex = event.id.to_hex();
    let kind_u32 = event_kind_u32(&event);
    debug!(event_id = %event_id_hex, kind = kind_u32, "EVENT");

    let (conn_id, pubkey_hex, pubkey_bytes, auth_pubkey) = {
        let auth = conn.auth_state.read().await;
        match &*auth {
            AuthState::Authenticated(ctx) => {
                if !ctx.scopes.is_empty() && !ctx.scopes.contains(&Scope::MessagesWrite) {
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
    if event.pubkey != auth_pubkey {
        conn.send(RelayMessage::ok(
            &event_id_hex,
            false,
            "invalid: event pubkey does not match authenticated identity",
        ));
        return;
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
        // Fail closed: reject the reaction if the target cannot be resolved.
        match derive_reaction_channel(&state.db, &event).await {
            Some(ch_id) => Some(ch_id),
            None => {
                warn!(
                    event_id = %event_id_hex,
                    "Rejecting reaction: target event not found or has no channel"
                );
                conn.send(RelayMessage::ok(
                    &event_id_hex,
                    false,
                    "invalid: reaction target event not found or not in a channel",
                ));
                return;
            }
        }
    } else {
        extract_channel_id(&event)
    };

    if let Some(ch_id) = channel_id {
        if let Err(msg) =
            check_channel_membership(&state, ch_id, &pubkey_bytes, conn_id, &event_id_hex).await
        {
            conn.send(msg);
            return;
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

    if let Some(ch_id) = channel_id {
        if let Err(e) = state.pubsub.publish_event(ch_id, &event).await {
            warn!(event_id = %event_id_hex, "Redis publish failed: {e}");
        }
    }

    let matches = state.sub_registry.fan_out(&stored_event);
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
    let audit_pubkey = pubkey_hex.clone();
    tokio::spawn(async move {
        let entry = NewAuditEntry {
            event_id: audit_event_id.clone(),
            event_kind: kind_u32,
            actor_pubkey: audit_pubkey,
            action: AuditAction::EventCreated,
            channel_id,
            metadata: serde_json::Value::Null,
        };
        if let Err(e) = audit.log(entry).await {
            error!(event_id = %audit_event_id, "Audit log failed: {e}");
        }
    });

    // Don't trigger workflows for workflow execution events (prevents infinite loops).
    let is_workflow_event = is_workflow_execution_kind(kind_u32);
    if !is_workflow_event {
        let wf = Arc::clone(&state.workflow_engine);
        let ev = stored_event.clone();
        tokio::spawn(async move {
            if let Err(e) = wf.on_event(&ev).await {
                tracing::error!(event_id = ?ev.event.id, "Workflow trigger failed: {e}");
            }
        });
    }

    conn.send(RelayMessage::ok(&event_id_hex, true, ""));

    info!(
        event_id = %event_id_hex,
        kind = kind_u32,
        conn_id = %conn_id,
        fan_out = matches.len(),
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

/// For NIP-25 reactions, derive the channel_id from the target event.
///
/// Reactions reference their target via an `e` tag containing a 64-hex event ID.
/// We look up that event in the DB to find its channel_id.
async fn derive_reaction_channel(db: &sprout_db::Db, event: &nostr::Event) -> Option<uuid::Uuid> {
    // Find the target event ID from NIP-25 `e` tags.
    // Per NIP-25, the last `e` tag is the target (in case of threading).
    let target_hex = event.tags.iter().rev().find_map(|tag| {
        let key = tag.kind().to_string();
        if key == "e" {
            tag.content().map(|s| s.to_string())
        } else {
            None
        }
    })?;

    // Must be a 64-char hex string (event ID), not a UUID
    if target_hex.len() != 64 {
        return None;
    }

    // Decode hex to bytes for DB lookup
    let id_bytes = match hex::decode(&target_hex) {
        Ok(b) if b.len() == 32 => b,
        _ => return None,
    };

    // Look up the target event to get its channel_id
    match db.get_event_by_id(&id_bytes).await {
        Ok(Some(target_event)) => {
            if let Some(ch_id) = target_event.channel_id {
                tracing::debug!(
                    reaction_id = %event.id.to_hex(),
                    target_id = %target_hex,
                    channel_id = %ch_id,
                    "Derived reaction channel from target event"
                );
                Some(ch_id)
            } else {
                tracing::debug!(
                    reaction_id = %event.id.to_hex(),
                    target_id = %target_hex,
                    "Target event has no channel — reaction will be global"
                );
                None
            }
        }
        Ok(None) => {
            tracing::debug!(
                reaction_id = %event.id.to_hex(),
                target_id = %target_hex,
                "Target event not found — reaction will be global"
            );
            None
        }
        Err(e) => {
            tracing::warn!(
                reaction_id = %event.id.to_hex(),
                target_id = %target_hex,
                "Failed to look up target event: {e}"
            );
            None
        }
    }
}

/// Extract a channel UUID from event tags.
///
/// Checks both `"channel"` custom tags and `"e"` reference tags (clients use
/// `Tag::parse(&["e", channel_id])` — the value is a UUID, not an event hash).
fn extract_channel_id(event: &Event) -> Option<uuid::Uuid> {
    for tag in event.tags.iter() {
        let key = tag.kind().to_string();
        if key == "channel" || key == "e" {
            if let Some(val) = tag.content() {
                if let Ok(id) = val.parse::<uuid::Uuid>() {
                    return Some(id);
                }
            }
        }
    }
    None
}
