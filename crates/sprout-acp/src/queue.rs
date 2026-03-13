//! Event queue state machine for sprout-acp.
//!
//! Manages per-channel event queues with a global one-in-flight constraint.
//! When the harness is ready to prompt the agent, it flushes the channel with
//! the oldest pending event, draining ALL events for that channel into a single
//! batch. Only one `session/prompt` is in flight at a time across all channels.
//!
//! ## Dedup modes
//!
//! - **Drop** (default) — while a prompt is in-flight for channel C, new events
//!   for channel C are silently dropped (debug-logged). Events for other channels
//!   still queue normally.
//! - **Queue** — all events accumulate; batched on the next flush cycle.

use nostr::{Event, ToBech32};
use std::collections::{HashMap, VecDeque};
use std::time::Instant;
use uuid::Uuid;

use crate::config::DedupMode;

// ── Types ─────────────────────────────────────────────────────────────────────

/// An event waiting in the queue.
#[derive(Debug, Clone)]
pub struct QueuedEvent {
    pub channel_id: Uuid,
    pub event: Event,
    pub received_at: Instant,
    /// Tag identifying which rule (or mode) matched this event.
    pub prompt_tag: String,
}

/// A single event inside a [`FlushBatch`].
#[derive(Debug)]
pub struct BatchEvent {
    pub event: Event,
    pub prompt_tag: String,
}

/// A batch of events to prompt the agent with.
#[derive(Debug)]
pub struct FlushBatch {
    pub channel_id: Uuid,
    pub events: Vec<BatchEvent>,
}

// ── EventQueue ────────────────────────────────────────────────────────────────

/// Per-channel event queue with global one-in-flight enforcement.
///
/// # State Machine
///
/// ```text
/// State:
///   queues:            Map<channel_id, VecDeque<QueuedEvent>>
///   in_flight_channel: Option<Uuid>
///   dedup_mode:        DedupMode
///
/// Transitions:
///   push(event):
///     if dedup_mode == Drop AND in_flight_channel == Some(event.channel_id):
///       debug log + discard
///     else:
///       queues[event.channel_id].push_back(event)
///
///   flush_next() → Option<FlushBatch>:
///     if in_flight_channel.is_some(): return None
///     if all queues empty: return None
///     channel = pick channel with oldest head event (min received_at)
///     events = drain queues[channel]
///     in_flight_channel = Some(channel)
///     return Some(FlushBatch { channel, events })
///
///   mark_complete():
///     in_flight_channel = None
/// ```
pub struct EventQueue {
    queues: HashMap<Uuid, VecDeque<QueuedEvent>>,
    in_flight_channel: Option<Uuid>,
    dedup_mode: DedupMode,
}

impl EventQueue {
    /// Create a new empty event queue with the given dedup mode.
    pub fn new(dedup_mode: DedupMode) -> Self {
        Self {
            queues: HashMap::new(),
            in_flight_channel: None,
            dedup_mode,
        }
    }

    /// Push an event into the queue for its channel.
    ///
    /// In [`DedupMode::Drop`], events for the currently in-flight channel are
    /// silently discarded (debug-logged).
    pub fn push(&mut self, event: QueuedEvent) {
        if matches!(self.dedup_mode, DedupMode::Drop)
            && self.in_flight_channel == Some(event.channel_id)
        {
            tracing::debug!(
                channel_id = %event.channel_id,
                "dropping event for in-flight channel (drop mode)"
            );
            return;
        }
        self.queues
            .entry(event.channel_id)
            .or_default()
            .push_back(event);
    }

    /// Try to flush the next batch.
    ///
    /// Returns `None` if a prompt is already in flight or if all queues are
    /// empty. Otherwise picks the channel with the oldest pending event (FIFO
    /// fairness across channels), drains ALL events for that channel into a
    /// single batch, sets `in_flight_channel`, and returns the batch.
    pub fn flush_next(&mut self) -> Option<FlushBatch> {
        if self.in_flight_channel.is_some() {
            return None;
        }

        // Find the channel whose head event has the oldest received_at.
        let channel_id = self
            .queues
            .iter()
            .filter(|(_, q)| !q.is_empty())
            .min_by_key(|(_, q)| q.front().unwrap().received_at)
            .map(|(id, _)| *id)?;

        // Drain ALL events for that channel.
        let queue = self.queues.remove(&channel_id)?;
        let events: Vec<BatchEvent> = queue
            .into_iter()
            .map(|qe| BatchEvent {
                event: qe.event,
                prompt_tag: qe.prompt_tag,
            })
            .collect();

        self.in_flight_channel = Some(channel_id);

        Some(FlushBatch { channel_id, events })
    }

    /// Mark the current prompt as complete. Clears `in_flight_channel`.
    pub fn mark_complete(&mut self) {
        self.in_flight_channel = None;
    }

    /// Re-queue a batch of events that failed to process.
    ///
    /// Events are pushed back to the **front** of the channel's queue so they
    /// are processed first on the next flush cycle. This prevents event loss
    /// when session creation or `session/prompt` fails transiently.
    ///
    /// Note: `received_at` is reset to `Instant::now()` for re-queued events.
    /// This means a re-queued channel competes fairly with other channels rather
    /// than always winning due to stale timestamps.
    pub fn requeue(&mut self, batch: FlushBatch) {
        let queue = self.queues.entry(batch.channel_id).or_default();
        // Push to front in reverse order so original order is preserved.
        for be in batch.events.into_iter().rev() {
            queue.push_front(QueuedEvent {
                channel_id: batch.channel_id,
                event: be.event,
                prompt_tag: be.prompt_tag,
                received_at: Instant::now(),
            });
        }
    }

    /// Whether a prompt is currently in flight.
    #[allow(dead_code)]
    pub fn is_in_flight(&self) -> bool {
        self.in_flight_channel.is_some()
    }

    /// Total number of pending events across all channels.
    #[allow(dead_code)]
    pub fn pending_count(&self) -> usize {
        self.queues.values().map(|q| q.len()).sum()
    }

    /// Number of channels with pending events.
    #[allow(dead_code)]
    pub fn pending_channels(&self) -> usize {
        self.queues.len()
    }
}

impl Default for EventQueue {
    fn default() -> Self {
        Self::new(DedupMode::Drop)
    }
}

// ── Prompt formatting ─────────────────────────────────────────────────────────

/// Stream-message kinds — these get the compact format (no raw Tags line).
const STREAM_MESSAGE_KINDS: &[u32] = &[
    sprout_core::kind::KIND_STREAM_MESSAGE,
    sprout_core::kind::KIND_STREAM_MESSAGE_V2,
];

/// Format the per-event block lines for a single [`BatchEvent`].
///
/// Non-stream-message kinds (anything not in `[40001, 40002]`) include a
/// `Tags:` line with the raw Nostr tags serialised as a JSON array-of-arrays.
fn format_event_block(channel_id: Uuid, be: &BatchEvent) -> String {
    let npub = be
        .event
        .pubkey
        .to_bech32()
        .unwrap_or_else(|_| be.event.pubkey.to_hex());

    let time = chrono::DateTime::from_timestamp(be.event.created_at.as_u64() as i64, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| be.event.created_at.as_u64().to_string());

    let kind = be.event.kind.as_u16() as u32;

    let mut block = format!(
        "Channel: {channel_id}\nKind: {kind}\nFrom: {npub}\nTime: {time}\nContent: {}",
        be.event.content,
    );

    // Include raw tags for non-stream-message kinds.
    if !STREAM_MESSAGE_KINDS.contains(&kind) {
        let tags_json: Vec<&[String]> = be.event.tags.iter().map(|t| t.as_slice()).collect();
        if let Ok(tags_str) = serde_json::to_string(&tags_json) {
            block.push_str(&format!("\nTags: {tags_str}"));
        }
    }

    block
}

/// Format a [`FlushBatch`] into a prompt string for the agent.
///
/// If `system_prompt` is `Some`, it is prepended as a `[System]` block.
///
/// Single-event format:
/// ```text
/// [Sprout event: <prompt_tag>]
/// Channel: ...
/// Kind: ...
/// From: ...
/// Time: ...
/// Content: ...
/// ```
///
/// Batch format (N > 1):
/// ```text
/// [Sprout events — N events]
///
/// --- Event 1 (<prompt_tag>) ---
/// Channel: ...
/// ...
/// ```
pub fn format_prompt(batch: &FlushBatch, system_prompt: Option<&str>) -> String {
    let body = if batch.events.len() == 1 {
        let be = &batch.events[0];
        format!(
            "[Sprout event: {}]\n{}",
            be.prompt_tag,
            format_event_block(batch.channel_id, be)
        )
    } else {
        let mut s = format!("[Sprout events — {} events]", batch.events.len());
        for (i, be) in batch.events.iter().enumerate() {
            s.push_str(&format!(
                "\n\n--- Event {} ({}) ---\n{}",
                i + 1,
                be.prompt_tag,
                format_event_block(batch.channel_id, be)
            ));
        }
        s
    };

    match system_prompt {
        Some(sp) => format!("[System]\n{sp}\n\n{body}"),
        None => body,
    }
}

// ─── Unit Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind};
    use std::time::Duration;

    /// Build a test event with the given content and kind.
    fn make_event(content: &str) -> Event {
        let keys = Keys::generate();
        EventBuilder::new(Kind::Custom(40001), content, [])
            .sign_with_keys(&keys)
            .unwrap()
    }

    /// Build a QueuedEvent for the given channel.
    fn make_queued(channel_id: Uuid, content: &str) -> QueuedEvent {
        QueuedEvent {
            channel_id,
            event: make_event(content),
            received_at: Instant::now(),
            prompt_tag: "test".into(),
        }
    }

    /// Build a QueuedEvent with a specific `received_at` offset from now.
    fn make_queued_at(channel_id: Uuid, content: &str, age: Duration) -> QueuedEvent {
        QueuedEvent {
            channel_id,
            event: make_event(content),
            received_at: Instant::now() - age,
            prompt_tag: "test".into(),
        }
    }

    // ── Test 1: push + flush_next basic ──────────────────────────────────────

    #[test]
    fn test_push_flush_basic() {
        let mut q = EventQueue::new(DedupMode::Queue);
        let ch = Uuid::new_v4();

        q.push(make_queued(ch, "hello"));

        let batch = q.flush_next().expect("should return a batch");
        assert_eq!(batch.channel_id, ch);
        assert_eq!(batch.events.len(), 1);
        assert_eq!(batch.events[0].event.content, "hello");

        // Queue should be empty now.
        assert_eq!(q.pending_count(), 0);
        assert_eq!(q.pending_channels(), 0);
    }

    // ── Test 2: in_flight blocks flush ───────────────────────────────────────

    #[test]
    fn test_in_flight_blocks_flush() {
        let mut q = EventQueue::new(DedupMode::Queue);
        let ch = Uuid::new_v4();

        q.push(make_queued(ch, "first"));
        let _batch = q.flush_next().expect("first flush should succeed");
        assert!(q.is_in_flight());

        // Push another event while in-flight.
        q.push(make_queued(ch, "second"));

        // flush_next must return None while in-flight.
        assert!(q.flush_next().is_none());
    }

    // ── Test 3: mark_complete enables flush ──────────────────────────────────

    #[test]
    fn test_mark_complete_enables_flush() {
        let mut q = EventQueue::new(DedupMode::Queue);
        let ch = Uuid::new_v4();

        q.push(make_queued(ch, "first"));
        let _batch = q.flush_next().expect("first flush should succeed");

        // Push while in-flight; flush blocked.
        q.push(make_queued(ch, "second"));
        assert!(q.flush_next().is_none());

        // Complete the in-flight prompt.
        q.mark_complete();
        assert!(!q.is_in_flight());

        // Now flush should succeed.
        let batch = q.flush_next().expect("should flush after mark_complete");
        assert_eq!(batch.channel_id, ch);
        assert_eq!(batch.events.len(), 1);
        assert_eq!(batch.events[0].event.content, "second");
    }

    // ── Test 4: batch drain ───────────────────────────────────────────────────

    #[test]
    fn test_batch_drain_all_events() {
        let mut q = EventQueue::new(DedupMode::Queue);
        let ch = Uuid::new_v4();

        q.push(make_queued(ch, "msg1"));
        q.push(make_queued(ch, "msg2"));
        q.push(make_queued(ch, "msg3"));

        assert_eq!(q.pending_count(), 3);

        let batch = q.flush_next().expect("should return batch");
        assert_eq!(batch.channel_id, ch);
        assert_eq!(batch.events.len(), 3);
        assert_eq!(batch.events[0].event.content, "msg1");
        assert_eq!(batch.events[1].event.content, "msg2");
        assert_eq!(batch.events[2].event.content, "msg3");

        // All drained.
        assert_eq!(q.pending_count(), 0);
        assert_eq!(q.pending_channels(), 0);
    }

    // ── Test 5: FIFO fairness ─────────────────────────────────────────────────

    #[test]
    fn test_fifo_fairness_picks_oldest_channel() {
        let mut q = EventQueue::new(DedupMode::Queue);
        let ch_a = Uuid::new_v4();
        let ch_b = Uuid::new_v4();

        // Channel A has an older event (2 seconds ago), B has a newer one (1 second ago).
        q.push(make_queued_at(ch_a, "from A", Duration::from_secs(2)));
        q.push(make_queued_at(ch_b, "from B", Duration::from_secs(1)));

        let batch = q.flush_next().expect("should return batch");
        // A is older, so it should be picked first.
        assert_eq!(batch.channel_id, ch_a);
        assert_eq!(batch.events[0].event.content, "from A");
    }

    // ── Test 6: multi-channel interleave ─────────────────────────────────────

    #[test]
    fn test_multi_channel_interleave() {
        let mut q = EventQueue::new(DedupMode::Queue);
        let ch_a = Uuid::new_v4();
        let ch_b = Uuid::new_v4();

        // A is older.
        q.push(make_queued_at(ch_a, "A-event", Duration::from_secs(2)));
        q.push(make_queued_at(ch_b, "B-event", Duration::from_secs(1)));

        // First flush picks A.
        let batch_a = q.flush_next().expect("first flush");
        assert_eq!(batch_a.channel_id, ch_a);
        assert!(q.is_in_flight());

        // B still pending.
        assert_eq!(q.pending_count(), 1);
        assert_eq!(q.pending_channels(), 1);

        q.mark_complete();

        // Second flush picks B.
        let batch_b = q.flush_next().expect("second flush");
        assert_eq!(batch_b.channel_id, ch_b);
        assert_eq!(batch_b.events[0].event.content, "B-event");

        assert_eq!(q.pending_count(), 0);
    }

    // ── Test 7: empty queue returns None ─────────────────────────────────────

    #[test]
    fn test_empty_queue_returns_none() {
        let mut q = EventQueue::new(DedupMode::Queue);
        assert!(q.flush_next().is_none());
    }

    // ── Test 8: pending_count ─────────────────────────────────────────────────

    #[test]
    fn test_pending_count() {
        let mut q = EventQueue::new(DedupMode::Queue);
        let ch_a = Uuid::new_v4();
        let ch_b = Uuid::new_v4();

        assert_eq!(q.pending_count(), 0);
        assert_eq!(q.pending_channels(), 0);

        q.push(make_queued(ch_a, "a1"));
        q.push(make_queued(ch_a, "a2"));
        q.push(make_queued(ch_b, "b1"));

        assert_eq!(q.pending_count(), 3);
        assert_eq!(q.pending_channels(), 2);

        // Flush A (2 events drained).
        let _ = q.flush_next();
        assert_eq!(q.pending_count(), 1);
        assert_eq!(q.pending_channels(), 1);

        q.mark_complete();

        // Flush B (1 event drained).
        let _ = q.flush_next();
        assert_eq!(q.pending_count(), 0);
        assert_eq!(q.pending_channels(), 0);
    }

    // ── Test 9: format_prompt single event ───────────────────────────────────

    #[test]
    fn test_format_prompt_single() {
        let ch = Uuid::new_v4();
        let event = make_event("Hello @agent");
        let npub = event
            .pubkey
            .to_bech32()
            .unwrap_or_else(|_| event.pubkey.to_hex());

        let batch = FlushBatch {
            channel_id: ch,
            events: vec![BatchEvent {
                event,
                prompt_tag: "@mention".into(),
            }],
        };

        let prompt = format_prompt(&batch, None);

        assert!(prompt.starts_with("[Sprout event: @mention]\n"));
        assert!(prompt.contains(&format!("Channel: {}", ch)));
        assert!(prompt.contains(&format!("From: {}", npub)));
        assert!(prompt.contains("Content: Hello @agent"));
        // Should NOT contain "--- Event 1 ---" (that's the multi-event format).
        assert!(!prompt.contains("--- Event 1 ---"));
    }

    // ── Test 9b: requeue preserves events ────────────────────────────────────

    #[test]
    fn test_requeue_preserves_events() {
        let mut queue = EventQueue::new(DedupMode::Queue);
        let ch = Uuid::new_v4();
        queue.push(make_queued(ch, "msg1"));
        queue.push(make_queued(ch, "msg2"));

        let batch = queue.flush_next().unwrap();
        assert_eq!(batch.events.len(), 2);
        assert!(queue.is_in_flight());

        // Simulate failure — requeue the batch.
        queue.requeue(batch);
        queue.mark_complete();

        // Should be able to flush again and get the same events in order.
        let batch2 = queue.flush_next().unwrap();
        assert_eq!(batch2.events.len(), 2);
        assert_eq!(batch2.events[0].event.content, "msg1");
        assert_eq!(batch2.events[1].event.content, "msg2");
    }

    #[test]
    fn test_requeue_interleaves_with_other_channels() {
        let mut queue = EventQueue::new(DedupMode::Queue);
        let ch_a = Uuid::new_v4();
        let ch_b = Uuid::new_v4();

        // ch_a has an older event.
        queue.push(make_queued_at(ch_a, "A-old", Duration::from_secs(5)));
        queue.push(make_queued_at(ch_b, "B-new", Duration::from_secs(1)));

        // Flush ch_a first (older).
        let batch_a = queue.flush_next().unwrap();
        assert_eq!(batch_a.channel_id, ch_a);

        // Requeue ch_a (simulating failure) and complete.
        queue.requeue(batch_a);
        queue.mark_complete();

        // After requeue, ch_a's received_at is reset to now, so ch_b (older) goes first.
        let next_batch = queue.flush_next().unwrap();
        assert_eq!(next_batch.channel_id, ch_b);
    }

    // ── Test 10: format_prompt batch ─────────────────────────────────────────

    #[test]
    fn test_format_prompt_batch() {
        let ch = Uuid::new_v4();
        let e1 = make_event("first message");
        let e2 = make_event("second message");
        let e3 = make_event("third message");

        let batch = FlushBatch {
            channel_id: ch,
            events: vec![
                BatchEvent {
                    event: e1,
                    prompt_tag: "tag-a".into(),
                },
                BatchEvent {
                    event: e2,
                    prompt_tag: "tag-b".into(),
                },
                BatchEvent {
                    event: e3,
                    prompt_tag: "tag-c".into(),
                },
            ],
        };

        let prompt = format_prompt(&batch, None);

        assert!(prompt.starts_with("[Sprout events — 3 events]"));
        assert!(prompt.contains("--- Event 1 (tag-a) ---"));
        assert!(prompt.contains("--- Event 2 (tag-b) ---"));
        assert!(prompt.contains("--- Event 3 (tag-c) ---"));
        assert!(prompt.contains("Content: first message"));
        assert!(prompt.contains("Content: second message"));
        assert!(prompt.contains("Content: third message"));
        // All events reference the same channel.
        assert_eq!(
            prompt.matches(&format!("Channel: {}", ch)).count(),
            3,
            "each event block should include the channel id"
        );
    }

    // ── Test 11: system prompt prepended ─────────────────────────────────────

    #[test]
    fn test_format_prompt_with_system_prompt() {
        let ch = Uuid::new_v4();
        let event = make_event("hello");

        let batch = FlushBatch {
            channel_id: ch,
            events: vec![BatchEvent {
                event,
                prompt_tag: "test".into(),
            }],
        };

        let prompt = format_prompt(&batch, Some("You are a triage bot."));
        assert!(prompt.starts_with("[System]\nYou are a triage bot.\n\n[Sprout event: test]\n"));
    }

    // ── Test 12: drop mode discards in-flight channel events ─────────────────

    #[test]
    fn test_drop_mode_discards_in_flight_events() {
        let mut q = EventQueue::new(DedupMode::Drop);
        let ch = Uuid::new_v4();

        q.push(make_queued(ch, "first"));
        let _batch = q.flush_next().expect("first flush");
        assert!(q.is_in_flight());

        // In drop mode, pushing to the in-flight channel should be discarded.
        q.push(make_queued(ch, "dropped"));
        assert_eq!(q.pending_count(), 0, "event should be dropped");

        q.mark_complete();
        // Nothing to flush.
        assert!(q.flush_next().is_none());
    }

    // ── Test 13: drop mode still queues other channels ────────────────────────

    #[test]
    fn test_drop_mode_queues_other_channels() {
        let mut q = EventQueue::new(DedupMode::Drop);
        let ch_a = Uuid::new_v4();
        let ch_b = Uuid::new_v4();

        q.push(make_queued(ch_a, "A-first"));
        let _batch = q.flush_next().expect("flush A");
        assert!(q.is_in_flight());

        // Events for ch_b should still queue.
        q.push(make_queued(ch_b, "B-event"));
        assert_eq!(q.pending_count(), 1);

        q.mark_complete();
        let batch_b = q.flush_next().expect("flush B");
        assert_eq!(batch_b.channel_id, ch_b);
    }
}
