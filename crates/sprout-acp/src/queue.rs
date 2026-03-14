//! Event queue state machine for sprout-acp.
//!
//! Manages per-channel event queues with per-channel in-flight tracking.
//! When the harness is ready to prompt the agent, it flushes the channel with
//! the oldest pending event, draining ALL events for that channel into a single
//! batch. Multiple channels can be in-flight simultaneously; each channel is
//! independent.
//!
//! ## Dedup modes
//!
//! - **Drop** (default) — while a prompt is in-flight for channel C, new events
//!   for channel C are silently dropped (debug-logged). Events for other channels
//!   still queue normally.
//! - **Queue** — all events accumulate; batched on the next flush cycle.

use nostr::{Event, ToBech32};
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};
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
#[derive(Debug, Clone)]
pub struct BatchEvent {
    pub event: Event,
    pub prompt_tag: String,
    pub received_at: Instant,
}

/// A batch of events to prompt the agent with.
#[derive(Debug, Clone)]
pub struct FlushBatch {
    pub channel_id: Uuid,
    pub events: Vec<BatchEvent>,
}

// ── EventQueue ────────────────────────────────────────────────────────────────

/// Per-channel event queue with per-channel in-flight enforcement.
///
/// # State Machine
///
/// ```text
/// State:
///   queues:            Map<channel_id, VecDeque<QueuedEvent>>
///   in_flight_channels: HashSet<Uuid>
///   retry_after:       Map<channel_id, Instant>
///   dedup_mode:        DedupMode
///
/// Transitions:
///   push(event):
///     if dedup_mode == Drop AND in_flight_channels.contains(event.channel_id):
///       debug log + discard
///     else:
///       queues[event.channel_id].push_back(event)
///
///   flush_next() → Option<FlushBatch>:
///     candidates = channels where queue non-empty
///                  AND NOT in in_flight_channels
///                  AND (no retry_after OR retry_after[c] <= now)
///     if candidates empty: return None
///     channel = pick candidate with oldest head event (min received_at)
///     events = drain queues[channel]
///     in_flight_channels.insert(channel)
///     return Some(FlushBatch { channel, events })
///
///   mark_complete(channel_id):
///     in_flight_channels.remove(channel_id)
///     (retry_after entries expire naturally via Instant check)
/// ```
pub struct EventQueue {
    queues: HashMap<Uuid, VecDeque<QueuedEvent>>,
    in_flight_channels: HashSet<Uuid>,
    retry_after: HashMap<Uuid, Instant>,
    dedup_mode: DedupMode,
}

impl EventQueue {
    /// Create a new empty event queue with the given dedup mode.
    pub fn new(dedup_mode: DedupMode) -> Self {
        Self {
            queues: HashMap::new(),
            in_flight_channels: HashSet::new(),
            retry_after: HashMap::new(),
            dedup_mode,
        }
    }

    /// Push an event into the queue for its channel.
    ///
    /// In [`DedupMode::Drop`], events for any currently in-flight channel are
    /// silently discarded (debug-logged).
    pub fn push(&mut self, event: QueuedEvent) {
        if matches!(self.dedup_mode, DedupMode::Drop)
            && self.in_flight_channels.contains(&event.channel_id)
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
    /// Returns `None` if all non-in-flight, non-throttled queues are empty.
    /// Otherwise picks the channel with the oldest pending event (FIFO fairness
    /// across channels), drains ALL events for that channel into a single batch,
    /// inserts into `in_flight_channels`, and returns the batch.
    pub fn flush_next(&mut self) -> Option<FlushBatch> {
        let now = Instant::now();

        // Find the channel whose head event has the oldest received_at,
        // excluding in-flight channels and throttled channels.
        let channel_id = self
            .queues
            .iter()
            .filter(|(id, q)| {
                !q.is_empty()
                    && !self.in_flight_channels.contains(id)
                    && self.retry_after.get(id).map_or(true, |&t| t <= now)
            })
            .min_by_key(|(_, q)| q.front().unwrap().received_at)
            .map(|(id, _)| *id)?;

        // Drain ALL events for that channel.
        let queue = self.queues.remove(&channel_id)?;
        let events: Vec<BatchEvent> = queue
            .into_iter()
            .map(|qe| BatchEvent {
                event: qe.event,
                prompt_tag: qe.prompt_tag,
                received_at: qe.received_at,
            })
            .collect();

        self.in_flight_channels.insert(channel_id);

        Some(FlushBatch { channel_id, events })
    }

    /// Mark the prompt for `channel_id` as complete.
    ///
    /// Removes the channel from `in_flight_channels`. Does NOT clear
    /// `retry_after` — those entries expire naturally via Instant check.
    pub fn mark_complete(&mut self, channel_id: Uuid) {
        self.in_flight_channels.remove(&channel_id);
    }

    /// Re-queue a batch of events that failed to process.
    ///
    /// Events are pushed back to the **front** of the channel's queue so they
    /// are processed first on the next flush cycle. This prevents event loss
    /// when session creation or `session/prompt` fails transiently.
    ///
    /// `received_at` is reset to `Instant::now()` for re-queued events.
    /// A 5-second `retry_after` throttle is set so the channel is not
    /// immediately re-flushed.
    ///
    /// Note: does NOT remove from `in_flight_channels` — caller must call
    /// `mark_complete` separately.
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
        self.retry_after
            .insert(batch.channel_id, Instant::now() + Duration::from_secs(5));
    }

    /// Re-queue a batch preserving original `received_at` timestamps.
    ///
    /// Used when a batch was flushed but no agent was available — we want to
    /// retry without penalizing the channel's position in the fairness queue
    /// and without imposing a retry throttle.
    ///
    /// Does NOT set `retry_after`. Does NOT remove from `in_flight_channels` —
    /// caller must call `mark_complete` separately.
    pub fn requeue_preserve_timestamps(&mut self, batch: FlushBatch) {
        let queue = self.queues.entry(batch.channel_id).or_default();
        // Push to front in reverse order so original order is preserved.
        for be in batch.events.into_iter().rev() {
            queue.push_front(QueuedEvent {
                channel_id: batch.channel_id,
                event: be.event,
                prompt_tag: be.prompt_tag,
                received_at: be.received_at,
            });
        }
    }

    /// Returns `true` if any channel has pending events that are not in-flight
    /// and not throttled by `retry_after`.
    pub fn has_flushable_work(&self) -> bool {
        let now = Instant::now();
        self.queues.iter().any(|(id, q)| {
            !q.is_empty()
                && !self.in_flight_channels.contains(id)
                && self.retry_after.get(id).map_or(true, |&t| t <= now)
        })
    }

    /// Whether any prompt is currently in flight.
    #[allow(dead_code)]
    pub fn is_in_flight(&self) -> bool {
        !self.in_flight_channels.is_empty()
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

    // ── Test 2: same channel cannot be flushed twice ─────────────────────────

    #[test]
    fn test_in_flight_blocks_same_channel() {
        let mut q = EventQueue::new(DedupMode::Queue);
        let ch = Uuid::new_v4();

        q.push(make_queued(ch, "first"));
        let _batch = q.flush_next().expect("first flush should succeed");
        assert!(q.is_in_flight());

        // Push another event while in-flight.
        q.push(make_queued(ch, "second"));

        // flush_next for the same channel must return None (it's in-flight).
        // No other channels exist, so result is None.
        assert!(q.flush_next().is_none());
    }

    // ── Test 3: mark_complete enables flush ──────────────────────────────────

    #[test]
    fn test_mark_complete_enables_flush() {
        let mut q = EventQueue::new(DedupMode::Queue);
        let ch = Uuid::new_v4();

        q.push(make_queued(ch, "first"));
        let _batch = q.flush_next().expect("first flush should succeed");

        // Push while in-flight; flush blocked (same channel in-flight).
        q.push(make_queued(ch, "second"));
        assert!(q.flush_next().is_none());

        // Complete the in-flight prompt.
        q.mark_complete(ch);
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

        q.mark_complete(ch_a);

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

        q.mark_complete(ch_a);

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
                received_at: Instant::now(),
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
        queue.mark_complete(ch);

        // retry_after is set, so manually clear it for this test.
        queue.retry_after.remove(&ch);

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
        queue.mark_complete(ch_a);

        // After requeue, ch_a has retry_after set (5s), so ch_b goes first.
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
                    received_at: Instant::now(),
                },
                BatchEvent {
                    event: e2,
                    prompt_tag: "tag-b".into(),
                    received_at: Instant::now(),
                },
                BatchEvent {
                    event: e3,
                    prompt_tag: "tag-c".into(),
                    received_at: Instant::now(),
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
                received_at: Instant::now(),
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

        q.mark_complete(ch);
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

        q.mark_complete(ch_a);
        let batch_b = q.flush_next().expect("flush B");
        assert_eq!(batch_b.channel_id, ch_b);
    }

    // ── Test 14: multiple channels can be in-flight simultaneously ────────────

    #[test]
    fn test_multiple_channels_in_flight_simultaneously() {
        let mut q = EventQueue::new(DedupMode::Queue);
        let ch_a = Uuid::new_v4();
        let ch_b = Uuid::new_v4();

        q.push(make_queued_at(ch_a, "A-event", Duration::from_secs(2)));
        q.push(make_queued_at(ch_b, "B-event", Duration::from_secs(1)));

        // Flush A — now A is in-flight.
        let batch_a = q.flush_next().expect("flush A");
        assert_eq!(batch_a.channel_id, ch_a);
        assert!(q.is_in_flight());

        // Flush B — B should also be flushable (different channel).
        let batch_b = q.flush_next().expect("flush B while A in-flight");
        assert_eq!(batch_b.channel_id, ch_b);

        // Both in-flight.
        assert_eq!(q.in_flight_channels.len(), 2);

        // Complete A only.
        q.mark_complete(ch_a);
        assert!(q.is_in_flight()); // B still in-flight.

        // Complete B.
        q.mark_complete(ch_b);
        assert!(!q.is_in_flight());
    }

    // ── Test 15: same channel cannot be flushed twice ─────────────────────────

    #[test]
    fn test_same_channel_not_flushed_twice() {
        let mut q = EventQueue::new(DedupMode::Queue);
        let ch = Uuid::new_v4();
        let ch2 = Uuid::new_v4();

        q.push(make_queued(ch, "first"));
        let _batch = q.flush_next().expect("first flush");

        // Push more events for same channel while in-flight.
        q.push(make_queued(ch, "second"));
        // Also push for another channel.
        q.push(make_queued(ch2, "other"));

        // flush_next should pick ch2, not ch (ch is in-flight).
        let batch2 = q.flush_next().expect("should flush ch2");
        assert_eq!(batch2.channel_id, ch2);

        // ch still in-flight — no more candidates.
        assert!(q.flush_next().is_none());
    }

    // ── Test 16: drop mode drops events for any in-flight channel ─────────────

    #[test]
    fn test_drop_mode_drops_for_any_in_flight_channel() {
        let mut q = EventQueue::new(DedupMode::Drop);
        let ch_a = Uuid::new_v4();
        let ch_b = Uuid::new_v4();

        q.push(make_queued_at(ch_a, "A-event", Duration::from_secs(2)));
        q.push(make_queued_at(ch_b, "B-event", Duration::from_secs(1)));

        // Flush both — both in-flight.
        let _batch_a = q.flush_next().expect("flush A");
        let _batch_b = q.flush_next().expect("flush B");

        // Drop mode: pushing to either in-flight channel is dropped.
        q.push(make_queued(ch_a, "A-dropped"));
        q.push(make_queued(ch_b, "B-dropped"));
        assert_eq!(q.pending_count(), 0);

        q.mark_complete(ch_a);
        q.mark_complete(ch_b);
    }

    // ── Test 17: flush_next picks oldest non-in-flight, non-throttled channel ─

    #[test]
    fn test_flush_next_picks_oldest_non_throttled() {
        let mut q = EventQueue::new(DedupMode::Queue);
        let ch_a = Uuid::new_v4();
        let ch_b = Uuid::new_v4();
        let ch_c = Uuid::new_v4();

        // A is oldest, B is middle, C is newest.
        q.push(make_queued_at(ch_a, "A", Duration::from_secs(10)));
        q.push(make_queued_at(ch_b, "B", Duration::from_secs(5)));
        q.push(make_queued_at(ch_c, "C", Duration::from_secs(1)));

        // Flush A (oldest).
        let batch = q.flush_next().expect("flush A");
        assert_eq!(batch.channel_id, ch_a);

        // A is in-flight; next oldest non-in-flight is B.
        let batch2 = q.flush_next().expect("flush B");
        assert_eq!(batch2.channel_id, ch_b);

        // A and B in-flight; only C left.
        let batch3 = q.flush_next().expect("flush C");
        assert_eq!(batch3.channel_id, ch_c);

        // All in-flight.
        assert!(q.flush_next().is_none());

        q.mark_complete(ch_a);
        q.mark_complete(ch_b);
        q.mark_complete(ch_c);
    }

    // ── Test 18: mark_complete(channel_id) clears only that channel ───────────

    #[test]
    fn test_mark_complete_clears_only_specified_channel() {
        let mut q = EventQueue::new(DedupMode::Queue);
        let ch_a = Uuid::new_v4();
        let ch_b = Uuid::new_v4();

        q.push(make_queued_at(ch_a, "A", Duration::from_secs(2)));
        q.push(make_queued_at(ch_b, "B", Duration::from_secs(1)));

        let _batch_a = q.flush_next().expect("flush A");
        let _batch_b = q.flush_next().expect("flush B");

        assert_eq!(q.in_flight_channels.len(), 2);

        // Complete only A.
        q.mark_complete(ch_a);
        assert_eq!(q.in_flight_channels.len(), 1);
        assert!(q.in_flight_channels.contains(&ch_b));
        assert!(!q.in_flight_channels.contains(&ch_a));

        // B still in-flight.
        assert!(q.is_in_flight());

        q.mark_complete(ch_b);
        assert!(!q.is_in_flight());
    }

    // ── Test 19: requeue_preserve_timestamps preserves received_at ───────────

    #[test]
    fn test_requeue_preserve_timestamps() {
        let mut q = EventQueue::new(DedupMode::Queue);
        let ch = Uuid::new_v4();
        let old_time = Instant::now() - Duration::from_secs(10);

        q.push(QueuedEvent {
            channel_id: ch,
            event: make_event("old-msg"),
            received_at: old_time,
            prompt_tag: "test".into(),
        });

        let batch = q.flush_next().expect("flush");
        let original_received_at = batch.events[0].received_at;

        // requeue_preserve_timestamps should keep the original timestamp.
        q.requeue_preserve_timestamps(batch);
        q.mark_complete(ch);

        // No retry_after set — should be immediately flushable.
        let batch2 = q.flush_next().expect("flush after requeue_preserve");
        assert_eq!(batch2.events[0].received_at, original_received_at);
    }

    // ── Test 20: requeue_preserve_timestamps does not set retry_after ─────────

    #[test]
    fn test_requeue_preserve_timestamps_no_retry_after() {
        let mut q = EventQueue::new(DedupMode::Queue);
        let ch = Uuid::new_v4();

        q.push(make_queued(ch, "msg"));
        let batch = q.flush_next().expect("flush");

        q.requeue_preserve_timestamps(batch);
        q.mark_complete(ch);

        // No retry_after — channel should be immediately flushable.
        assert!(q.retry_after.get(&ch).is_none());
        assert!(q.flush_next().is_some());
    }

    // ── Test 21: has_flushable_work returns correct results ───────────────────

    #[test]
    fn test_has_flushable_work() {
        let mut q = EventQueue::new(DedupMode::Queue);
        let ch = Uuid::new_v4();

        // Empty queue — no flushable work.
        assert!(!q.has_flushable_work());

        q.push(make_queued(ch, "msg"));
        assert!(q.has_flushable_work());

        // Flush — now in-flight, no flushable work.
        let _batch = q.flush_next().expect("flush");
        assert!(!q.has_flushable_work());

        // Complete — no pending events, no flushable work.
        q.mark_complete(ch);
        assert!(!q.has_flushable_work());

        // Requeue with retry_after — throttled, no flushable work.
        q.push(make_queued(ch, "msg2"));
        let batch2 = q.flush_next().expect("flush2");
        q.requeue(batch2);
        q.mark_complete(ch);
        assert!(!q.has_flushable_work(), "throttled channel should not be flushable");

        // Manually expire the retry_after to simulate time passing.
        q.retry_after.insert(ch, Instant::now() - Duration::from_secs(1));
        assert!(q.has_flushable_work(), "expired throttle should be flushable");
    }

    // ── Test 22: retry throttle blocks re-flush for 5 seconds ─────────────────

    #[test]
    fn test_retry_throttle_blocks_requeue_channel() {
        let mut q = EventQueue::new(DedupMode::Queue);
        let ch = Uuid::new_v4();
        let ch2 = Uuid::new_v4();

        q.push(make_queued(ch, "msg"));
        let batch = q.flush_next().expect("flush");

        // Requeue sets retry_after.
        q.requeue(batch);
        q.mark_complete(ch);

        // Channel is throttled — flush_next should return None (no other channels).
        assert!(q.flush_next().is_none());

        // Add a different channel — it should be flushable.
        q.push(make_queued(ch2, "other"));
        let batch2 = q.flush_next().expect("ch2 should be flushable");
        assert_eq!(batch2.channel_id, ch2);

        // After retry_after expires, ch should be flushable again.
        q.retry_after.insert(ch, Instant::now() - Duration::from_secs(1));
        q.mark_complete(ch2);
        let batch3 = q.flush_next().expect("ch should be flushable after throttle expires");
        assert_eq!(batch3.channel_id, ch);
    }
}
