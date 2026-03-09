#![deny(unsafe_code)]
#![warn(missing_docs)]
//! `sprout-workflow` — Workflow engine for Sprout.
//!
//! Channel-scoped automations with sequential execution, variable substitution,
//! conditional logic, and execution traces.
//!
//! ## Architecture
//!
//! - [`WorkflowEngine`] — top-level handle; lives in `AppState`
//! - [`schema`] — YAML/JSON definition types (`WorkflowDef`, `TriggerDef`, `ActionDef`, `Step`)
//! - [`executor`] — sequential execution, template resolution, condition evaluation
//! - [`error`] — [`WorkflowError`] enum
//!
//! ## Usage
//!
//! ```rust,ignore
//! let engine = Arc::new(WorkflowEngine::new(db, WorkflowConfig::default()));
//!
//! // Parse and validate a YAML definition.
//! let (def, json) = WorkflowEngine::parse_yaml(yaml_str)?;
//!
//! // React to an incoming event (called from event handler post-store hook).
//! engine.on_event(&stored_event).await?;
//!
//! // Run the background scheduler (cron triggers).
//! tokio::spawn(async move { engine.run().await });
//! ```

pub mod error;
pub mod executor;
pub mod schema;

pub use error::WorkflowError;
pub use executor::ExecutionResult;
pub use schema::{ActionDef, Step, TriggerDef, WorkflowDef};

use std::collections::HashMap;
use std::sync::Arc;

use sprout_core::kind::{event_kind_u32, is_workflow_execution_kind, KIND_REACTION};
use sprout_db::workflow::RunStatus;
use sprout_db::Db;
use tokio::sync::Semaphore;

// ── Configuration ─────────────────────────────────────────────────────────────

/// Runtime configuration for the workflow engine.
#[derive(Clone, Debug)]
pub struct WorkflowConfig {
    /// Maximum number of concurrently executing workflow runs. Default: 100.
    pub max_concurrent: usize,
    /// Default per-step timeout in seconds. Default: 300 (5 minutes).
    pub default_timeout_secs: u64,
}

impl Default for WorkflowConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 100,
            default_timeout_secs: 300,
        }
    }
}

// ── Engine ────────────────────────────────────────────────────────────────────

/// The workflow engine. Clone is cheap (Arc-backed DB pool + semaphore).
pub struct WorkflowEngine {
    pub(crate) db: Db,
    pub(crate) config: WorkflowConfig,
    /// Semaphore enforcing `config.max_concurrent` simultaneous workflow runs.
    pub(crate) run_semaphore: Arc<Semaphore>,
}

impl WorkflowEngine {
    /// Create a new `WorkflowEngine`.
    pub fn new(db: Db, config: WorkflowConfig) -> Self {
        let permits = config.max_concurrent.max(1);
        let run_semaphore = Arc::new(Semaphore::new(permits));
        Self {
            db,
            config,
            run_semaphore,
        }
    }

    /// Parse and validate a YAML workflow definition.
    ///
    /// Returns `(WorkflowDef, canonical_json)` on success. The canonical JSON
    /// is suitable for storage in the `definition` column.
    pub fn parse_yaml(yaml: &str) -> Result<(WorkflowDef, String), WorkflowError> {
        schema::parse_yaml(yaml)
    }

    /// Called from the event handler post-store hook for every stored event.
    ///
    /// Checks whether any workflow in the event's channel has a matching trigger.
    /// Workflow execution events (kinds 46001–46012) are excluded to prevent loops.
    ///
    /// For each matching workflow:
    /// 1. Evaluates the trigger filter expression (if present).
    /// 2. Builds a [`executor::TriggerContext`] from the event.
    /// 3. Creates a `workflow_run` row in the DB (status: `pending`).
    /// 4. Spawns an async task to execute the run via [`executor::execute_run`].
    ///
    /// The method takes `self: &Arc<Self>` so that the spawned task can hold a
    /// clone of the `Arc` without requiring `'static` on `&self`.
    pub async fn on_event(
        self: &Arc<Self>,
        event: &sprout_core::StoredEvent,
    ) -> Result<(), WorkflowError> {
        let Some(channel_id) = event.channel_id else {
            tracing::debug!(
                event_id = %event.event.id.to_hex(),
                kind = event_kind_u32(&event.event),
                "Skipping workflow trigger — event has no channel_id"
            );
            return Ok(());
        };

        let kind_u32 = event_kind_u32(&event.event);

        // Exclude workflow execution events to prevent infinite loops.
        // See Decision 10 in PLANS/SPROUT_WORKFLOWS.md.
        if is_workflow_execution_kind(kind_u32) {
            return Ok(());
        }

        // Load enabled workflows for this channel.
        let workflows = self
            .db
            .list_enabled_channel_workflows(channel_id)
            .await
            .map_err(WorkflowError::from)?;

        if workflows.is_empty() {
            return Ok(());
        }

        // Build TriggerContext once — all matching workflows in this channel
        // share the same triggering event.
        let trigger_ctx = build_trigger_context(event);

        for workflow in &workflows {
            // Parse the stored JSON definition.
            let def: WorkflowDef = match serde_json::from_value(workflow.definition.clone()) {
                Ok(d) => d,
                Err(e) => {
                    tracing::warn!(
                        workflow_id = %workflow.id,
                        "Failed to parse workflow definition: {e}"
                    );
                    continue;
                }
            };

            if !def.enabled {
                continue;
            }

            // Check if the trigger type matches the event kind.
            if !trigger_matches_event(&def.trigger, kind_u32) {
                continue;
            }

            // Enforce reaction emoji filter: if the workflow specifies a specific
            // emoji, skip events whose content doesn't match. NIP-25 stores the
            // emoji character (or shortcode) in the event content field.
            if let TriggerDef::ReactionAdded {
                emoji: Some(ref expected),
            } = def.trigger
            {
                let actual = &trigger_ctx.emoji;
                if actual != expected {
                    tracing::debug!(
                        workflow_id = %workflow.id,
                        expected_emoji = %expected,
                        actual_emoji = %actual,
                        "Reaction emoji mismatch — skipping workflow"
                    );
                    continue;
                }
            }

            // Evaluate the trigger filter expression (MessagePosted only).
            // A filter that evaluates to false skips this workflow entirely.
            if let TriggerDef::MessagePosted {
                filter: Some(ref expr),
            } = def.trigger
            {
                match executor::evaluate_condition(expr, &trigger_ctx, &HashMap::new()).await {
                    Ok(true) => {}
                    Ok(false) => {
                        tracing::debug!(
                            workflow_id = %workflow.id,
                            "Trigger filter evaluated false — skipping workflow"
                        );
                        continue;
                    }
                    Err(e) => {
                        tracing::warn!(
                            workflow_id = %workflow.id,
                            "Trigger filter error: {e} — skipping workflow"
                        );
                        continue;
                    }
                }
            }

            // Serialize TriggerContext for DB storage.
            let trigger_ctx_json = match serde_json::to_value(&trigger_ctx) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(
                        workflow_id = %workflow.id,
                        "Failed to serialize TriggerContext: {e}"
                    );
                    continue;
                }
            };

            // Create the workflow_run row (status: pending).
            let trigger_event_id_bytes = event.event.id.as_bytes().to_vec();
            let run_id = match self
                .db
                .create_workflow_run(
                    workflow.id,
                    Some(&trigger_event_id_bytes),
                    Some(&trigger_ctx_json),
                )
                .await
            {
                Ok(id) => id,
                Err(e) => {
                    tracing::error!(
                        workflow_id = %workflow.id,
                        "Failed to create workflow_run: {e}"
                    );
                    continue;
                }
            };

            tracing::debug!(
                workflow_id = %workflow.id,
                run_id = %run_id,
                "Workflow triggered — spawning execution"
            );

            // Spawn execution. Clone Arc so the task owns its references.
            let engine = Arc::clone(self);
            let def_clone = def.clone();
            let ctx_clone = trigger_ctx.clone();

            tokio::spawn(async move {
                match executor::execute_run(&engine, run_id, &def_clone, &ctx_clone).await {
                    Ok(result) => {
                        let trace_json = match serde_json::to_value(&result.trace) {
                            Ok(v) => v,
                            Err(e) => {
                                tracing::warn!(run_id = %run_id, "Failed to serialize trace: {e}");
                                serde_json::json!([])
                            }
                        };
                        let step_count = result.step_index as i32;

                        if result.approval_token.is_some() {
                            // Approval gates are not yet implemented (WF-08).
                            // Fail explicitly rather than creating unreachable WaitingApproval rows.
                            tracing::warn!(
                                run_id = %run_id,
                                step_index = result.step_index,
                                "Workflow hit approval gate — not yet implemented, marking as failed"
                            );
                            let _ = engine
                                .db
                                .update_workflow_run(
                                    run_id,
                                    RunStatus::Failed,
                                    step_count,
                                    &trace_json,
                                    Some("approval gates not yet implemented — see WF-08"),
                                )
                                .await;
                        } else {
                            // Normal completion.
                            tracing::info!(run_id = %run_id, "Workflow run completed");
                            if let Err(e) = engine
                                .db
                                .update_workflow_run(
                                    run_id,
                                    RunStatus::Completed,
                                    step_count,
                                    &trace_json,
                                    None,
                                )
                                .await
                            {
                                tracing::error!(
                                    run_id = %run_id,
                                    "Failed to update run to Completed: {e}"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(run_id = %run_id, "Workflow run failed: {e}");
                        if let Err(db_err) = engine
                            .db
                            .update_workflow_run(
                                run_id,
                                RunStatus::Failed,
                                0,
                                &serde_json::json!([]),
                                Some(&e.to_string()),
                            )
                            .await
                        {
                            tracing::error!(
                                run_id = %run_id,
                                "Failed to update run to Failed: {db_err}"
                            );
                        }
                    }
                }
            });
        }

        Ok(())
    }

    /// Background task for scheduled (cron) triggers.
    ///
    /// Runs indefinitely. Cron trigger matching requires a cross-channel
    /// workflow query (`list_all_enabled_workflows`) that doesn't exist yet.
    /// Interval triggers need last-run tracking. Both are deferred to WF-09.
    pub async fn run(&self) {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            // Cron trigger matching requires a cross-channel workflow query
            // (list_all_enabled_workflows) that doesn't exist yet. Interval
            // triggers need last-run tracking. Both are deferred to WF-09.
            tracing::trace!("WorkflowEngine::run tick — cron/interval triggers not yet wired");
        }
    }
}

// ── Trigger context builder ───────────────────────────────────────────────────

/// Build a [`executor::TriggerContext`] from a [`sprout_core::StoredEvent`].
///
/// - `text` — event content (message body or reaction emoji character)
/// - `author` — pubkey hex string
/// - `channel_id` — channel UUID as string (empty if no channel scope)
/// - `timestamp` — Unix timestamp as string
/// - `emoji` — for `KIND_REACTION` events, the content is the emoji; otherwise empty
/// - `message_id` — for reactions, the target message's event ID (from `e` tag);
///   for all other events, the event's own ID
pub fn build_trigger_context(event: &sprout_core::StoredEvent) -> executor::TriggerContext {
    let kind_u32 = event_kind_u32(&event.event);
    let content = event.event.content.clone();

    // For reaction events (NIP-25), the content field holds the emoji character
    // or shortcode (e.g. "👍", "+", "-"). Expose it as `emoji`.
    let emoji = if kind_u32 == KIND_REACTION {
        content.clone()
    } else {
        String::new()
    };

    // For reactions (NIP-25), `message_id` should be the target message, not
    // the reaction event itself. NIP-25 stores the target in an `e` tag whose
    // value is a 64-char hex event ID (not a UUID channel reference).
    // Per NIP-25, the last `e` tag is the direct target (earlier ones may be thread roots).
    let message_id = if kind_u32 == KIND_REACTION {
        event
            .event
            .tags
            .iter()
            .rev()
            .find_map(|tag| {
                let key = tag.kind().to_string();
                if key == "e" {
                    tag.content().and_then(|v| {
                        // Distinguish hex event IDs (64 chars) from UUID channel refs.
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
            // Fallback to the reaction event's own ID if no valid `e` tag found.
            .unwrap_or_else(|| event.event.id.to_hex())
    } else {
        event.event.id.to_hex()
    };

    executor::TriggerContext {
        text: content,
        author: event.event.pubkey.to_hex(),
        channel_id: event
            .channel_id
            .map(|id| id.to_string())
            .unwrap_or_default(),
        timestamp: event.event.created_at.as_u64().to_string(),
        emoji,
        message_id,
        webhook_fields: HashMap::new(),
    }
}

// ── Trigger matching ──────────────────────────────────────────────────────────

/// Returns `true` if the trigger type matches the given event kind.
fn trigger_matches_event(trigger: &TriggerDef, kind_u32: u32) -> bool {
    use sprout_core::kind::{KIND_REACTION, KIND_STREAM_MESSAGE};
    match trigger {
        TriggerDef::MessagePosted { .. } => kind_u32 == KIND_STREAM_MESSAGE,
        TriggerDef::ReactionAdded { .. } => kind_u32 == KIND_REACTION,
        // Schedule and Webhook triggers are not fired by channel events.
        TriggerDef::Schedule { .. } | TriggerDef::Webhook => false,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_config_defaults() {
        let cfg = WorkflowConfig::default();
        assert_eq!(cfg.max_concurrent, 100);
        assert_eq!(cfg.default_timeout_secs, 300);
    }

    #[test]
    fn parse_yaml_roundtrip() {
        let yaml = r#"
name: "Test Workflow"
trigger:
  on: message_posted
steps:
  - id: s1
    action: send_message
    text: "Hello {{trigger.author}}"
"#;
        let (def, json) = WorkflowEngine::parse_yaml(yaml).expect("parse failed");
        assert_eq!(def.name, "Test Workflow");

        // JSON must round-trip.
        let reparsed: WorkflowDef = serde_json::from_str(&json).expect("json round-trip");
        assert_eq!(reparsed.name, def.name);
        assert_eq!(reparsed.steps.len(), 1);
    }

    #[test]
    fn trigger_matches_stream_message() {
        let trigger = TriggerDef::MessagePosted { filter: None };
        assert!(trigger_matches_event(
            &trigger,
            sprout_core::kind::KIND_STREAM_MESSAGE
        ));
        assert!(!trigger_matches_event(
            &trigger,
            sprout_core::kind::KIND_REACTION
        ));
    }

    #[test]
    fn trigger_matches_reaction() {
        let trigger = TriggerDef::ReactionAdded { emoji: None };
        assert!(trigger_matches_event(
            &trigger,
            sprout_core::kind::KIND_REACTION
        ));
        assert!(!trigger_matches_event(
            &trigger,
            sprout_core::kind::KIND_STREAM_MESSAGE
        ));
    }

    #[test]
    fn schedule_trigger_never_matches_events() {
        let trigger = TriggerDef::Schedule {
            cron: Some("0 9 * * 1-5".to_owned()),
            interval: None,
        };
        // Schedule triggers are fired by the cron loop, not by events.
        assert!(!trigger_matches_event(
            &trigger,
            sprout_core::kind::KIND_STREAM_MESSAGE
        ));
        assert!(!trigger_matches_event(
            &trigger,
            sprout_core::kind::KIND_REACTION
        ));
        assert!(!trigger_matches_event(
            &trigger,
            sprout_core::kind::KIND_WORKFLOW_TRIGGERED
        ));
    }

    #[test]
    fn webhook_trigger_never_matches_events() {
        let trigger = TriggerDef::Webhook;
        assert!(!trigger_matches_event(
            &trigger,
            sprout_core::kind::KIND_STREAM_MESSAGE
        ));
        assert!(!trigger_matches_event(&trigger, 0));
    }

    // ── Trigger matching edge cases ───────────────────────────────────────────

    #[test]
    fn message_posted_matches_kind_40001_only() {
        let trigger = TriggerDef::MessagePosted { filter: None };
        // Must match KIND_STREAM_MESSAGE = 40001.
        assert!(trigger_matches_event(&trigger, 40001));
        // Must NOT match reaction (kind 7).
        assert!(!trigger_matches_event(&trigger, 7));
        // Must NOT match forum post (kind 45001).
        assert!(!trigger_matches_event(&trigger, 45001));
        // Must NOT match stream message v2 (kind 40002).
        assert!(!trigger_matches_event(&trigger, 40002));
    }

    #[test]
    fn reaction_added_matches_kind_7_only() {
        let trigger = TriggerDef::ReactionAdded { emoji: None };
        // Must match KIND_REACTION = 7.
        assert!(trigger_matches_event(&trigger, 7));
        // Must NOT match stream message (kind 40001).
        assert!(!trigger_matches_event(&trigger, 40001));
        // Must NOT match forum post (kind 45001).
        assert!(!trigger_matches_event(&trigger, 45001));
    }

    #[test]
    fn reaction_added_with_emoji_filter_still_matches_kind_7() {
        // The emoji filter is evaluated at execution time, not trigger-matching time.
        // trigger_matches_event only checks the kind number.
        let trigger = TriggerDef::ReactionAdded {
            emoji: Some("thumbsup".to_owned()),
        };
        assert!(trigger_matches_event(&trigger, 7));
        assert!(!trigger_matches_event(&trigger, 40001));
    }

    #[test]
    fn message_posted_with_filter_still_matches_kind_40001() {
        // The filter expression is evaluated at execution time, not trigger-matching time.
        let trigger = TriggerDef::MessagePosted {
            filter: Some("str_contains(trigger_text, \"P1\")".to_owned()),
        };
        assert!(trigger_matches_event(&trigger, 40001));
        assert!(!trigger_matches_event(&trigger, 7));
    }

    #[test]
    fn workflow_execution_kinds_do_not_match_any_trigger() {
        // Workflow execution events (46001–46012) must never match triggers
        // to prevent infinite loops. The on_event() method filters these out
        // before calling trigger_matches_event, but verify the function itself
        // also returns false for these kinds.
        let msg_trigger = TriggerDef::MessagePosted { filter: None };
        let react_trigger = TriggerDef::ReactionAdded { emoji: None };

        for kind in sprout_core::kind::KIND_WORKFLOW_TRIGGERED
            ..=sprout_core::kind::KIND_WORKFLOW_APPROVAL_DENIED
        {
            assert!(
                !trigger_matches_event(&msg_trigger, kind),
                "message_posted should not match workflow execution kind {kind}"
            );
            assert!(
                !trigger_matches_event(&react_trigger, kind),
                "reaction_added should not match workflow execution kind {kind}"
            );
        }
    }

    #[test]
    fn trigger_matches_event_kind_zero_matches_nothing() {
        // Kind 0 is a profile event — no trigger should match it.
        let msg_trigger = TriggerDef::MessagePosted { filter: None };
        let react_trigger = TriggerDef::ReactionAdded { emoji: None };
        let sched_trigger = TriggerDef::Schedule {
            cron: None,
            interval: Some("1h".to_owned()),
        };
        let webhook_trigger = TriggerDef::Webhook;

        assert!(!trigger_matches_event(&msg_trigger, 0));
        assert!(!trigger_matches_event(&react_trigger, 0));
        assert!(!trigger_matches_event(&sched_trigger, 0));
        assert!(!trigger_matches_event(&webhook_trigger, 0));
    }

    #[test]
    fn workflow_config_custom_values() {
        let cfg = WorkflowConfig {
            max_concurrent: 50,
            default_timeout_secs: 600,
        };
        assert_eq!(cfg.max_concurrent, 50);
        assert_eq!(cfg.default_timeout_secs, 600);
    }

    // ── build_trigger_context ─────────────────────────────────────────────────

    fn make_message_event() -> sprout_core::StoredEvent {
        use nostr::{EventBuilder, Keys, Kind};
        use uuid::Uuid;
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::Custom(40001), "hello world", [])
            .sign_with_keys(&keys)
            .expect("sign");
        sprout_core::StoredEvent::new(event, Some(Uuid::new_v4()))
    }

    /// Create a reaction event with an `e` tag pointing to a target message.
    fn make_reaction_event() -> (sprout_core::StoredEvent, String) {
        use nostr::{EventBuilder, Keys, Kind, Tag};
        use uuid::Uuid;
        let keys = Keys::generate();
        // Create a dummy target message ID (64-char hex).
        let target_keys = Keys::generate();
        let target_event = EventBuilder::new(Kind::Custom(40001), "target msg", [])
            .sign_with_keys(&target_keys)
            .expect("sign target");
        let target_id_hex = target_event.id.to_hex();
        // NIP-25: reaction references the target via an `e` tag.
        let e_tag = Tag::parse(&["e", &target_id_hex]).expect("tag parse");
        let event = EventBuilder::new(Kind::Reaction, "👍", [e_tag])
            .sign_with_keys(&keys)
            .expect("sign");
        (
            sprout_core::StoredEvent::new(event, Some(Uuid::new_v4())),
            target_id_hex,
        )
    }

    #[test]
    fn build_trigger_context_message_event() {
        let stored = make_message_event();
        let ctx = build_trigger_context(&stored);

        assert_eq!(ctx.text, "hello world");
        assert_eq!(ctx.author, stored.event.pubkey.to_hex());
        assert_eq!(ctx.channel_id, stored.channel_id.unwrap().to_string());
        assert_eq!(ctx.timestamp, stored.event.created_at.as_u64().to_string());
        assert_eq!(ctx.message_id, stored.event.id.to_hex());
        // Non-reaction events have empty emoji.
        assert_eq!(ctx.emoji, "");
        assert!(ctx.webhook_fields.is_empty());
    }

    #[test]
    fn build_trigger_context_reaction_event() {
        let (stored, target_id_hex) = make_reaction_event();
        let ctx = build_trigger_context(&stored);

        // For reactions, content IS the emoji.
        assert_eq!(ctx.text, "👍");
        assert_eq!(ctx.emoji, "👍");
        assert_eq!(ctx.author, stored.event.pubkey.to_hex());
        // message_id should be the TARGET message, not the reaction event itself.
        assert_eq!(ctx.message_id, target_id_hex);
        assert_ne!(ctx.message_id, stored.event.id.to_hex());
        assert!(ctx.webhook_fields.is_empty());
    }

    #[test]
    fn build_trigger_context_no_channel_id() {
        use nostr::{EventBuilder, Keys, Kind};
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::Custom(40001), "msg", [])
            .sign_with_keys(&keys)
            .expect("sign");
        // channel_id = None (global/DM event)
        let stored = sprout_core::StoredEvent::new(event, None);
        let ctx = build_trigger_context(&stored);

        assert_eq!(ctx.channel_id, "");
        assert_eq!(ctx.text, "msg");
    }

    #[test]
    fn build_trigger_context_author_is_hex_pubkey() {
        let stored = make_message_event();
        let ctx = build_trigger_context(&stored);
        // Pubkey hex is 64 lowercase hex characters.
        assert_eq!(ctx.author.len(), 64);
        assert!(ctx.author.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn build_trigger_context_message_id_is_hex() {
        let stored = make_message_event();
        let ctx = build_trigger_context(&stored);
        // Event ID hex is 64 lowercase hex characters.
        assert_eq!(ctx.message_id.len(), 64);
        assert!(ctx.message_id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn build_trigger_context_timestamp_is_numeric_string() {
        let stored = make_message_event();
        let ctx = build_trigger_context(&stored);
        // Timestamp must parse as a u64.
        ctx.timestamp
            .parse::<u64>()
            .expect("timestamp should be a u64 string");
    }

    #[test]
    fn test_build_trigger_context_reaction_multiple_e_tags() {
        // NIP-25: last e tag is the direct target, first may be thread root
        use nostr::{EventBuilder, EventId, Keys, Kind, Tag};
        use uuid::Uuid;

        let keys = Keys::generate();
        let thread_root_id = EventId::all_zeros();
        let direct_target_id = EventId::from_byte_array([0x42; 32]);

        let event = EventBuilder::new(
            Kind::Reaction,
            "👍",
            [
                Tag::parse(&["e", &thread_root_id.to_hex()]).unwrap(),
                Tag::parse(&["e", &direct_target_id.to_hex()]).unwrap(),
            ],
        )
        .sign_with_keys(&keys)
        .expect("sign");

        let stored = sprout_core::StoredEvent::new(event, Some(Uuid::new_v4()));
        let ctx = build_trigger_context(&stored);

        // Should pick the LAST e tag (direct target), not the first (thread root)
        assert_eq!(ctx.message_id, direct_target_id.to_hex());
    }
}
