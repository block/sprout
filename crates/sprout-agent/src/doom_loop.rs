use std::{
    collections::{hash_map::DefaultHasher, VecDeque},
    hash::{Hash, Hasher},
};

use crate::types::ToolCall;

const MIN_THRESHOLD: usize = 2;
const MAX_THRESHOLD: usize = 16;
const MIN_CYCLIC_WIDTH: usize = 2;
// Width 5+ with threshold 3 needs 15+ turns before detection, which is slow to
// act on and has diminishing returns for catching real loops.
const MAX_CYCLIC_WIDTH: usize = 4;
const MAX_BUFFER: usize = MAX_THRESHOLD * MAX_CYCLIC_WIDTH;
const SKIPPED_TOOL_NAMES: &[&str] = &[crate::todo::TOOL_NAME];

const MESSAGE_SUFFIX: &str = "Stop and reassess before using another tool. Summarize what the previous tool results showed, explain why the repeated calls are not making progress, then choose a different next action or ask for clarification if blocked.";

/// Detects repeated tool-call turns.
///
/// `threshold` is the number of repetitions of the pattern. For consecutive
/// loops, that means N identical turns. For cyclic loops of width W, that means
/// N repetitions of a W-turn sequence, requiring W*N buffered entries.
#[derive(Debug)]
pub struct DoomLoop {
    enabled: bool,
    threshold: usize,
    calls: VecDeque<u64>,
    tool_names_per_turn: VecDeque<Vec<String>>,
}

impl DoomLoop {
    pub fn new(enabled: bool, threshold: usize) -> Self {
        let cap = if enabled { MAX_BUFFER } else { 0 };
        Self {
            enabled,
            threshold: threshold.clamp(MIN_THRESHOLD, MAX_THRESHOLD),
            calls: VecDeque::with_capacity(cap),
            tool_names_per_turn: VecDeque::with_capacity(cap),
        }
    }

    /// Clears all buffered state. Call at the start of each user prompt so
    /// patterns from a previous prompt don't bleed into the next.
    pub fn reset(&mut self) {
        self.calls.clear();
        self.tool_names_per_turn.clear();
    }

    pub fn record_turn(&mut self, calls: &[ToolCall]) {
        if !self.enabled {
            return;
        }
        let Some((fingerprint, tool_names)) = fingerprint_turn(calls) else {
            return;
        };
        if self.calls.len() == MAX_BUFFER {
            self.calls.pop_front();
            self.tool_names_per_turn.pop_front();
        }
        self.calls.push_back(fingerprint);
        self.tool_names_per_turn.push_back(tool_names);
    }

    pub fn check(&mut self) -> Option<String> {
        if !self.enabled || self.calls.len() < self.threshold {
            return None;
        }
        let width = self.repeated_width()?;
        let tools = self.tool_label(width);
        eprintln!(
            "sprout-agent: doom: loop detected: tools=[{tools}] threshold={} width={width}",
            self.threshold
        );
        self.reset();
        Some(format!(
            "You have called {tools} with the same arguments {} times. {MESSAGE_SUFFIX}",
            self.threshold
        ))
    }

    fn repeated_width(&self) -> Option<usize> {
        self.is_repeated_pattern(1, self.threshold)
            .then_some(1)
            .or_else(|| {
                (MIN_CYCLIC_WIDTH..=MAX_CYCLIC_WIDTH).find(|width| {
                    self.is_repeated_pattern(*width, width.saturating_mul(self.threshold))
                })
            })
    }

    fn is_repeated_pattern(&self, width: usize, need: usize) -> bool {
        let len = self.calls.len();
        if width == 0 || len < need || need <= width {
            return false;
        }
        let start = len - need;
        (0..need - width).all(|i| {
            matches!(
                (self.calls.get(start + i), self.calls.get(start + i + width)),
                (Some(left), Some(right)) if left == right
            )
        })
    }

    /// Builds a human-readable label for the repeated pattern. For width=1,
    /// reports the single repeated turn. For width>1, reports the full cycle
    /// joined by " -> " so the model sees the actual sequence it's stuck in.
    fn tool_label(&self, width: usize) -> String {
        let len = self.tool_names_per_turn.len();
        if len == 0 || width == 0 {
            return "the same tool turn".into();
        }
        let start = len.saturating_sub(width);
        let segments: Vec<String> = (start..len)
            .filter_map(|i| self.tool_names_per_turn.get(i))
            .map(|names| {
                if names.is_empty() {
                    "<turn>".into()
                } else {
                    names.join(", ")
                }
            })
            .collect();
        if segments.is_empty() {
            "the same tool turn".into()
        } else {
            segments.join(" -> ")
        }
    }
}

fn fingerprint_turn(calls: &[ToolCall]) -> Option<(u64, Vec<String>)> {
    // Collect non-skipped calls, sort by (name, arguments) so parallel tool
    // calls are order-independent in the fingerprint.
    let mut entries: Vec<(&str, Vec<u8>)> = calls
        .iter()
        .filter(|call| !SKIPPED_TOOL_NAMES.contains(&call.name.as_str()))
        .filter_map(|call| {
            serde_json::to_vec(&call.arguments)
                .ok()
                .map(|args| (call.name.as_str(), args))
        })
        .collect();
    if entries.is_empty() {
        return None;
    }
    entries.sort_by(|a, b| a.0.cmp(b.0).then_with(|| a.1.cmp(&b.1)));

    let mut hasher = DefaultHasher::new();
    let mut tool_names: Vec<String> = Vec::new();
    for (name, args) in &entries {
        name.hash(&mut hasher);
        args.hash(&mut hasher);
        if !tool_names.iter().any(|n| n == name) {
            tool_names.push((*name).to_string());
        }
    }
    entries.len().hash(&mut hasher);
    Some((hasher.finish(), tool_names))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call(name: &str, value: i64) -> ToolCall {
        ToolCall {
            provider_id: "id".into(),
            name: name.into(),
            arguments: json!({ "value": value }),
        }
    }

    fn todo(value: i64) -> ToolCall {
        call(crate::todo::TOOL_NAME, value)
    }

    fn record(loop_detector: &mut DoomLoop, name: &str, value: i64) {
        loop_detector.record_turn(&[call(name, value)]);
    }

    #[test]
    fn empty_and_below_threshold_do_not_fire() {
        let mut detector = DoomLoop::new(true, 3);

        detector.record_turn(&[]);
        assert!(detector.check().is_none());

        record(&mut detector, "search", 1);
        record(&mut detector, "search", 1);
        assert!(detector.check().is_none());
    }

    #[test]
    fn exactly_threshold_identical_fires() {
        let mut detector = DoomLoop::new(true, 3);

        record(&mut detector, "search", 1);
        record(&mut detector, "search", 1);
        record(&mut detector, "search", 1);

        let message = detector.check();
        assert!(message.as_deref().is_some_and(
            |msg| msg.contains("You have called search with the same arguments 3 times")
        ));
    }

    #[test]
    fn multiple_calls_are_one_turn_fingerprint() {
        let mut detector = DoomLoop::new(true, 2);

        detector.record_turn(&[call("search", 1), call("open", 2)]);
        assert!(detector.check().is_none());
        detector.record_turn(&[call("search", 1), call("open", 2)]);

        let message = detector.check().expect("should fire");
        assert!(message.contains("You have called"));
        assert!(message.contains("search"));
        assert!(message.contains("open"));
        assert!(message.contains("with the same arguments 2 times"));
    }

    #[test]
    fn parallel_tool_calls_are_order_independent() {
        let mut detector = DoomLoop::new(true, 2);

        detector.record_turn(&[call("search", 1), call("open", 2)]);
        // Same calls in reverse order — should hash identically.
        detector.record_turn(&[call("open", 2), call("search", 1)]);

        assert!(detector.check().is_some());
    }

    #[test]
    fn fire_clears_buffer_and_requires_threshold_more_turns() {
        let mut detector = DoomLoop::new(true, 2);

        record(&mut detector, "search", 1);
        record(&mut detector, "search", 1);
        assert!(detector.check().is_some());
        assert!(detector.check().is_none());

        record(&mut detector, "search", 1);
        assert!(detector.check().is_none());
        record(&mut detector, "search", 1);
        assert!(detector.check().is_some());
    }

    #[test]
    fn reset_clears_state() {
        let mut detector = DoomLoop::new(true, 2);

        record(&mut detector, "search", 1);
        record(&mut detector, "search", 1);
        detector.reset();
        assert!(detector.check().is_none());
        assert!(detector.calls.is_empty());
        assert!(detector.tool_names_per_turn.is_empty());

        // After reset, need full threshold of new turns to fire.
        record(&mut detector, "search", 1);
        assert!(detector.check().is_none());
        record(&mut detector, "search", 1);
        assert!(detector.check().is_some());
    }

    #[test]
    fn abab_pattern_fires() {
        let mut detector = DoomLoop::new(true, 3);

        for (name, value) in [
            ("search", 1),
            ("open", 2),
            ("search", 1),
            ("open", 2),
            ("search", 1),
            ("open", 2),
        ] {
            record(&mut detector, name, value);
        }

        assert!(detector.check().is_some());
    }

    #[test]
    fn abcabc_pattern_fires() {
        let mut detector = DoomLoop::new(true, 3);

        for (name, value) in [
            ("search", 1),
            ("open", 2),
            ("read", 3),
            ("search", 1),
            ("open", 2),
            ("read", 3),
            ("search", 1),
            ("open", 2),
            ("read", 3),
        ] {
            record(&mut detector, name, value);
        }

        assert!(detector.check().is_some());
    }

    #[test]
    fn abcd_width_4_pattern_fires() {
        let mut detector = DoomLoop::new(true, 3);

        let cycle = [("a", 1), ("b", 2), ("c", 3), ("d", 4)];
        for _ in 0..3 {
            for (name, value) in cycle {
                record(&mut detector, name, value);
            }
        }

        assert!(detector.check().is_some());
    }

    #[test]
    fn width_5_pattern_does_not_fire() {
        let mut detector = DoomLoop::new(true, 3);

        let cycle = [("a", 1), ("b", 2), ("c", 3), ("d", 4), ("e", 5)];
        for _ in 0..3 {
            for (name, value) in cycle {
                record(&mut detector, name, value);
            }
        }

        // Width 5 is beyond MAX_CYCLIC_WIDTH; even though the pattern is
        // perfectly periodic, the detector must not report it.
        assert!(detector.check().is_none());
    }

    #[test]
    fn cyclic_message_includes_full_sequence() {
        let mut detector = DoomLoop::new(true, 3);

        for (name, value) in [
            ("search", 1),
            ("open", 2),
            ("search", 1),
            ("open", 2),
            ("search", 1),
            ("open", 2),
        ] {
            record(&mut detector, name, value);
        }

        let message = detector.check().expect("should fire");
        // The full cycle should be reported, not just the last turn.
        assert!(
            message.contains("search -> open"),
            "message should include full cycle, got: {message}"
        );
    }

    #[test]
    fn broken_pattern_does_not_fire() {
        let mut detector = DoomLoop::new(true, 3);

        for (name, value) in [
            ("search", 1),
            ("open", 2),
            ("search", 1),
            ("read", 3),
            ("search", 1),
            ("open", 2),
        ] {
            record(&mut detector, name, value);
        }

        assert!(detector.check().is_none());
    }

    #[test]
    fn disabled_never_fires_and_does_not_preallocate() {
        let mut detector = DoomLoop::new(false, 3);
        assert_eq!(detector.calls.capacity(), 0);
        assert_eq!(detector.tool_names_per_turn.capacity(), 0);

        for _ in 0..20 {
            record(&mut detector, "search", 1);
        }

        assert!(detector.check().is_none());
    }

    #[test]
    fn todo_calls_are_excluded_from_fingerprint() {
        let mut detector = DoomLoop::new(true, 2);

        detector.record_turn(&[todo(1), call("search", 1)]);
        detector.record_turn(&[todo(2), call("search", 1)]);

        assert!(detector.check().is_some());
    }

    #[test]
    fn todo_only_turns_do_not_add_to_buffer() {
        let mut detector = DoomLoop::new(true, 2);

        detector.record_turn(&[todo(1)]);
        detector.record_turn(&[todo(2)]);
        detector.record_turn(&[todo(3)]);
        assert!(detector.calls.is_empty());
        assert!(detector.tool_names_per_turn.is_empty());
        assert!(detector.check().is_none());
    }

    #[test]
    fn threshold_is_clamped() {
        let mut low = DoomLoop::new(true, 1);
        assert_eq!(low.threshold, 2);
        record(&mut low, "search", 1);
        assert!(low.check().is_none());
        record(&mut low, "search", 1);
        assert!(low.check().is_some());

        let high = DoomLoop::new(true, 100);
        assert_eq!(high.threshold, 16);
    }
}
