//! Relay-side implementation of [`ActionSink`] for workflow actions.
//!
//! Builds Nostr events, persists them, and delegates post-persist side effects
//! (WebSocket fan-out, Redis pub/sub, search indexing, audit logging) to the
//! existing [`dispatch_persistent_event`] helper.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Weak};

use chrono::Utc;
use nostr::{EventBuilder, Kind, Tag};
use sprout_core::kind::{KIND_REACTION, KIND_STREAM_MESSAGE};
use sprout_workflow::action_sink::{ActionSink, ActionSinkError};
use tracing::info;
use uuid::Uuid;

use crate::handlers::event::dispatch_persistent_event;
use crate::state::AppState;

/// Relay-side action sink — executes workflow side-effects directly.
///
/// Holds a **weak** reference to `AppState` to avoid an `Arc` reference cycle:
/// `AppState` → `WorkflowEngine` → `ActionSink` → `AppState`. Using `Weak`
/// breaks the cycle so all structs can be dropped on shutdown.
///
/// Post-persist side effects are delegated to [`dispatch_persistent_event`]
/// for consistency with the REST/WebSocket paths.
pub struct RelayActionSink {
    state: Weak<AppState>,
}

impl RelayActionSink {
    /// Create a new `RelayActionSink` from the shared application state.
    pub fn new(state: &Arc<AppState>) -> Self {
        Self {
            state: Arc::downgrade(state),
        }
    }
}

impl ActionSink for RelayActionSink {
    fn send_message(
        &self,
        channel_id: &str,
        text: &str,
        author_pubkey: &str,
    ) -> Pin<Box<dyn Future<Output = Result<String, ActionSinkError>> + Send + '_>> {
        let channel_id = channel_id.to_owned();
        let text = text.to_owned();
        let author_pubkey = author_pubkey.to_owned();

        Box::pin(async move {
            // 0. Upgrade weak reference — fails only during shutdown.
            let state = self
                .state
                .upgrade()
                .ok_or_else(|| ActionSinkError::Database("relay is shutting down".into()))?;

            // 1. Validate content is not empty/whitespace-only
            if text.trim().is_empty() {
                return Err(ActionSinkError::EmptyContent);
            }

            // 2. Parse and validate channel — canonicalize UUID immediately
            let channel_uuid = Uuid::parse_str(&channel_id)
                .map_err(|e| ActionSinkError::InvalidInput(format!("invalid UUID: {e}")))?;
            let channel_id_canonical = channel_uuid.to_string();

            let channel = state
                .db
                .get_channel(channel_uuid)
                .await
                .map_err(|e| match &e {
                    sprout_db::DbError::ChannelNotFound(_) | sprout_db::DbError::NotFound(_) => {
                        ActionSinkError::ChannelNotFound(channel_id_canonical.clone())
                    }
                    _ => ActionSinkError::Database(e.to_string()),
                })?;

            if channel.archived_at.is_some() {
                return Err(ActionSinkError::ChannelArchived(
                    channel_id_canonical.clone(),
                ));
            }

            // 3. Build kind:9 Nostr event
            //    - Signed by relay keypair (event.pubkey = relay pubkey)
            //    - `p` tag attributes the message to the workflow owner
            //    - `h` tag scopes to the channel (NIP-29, canonical UUID)
            //    - `sprout:workflow` tag prevents recursive workflow triggering
            let tags = vec![
                Tag::parse(&["p", &author_pubkey])
                    .map_err(|e| ActionSinkError::EventBuild(format!("p tag: {e}")))?,
                Tag::parse(&["h", &channel_id_canonical])
                    .map_err(|e| ActionSinkError::EventBuild(format!("h tag: {e}")))?,
                Tag::parse(&["sprout:workflow", "true"])
                    .map_err(|e| ActionSinkError::EventBuild(format!("workflow tag: {e}")))?,
            ];

            let kind = Kind::from(KIND_STREAM_MESSAGE as u16);
            let event = EventBuilder::new(kind, &text, tags)
                .sign_with_keys(&state.relay_keypair)
                .map_err(|e| ActionSinkError::EventBuild(format!("signing: {e}")))?;

            let event_id_hex = event.id.to_hex();
            let event_id_bytes = event.id.as_bytes().to_vec();
            let kind_u32 = KIND_STREAM_MESSAGE;

            let event_created_at = {
                let ts = event.created_at.as_u64() as i64;
                chrono::DateTime::from_timestamp(ts, 0).unwrap_or_else(Utc::now)
            };

            info!(
                event_id = %event_id_hex,
                channel_id = %channel_id_canonical,
                author = %author_pubkey,
                "Workflow SendMessage: posting kind {kind_u32} event"
            );

            // 4. Persist event with thread metadata (matches REST handler path).
            //    Workflow messages are always top-level: depth=0, no parent/root.
            let thread_meta = Some(sprout_db::event::ThreadMetadataParams {
                event_id: &event_id_bytes,
                event_created_at,
                channel_id: channel_uuid,
                parent_event_id: None,
                parent_event_created_at: None,
                root_event_id: None,
                root_event_created_at: None,
                depth: 0,
                broadcast: false,
            });

            let (stored_event, was_inserted) = state
                .db
                .insert_event_with_thread_metadata(&event, Some(channel_uuid), thread_meta)
                .await
                .map_err(|e| ActionSinkError::Database(e.to_string()))?;

            // 5. Post-persist side effects (fan-out, search, audit)
            //    Only if actually inserted (idempotency guard).
            if was_inserted {
                let _ = dispatch_persistent_event(&state, &stored_event, kind_u32, &author_pubkey)
                    .await;
            }

            Ok(event_id_hex)
        })
    }

    fn add_reaction(
        &self,
        channel_id: &str,
        message_id: &str,
        emoji: &str,
        author_pubkey: &str,
    ) -> Pin<Box<dyn Future<Output = Result<String, ActionSinkError>> + Send + '_>> {
        let channel_id = channel_id.to_owned();
        let message_id = message_id.to_owned();
        let emoji = emoji.to_owned();
        let author_pubkey = author_pubkey.to_owned();

        Box::pin(async move {
            // 0. Upgrade weak reference — fails only during shutdown.
            let state = self
                .state
                .upgrade()
                .ok_or_else(|| ActionSinkError::Database("relay is shutting down".into()))?;

            // 1. Validate inputs.
            if emoji.is_empty() {
                return Err(ActionSinkError::InvalidInput(
                    "emoji must not be empty".into(),
                ));
            }
            if message_id.len() != 64 || !message_id.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(ActionSinkError::InvalidInput(format!(
                    "invalid message_id hex: {message_id}"
                )));
            }

            // 2. Decode message_id and look up target event.
            let target_id_bytes = hex::decode(&message_id)
                .map_err(|e| ActionSinkError::InvalidInput(format!("hex decode: {e}")))?;

            let target_event = state
                .db
                .get_event_by_id(&target_id_bytes)
                .await
                .map_err(|e| ActionSinkError::Database(format!("get_event_by_id: {e}")))?
                .ok_or_else(|| {
                    ActionSinkError::InvalidInput(format!(
                        "reaction target event not found: {message_id}"
                    ))
                })?;

            let target_created_at =
                chrono::DateTime::from_timestamp(target_event.event.created_at.as_u64() as i64, 0)
                    .unwrap_or_else(Utc::now);

            // 3. Resolve channel UUID — use provided channel_id if non-empty,
            //    otherwise derive from the target event.
            let channel_uuid = if !channel_id.is_empty() {
                Uuid::parse_str(&channel_id)
                    .map_err(|e| ActionSinkError::InvalidInput(format!("invalid UUID: {e}")))?
            } else {
                target_event.channel_id.ok_or_else(|| {
                    ActionSinkError::InvalidInput(
                        "no channel_id provided and target event has no channel".into(),
                    )
                })?
            };

            // 4. Decode author pubkey.
            let actor_bytes = hex::decode(&author_pubkey).map_err(|e| {
                ActionSinkError::InvalidInput(format!("invalid author_pubkey hex: {e}"))
            })?;

            // 5. Build NIP-25 kind:7 reaction event.
            let tags = vec![
                Tag::parse(&["e", &message_id])
                    .map_err(|e| ActionSinkError::EventBuild(format!("e tag: {e}")))?,
                Tag::parse(&["p", &author_pubkey])
                    .map_err(|e| ActionSinkError::EventBuild(format!("p tag: {e}")))?,
                Tag::parse(&["h", &channel_uuid.to_string()])
                    .map_err(|e| ActionSinkError::EventBuild(format!("h tag: {e}")))?,
                Tag::parse(&["sprout:workflow", "true"])
                    .map_err(|e| ActionSinkError::EventBuild(format!("workflow tag: {e}")))?,
            ];

            let kind = Kind::from(KIND_REACTION as u16);
            let event = EventBuilder::new(kind, &emoji, tags)
                .sign_with_keys(&state.relay_keypair)
                .map_err(|e| ActionSinkError::EventBuild(format!("signing: {e}")))?;

            let event_id_hex = event.id.to_hex();

            info!(
                event_id = %event_id_hex,
                target = %message_id,
                channel_id = %channel_uuid,
                author = %author_pubkey,
                emoji = %emoji,
                "Workflow AddReaction: posting kind {KIND_REACTION} event"
            );

            // 6. Dedup — add_reaction returns false if already exists.
            let inserted = state
                .db
                .add_reaction(
                    &target_id_bytes,
                    target_created_at,
                    &actor_bytes,
                    &emoji,
                    None,
                )
                .await
                .map_err(|e| ActionSinkError::Database(format!("add_reaction: {e}")))?;

            if !inserted {
                return Ok(event_id_hex);
            }

            // 7. Persist the event — no thread metadata needed for reactions.
            let (stored_event, was_inserted) = match state
                .db
                .insert_event_with_thread_metadata(&event, Some(channel_uuid), None)
                .await
            {
                Ok(result) => result,
                Err(e) => {
                    // Compensate: undo the reaction row so state stays consistent.
                    if let Err(re) = state
                        .db
                        .remove_reaction(&target_id_bytes, target_created_at, &actor_bytes, &emoji)
                        .await
                    {
                        tracing::warn!(
                            event_id = %event_id_hex,
                            "reaction compensation failed: {re}"
                        );
                    }
                    return Err(ActionSinkError::Database(format!("insert_event: {e}")));
                }
            };

            // 8. Backfill reaction_event_id.
            if was_inserted {
                if let Err(e) = state
                    .db
                    .set_reaction_event_id(
                        &target_id_bytes,
                        target_created_at,
                        &actor_bytes,
                        &emoji,
                        event.id.as_bytes(),
                    )
                    .await
                {
                    tracing::warn!(
                        event_id = %event_id_hex,
                        "set_reaction_event_id failed: {e}"
                    );
                }
            }

            // 9. Fan-out side effects.
            if was_inserted {
                let _ =
                    dispatch_persistent_event(&state, &stored_event, KIND_REACTION, &author_pubkey)
                        .await;
            }

            Ok(event_id_hex)
        })
    }
}
