//! Session task list. Two statuses (open / done), full-list replace.
//!
//! End-turn gate: blocks the agent from stopping while open items exist.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::types::ToolDef;

pub const TOOL_NAME: &str = "todo";
const MAX_ITEMS: usize = 50;
const MAX_ID: u32 = 9999;
const MAX_TITLE_CHARS: usize = 200;

const DESCRIPTION: &str = "Session task list. Omit `todos` to read. \
Provide full replacement array to update. Cannot end turn with open items.";

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Item {
    pub id: u32,
    pub title: String,
    #[serde(default)]
    pub done: bool,
}

#[derive(Debug, Deserialize)]
struct Input {
    todos: Option<Vec<Item>>,
}

#[derive(Debug)]
pub struct Todos {
    enabled: bool,
    items: Vec<Item>,
}

impl Todos {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            items: Vec::new(),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
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
                            "properties": {
                                "id":    { "type": "integer", "minimum": 0, "maximum": MAX_ID },
                                "title": { "type": "string", "minLength": 1, "maxLength": MAX_TITLE_CHARS },
                                "done":  { "type": "boolean" }
                            }
                        }
                    }
                }
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
        }
        self.items = new_items;
        Ok(())
    }

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

    pub fn check_end_turn(&self) -> Option<String> {
        if !self.enabled || !self.has_open() {
            return None;
        }
        Some(format!(
            "You have open todo items. Keep working.\n\n{}",
            self.render()
        ))
    }

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
        assert!(Todos::new(false).check_end_turn().is_none());
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
        t.handle_call(&json!({ "todos": [{ "id": 1, "title": "a" }] }))
            .unwrap();
        assert!(t.has_open());
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
    fn rejects_too_many_items() {
        let mut t = Todos::new(true);
        let many: Vec<(u32, &str, bool)> = (0u32..=(MAX_ITEMS as u32))
            .map(|i| (i, "x", false))
            .collect();
        let err = call(&mut t, &many).unwrap_err();
        assert!(err.contains("too many items"));
    }

    #[test]
    fn title_length_uses_char_count() {
        let title: String = "é".repeat(MAX_TITLE_CHARS);
        let mut t = Todos::new(true);
        assert!(t
            .handle_call(&json!({ "todos": [{ "id": 1, "title": title }] }))
            .is_ok());
        let too_long: String = "é".repeat(MAX_TITLE_CHARS + 1);
        let err = t
            .handle_call(&json!({ "todos": [{ "id": 1, "title": too_long }] }))
            .unwrap_err();
        assert!(err.contains("exceeds"));
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
        assert!(out.lines().nth(1).unwrap().contains("← next"));
        assert!(!out.lines().nth(2).unwrap().contains("← next"));
    }

    #[test]
    fn check_end_turn_blocks_while_open() {
        let mut t = Todos::new(true);
        call(&mut t, &[(1, "a", false)]).unwrap();
        assert!(t.check_end_turn().is_some());
    }

    #[test]
    fn check_end_turn_allows_when_all_done() {
        let mut t = Todos::new(true);
        call(&mut t, &[(1, "a", true)]).unwrap();
        assert!(t.check_end_turn().is_none());
    }

    #[test]
    fn handle_call_disabled_errors() {
        let mut t = Todos::new(false);
        assert!(t.handle_call(&json!({})).unwrap_err().contains("disabled"));
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
    }

    #[test]
    fn can_remove_open_items_freely() {
        let mut t = Todos::new(true);
        call(&mut t, &[(1, "a", false), (2, "b", false)]).unwrap();
        // Can drop open items without marking done first
        call(&mut t, &[(2, "b", false)]).unwrap();
        assert_eq!(t.items.len(), 1);
    }
}
