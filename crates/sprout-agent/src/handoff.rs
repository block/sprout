use crate::agent::RunCtx;
use crate::config::{
    HANDOFF_MAX_OUTPUT_TOKENS, HANDOFF_MAX_TOOL_NAMES, HANDOFF_ORIGINAL_TASK_MAX_BYTES,
    HANDOFF_PROMPT_MAX_BYTES, HANDOFF_TAIL_ITEMS, HANDOFF_THRESHOLD,
};
use crate::types::HistoryItem;

pub(crate) enum HandoffOutcome {
    Performed,
    Skipped,
    Cancelled,
}

const HANDOFF_SYSTEM_PROMPT: &str = "You are generating a context handoff summary for the next \
turn of an autonomous agent. Be concise but thorough. Cover: what the original task was, what \
you accomplished, key decisions made, what remains, and one concrete next step. Output plain \
text only — no tool calls, no JSON. Stay under 8192 tokens.";

const HANDOFF_SNIPPET_BYTES: usize = 2048;

impl RunCtx<'_> {
    pub(crate) async fn maybe_handoff(&mut self) -> HandoffOutcome {
        if !self.should_handoff() {
            return HandoffOutcome::Skipped;
        }
        if *self.handoff_count >= self.cfg.max_handoffs {
            eprintln!(
                "sprout-agent: agent: handoff cap reached ({}); using truncation",
                self.cfg.max_handoffs
            );
            return HandoffOutcome::Skipped;
        }
        let prompt = self.build_handoff_prompt();
        let summary = tokio::select! {
            biased;
            _ = self.cancel.changed() => return HandoffOutcome::Cancelled,
            r = self.llm.summarize(
                self.cfg,
                HANDOFF_SYSTEM_PROMPT,
                &prompt,
                HANDOFF_MAX_OUTPUT_TOKENS,
            ) => match r {
                Ok(s) if !s.trim().is_empty() => s,
                Ok(_) => {
                    eprintln!("sprout-agent: agent: handoff returned empty summary; truncating");
                    return HandoffOutcome::Skipped;
                }
                Err(e) => {
                    eprintln!("sprout-agent: agent: handoff failed: {e}; truncating");
                    return HandoffOutcome::Skipped;
                }
            },
        };
        let current_prompt = self.history.iter().rev().find_map(|item| match item {
            HistoryItem::User(s) => Some(s.clone()),
            _ => None,
        });
        let prior = self.history.len();
        self.history.clear();
        // Replay the todo state into the post-handoff context. Without
        // this the next turn forgets the plan.
        let mut handoff_text = format!("[Context Handoff]\n{summary}");
        if let Some(block) = self.todos.handoff_block() {
            handoff_text.push_str("\n\n");
            handoff_text.push_str(&block);
        }
        self.history.push(HistoryItem::User(handoff_text));
        if let Some(prompt) = current_prompt {
            self.history.push(HistoryItem::User(prompt));
        }
        *self.handoff_count += 1;
        eprintln!(
            "sprout-agent: agent: handoff #{} (history {prior} -> 1 item)",
            *self.handoff_count
        );
        HandoffOutcome::Performed
    }

    fn should_handoff(&self) -> bool {
        let usage: usize = self.history.iter().map(HistoryItem::estimated_bytes).sum();
        let threshold = (self.cfg.max_history_bytes as f64 * HANDOFF_THRESHOLD) as usize;
        usage > threshold
    }

    fn build_handoff_prompt(&self) -> String {
        let mut head = String::new();
        head.push_str(&format!(
            "[Internal handoff #{} — context reset]\n\n",
            *self.handoff_count + 1
        ));
        head.push_str("# Original Task\n");
        let task = self.original_task.as_deref().unwrap_or("(unknown)");
        head.push_str(&clamp_bytes(task, HANDOFF_ORIGINAL_TASK_MAX_BYTES));
        head.push_str("\n\n# Available Tools\n");
        let mut all_tools = self.mcp.tools();
        if let Some(td) = self.todos.tool_def() {
            all_tools.push(td);
        }
        let total = all_tools.len();
        if total == 0 {
            head.push_str("(none)\n");
        } else {
            let shown = total.min(HANDOFF_MAX_TOOL_NAMES);
            let names: Vec<&str> = all_tools[..shown].iter().map(|t| t.name.as_str()).collect();
            head.push_str(&names.join(", "));
            if shown < total {
                head.push_str(&format!(", … (+{} more)", total - shown));
            }
            head.push('\n');
        }
        if let Some(block) = self.todos.handoff_block() {
            head.push('\n');
            head.push_str(&block);
            head.push('\n');
        }

        let tail = "\n# Instructions\n\
             Produce a context handoff summary covering: (1) original task, \
             (2) what was accomplished, (3) key decisions, (4) what remains, \
             (5) one concrete next step. Be concise but thorough. Plain text.\n";
        let history_header = "\n# Recent History (most recent last)\n";

        let start = self.history.len().saturating_sub(HANDOFF_TAIL_ITEMS);
        let mut snippets: Vec<String> = self.history[start..]
            .iter()
            .map(|item| {
                let mut s = String::new();
                push_history_snippet(&mut s, item);
                s
            })
            .collect();

        let fixed = head.len() + history_header.len() + tail.len();
        let mut snippets_bytes: usize = snippets.iter().map(String::len).sum();
        let mut dropped = 0usize;
        while fixed + snippets_bytes > HANDOFF_PROMPT_MAX_BYTES && !snippets.is_empty() {
            let removed = snippets.remove(0);
            snippets_bytes -= removed.len();
            dropped += 1;
        }
        if dropped > 0 {
            eprintln!("sprout-agent: agent: handoff prompt cap, dropped {dropped} oldest snippets");
        }

        let mut out =
            String::with_capacity(fixed + snippets_bytes + if dropped > 0 { 32 } else { 0 });
        out.push_str(&head);
        out.push_str(history_header);
        if dropped > 0 {
            out.push_str(&format!("(… {dropped} older items omitted)\n"));
        }
        for s in &snippets {
            out.push_str(s);
        }
        out.push_str(tail);
        out
    }
}

fn push_history_snippet(out: &mut String, item: &HistoryItem) {
    match item {
        HistoryItem::User(s) => {
            out.push_str("[user] ");
            out.push_str(&clamp_for_snippet(s));
            out.push('\n');
        }
        HistoryItem::Assistant { text, tool_calls } => {
            out.push_str("[assistant] ");
            if !text.is_empty() {
                out.push_str(&clamp_for_snippet(text));
            }
            for c in tool_calls {
                out.push_str(&format!(" tool:{}", c.name));
            }
            out.push('\n');
        }
        HistoryItem::ToolResult(r) => {
            out.push_str(if r.is_error { "[tool_err] " } else { "[tool] " });
            out.push_str(&clamp_for_snippet(&r.text));
            out.push('\n');
        }
    }
}

fn clamp_for_snippet(s: &str) -> String {
    clamp_bytes(s, HANDOFF_SNIPPET_BYTES)
}

pub(crate) fn clamp_bytes(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_owned();
    }
    if max_bytes < 4 {
        let mut cut = max_bytes.min(s.len());
        while cut > 0 && !s.is_char_boundary(cut) {
            cut -= 1;
        }
        return s[..cut].to_owned();
    }
    let target = max_bytes - "…".len();
    let mut cut = target;
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}…", &s[..cut])
}
