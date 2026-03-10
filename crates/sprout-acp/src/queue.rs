//! Event queue state machine for sprout-acp.
//!
//! Manages per-channel event queues with a global one-in-flight constraint.
//! When the harness is ready to prompt the agent, it flushes the channel with
//! the oldest pending event, draining ALL events for that channel into a single
//! batch. Only one `session/prompt` is in flight at a time across all channels.

use nostr::{Event, ToBech32};
use std::collections::{HashMap, VecDeque};
use std::time::Instant;
use uuid::Uuid;

/// An event waiting in the queue.
#[derive(Debug, Clone)]
pub struct QueuedEvent {
    pub channel_id: Uuid,
    pub event: Event,
    pub received_at: Instant,
}

/// A batch of events to prompt the agent with.
#[derive(Debug)]
pub struct FlushBatch {
    pub channel_id: Uuid,
    pub events: Vec<Event>,
}

/// Per-channel event queue with global one-in-flight enforcement.
///
/// # State Machine
///
/// ```text
/// State:
///   queues: Map<channel_id, VecDeque<QueuedEvent>>
///   prompt_in_flight: bool
///
/// Transitions:
///   push(event):
///     queues[event.channel_id].push_back(event)
///
///   flush_next() → Option<FlushBatch>:
///     if prompt_in_flight: return None
///     if all queues empty: return None
///     channel = pick channel with oldest head event (min received_at)
///     events = drain queues[channel]
///     remove queues[channel] if now empty
///     prompt_in_flight = true
///     return Some(FlushBatch { channel, events })
///
///   mark_complete():
///     prompt_in_flight = false
/// ```
pub struct EventQueue {
    queues: HashMap<Uuid, VecDeque<QueuedEvent>>,
    prompt_in_flight: bool,
}

impl EventQueue {
    /// Create a new empty event queue.
    pub fn new() -> Self {
        Self {
            queues: HashMap::new(),
            prompt_in_flight: false,
        }
    }

    /// Push an event into the queue for its channel.
    pub fn push(&mut self, event: QueuedEvent) {
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
    /// single batch, sets `prompt_in_flight = true`, and returns the batch.
    pub fn flush_next(&mut self) -> Option<FlushBatch> {
        if self.prompt_in_flight {
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
        let events: Vec<Event> = queue.into_iter().map(|qe| qe.event).collect();

        self.prompt_in_flight = true;

        Some(FlushBatch { channel_id, events })
    }

    /// Mark the current prompt as complete. Clears `prompt_in_flight`.
    pub fn mark_complete(&mut self) {
        self.prompt_in_flight = false;
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
        for event in batch.events.into_iter().rev() {
            queue.push_front(QueuedEvent {
                channel_id: batch.channel_id,
                event,
                received_at: Instant::now(),
            });
        }
    }

    /// Whether a prompt is currently in flight.
    #[allow(dead_code)]
    pub fn is_in_flight(&self) -> bool {
        self.prompt_in_flight
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
        Self::new()
    }
}

/// Format the Channel/From/Time/Message lines for a single event.
fn format_event_lines(channel_id: Uuid, event: &Event) -> String {
    format!(
        "Channel: {}\nFrom: {}\nTime: {}\nMessage: {}",
        channel_id,
        event
            .pubkey
            .to_bech32()
            .unwrap_or_else(|_| event.pubkey.to_hex()),
        chrono::DateTime::from_timestamp(event.created_at.as_u64() as i64, 0)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| event.created_at.as_u64().to_string()),
        event.content,
    )
}

/// Format a batch of events into a prompt string for the agent.
pub fn format_prompt(batch: &FlushBatch) -> String {
    if batch.events.len() == 1 {
        format!(
            "[Sprout @mention]\n{}",
            format_event_lines(batch.channel_id, &batch.events[0])
        )
    } else {
        let mut prompt = format!("[Sprout @mention — {} events]\n", batch.events.len());
        for (i, event) in batch.events.iter().enumerate() {
            prompt.push_str(&format!(
                "\n--- Event {} ---\n{}\n",
                i + 1,
                format_event_lines(batch.channel_id, event)
            ));
        }
        prompt
    }
}

// ─── Unit Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind};
    use std::time::Duration;

    /// Build a test event with the given content.
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
        }
    }

    /// Build a QueuedEvent with a specific `received_at` offset from now.
    fn make_queued_at(channel_id: Uuid, content: &str, age: Duration) -> QueuedEvent {
        QueuedEvent {
            channel_id,
            event: make_event(content),
            received_at: Instant::now() - age,
        }
    }

    // ── Test 1: push + flush_next basic ──────────────────────────────────────

    #[test]
    fn test_push_flush_basic() {
        let mut q = EventQueue::new();
        let ch = Uuid::new_v4();

        q.push(make_queued(ch, "hello"));

        let batch = q.flush_next().expect("should return a batch");
        assert_eq!(batch.channel_id, ch);
        assert_eq!(batch.events.len(), 1);
        assert_eq!(batch.events[0].content, "hello");

        // Queue should be empty now.
        assert_eq!(q.pending_count(), 0);
        assert_eq!(q.pending_channels(), 0);
    }

    // ── Test 2: in_flight blocks flush ───────────────────────────────────────

    #[test]
    fn test_in_flight_blocks_flush() {
        let mut q = EventQueue::new();
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
        let mut q = EventQueue::new();
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
        assert_eq!(batch.events[0].content, "second");
    }

    // ── Test 4: batch drain ───────────────────────────────────────────────────

    #[test]
    fn test_batch_drain_all_events() {
        let mut q = EventQueue::new();
        let ch = Uuid::new_v4();

        q.push(make_queued(ch, "msg1"));
        q.push(make_queued(ch, "msg2"));
        q.push(make_queued(ch, "msg3"));

        assert_eq!(q.pending_count(), 3);

        let batch = q.flush_next().expect("should return batch");
        assert_eq!(batch.channel_id, ch);
        assert_eq!(batch.events.len(), 3);
        assert_eq!(batch.events[0].content, "msg1");
        assert_eq!(batch.events[1].content, "msg2");
        assert_eq!(batch.events[2].content, "msg3");

        // All drained.
        assert_eq!(q.pending_count(), 0);
        assert_eq!(q.pending_channels(), 0);
    }

    // ── Test 5: FIFO fairness ─────────────────────────────────────────────────

    #[test]
    fn test_fifo_fairness_picks_oldest_channel() {
        let mut q = EventQueue::new();
        let ch_a = Uuid::new_v4();
        let ch_b = Uuid::new_v4();

        // Channel A has an older event (2 seconds ago), B has a newer one (1 second ago).
        q.push(make_queued_at(ch_a, "from A", Duration::from_secs(2)));
        q.push(make_queued_at(ch_b, "from B", Duration::from_secs(1)));

        let batch = q.flush_next().expect("should return batch");
        // A is older, so it should be picked first.
        assert_eq!(batch.channel_id, ch_a);
        assert_eq!(batch.events[0].content, "from A");
    }

    // ── Test 6: multi-channel interleave ─────────────────────────────────────

    #[test]
    fn test_multi_channel_interleave() {
        let mut q = EventQueue::new();
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
        assert_eq!(batch_b.events[0].content, "B-event");

        assert_eq!(q.pending_count(), 0);
    }

    // ── Test 7: empty queue returns None ─────────────────────────────────────

    #[test]
    fn test_empty_queue_returns_none() {
        let mut q = EventQueue::new();
        assert!(q.flush_next().is_none());
    }

    // ── Test 8: pending_count ─────────────────────────────────────────────────

    #[test]
    fn test_pending_count() {
        let mut q = EventQueue::new();
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
            events: vec![event],
        };

        let prompt = format_prompt(&batch);

        assert!(prompt.starts_with("[Sprout @mention]\n"));
        assert!(prompt.contains(&format!("Channel: {}", ch)));
        assert!(prompt.contains(&format!("From: {}", npub)));
        assert!(prompt.contains("Message: Hello @agent"));
        // Should NOT contain "--- Event 1 ---" (that's the multi-event format).
        assert!(!prompt.contains("--- Event 1 ---"));
    }

    // ── Test 9b: requeue preserves events ────────────────────────────────────

    #[test]
    fn test_requeue_preserves_events() {
        let mut queue = EventQueue::new();
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
        assert_eq!(batch2.events[0].content, "msg1");
        assert_eq!(batch2.events[1].content, "msg2");
    }

    #[test]
    fn test_requeue_interleaves_with_other_channels() {
        let mut queue = EventQueue::new();
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
            events: vec![e1, e2, e3],
        };

        let prompt = format_prompt(&batch);

        assert!(prompt.starts_with("[Sprout @mention — 3 events]\n"));
        assert!(prompt.contains("--- Event 1 ---"));
        assert!(prompt.contains("--- Event 2 ---"));
        assert!(prompt.contains("--- Event 3 ---"));
        assert!(prompt.contains("Message: first message"));
        assert!(prompt.contains("Message: second message"));
        assert!(prompt.contains("Message: third message"));
        // All events reference the same channel.
        assert_eq!(
            prompt.matches(&format!("Channel: {}", ch)).count(),
            3,
            "each event block should include the channel id"
        );
    }
}
