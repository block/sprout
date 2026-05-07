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
    last_tool_names: Vec<String>,
}

impl DoomLoop {
    pub fn new(enabled: bool, threshold: usize) -> Self {
        Self {
            enabled,
            threshold: threshold.clamp(MIN_THRESHOLD, MAX_THRESHOLD),
            calls: if enabled {
                VecDeque::with_capacity(MAX_BUFFER)
            } else {
                VecDeque::new()
            },
            last_tool_names: Vec::new(),
        }
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
        }
        self.calls.push_back(fingerprint);
        self.last_tool_names = tool_names;
    }

    pub fn check(&mut self) -> Option<String> {
        if !self.enabled || self.calls.len() < self.threshold {
            return None;
        }
        let width = self.repeated_width()?;
        let tools = self.tool_label();
        tracing::warn!(
            tools = %tools,
            threshold = self.threshold,
            pattern_width = width,
            "doom loop detected"
        );
        self.calls.clear();
        self.last_tool_names.clear();
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

    fn tool_label(&self) -> String {
        if self.last_tool_names.is_empty() {
            "the same tool turn".into()
        } else {
            self.last_tool_names.join(", ")
        }
    }
}

fn fingerprint_turn(calls: &[ToolCall]) -> Option<(u64, Vec<String>)> {
    let mut hasher = DefaultHasher::new();
    let mut tool_names = Vec::new();
    let mut count = 0usize;

    for call in calls
        .iter()
        .filter(|call| !SKIPPED_TOOL_NAMES.contains(&call.name.as_str()))
    {
        call.name.hash(&mut hasher);
        serde_json::to_vec(&call.arguments).ok()?.hash(&mut hasher);
        if !tool_names.iter().any(|name| name == &call.name) {
            tool_names.push(call.name.clone());
        }
        count = count.saturating_add(1);
    }

    (count > 0).then(|| {
        count.hash(&mut hasher);
        (hasher.finish(), tool_names)
    })
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

        let message = detector.check();
        assert!(message.as_deref().is_some_and(
            |msg| msg.contains("You have called search, open with the same arguments 2 times")
        ));
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
