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
//! let engine = WorkflowEngine::new(db, WorkflowConfig::default());
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

use std::sync::Arc;

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
    /// Full trigger matching and execution spawning is wired in WF-07/08.
    pub async fn on_event(&self, event: &sprout_core::StoredEvent) -> Result<(), WorkflowError> {
        let Some(channel_id) = event.channel_id else {
            return Ok(());
        };

        let kind_u32 = event.event.kind.as_u16() as u32;

        // Exclude workflow execution events to prevent infinite loops.
        // See Decision 10 in PLANS/SPROUT_WORKFLOWS.md.
        if (46001..=46012).contains(&kind_u32) {
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

            // TODO (WF-07): evaluate trigger filter expression, create workflow_run
            // in DB, build TriggerContext from event, spawn execute_run().
            tracing::debug!(
                workflow_id = %workflow.id,
                event_kind = kind_u32,
                "Workflow trigger matched — execution wired in WF-07"
            );
        }

        Ok(())
    }

    /// Background task for scheduled (cron) triggers.
    ///
    /// Runs indefinitely. Checks cron schedules every minute and fires
    /// matching workflows.
    ///
    /// TODO (WF-07): implement cron schedule matching and execution.
    pub async fn run(&self) {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            // TODO (WF-07): load schedule-triggered workflows, check cron expressions,
            // spawn executions for any that are due.
            tracing::debug!("WorkflowEngine::run tick — cron check (not yet implemented)");
        }
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
        assert!(!trigger_matches_event(&trigger, 46001));
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

        for kind in 46001u32..=46012 {
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
}
