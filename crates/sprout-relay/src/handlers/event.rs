//! EVENT handler — WS dispatcher → ingest pipeline → fan-out.

use std::sync::Arc;

use tracing::{debug, error, info, warn};

use nostr::Event;
use sprout_core::event::StoredEvent;
use sprout_core::kind::{event_kind_u32, is_ephemeral, KIND_GIFT_WRAP, KIND_PRESENCE_UPDATE};
use sprout_core::verification::verify_event;

use crate::connection::{AuthState, ConnectionState};
use crate::protocol::RelayMessage;
use crate::state::AppState;

use super::ingest::{IngestAuth, IngestError};

/// Increment the rejection counter with a bounded reason label.
fn reject(reason: &'static str) {
    metrics::counter!("sprout_events_rejected_total", "reason" => reason).increment(1);
}

/// Bound the `kind` label to prevent cardinality explosion from arbitrary Nostr kinds.
fn bounded_kind_label(kind: u32) -> String {
    match kind {
        0..=9 | 1059 | 1063 => kind.to_string(),
        9000..=9022 | 9100 | 9110 | 9900 => kind.to_string(),
        20000..=29999 => kind.to_string(),
        30023 | 39000..=39003 => kind.to_string(),
        40002..=40100 => kind.to_string(),
        41001..=41003 => kind.to_string(),
        42001..=42003 => kind.to_string(),
        43001..=43006 => kind.to_string(),
        44001..=44004 | 44100..=44101 => kind.to_string(),
        45001..=45003 => kind.to_string(),
        46001..=46012 => kind.to_string(),
        47001..=47003 => kind.to_string(),
        48001..=48005 | 48100..=48105 => kind.to_string(),
        49001 => kind.to_string(),
        _ => "other".to_string(),
    }
}

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

    // Skip search indexing for NIP-17 gift wraps — content is ciphertext.
    if kind_u32 != KIND_GIFT_WRAP
        && state
            .search_index_tx
            .try_send(stored_event.clone())
            .is_err()
    {
        metrics::counter!("sprout_search_index_errors_total").increment(1);
        warn!(event_id = %event_id_hex, "Search index channel full — dropping event");
    }

    let audit = Arc::clone(&state.audit);
    let audit_event_id = event_id_hex.clone();
    let audit_actor_pubkey = actor_pubkey_hex.to_string();
    let audit_channel_id = stored_event.channel_id;
    tokio::spawn(async move {
        let entry = sprout_audit::NewAuditEntry {
            event_id: audit_event_id.clone(),
            event_kind: kind_u32,
            actor_pubkey: audit_actor_pubkey,
            action: sprout_audit::AuditAction::EventCreated,
            channel_id: audit_channel_id,
            metadata: serde_json::Value::Null,
        };
        let t = std::time::Instant::now();
        if let Err(e) = audit.log(entry).await {
            error!(event_id = %audit_event_id, "Audit log failed: {e}");
        } else {
            metrics::histogram!("sprout_audit_log_seconds").record(t.elapsed().as_secs_f64());
        }
    });

    // Skip workflow triggering for workflow-execution kinds and relay-signed workflow messages.
    let is_relay_workflow_msg = stored_event.event.pubkey == state.relay_keypair.public_key()
        && stored_event
            .event
            .tags
            .iter()
            .any(|t| t.as_slice().first().map(|s| s.as_str()) == Some("sprout:workflow"));

    if !sprout_core::kind::is_workflow_execution_kind(kind_u32)
        && !is_relay_workflow_msg
        && kind_u32 != KIND_GIFT_WRAP
    {
        let workflow_engine = Arc::clone(&state.workflow_engine);
        let workflow_event = stored_event.clone();
        let trigger_kind = kind_u32.to_string();
        tokio::spawn(async move {
            if let Err(e) = workflow_engine.on_event(&workflow_event).await {
                tracing::error!(event_id = ?workflow_event.event.id, "Workflow trigger failed: {e}");
            } else {
                metrics::counter!("sprout_workflow_runs_total", "trigger" => trigger_kind)
                    .increment(1);
            }
        });
    }

    matches.len()
}

/// Handle an EVENT message from a WebSocket connection.
///
/// Extracts auth from the WS connection, dispatches ephemeral events locally,
/// and delegates persistent events to [`super::ingest::ingest_event`].
pub async fn handle_event(event: Event, conn: Arc<ConnectionState>, state: Arc<AppState>) {
    let start = std::time::Instant::now();
    let event_id_hex = event.id.to_hex();
    let kind_u32 = event_kind_u32(&event);
    let kind_str = bounded_kind_label(kind_u32);
    debug!(event_id = %event_id_hex, kind = kind_u32, "EVENT");
    metrics::counter!("sprout_events_received_total", "kind" => kind_str.clone()).increment(1);

    // ── Extract auth from WS connection state ────────────────────────────
    let (conn_id, pubkey_bytes, auth_pubkey, scopes, channel_ids) = {
        let auth = conn.auth_state.read().await;
        match &*auth {
            AuthState::Authenticated(ctx) => (
                conn.conn_id,
                ctx.pubkey.serialize().to_vec(),
                ctx.pubkey,
                ctx.scopes.clone(),
                ctx.channel_ids.clone(),
            ),
            _ => {
                reject("auth");
                conn.send(RelayMessage::ok(
                    &event_id_hex,
                    false,
                    "auth-required: not authenticated",
                ));
                return;
            }
        }
    };

    // ── Pubkey / auth identity match (all events) ─────────────────────
    // Must run before both ephemeral and persistent branches. Persistent
    // events get a second check inside ingest_event() (step 3), but
    // ephemeral events bypass the pipeline entirely.
    let has_proxy_scope = scopes.contains(&sprout_auth::Scope::ProxySubmit);
    let is_gift_wrap = kind_u32 == KIND_GIFT_WRAP;
    if event.pubkey != auth_pubkey && !has_proxy_scope && !is_gift_wrap {
        reject("invalid");
        conn.send(RelayMessage::ok(
            &event_id_hex,
            false,
            "invalid: event pubkey does not match authenticated identity",
        ));
        return;
    }

    // ── Blocked kinds (both ephemeral and persistent) ─────────────────
    if kind_u32 == sprout_core::kind::KIND_AUTH {
        reject("invalid");
        conn.send(RelayMessage::ok(
            &event_id_hex,
            false,
            "invalid: AUTH events cannot be submitted via EVENT",
        ));
        return;
    }

    // ── Ephemeral events are WS-only (never stored) ──────────────────────
    // Scope enforcement for ephemeral kinds: require MessagesWrite or
    // ProxySubmit. Persistent events skip this gate and rely on
    // ingest_event()'s per-kind scope allowlist instead, so a token with
    // only ChannelsWrite can still submit kind:9002 via WS.
    if is_ephemeral(kind_u32) {
        if !scopes.is_empty()
            && !scopes.contains(&sprout_auth::Scope::MessagesWrite)
            && !has_proxy_scope
        {
            reject("scope");
            conn.send(RelayMessage::ok(
                &event_id_hex,
                false,
                "restricted: insufficient scope for ephemeral events",
            ));
            return;
        }
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

    // ── Persistent events → ingest pipeline ──────────────────────────────
    let ingest_auth = IngestAuth::Nip42 {
        pubkey: auth_pubkey,
        scopes,
        channel_ids,
        conn_id,
    };

    match super::ingest::ingest_event(&state, event, ingest_auth).await {
        Ok(result) => {
            if result.accepted {
                metrics::counter!("sprout_events_stored_total", "kind" => kind_str).increment(1);
                info!(
                    event_id = %result.event_id,
                    kind = kind_u32,
                    conn_id = %conn_id,
                    "Event ingested"
                );
            }
            metrics::histogram!("sprout_event_processing_seconds")
                .record(start.elapsed().as_secs_f64());
            conn.send(RelayMessage::ok(
                &result.event_id,
                result.accepted,
                &result.message,
            ));
        }
        Err(e) => {
            // Sanitize internal errors — don't leak DB/system details over WS.
            let (msg, reason) = match &e {
                IngestError::Rejected(m) => (m.clone(), "invalid"),
                IngestError::AuthFailed(m) => (m.clone(), "auth"),
                IngestError::Internal(_) => ("error: internal server error".to_string(), "error"),
            };
            reject(reason);
            conn.send(RelayMessage::ok(&event_id_hex, false, &msg));
        }
    }
}

/// Handle ephemeral events (kind 20000–29999) — WS-only, never stored.
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

        if status == "offline" {
            let _ = state.pubsub.clear_presence(&auth_pubkey).await;
        } else {
            let _ = state.pubsub.set_presence(&auth_pubkey, &status).await;
        }

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

    // Check channel membership before publishing other ephemeral events.
    if let Some(ch_id) = super::ingest::extract_channel_id(&event) {
        if let Err(msg) =
            super::ingest::check_channel_membership(&state, ch_id, &pubkey_bytes).await
        {
            conn.send(RelayMessage::ok(event_id_hex, false, &msg));
            return;
        }

        // Mark as local before Redis publish to prevent double-delivery when
        // the event comes back through the Redis subscriber loop.
        state.mark_local_event(&event.id);

        if let Err(e) = state.pubsub.publish_event(ch_id, &event).await {
            state.local_event_ids.invalidate(&event.id.to_bytes());
            warn!(conn_id = %conn_id, event_id = %event_id_hex, "Ephemeral publish failed: {e}");
        }

        // Direct fan-out to local WS subscribers.
        // Pass the channel_id so fan_out() uses the channel-kind index.
        let stored_event = StoredEvent::new(event.clone(), Some(ch_id));
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
    }

    conn.send(RelayMessage::ok(event_id_hex, true, ""));
}

#[cfg(test)]
mod tests {
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
                super::super::ingest::requires_h_channel_scope(kind),
                "kind {kind} should require h"
            );
        }
    }

    #[test]
    fn non_channel_kinds_do_not_require_h_tags() {
        assert!(
            !super::super::ingest::requires_h_channel_scope(nostr::Kind::Reaction.as_u16().into()),
            "reactions derive channel from the target event"
        );
        assert!(
            !super::super::ingest::requires_h_channel_scope(KIND_PRESENCE_UPDATE),
            "presence updates are global/ephemeral"
        );
    }
}
