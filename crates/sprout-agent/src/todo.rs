//! Session todo list. Single tool, atomic full-list replace, in-memory only.
//!
//! Mental model: the LLM owns a tiny mutable list. The agent intercepts the
//! `todo` tool before MCP, validates, renders, and gates `end_turn` while
//! pending work remains. Three strikes → force-stop.
//!
//! Banner ownership: `render_list()` produces the bare list. `decorate()`
//! is the single place that prepends the banner to tool results, so
//! `handle_call()` returns `render_list()` (no banner) and lets the agent's
//! post-call `decorate()` add it once.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::types::ToolDef;

pub const TOOL_NAME: &str = "todo";
const MAX_ITEMS: usize = 50;
const MAX_ID: u32 = 9999;
const MAX_TITLE_CHARS: usize = 200;
const MAX_STRIKES: u32 = 3;
const WARN_PREFIX: &str = "⚠ Pending todos remain. Update the `todo` list (mark in_progress / completed) before ending the turn.\n\n";

const DESCRIPTION: &str = "Session task list with end-turn enforcement. Full-list atomic replace each call. Omit todos key to read. Max 50 items, IDs 0-9999, titles max 200 chars.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub id: u32,
    pub title: String,
    pub status: Status,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    todos: Option<Vec<Item>>,
}

#[derive(Debug)]
pub enum EndTurn {
    Allow,
    Continue(String),
    Stop(String),
}

#[derive(Debug)]
pub struct Todos {
    enabled: bool,
    items: Vec<Item>,
    strikes: u32,
    /// Number of completed items at the last strike. Strikes only reset
    /// when the count of completed items INCREASES — actual progress.
    /// Reordering, retitling completed items, or any other shuffle that
    /// doesn't move work forward will not reset strikes.
    last_strike_completed_count: Option<usize>,
}

impl Todos {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            items: Vec::new(),
            strikes: 0,
            last_strike_completed_count: None,
        }
    }

    fn completed_count(&self) -> usize {
        self.items
            .iter()
            .filter(|i| i.status == Status::Completed)
            .count()
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn tool_def(&self) -> Option<ToolDef> {
        if !self.enabled {
            return None;
        }
        Some(ToolDef {
            name: TOOL_NAME.into(),
            description: DESCRIPTION.into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "todos": {
                        "type": ["array", "null"],
                        "description": "Full replacement list. Omit or null to read current state.",
                        "maxItems": MAX_ITEMS,
                        "items": {
                            "type": "object",
                            "required": ["id", "title", "status"],
                            "additionalProperties": false,
                            "properties": {
                                "id":     { "type": "integer", "minimum": 0, "maximum": MAX_ID },
                                "title":  { "type": "string", "minLength": 1, "maxLength": MAX_TITLE_CHARS },
                                "status": { "type": "string", "enum": ["pending", "in_progress", "completed"] }
                            }
                        }
                    }
                },
                "additionalProperties": false
            }),
        })
    }

    /// Handle a `todo` tool call. Returns the bare rendered list (no
    /// banner) on success, or an `Error: ...` string on validation failure
    /// (caller marks is_error). The agent's post-call `decorate()` adds
    /// the warning banner once if pending work remains.
    pub fn handle_call(&mut self, args: &Value) -> Result<String, String> {
        if !self.enabled {
            return Err("todo tool is disabled".into());
        }
        let input: Input =
            serde_json::from_value(args.clone()).map_err(|e| format!("invalid args: {e}"))?;
        if let Some(new_items) = input.todos {
            self.replace(new_items)?;
        }
        Ok(self.render_list())
    }

    fn replace(&mut self, new_items: Vec<Item>) -> Result<(), String> {
        if new_items.len() > MAX_ITEMS {
            return Err(format!("too many items (max {MAX_ITEMS})"));
        }
        let mut in_progress = 0u32;
        let mut seen_ids = std::collections::HashSet::with_capacity(new_items.len());
        for it in &new_items {
            if it.id > MAX_ID {
                return Err(format!("id {} exceeds max {MAX_ID}", it.id));
            }
            if !seen_ids.insert(it.id) {
                return Err(format!("duplicate id {}", it.id));
            }
            let trimmed = it.title.trim();
            if trimmed.is_empty() {
                return Err(format!("item {}: title is empty", it.id));
            }
            if it.title.chars().count() > MAX_TITLE_CHARS {
                return Err(format!(
                    "item {}: title exceeds {MAX_TITLE_CHARS} characters",
                    it.id
                ));
            }
            // Reject control chars (C0, DEL, and C1). These corrupt the
            // rendered list and could be used to inject fake "system"
            // lines into tool output the LLM consumes next turn.
            if let Some(bad) = it.title.chars().find(|c| c.is_control()) {
                return Err(format!(
                    "item {}: title contains control character (U+{:04X})",
                    it.id, bad as u32
                ));
            }
            if it.status == Status::InProgress {
                in_progress += 1;
            }
        }
        if in_progress > 1 {
            return Err(format!(
                "only one item may be in_progress (got {in_progress})"
            ));
        }
        // No incomplete item from current state may be silently dropped,
        // and no incomplete item may be retitled (would let the model
        // dodge work by replacing the title with something trivial under
        // the same id). Completed items may be retitled freely.
        for old in &self.items {
            if old.status == Status::Completed {
                continue;
            }
            match new_items.iter().find(|n| n.id == old.id) {
                None => {
                    let status_name = match old.status {
                        Status::Pending => "pending",
                        Status::InProgress => "in_progress",
                        Status::Completed => "completed",
                    };
                    return Err(format!(
                        "cannot remove incomplete item {} ({}); mark it completed first",
                        old.id, status_name
                    ));
                }
                Some(new) if new.title != old.title => {
                    return Err(format!(
                        "item {}: cannot change title of an incomplete item; \
                         mark it completed first or keep the title",
                        old.id
                    ));
                }
                Some(_) => {}
            }
        }
        // Only replace when the list actually changed by value.
        // Otherwise a model could resend an identical pending list every
        // turn to reset its strike count and never finish.
        if new_items != self.items {
            self.items = new_items;
        }
        Ok(())
    }

    fn has_pending(&self) -> bool {
        self.items.iter().any(|i| i.status != Status::Completed)
    }

    /// Render the bare list with no banner. The `← next` marker points at
    /// the in_progress item if one exists, otherwise the first pending.
    pub fn render_list(&self) -> String {
        if self.items.is_empty() {
            return "(todo list is empty)".into();
        }
        let next_idx = self
            .items
            .iter()
            .position(|i| i.status == Status::InProgress)
            .or_else(|| self.items.iter().position(|i| i.status == Status::Pending));
        let mut out = String::with_capacity(64 * self.items.len());
        for (i, it) in self.items.iter().enumerate() {
            let box_ = match it.status {
                Status::Completed => "[x]",
                Status::InProgress => "[~]",
                Status::Pending => "[ ]",
            };
            out.push_str(box_);
            out.push(' ');
            out.push_str(&it.id.to_string());
            out.push_str(". ");
            out.push_str(&it.title);
            if Some(i) == next_idx {
                out.push_str("  ← next");
            }
            out.push('\n');
        }
        out
    }

    /// Prepend a pending-todo warning to any tool result text. No-op if
    /// disabled or no pending items. Single source of banner truth for
    /// non-todo tool results.
    pub fn decorate(&self, text: &mut String) {
        if !self.enabled || !self.has_pending() {
            return;
        }
        // Idempotent: don't double-decorate if the text already starts
        // with the banner (e.g. handle_call's render() output, or a
        // re-decorated result if some caller invokes us twice).
        if text.starts_with(WARN_PREFIX) {
            return;
        }
        text.insert_str(0, WARN_PREFIX);
    }

    /// Called when the LLM signals end_turn. Strikes ONLY reset when the
    /// number of completed items has increased since the last strike —
    /// real forward progress. Reordering items, retitling completed
    /// items, or other no-op edits won't reset strikes. Three consecutive
    /// ignored reminders force-stop. After a force-stop we reset the
    /// strike state so a later prompt on the same session starts clean.
    pub fn check_end_turn(&mut self) -> EndTurn {
        if !self.enabled || !self.has_pending() {
            self.strikes = 0;
            self.last_strike_completed_count = None;
            return EndTurn::Allow;
        }
        let done_now = self.completed_count();
        if let Some(prev) = self.last_strike_completed_count {
            if done_now > prev {
                self.strikes = 0;
            }
        }
        self.strikes = self.strikes.saturating_add(1);
        self.last_strike_completed_count = Some(done_now);
        if self.strikes >= MAX_STRIKES {
            let body = self.render_list();
            let msg = format!(
                "{WARN_PREFIX}Force-stopping after {MAX_STRIKES} ignored reminders. \
                 Pending todos were not completed:\n\n{body}"
            );
            // Reset so the next prompt isn't already at MAX strikes.
            self.strikes = 0;
            self.last_strike_completed_count = None;
            return EndTurn::Stop(msg);
        }
        let body = self.render_list();
        EndTurn::Continue(format!(
            "{WARN_PREFIX}Strike {}/{MAX_STRIKES}. Either finish the work and mark items \
             completed, or revise the list. Current state:\n\n{body}",
            self.strikes,
        ))
    }

    /// Block to inject into handoff prompts and the post-handoff user
    /// message. None when disabled or empty.
    pub fn handoff_block(&self) -> Option<String> {
        if !self.enabled || self.items.is_empty() {
            return None;
        }
        Some(format!("# Todo List\n{}", self.render_list()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call_json(t: &mut Todos, items: &[(u32, &str, &str)]) -> Result<String, String> {
        let arr: Vec<Value> = items
            .iter()
            .map(|(id, title, st)| json!({ "id": id, "title": title, "status": st }))
            .collect();
        t.handle_call(&json!({ "todos": arr }))
    }
    fn call_read(t: &mut Todos) -> Result<String, String> {
        t.handle_call(&json!({}))
    }

    #[test]
    fn disabled_has_no_tool_def() {
        let t = Todos::new(false);
        assert!(t.tool_def().is_none());
        assert!(!t.is_enabled());
    }

    #[test]
    fn disabled_check_end_turn_allows() {
        let mut t = Todos::new(false);
        assert!(matches!(t.check_end_turn(), EndTurn::Allow));
    }

    #[test]
    fn disabled_decorate_noop() {
        let t = Todos::new(false);
        let mut s = "hello".to_string();
        t.decorate(&mut s);
        assert_eq!(s, "hello");
    }

    #[test]
    fn enabled_has_tool_def() {
        let t = Todos::new(true);
        assert!(t.tool_def().is_some());
    }

    #[test]
    fn empty_read_returns_placeholder() {
        let mut t = Todos::new(true);
        let out = call_read(&mut t).unwrap();
        assert!(out.contains("empty"));
    }

    #[test]
    fn rejects_unknown_top_level_field() {
        let mut t = Todos::new(true);
        let err = t
            .handle_call(&json!({ "todos": [], "extra": 1 }))
            .unwrap_err();
        assert!(err.contains("invalid args"), "got: {err}");
    }

    #[test]
    fn rejects_unknown_item_field() {
        let mut t = Todos::new(true);
        let err = t
            .handle_call(&json!({
                "todos": [{ "id": 1, "title": "x", "status": "pending", "extra": 1 }]
            }))
            .unwrap_err();
        assert!(err.contains("invalid args"), "got: {err}");
    }

    #[test]
    fn rejects_duplicate_ids() {
        let mut t = Todos::new(true);
        let err = call_json(&mut t, &[(1, "a", "pending"), (1, "b", "pending")]).unwrap_err();
        assert!(err.contains("duplicate id 1"));
    }

    #[test]
    fn rejects_multiple_in_progress() {
        let mut t = Todos::new(true);
        let err =
            call_json(&mut t, &[(1, "a", "in_progress"), (2, "b", "in_progress")]).unwrap_err();
        assert!(err.contains("only one item may be in_progress"));
    }

    #[test]
    fn rejects_empty_title() {
        let mut t = Todos::new(true);
        let err = call_json(&mut t, &[(1, "   ", "pending")]).unwrap_err();
        assert!(err.contains("title is empty"));
    }

    #[test]
    fn rejects_control_chars_in_title() {
        let mut t = Todos::new(true);
        let err = call_json(&mut t, &[(1, "ab\ncd", "pending")]).unwrap_err();
        assert!(err.contains("control character"), "got: {err}");
        let err = call_json(&mut t, &[(2, "ab\tcd", "pending")]).unwrap_err();
        assert!(err.contains("control character"), "got: {err}");
    }

    #[test]
    fn rejects_too_many_items() {
        let mut t = Todos::new(true);
        let many: Vec<(u32, &str, &str)> = (0u32..=(MAX_ITEMS as u32))
            .map(|i| (i, "x", "pending"))
            .collect();
        let err = call_json(&mut t, &many).unwrap_err();
        assert!(err.contains("too many items"));
    }

    #[test]
    fn title_length_uses_char_count_not_bytes() {
        // 200 multi-byte chars: byte length far exceeds 200 but char count
        // is exactly the limit, so this should pass.
        let title: String = "é".repeat(MAX_TITLE_CHARS);
        let mut t = Todos::new(true);
        let r = t.handle_call(&json!({
            "todos": [{ "id": 1, "title": title, "status": "pending" }]
        }));
        assert!(r.is_ok(), "expected ok, got {r:?}");
        // 201 chars: should fail.
        let too_long: String = "é".repeat(MAX_TITLE_CHARS + 1);
        let mut t2 = Todos::new(true);
        let err = t2
            .handle_call(&json!({
                "todos": [{ "id": 1, "title": too_long, "status": "pending" }]
            }))
            .unwrap_err();
        assert!(err.contains("exceeds"), "got: {err}");
    }

    #[test]
    fn cannot_silently_drop_incomplete() {
        let mut t = Todos::new(true);
        call_json(&mut t, &[(1, "a", "pending"), (2, "b", "pending")]).unwrap();
        // Try to drop id 1 while still pending.
        let err = call_json(&mut t, &[(2, "b", "pending")]).unwrap_err();
        assert!(err.contains("cannot remove incomplete item 1"));
        // Use schema name, not Rust debug.
        assert!(err.contains("pending"));
        assert!(!err.contains("Pending"));
    }

    #[test]
    fn render_list_marks_in_progress_as_next() {
        let mut t = Todos::new(true);
        call_json(
            &mut t,
            &[
                (1, "first", "pending"),
                (2, "second", "in_progress"),
                (3, "third", "pending"),
            ],
        )
        .unwrap();
        let out = t.render_list();
        // The in_progress item gets the marker, not the first pending.
        let line2 = out.lines().nth(1).unwrap();
        assert!(line2.contains("← next"), "got: {out}");
        let line0 = out.lines().next().unwrap();
        assert!(!line0.contains("← next"), "got: {out}");
    }

    #[test]
    fn render_list_marks_first_pending_when_no_in_progress() {
        let mut t = Todos::new(true);
        call_json(
            &mut t,
            &[
                (1, "first", "completed"),
                (2, "second", "pending"),
                (3, "third", "pending"),
            ],
        )
        .unwrap();
        let out = t.render_list();
        let line1 = out.lines().nth(1).unwrap();
        assert!(line1.contains("← next"), "got: {out}");
    }

    #[test]
    fn handle_call_returns_no_banner() {
        // The agent's post-tool decorate() owns the banner; handle_call
        // must not include it or the banner appears twice.
        let mut t = Todos::new(true);
        let out = call_json(&mut t, &[(1, "a", "pending")]).unwrap();
        assert!(!out.contains("Pending todos remain"), "got: {out}");
    }

    #[test]
    fn decorate_adds_banner_once() {
        let mut t = Todos::new(true);
        call_json(&mut t, &[(1, "a", "pending")]).unwrap();
        let mut text = "result text".to_string();
        t.decorate(&mut text);
        assert!(text.starts_with("⚠ Pending todos remain"));
        // Decorating again would add a second banner — agent code should
        // call decorate() at most once per result. We just verify the
        // single-call shape.
        assert_eq!(text.matches("Pending todos remain").count(), 1);
    }

    #[test]
    fn strikes_force_stop_after_three_unchanged_lists() {
        let mut t = Todos::new(true);
        call_json(&mut t, &[(1, "a", "pending")]).unwrap();
        assert!(matches!(t.check_end_turn(), EndTurn::Continue(_)));
        assert!(matches!(t.check_end_turn(), EndTurn::Continue(_)));
        match t.check_end_turn() {
            EndTurn::Stop(_) => {}
            other => panic!("expected Stop, got {other:?}"),
        }
    }

    #[test]
    fn force_stop_resets_strikes() {
        let mut t = Todos::new(true);
        call_json(&mut t, &[(1, "a", "pending")]).unwrap();
        let _ = t.check_end_turn();
        let _ = t.check_end_turn();
        let _ = t.check_end_turn(); // ForceStop, also resets.
                                    // After force-stop, strikes are back to 0; first call_end_turn is
                                    // Continue (strike 1/3) again, not ForceStop.
        assert!(matches!(t.check_end_turn(), EndTurn::Continue(_)));
    }

    #[test]
    fn semantic_revision_blocks_resend_bypass() {
        // The model resends the SAME pending list three times. Strikes
        // must NOT reset between calls.
        let mut t = Todos::new(true);
        call_json(&mut t, &[(1, "a", "pending")]).unwrap();
        // Resend identical list.
        call_json(&mut t, &[(1, "a", "pending")]).unwrap();

        assert!(matches!(t.check_end_turn(), EndTurn::Continue(_))); // 1
                                                                     // Resend identical list — no semantic change.
        call_json(&mut t, &[(1, "a", "pending")]).unwrap();
        assert!(matches!(t.check_end_turn(), EndTurn::Continue(_))); // 2
        call_json(&mut t, &[(1, "a", "pending")]).unwrap();
        // Third strike → Stop, despite three "writes" between.
        match t.check_end_turn() {
            EndTurn::Stop(_) => {}
            other => panic!("resend bypass not blocked: {other:?}"),
        }
    }

    #[test]
    fn completing_an_item_resets_strikes() {
        // Strikes reset only when completed-count goes UP. Adding a new
        // pending item or reordering does NOT reset.
        let mut t = Todos::new(true);
        call_json(&mut t, &[(1, "a", "pending"), (2, "b", "pending")]).unwrap();
        assert!(matches!(t.check_end_turn(), EndTurn::Continue(_))); // strike 1
        assert!(matches!(t.check_end_turn(), EndTurn::Continue(_))); // strike 2
                                                                     // Mark one item completed: real progress.
        call_json(&mut t, &[(1, "a", "completed"), (2, "b", "pending")]).unwrap();
        // Strikes reset on next end_turn check; expect Continue (1/3) again,
        // not ForceStop (which would be strike 3).
        assert!(matches!(t.check_end_turn(), EndTurn::Continue(_)));
    }

    #[test]
    fn reordering_does_not_reset_strikes() {
        // Reordering doesn't increase completed count.
        // Strikes must NOT reset — model could otherwise dodge by shuffling.
        let mut t = Todos::new(true);
        call_json(&mut t, &[(1, "a", "pending"), (2, "b", "pending")]).unwrap();
        assert!(matches!(t.check_end_turn(), EndTurn::Continue(_))); // 1
        assert!(matches!(t.check_end_turn(), EndTurn::Continue(_))); // 2
                                                                     // Reorder.
        call_json(&mut t, &[(2, "b", "pending"), (1, "a", "pending")]).unwrap();
        // No completion progress → still strike 3 → Stop.
        match t.check_end_turn() {
            EndTurn::Stop(_) => {}
            other => panic!("reorder bypass not blocked: {other:?}"),
        }
    }

    #[test]
    fn rejects_retitling_incomplete_item() {
        let mut t = Todos::new(true);
        call_json(&mut t, &[(1, "real work", "pending")]).unwrap();
        let err = call_json(&mut t, &[(1, "trivial", "pending")]).unwrap_err();
        assert!(err.contains("cannot change title"), "got: {err}");
    }

    #[test]
    fn allows_retitling_completed_item() {
        let mut t = Todos::new(true);
        call_json(&mut t, &[(1, "did the thing", "completed")]).unwrap();
        // Retitling a completed item is allowed.
        call_json(&mut t, &[(1, "did THE thing better", "completed")]).unwrap();
    }

    #[test]
    fn rejects_del_control_char() {
        let mut t = Todos::new(true);
        // 0x7F (DEL) was missed by the old `< 0x20` check.
        let err = t
            .handle_call(&json!({
                "todos": [{ "id": 1, "title": "ab\u{7F}cd", "status": "pending" }]
            }))
            .unwrap_err();
        assert!(err.contains("control character"), "got: {err}");
    }

    #[test]
    fn handle_call_disabled_errors() {
        let mut t = Todos::new(false);
        let err = t.handle_call(&json!({})).unwrap_err();
        assert!(err.contains("disabled"), "got: {err}");
    }

    #[test]
    fn decorate_idempotent() {
        let mut t = Todos::new(true);
        call_json(&mut t, &[(1, "a", "pending")]).unwrap();
        let mut text = "result text".to_string();
        t.decorate(&mut text);
        t.decorate(&mut text);
        t.decorate(&mut text);
        assert_eq!(text.matches("Pending todos remain").count(), 1);
        assert!(text.starts_with("⚠ Pending todos remain"));
    }

    #[test]
    fn allow_when_all_completed() {
        let mut t = Todos::new(true);
        call_json(&mut t, &[(1, "a", "completed")]).unwrap();
        assert!(matches!(t.check_end_turn(), EndTurn::Allow));
    }

    #[test]
    fn handoff_block_disabled_or_empty_is_none() {
        let t = Todos::new(false);
        assert!(t.handoff_block().is_none());
        let t = Todos::new(true);
        assert!(t.handoff_block().is_none());
    }

    #[test]
    fn handoff_block_when_populated() {
        let mut t = Todos::new(true);
        call_json(&mut t, &[(1, "a", "pending")]).unwrap();
        let b = t.handoff_block().unwrap();
        assert!(b.starts_with("# Todo List\n"));
        // No banner inside handoff block.
        assert!(!b.contains("Pending todos remain"));
    }
}
