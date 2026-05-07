//! Session task list. Two statuses (open / done), full-list replace.
//!
//! Enforcement:
//!   - Cannot remove an open item without first marking it done.
//!   - Cannot end the turn while open items remain (3 strikes → force-stop).
//!
//! The first open item is implicitly "next." Tool results are decorated
//! with a one-line warning while open items remain.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::types::ToolDef;

pub const TOOL_NAME: &str = "todo";
const MAX_ITEMS: usize = 50;
const MAX_ID: u32 = 9999;
const MAX_TITLE_CHARS: usize = 200;
const MAX_STRIKES: u32 = 3;
const WARN_PREFIX: &str = "⚠ Open todos remain. Update the `todo` list before ending the turn.\n\n";

const DESCRIPTION: &str = "Session task list. Full-list replace. Items have id (int), \
title (string), done (bool). Cannot remove open items. Cannot end turn with open items.";

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub id: u32,
    pub title: String,
    #[serde(default)]
    pub done: bool,
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
    /// Number of done items at the last strike. Strikes only reset when
    /// done count INCREASES — actual progress, not reordering.
    last_strike_done_count: Option<usize>,
}

impl Todos {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            items: Vec::new(),
            strikes: 0,
            last_strike_done_count: None,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn done_count(&self) -> usize {
        self.items.iter().filter(|i| i.done).count()
    }

    fn has_open(&self) -> bool {
        self.items.iter().any(|i| !i.done)
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
                        "description": "Full replacement list. Omit to read.",
                        "maxItems": MAX_ITEMS,
                        "items": {
                            "type": "object",
                            "required": ["id", "title"],
                            "additionalProperties": false,
                            "properties": {
                                "id":    { "type": "integer", "minimum": 0, "maximum": MAX_ID },
                                "title": { "type": "string", "minLength": 1, "maxLength": MAX_TITLE_CHARS },
                                "done":  { "type": "boolean" }
                            }
                        }
                    }
                },
                "additionalProperties": false
            }),
        })
    }

    pub fn handle_call(&mut self, args: &Value) -> Result<String, String> {
        if !self.enabled {
            return Err("todo tool is disabled".into());
        }
        let input: Input =
            serde_json::from_value(args.clone()).map_err(|e| format!("invalid args: {e}"))?;
        if let Some(new_items) = input.todos {
            self.replace(new_items)?;
        }
        Ok(self.render())
    }

    fn replace(&mut self, new_items: Vec<Item>) -> Result<(), String> {
        if new_items.len() > MAX_ITEMS {
            return Err(format!("too many items (max {MAX_ITEMS})"));
        }
        let mut seen = std::collections::HashSet::with_capacity(new_items.len());
        for it in &new_items {
            if it.id > MAX_ID {
                return Err(format!("id {} exceeds max {MAX_ID}", it.id));
            }
            if !seen.insert(it.id) {
                return Err(format!("duplicate id {}", it.id));
            }
            if it.title.trim().is_empty() {
                return Err(format!("item {}: title is empty", it.id));
            }
            if it.title.chars().count() > MAX_TITLE_CHARS {
                return Err(format!(
                    "item {}: title exceeds {MAX_TITLE_CHARS} characters",
                    it.id
                ));
            }
            // Reject control chars (C0, DEL, C1) — they corrupt rendering
            // and could inject fake "system" lines into tool output.
            if let Some(bad) = it.title.chars().find(|c| c.is_control()) {
                return Err(format!(
                    "item {}: title contains control character (U+{:04X})",
                    it.id, bad as u32
                ));
            }
        }
        // Anti-drop: an open item from current state cannot disappear
        // without being marked done. Done items can be reorganized freely.
        for old in &self.items {
            if old.done {
                continue;
            }
            if !new_items.iter().any(|n| n.id == old.id) {
                return Err(format!(
                    "cannot remove open item {}; mark it done first",
                    old.id
                ));
            }
        }
        self.items = new_items;
        Ok(())
    }

    /// `[x] 1. title` for done; `[ ] 2. title  ← next` for the first
    /// open item; `[ ] 3. title` for the rest.
    pub fn render(&self) -> String {
        if self.items.is_empty() {
            return "(todo list is empty)".into();
        }
        let next = self.items.iter().position(|i| !i.done);
        let mut out = String::with_capacity(64 * self.items.len());
        for (i, it) in self.items.iter().enumerate() {
            let box_ = if it.done { "[x]" } else { "[ ]" };
            out.push_str(box_);
            out.push(' ');
            out.push_str(&it.id.to_string());
            out.push_str(". ");
            out.push_str(&it.title);
            if Some(i) == next {
                out.push_str("  ← next");
            }
            out.push('\n');
        }
        out
    }

    /// Prepend the warning banner to a tool result while open items
    /// remain. No-op when disabled or all done.
    pub fn decorate(&self, text: &mut String) {
        if !self.enabled || !self.has_open() {
            return;
        }
        text.insert_str(0, WARN_PREFIX);
    }

    /// Called when the LLM signals end_turn. Strikes only reset when the
    /// done count increases. Three ignored reminders force-stop.
    pub fn check_end_turn(&mut self) -> EndTurn {
        if !self.enabled || !self.has_open() {
            self.strikes = 0;
            self.last_strike_done_count = None;
            return EndTurn::Allow;
        }
        let done_now = self.done_count();
        if let Some(prev) = self.last_strike_done_count {
            if done_now > prev {
                self.strikes = 0;
            }
        }
        self.strikes = self.strikes.saturating_add(1);
        self.last_strike_done_count = Some(done_now);
        let body = self.render();
        if self.strikes >= MAX_STRIKES {
            let msg = format!(
                "{WARN_PREFIX}Force-stopping after {MAX_STRIKES} ignored reminders. \
                 Open todos:\n\n{body}"
            );
            self.strikes = 0;
            self.last_strike_done_count = None;
            return EndTurn::Stop(msg);
        }
        EndTurn::Continue(format!(
            "{WARN_PREFIX}Strike {}/{MAX_STRIKES}. Finish the work and mark items done, \
             or revise the list. Current state:\n\n{body}",
            self.strikes,
        ))
    }

    /// Block injected into handoff prompts so the next turn inherits the
    /// list. None when disabled or empty.
    pub fn handoff_block(&self) -> Option<String> {
        if !self.enabled || self.items.is_empty() {
            return None;
        }
        Some(format!("# Todo List\n{}", self.render()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(t: &mut Todos, items: &[(u32, &str, bool)]) -> Result<String, String> {
        let arr: Vec<Value> = items
            .iter()
            .map(|(id, title, done)| json!({ "id": id, "title": title, "done": done }))
            .collect();
        t.handle_call(&json!({ "todos": arr }))
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
        assert!(Todos::new(true).tool_def().is_some());
    }

    #[test]
    fn empty_read_returns_placeholder() {
        let mut t = Todos::new(true);
        let out = t.handle_call(&json!({})).unwrap();
        assert!(out.contains("empty"));
    }

    #[test]
    fn done_defaults_to_false() {
        let mut t = Todos::new(true);
        // Omit `done` — should default to false (open).
        t.handle_call(&json!({ "todos": [{ "id": 1, "title": "a" }] }))
            .unwrap();
        assert!(t.has_open());
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
                "todos": [{ "id": 1, "title": "x", "done": false, "extra": 1 }]
            }))
            .unwrap_err();
        assert!(err.contains("invalid args"), "got: {err}");
    }

    #[test]
    fn rejects_duplicate_ids() {
        let mut t = Todos::new(true);
        let err = call(&mut t, &[(1, "a", false), (1, "b", false)]).unwrap_err();
        assert!(err.contains("duplicate id 1"));
    }

    #[test]
    fn rejects_empty_title() {
        let mut t = Todos::new(true);
        let err = call(&mut t, &[(1, "   ", false)]).unwrap_err();
        assert!(err.contains("title is empty"));
    }

    #[test]
    fn rejects_control_chars_in_title() {
        let mut t = Todos::new(true);
        assert!(call(&mut t, &[(1, "ab\ncd", false)])
            .unwrap_err()
            .contains("control character"));
        assert!(call(&mut t, &[(2, "ab\tcd", false)])
            .unwrap_err()
            .contains("control character"));
    }

    #[test]
    fn rejects_del_control_char() {
        let mut t = Todos::new(true);
        let err = t
            .handle_call(&json!({
                "todos": [{ "id": 1, "title": "ab\u{7F}cd", "done": false }]
            }))
            .unwrap_err();
        assert!(err.contains("control character"), "got: {err}");
    }

    #[test]
    fn rejects_too_many_items() {
        let mut t = Todos::new(true);
        let many: Vec<(u32, &str, bool)> = (0u32..=(MAX_ITEMS as u32))
            .map(|i| (i, "x", false))
            .collect();
        let err = call(&mut t, &many).unwrap_err();
        assert!(err.contains("too many items"));
    }

    #[test]
    fn title_length_uses_char_count_not_bytes() {
        let title: String = "é".repeat(MAX_TITLE_CHARS);
        let mut t = Todos::new(true);
        assert!(t
            .handle_call(&json!({ "todos": [{ "id": 1, "title": title, "done": false }] }))
            .is_ok());
        let too_long: String = "é".repeat(MAX_TITLE_CHARS + 1);
        let mut t2 = Todos::new(true);
        let err = t2
            .handle_call(&json!({ "todos": [{ "id": 1, "title": too_long, "done": false }] }))
            .unwrap_err();
        assert!(err.contains("exceeds"), "got: {err}");
    }

    #[test]
    fn cannot_silently_drop_open() {
        let mut t = Todos::new(true);
        call(&mut t, &[(1, "a", false), (2, "b", false)]).unwrap();
        let err = call(&mut t, &[(2, "b", false)]).unwrap_err();
        assert!(err.contains("cannot remove open item 1"), "got: {err}");
    }

    #[test]
    fn allows_dropping_done_items() {
        let mut t = Todos::new(true);
        call(&mut t, &[(1, "a", true), (2, "b", false)]).unwrap();
        // Drop the done item — fine.
        call(&mut t, &[(2, "b", false)]).unwrap();
    }

    #[test]
    fn allows_retitling_open_item() {
        // Anti-retitle guard removed: reorganizing the plan is allowed.
        // Enforcement is "can't STOP with open items", not "can't edit."
        let mut t = Todos::new(true);
        call(&mut t, &[(1, "rough idea", false)]).unwrap();
        call(&mut t, &[(1, "refined plan", false)]).unwrap();
    }

    #[test]
    fn render_marks_first_open_as_next() {
        let mut t = Todos::new(true);
        call(
            &mut t,
            &[
                (1, "first", true),
                (2, "second", false),
                (3, "third", false),
            ],
        )
        .unwrap();
        let out = t.render();
        let line1 = out.lines().nth(1).unwrap();
        assert!(line1.contains("← next"), "got: {out}");
        let line2 = out.lines().nth(2).unwrap();
        assert!(!line2.contains("← next"), "got: {out}");
    }

    #[test]
    fn handle_call_returns_no_banner() {
        // The agent's post-tool decorate() owns the banner.
        let mut t = Todos::new(true);
        let out = call(&mut t, &[(1, "a", false)]).unwrap();
        assert!(!out.contains("Open todos remain"), "got: {out}");
    }

    #[test]
    fn decorate_adds_banner() {
        let mut t = Todos::new(true);
        call(&mut t, &[(1, "a", false)]).unwrap();
        let mut text = "result text".to_string();
        t.decorate(&mut text);
        assert!(text.starts_with("⚠ Open todos remain"));
    }

    #[test]
    fn decorate_noop_when_all_done() {
        let mut t = Todos::new(true);
        call(&mut t, &[(1, "a", true)]).unwrap();
        let mut text = "result text".to_string();
        t.decorate(&mut text);
        assert_eq!(text, "result text");
    }

    #[test]
    fn strikes_force_stop_after_three() {
        let mut t = Todos::new(true);
        call(&mut t, &[(1, "a", false)]).unwrap();
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
        call(&mut t, &[(1, "a", false)]).unwrap();
        let _ = t.check_end_turn();
        let _ = t.check_end_turn();
        let _ = t.check_end_turn(); // Stop, also resets.
        assert!(matches!(t.check_end_turn(), EndTurn::Continue(_)));
    }

    #[test]
    fn marking_done_resets_strikes() {
        let mut t = Todos::new(true);
        call(&mut t, &[(1, "a", false), (2, "b", false)]).unwrap();
        assert!(matches!(t.check_end_turn(), EndTurn::Continue(_))); // 1
        assert!(matches!(t.check_end_turn(), EndTurn::Continue(_))); // 2
        call(&mut t, &[(1, "a", true), (2, "b", false)]).unwrap();
        // Strikes reset → next is Continue (1/3), not Stop.
        assert!(matches!(t.check_end_turn(), EndTurn::Continue(_)));
    }

    #[test]
    fn reordering_does_not_reset_strikes() {
        let mut t = Todos::new(true);
        call(&mut t, &[(1, "a", false), (2, "b", false)]).unwrap();
        assert!(matches!(t.check_end_turn(), EndTurn::Continue(_))); // 1
        assert!(matches!(t.check_end_turn(), EndTurn::Continue(_))); // 2
                                                                     // Reorder — no done count increase.
        call(&mut t, &[(2, "b", false), (1, "a", false)]).unwrap();
        match t.check_end_turn() {
            EndTurn::Stop(_) => {}
            other => panic!("reorder bypass not blocked: {other:?}"),
        }
    }

    #[test]
    fn allow_when_all_done() {
        let mut t = Todos::new(true);
        call(&mut t, &[(1, "a", true)]).unwrap();
        assert!(matches!(t.check_end_turn(), EndTurn::Allow));
    }

    #[test]
    fn handle_call_disabled_errors() {
        let mut t = Todos::new(false);
        let err = t.handle_call(&json!({})).unwrap_err();
        assert!(err.contains("disabled"), "got: {err}");
    }

    #[test]
    fn handoff_block_disabled_or_empty_is_none() {
        assert!(Todos::new(false).handoff_block().is_none());
        assert!(Todos::new(true).handoff_block().is_none());
    }

    #[test]
    fn handoff_block_when_populated() {
        let mut t = Todos::new(true);
        call(&mut t, &[(1, "a", false)]).unwrap();
        let b = t.handoff_block().unwrap();
        assert!(b.starts_with("# Todo List\n"));
        assert!(!b.contains("Open todos remain"));
    }
}
