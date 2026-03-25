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
    ///
    /// Returns `true` if the event was accepted, `false` if dropped.
    pub fn push(&mut self, event: QueuedEvent) -> bool {
        if matches!(self.dedup_mode, DedupMode::Drop)
            && self.in_flight_channels.contains(&event.channel_id)
        {
            tracing::debug!(
                channel_id = %event.channel_id,
                "dropping event for in-flight channel (drop mode)"
            );
            return false;
        }
        self.queues
            .entry(event.channel_id)
            .or_default()
            .push_back(event);
        true
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
                    && self.retry_after.get(id).is_none_or(|&t| t <= now)
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
                && self.retry_after.get(id).is_none_or(|&t| t <= now)
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

    /// Drop all queued (non-in-flight) events for a channel.
    ///
    /// Used when the agent is removed from a channel — any pending events
    /// for that channel are stale and should not be prompted. Does NOT
    /// affect in-flight prompts (those will complete normally; the agent
    /// may fail to act if it lost access, but that's handled by the relay).
    ///
    /// Also clears any `retry_after` throttle for the channel.
    ///
    /// Returns the number of events dropped.
    /// Drop all queued (non-in-flight) events for a channel.
    ///
    /// Returns the event IDs of dropped events so the caller can clean up
    /// any reactions (👀) that were added at queue-push time.
    pub fn drain_channel(&mut self, channel_id: Uuid) -> Vec<String> {
        let ids = self
            .queues
            .remove(&channel_id)
            .map(|q| q.into_iter().map(|e| e.event.id.to_hex()).collect())
            .unwrap_or_default();
        self.retry_after.remove(&channel_id);
        ids
    }

    /// Whether a prompt is currently in-flight for the given channel.
    #[allow(dead_code)]
    pub fn is_channel_in_flight(&self, channel_id: Uuid) -> bool {
        self.in_flight_channels.contains(&channel_id)
    }
}

impl Default for EventQueue {
    fn default() -> Self {
        Self::new(DedupMode::Drop)
    }
}

// ── NIP-10 tag parsing ────────────────────────────────────────────────────────

/// Parsed thread relationship from NIP-10 `e` tags.
#[derive(Debug, Clone, Default)]
pub struct ThreadTags {
    /// Root event ID (hex). Present for all thread replies.
    pub root_event_id: Option<String>,
    /// Parent event ID (hex). For direct replies to root, equals root.
    pub parent_event_id: Option<String>,
    /// Mentioned pubkeys from `p` tags (hex).
    pub mentioned_pubkeys: Vec<String>,
}

/// Parse NIP-10 thread tags from a Nostr event.
///
/// Detection logic (per research doc §4c):
/// - Find an `e` tag with `root` marker → its value is `root_event_id`
/// - Find an `e` tag with `reply` marker → its value is `parent_event_id`
/// - If only `reply` marker found (direct reply to root), root == parent
/// - `p` tags → mentioned pubkeys
///
/// NOTE: Only handles NIP-10 marker-based format (preferred). The deprecated
/// positional format (no markers, `["e", id, relay_url]`) is not supported —
/// Sprout always generates marker-based tags (see relay messages.rs:762-783).
pub fn parse_thread_tags(event: &Event) -> ThreadTags {
    let mut root = None;
    let mut reply = None;
    let mut mentions = Vec::new();

    for tag in event.tags.iter() {
        let parts = tag.as_slice();
        match parts.first().map(|s| s.as_str()) {
            Some("e") if parts.len() >= 4 => {
                let id = &parts[1];
                let marker = &parts[3];
                match marker.as_str() {
                    "root" => root = Some(id.clone()),
                    "reply" => reply = Some(id.clone()),
                    _ => {}
                }
            }
            Some("p") if parts.len() >= 2 => {
                mentions.push(parts[1].clone());
            }
            _ => {}
        }
    }

    // For direct replies to root: single "reply" tag, no "root" tag.
    // In that case, root == parent.
    let (root_event_id, parent_event_id) = match (root, reply) {
        (Some(r), Some(p)) => (Some(r), Some(p)),
        (Some(r), None) => (Some(r.clone()), Some(r)),
        (None, Some(p)) => (Some(p.clone()), Some(p)),
        (None, None) => (None, None),
    };

    ThreadTags {
        root_event_id,
        parent_event_id,
        mentioned_pubkeys: mentions,
    }
}

// ── Prompt formatting ─────────────────────────────────────────────────────────

/// Conversation context fetched by the harness before prompting.
#[derive(Debug, Clone)]
pub enum ConversationContext {
    /// Thread context for a reply event.
    Thread {
        messages: Vec<ContextMessage>,
        total: usize,
        truncated: bool,
    },
    /// DM conversation history.
    Dm {
        messages: Vec<ContextMessage>,
        total: usize,
        truncated: bool,
    },
}

/// A single message in a conversation context section.
#[derive(Debug, Clone)]
pub struct ContextMessage {
    pub pubkey: String,
    pub timestamp: String,
    pub content: String,
}

/// Channel metadata for prompt formatting.
#[derive(Debug, Clone)]
pub struct PromptChannelInfo {
    pub name: String,
    pub channel_type: String,
}

/// Minimal profile fields needed to label users in ACP prompts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromptProfile {
    pub display_name: Option<String>,
    pub nip05_handle: Option<String>,
}

/// Pubkey-keyed profile lookup used while formatting ACP prompts.
pub type PromptProfileLookup = HashMap<String, PromptProfile>;

fn normalize_lookup_key(pubkey: &str) -> String {
    pubkey.trim().to_ascii_lowercase()
}

fn resolve_prompt_label<'a>(
    pubkey: &str,
    profile_lookup: Option<&'a PromptProfileLookup>,
) -> Option<&'a str> {
    let profile = profile_lookup?.get(&normalize_lookup_key(pubkey))?;

    profile
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            profile
                .nip05_handle
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
}

fn format_prompt_actor(pubkey: &str, profile_lookup: Option<&PromptProfileLookup>) -> String {
    match resolve_prompt_label(pubkey, profile_lookup) {
        Some(label) => format!("{label} ({pubkey})"),
        None => pubkey.to_string(),
    }
}

/// Format the per-event `[Event]` block for a single [`BatchEvent`].
///
/// Includes: event_id, channel (name + UUID), kind, sender (hex + npub),
/// time, content, all tags (never stripped), and parsed structural fields.
fn format_event_block(
    channel_id: Uuid,
    channel_info: Option<&PromptChannelInfo>,
    be: &BatchEvent,
    profile_lookup: Option<&PromptProfileLookup>,
) -> String {
    let hex = be.event.pubkey.to_hex();
    let npub = be.event.pubkey.to_bech32().unwrap_or_else(|_| hex.clone());

    let time = chrono::DateTime::from_timestamp(be.event.created_at.as_u64() as i64, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| be.event.created_at.as_u64().to_string());

    let kind = be.event.kind.as_u16() as u32;
    let event_id = be.event.id.to_hex();

    let channel_display = match channel_info {
        Some(ci) => format!("{} (#{channel_id})", ci.name),
        None => channel_id.to_string(),
    };

    let mut block = format!(
        "Event ID: {event_id}\n\
         Channel: {channel_display}\n\
         Kind: {kind}\n\
         From: {}\n\
         Time: {time}\n\
         Content: {}",
        match resolve_prompt_label(&hex, profile_lookup) {
            Some(label) => format!("{label} (npub: {npub}, hex: {hex})"),
            None => format!("{npub} (hex: {hex})"),
        },
        be.event.content,
    );

    // Always include tags — they carry structural information.
    let tags_json: Vec<&[String]> = be.event.tags.iter().map(|t| t.as_slice()).collect();
    if let Ok(tags_str) = serde_json::to_string(&tags_json) {
        block.push_str(&format!("\nTags: {tags_str}"));
    }

    // Parsed structural fields.
    let thread = parse_thread_tags(&be.event);
    let mut parsed_parts = Vec::new();
    if let Some(ref p) = thread.parent_event_id {
        parsed_parts.push(format!("parent={p}"));
    }
    if let Some(ref r) = thread.root_event_id {
        parsed_parts.push(format!("root={r}"));
    }
    if !thread.mentioned_pubkeys.is_empty() {
        parsed_parts.push(format!(
            "mentions=[{}]",
            thread
                .mentioned_pubkeys
                .iter()
                .map(|pubkey| format_prompt_actor(pubkey, profile_lookup))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !parsed_parts.is_empty() {
        block.push_str(&format!("\nParsed: {}", parsed_parts.join(", ")));
    }

    block
}

/// Format a `[Context]` hints section based on event scope.
fn format_context_hints(
    channel_id: Uuid,
    channel_info: Option<&PromptChannelInfo>,
    thread_tags: &ThreadTags,
    is_dm: bool,
    has_conversation_context: bool,
) -> String {
    let channel_display = match channel_info {
        Some(ci) => format!("{} (#{channel_id})", ci.name),
        None => channel_id.to_string(),
    };

    // DM check comes first — a DM reply has both thread tags AND is_dm=true,
    // and the scope should be "dm" (not "thread") because the agent is in a DM.
    if is_dm {
        let is_reply = thread_tags.root_event_id.is_some();
        // DM replies use get_thread() because /messages excludes thread replies.
        // DM non-replies use get_channel_history() for recent conversation.
        let ctx_hint = if has_conversation_context && is_reply {
            "Thread context included below. Use get_thread() for full history if truncated."
        } else if has_conversation_context {
            "Conversation context included below. Use get_channel_history() for full history if truncated."
        } else if is_reply {
            "Use get_thread() to fetch the reply chain."
        } else {
            "Use get_channel_history() for conversation context."
        };
        let mut s = format!(
            "[Context]\n\
             Scope: dm\n\
             Channel: {channel_display}\n\
             {ctx_hint}"
        );
        // If this is a DM reply, include thread structural info as supplementary.
        if let Some(ref root) = thread_tags.root_event_id {
            s.push_str(&format!("\nThread root: {root}"));
            if let Some(ref parent) = thread_tags.parent_event_id {
                if parent != root {
                    s.push_str(&format!("\nParent: {parent}"));
                }
            }
        }
        s
    } else if let Some(ref root) = thread_tags.root_event_id {
        let ctx_hint = if has_conversation_context {
            "Thread context included below. Use get_thread() for full history if truncated."
        } else {
            "Use get_thread() to fetch thread context."
        };
        let mut s = format!(
            "[Context]\n\
             Scope: thread\n\
             Channel: {channel_display}\n\
             Thread root: {root}"
        );
        if let Some(ref parent) = thread_tags.parent_event_id {
            if parent != root {
                s.push_str(&format!("\nParent: {parent}"));
            }
        }
        s.push_str(&format!("\n{ctx_hint}"));
        s
    } else {
        format!(
            "[Context]\n\
             Scope: channel\n\
             Channel: {channel_display}\n\
             Hint: Use get_channel_history() for recent messages if needed."
        )
    }
}

/// Format a conversation context section (thread or DM).
fn format_conversation_context(
    ctx: &ConversationContext,
    profile_lookup: Option<&PromptProfileLookup>,
) -> String {
    let (label, messages, total, truncated) = match ctx {
        ConversationContext::Thread {
            messages,
            total,
            truncated,
        } => ("Thread Context", messages, total, truncated),
        ConversationContext::Dm {
            messages,
            total,
            truncated,
        } => ("Conversation Context", messages, total, truncated),
    };

    let trunc_label = if *truncated { ", truncated" } else { "" };
    let mut s = format!(
        "[{label} ({} of {total} messages{trunc_label})]",
        messages.len()
    );
    for (i, msg) in messages.iter().enumerate() {
        s.push_str(&format!(
            "\n[{}] {} ({}): {}",
            i + 1,
            format_prompt_actor(&msg.pubkey, profile_lookup),
            msg.timestamp,
            msg.content,
        ));
    }
    s
}

/// Format a [`FlushBatch`] into a prompt string for the agent.
///
/// Produces a stable prompt with these sections (in order):
/// 1. `[System]` — system prompt (if configured)
/// 2. `[Context]` — scope, channel name, structural hints
/// 3. `[Thread Context]` or `[Conversation Context]` — if fetched
/// 4. `[Event]` / `[Sprout events]` — the triggering event(s)
pub fn format_prompt(
    batch: &FlushBatch,
    system_prompt: Option<&str>,
    channel_info: Option<&PromptChannelInfo>,
    conversation_context: Option<&ConversationContext>,
) -> String {
    format_prompt_with_profiles(
        batch,
        system_prompt,
        channel_info,
        conversation_context,
        None,
    )
}

/// Like [`format_prompt`], but labels known users with display names when available.
pub fn format_prompt_with_profiles(
    batch: &FlushBatch,
    system_prompt: Option<&str>,
    channel_info: Option<&PromptChannelInfo>,
    conversation_context: Option<&ConversationContext>,
    profile_lookup: Option<&PromptProfileLookup>,
) -> String {
    // Scope is always derived from the LAST event in the batch — that's the
    // one the agent is responding to. Thread/DM context is supplementary info
    // included alongside, not a scope override. This prevents mixed batches
    // (thread reply + later plain message) from being mislabeled as "thread".
    let last_event = batch.events.last().expect("batch must have ≥1 event");
    let thread_tags = parse_thread_tags(&last_event.event);
    let is_dm = channel_info
        .map(|ci| ci.channel_type == "dm")
        .unwrap_or(false);

    let mut sections: Vec<String> = Vec::with_capacity(4);

    // 1. System prompt.
    if let Some(sp) = system_prompt {
        sections.push(format!("[System]\n{sp}"));
    }

    // 2. Context hints.
    sections.push(format_context_hints(
        batch.channel_id,
        channel_info,
        &thread_tags,
        is_dm,
        conversation_context.is_some(),
    ));

    // 3. Conversation context (thread or DM).
    if let Some(ctx) = conversation_context {
        sections.push(format_conversation_context(ctx, profile_lookup));
    }

    // 4. Event block(s).
    let event_section = if batch.events.len() == 1 {
        let be = &batch.events[0];
        format!(
            "[Sprout event: {}]\n{}",
            be.prompt_tag,
            format_event_block(batch.channel_id, channel_info, be, profile_lookup)
        )
    } else {
        let mut s = format!("[Sprout events — {} events]", batch.events.len());
        for (i, be) in batch.events.iter().enumerate() {
            s.push_str(&format!(
                "\n\n--- Event {} ({}) ---\n{}",
                i + 1,
                be.prompt_tag,
                format_event_block(batch.channel_id, channel_info, be, profile_lookup)
            ));
        }
        s
    };
    sections.push(event_section);

    sections.join("\n\n")
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
        EventBuilder::new(Kind::Custom(9), content, [])
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

        let prompt = format_prompt(&batch, None, None, None);

        // Should contain [Context] section before the event.
        assert!(prompt.contains("[Context]"));
        assert!(prompt.contains("Scope: channel"));
        assert!(prompt.contains("[Sprout event: @mention]\n"));
        assert!(prompt.contains(&format!("Channel: {}", ch)));
        assert!(prompt.contains(&format!("From: {}", npub)));
        assert!(prompt.contains("Content: Hello @agent"));
        // Event ID should be present.
        assert!(prompt.contains("Event ID:"));
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

        let prompt = format_prompt(&batch, None, None, None);

        assert!(prompt.contains("[Context]"));
        assert!(prompt.contains("[Sprout events — 3 events]"));
        assert!(prompt.contains("--- Event 1 (tag-a) ---"));
        assert!(prompt.contains("--- Event 2 (tag-b) ---"));
        assert!(prompt.contains("--- Event 3 (tag-c) ---"));
        assert!(prompt.contains("Content: first message"));
        assert!(prompt.contains("Content: second message"));
        assert!(prompt.contains("Content: third message"));
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

        let prompt = format_prompt(&batch, Some("You are a triage bot."), None, None);
        assert!(prompt.starts_with("[System]\nYou are a triage bot.\n\n[Context]"));
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
        assert!(!q.retry_after.contains_key(&ch));
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
        assert!(
            !q.has_flushable_work(),
            "throttled channel should not be flushable"
        );

        // Manually expire the retry_after to simulate time passing.
        q.retry_after
            .insert(ch, Instant::now() - Duration::from_secs(1));
        assert!(
            q.has_flushable_work(),
            "expired throttle should be flushable"
        );
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
        q.retry_after
            .insert(ch, Instant::now() - Duration::from_secs(1));
        q.mark_complete(ch2);
        let batch3 = q
            .flush_next()
            .expect("ch should be flushable after throttle expires");
        assert_eq!(batch3.channel_id, ch);
    }

    // ── NIP-10 tag parsing tests ─────────────────────────────────────────────

    /// Build an event with specific tags for thread testing.
    fn make_event_with_tags(content: &str, tags: Vec<Vec<String>>) -> Event {
        let keys = Keys::generate();
        let nostr_tags: Vec<nostr::Tag> = tags
            .iter()
            .map(|t| {
                let strs: Vec<&str> = t.iter().map(|s| s.as_str()).collect();
                nostr::Tag::parse(&strs).unwrap()
            })
            .collect();
        EventBuilder::new(Kind::Custom(9), content, nostr_tags)
            .sign_with_keys(&keys)
            .unwrap()
    }

    #[test]
    fn test_parse_thread_tags_no_tags() {
        let event = make_event("plain message");
        let tags = parse_thread_tags(&event);
        assert!(tags.root_event_id.is_none());
        assert!(tags.parent_event_id.is_none());
        assert!(tags.mentioned_pubkeys.is_empty());
    }

    #[test]
    fn test_parse_thread_tags_direct_reply() {
        // Direct reply to root: single "reply" tag.
        let event = make_event_with_tags(
            "reply to root",
            vec![vec!["e".into(), "abc123".into(), "".into(), "reply".into()]],
        );
        let tags = parse_thread_tags(&event);
        assert_eq!(tags.root_event_id.as_deref(), Some("abc123"));
        assert_eq!(tags.parent_event_id.as_deref(), Some("abc123"));
    }

    #[test]
    fn test_parse_thread_tags_nested_reply() {
        // Nested reply: root + reply tags.
        let event = make_event_with_tags(
            "nested reply",
            vec![
                vec!["e".into(), "root123".into(), "".into(), "root".into()],
                vec!["e".into(), "parent456".into(), "".into(), "reply".into()],
            ],
        );
        let tags = parse_thread_tags(&event);
        assert_eq!(tags.root_event_id.as_deref(), Some("root123"));
        assert_eq!(tags.parent_event_id.as_deref(), Some("parent456"));
    }

    #[test]
    fn test_parse_thread_tags_with_mentions() {
        let event = make_event_with_tags(
            "hey @alice",
            vec![
                vec!["p".into(), "alice_pubkey".into()],
                vec!["p".into(), "bob_pubkey".into()],
            ],
        );
        let tags = parse_thread_tags(&event);
        assert!(tags.root_event_id.is_none());
        assert_eq!(tags.mentioned_pubkeys, vec!["alice_pubkey", "bob_pubkey"]);
    }

    #[test]
    fn test_parse_thread_tags_root_only() {
        // Only root marker, no reply marker — root == parent.
        let event = make_event_with_tags(
            "reply",
            vec![vec!["e".into(), "root123".into(), "".into(), "root".into()]],
        );
        let tags = parse_thread_tags(&event);
        assert_eq!(tags.root_event_id.as_deref(), Some("root123"));
        assert_eq!(tags.parent_event_id.as_deref(), Some("root123"));
    }

    // ── Context formatting tests ─────────────────────────────────────────────

    #[test]
    fn test_format_prompt_with_channel_info() {
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
        let ci = PromptChannelInfo {
            name: "engineering".into(),
            channel_type: "stream".into(),
        };

        let prompt = format_prompt(&batch, None, Some(&ci), None);
        assert!(prompt.contains("engineering (#"));
        assert!(prompt.contains("Scope: channel"));
    }

    #[test]
    fn test_format_prompt_dm_scope() {
        let ch = Uuid::new_v4();
        let event = make_event("hey");
        let batch = FlushBatch {
            channel_id: ch,
            events: vec![BatchEvent {
                event,
                prompt_tag: "dm".into(),
                received_at: Instant::now(),
            }],
        };
        let ci = PromptChannelInfo {
            name: "DM".into(),
            channel_type: "dm".into(),
        };

        let prompt = format_prompt(&batch, None, Some(&ci), None);
        assert!(prompt.contains("Scope: dm"));
    }

    #[test]
    fn test_format_prompt_thread_scope() {
        let ch = Uuid::new_v4();
        let event = make_event_with_tags(
            "yes go ahead",
            vec![vec![
                "e".into(),
                "root123".into(),
                "".into(),
                "reply".into(),
            ]],
        );
        let batch = FlushBatch {
            channel_id: ch,
            events: vec![BatchEvent {
                event,
                prompt_tag: "@mention".into(),
                received_at: Instant::now(),
            }],
        };

        let prompt = format_prompt(&batch, None, None, None);
        assert!(prompt.contains("Scope: thread"));
        assert!(prompt.contains("Thread root: root123"));
    }

    #[test]
    fn test_format_prompt_with_thread_context() {
        let ch = Uuid::new_v4();
        let event = make_event_with_tags(
            "yes go ahead",
            vec![vec![
                "e".into(),
                "root123".into(),
                "".into(),
                "reply".into(),
            ]],
        );
        let batch = FlushBatch {
            channel_id: ch,
            events: vec![BatchEvent {
                event,
                prompt_tag: "@mention".into(),
                received_at: Instant::now(),
            }],
        };
        let ctx = ConversationContext::Thread {
            messages: vec![
                ContextMessage {
                    pubkey: "npub1xyz".into(),
                    timestamp: "2026-03-15T16:30:00Z".into(),
                    content: "Let's refactor auth".into(),
                },
                ContextMessage {
                    pubkey: "npub1def".into(),
                    timestamp: "2026-03-15T16:35:00Z".into(),
                    content: "yes go ahead".into(),
                },
            ],
            total: 5,
            truncated: true,
        };

        let prompt = format_prompt(&batch, None, None, Some(&ctx));
        assert!(prompt.contains("[Thread Context (2 of 5 messages, truncated)]"));
        assert!(prompt.contains("Let's refactor auth"));
        assert!(prompt.contains("Thread context included below"));
    }

    #[test]
    fn test_format_prompt_with_dm_context() {
        let ch = Uuid::new_v4();
        let event = make_event("ok do that");
        let batch = FlushBatch {
            channel_id: ch,
            events: vec![BatchEvent {
                event,
                prompt_tag: "dm".into(),
                received_at: Instant::now(),
            }],
        };
        let ci = PromptChannelInfo {
            name: "DM".into(),
            channel_type: "dm".into(),
        };
        let ctx = ConversationContext::Dm {
            messages: vec![ContextMessage {
                pubkey: "npub1abc".into(),
                timestamp: "2026-03-15T16:00:00Z".into(),
                content: "Can you deploy?".into(),
            }],
            total: 1,
            truncated: false,
        };

        let prompt = format_prompt(&batch, None, Some(&ci), Some(&ctx));
        assert!(prompt.contains("Scope: dm"));
        assert!(prompt.contains("[Conversation Context (1 of 1 messages)]"));
        assert!(prompt.contains("Can you deploy?"));
    }

    #[test]
    fn test_format_prompt_with_profiles_prefers_display_names() {
        let ch = Uuid::new_v4();
        let event = make_event_with_tags(
            "hello there",
            vec![vec![
                "p".into(),
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            ]],
        );
        let author_hex = event.pubkey.to_hex();
        let batch = FlushBatch {
            channel_id: ch,
            events: vec![BatchEvent {
                event,
                prompt_tag: "@mention".into(),
                received_at: Instant::now(),
            }],
        };
        let ctx = ConversationContext::Thread {
            messages: vec![ContextMessage {
                pubkey: author_hex.clone(),
                timestamp: "2026-03-25T05:51:25Z".into(),
                content: "follow up".into(),
            }],
            total: 1,
            truncated: false,
        };
        let profiles = HashMap::from([
            (
                author_hex.clone(),
                PromptProfile {
                    display_name: Some("Wes".into()),
                    nip05_handle: None,
                },
            ),
            (
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                PromptProfile {
                    display_name: Some("Rick".into()),
                    nip05_handle: None,
                },
            ),
        ]);

        let prompt = format_prompt_with_profiles(&batch, None, None, Some(&ctx), Some(&profiles));

        assert!(prompt.contains("From: Wes (npub:"));
        assert!(prompt.contains(
            "mentions=[Rick (aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa)]"
        ));
        assert!(prompt.contains("[1] Wes ("));
    }

    #[test]
    fn test_format_prompt_dm_reply_hints_get_thread() {
        let ch = Uuid::new_v4();
        // DM reply event — has thread e-tags.
        let event = make_event_with_tags(
            "sounds good, do it",
            vec![vec![
                "e".into(),
                "root123".into(),
                "".into(),
                "reply".into(),
            ]],
        );
        let batch = FlushBatch {
            channel_id: ch,
            events: vec![BatchEvent {
                event,
                prompt_tag: "dm".into(),
                received_at: Instant::now(),
            }],
        };
        let ci = PromptChannelInfo {
            name: "DM".into(),
            channel_type: "dm".into(),
        };
        // Thread context fetched (as the fetch path does for DM replies).
        let ctx = ConversationContext::Thread {
            messages: vec![ContextMessage {
                pubkey: "npub1xyz".into(),
                timestamp: "2026-03-15T16:30:00Z".into(),
                content: "Should I deploy?".into(),
            }],
            total: 1,
            truncated: false,
        };

        let prompt = format_prompt(&batch, None, Some(&ci), Some(&ctx));
        // Scope should be "dm", not "thread".
        assert!(
            prompt.contains("Scope: dm"),
            "DM reply should have Scope: dm, got:\n{prompt}"
        );
        // Hint should point to get_thread(), not get_channel_history().
        assert!(
            prompt.contains("get_thread()"),
            "DM reply hint should mention get_thread(), got:\n{prompt}"
        );
        // Thread structural info should be present.
        assert!(
            prompt.contains("Thread root: root123"),
            "DM reply should include thread root"
        );
        // Thread context should be included.
        assert!(prompt.contains("Should I deploy?"));
    }

    #[test]
    fn test_format_prompt_dm_non_reply_hints_get_channel_history() {
        let ch = Uuid::new_v4();
        let event = make_event("hey there");
        let batch = FlushBatch {
            channel_id: ch,
            events: vec![BatchEvent {
                event,
                prompt_tag: "dm".into(),
                received_at: Instant::now(),
            }],
        };
        let ci = PromptChannelInfo {
            name: "DM".into(),
            channel_type: "dm".into(),
        };

        // No context fetched — hints only.
        let prompt = format_prompt(&batch, None, Some(&ci), None);
        assert!(prompt.contains("Scope: dm"));
        assert!(
            prompt.contains("get_channel_history()"),
            "DM non-reply hint should mention get_channel_history()"
        );
        assert!(
            !prompt.contains("get_thread()"),
            "DM non-reply should NOT mention get_thread()"
        );
    }

    #[test]
    fn test_format_event_block_includes_event_id() {
        let ch = Uuid::new_v4();
        let event = make_event("test");
        let event_id = event.id.to_hex();
        let batch = FlushBatch {
            channel_id: ch,
            events: vec![BatchEvent {
                event,
                prompt_tag: "test".into(),
                received_at: Instant::now(),
            }],
        };

        let prompt = format_prompt(&batch, None, None, None);
        assert!(
            prompt.contains(&format!("Event ID: {event_id}")),
            "prompt should contain the event ID"
        );
    }

    #[test]
    fn test_format_event_block_includes_hex_and_npub() {
        let ch = Uuid::new_v4();
        let event = make_event("test");
        let hex = event.pubkey.to_hex();
        let npub = event.pubkey.to_bech32().unwrap();
        let batch = FlushBatch {
            channel_id: ch,
            events: vec![BatchEvent {
                event,
                prompt_tag: "test".into(),
                received_at: Instant::now(),
            }],
        };

        let prompt = format_prompt(&batch, None, None, None);
        assert!(
            prompt.contains(&format!("From: {npub} (hex: {hex})")),
            "prompt should contain both npub and hex"
        );
    }

    #[test]
    fn test_format_event_block_always_includes_tags() {
        let ch = Uuid::new_v4();
        // Kind 9 (stream message) — tags were previously stripped.
        let event = make_event_with_tags("hello", vec![vec!["h".into(), ch.to_string()]]);
        let batch = FlushBatch {
            channel_id: ch,
            events: vec![BatchEvent {
                event,
                prompt_tag: "test".into(),
                received_at: Instant::now(),
            }],
        };

        let prompt = format_prompt(&batch, None, None, None);
        assert!(
            prompt.contains("Tags:"),
            "tags should always be included, even for stream messages"
        );
    }

    // ── drain_channel tests ──────────────────────────────────────────────────

    #[test]
    fn test_drain_channel_removes_pending_events() {
        let mut q = EventQueue::new(DedupMode::Queue);
        let ch = Uuid::new_v4();

        q.push(make_queued(ch, "msg1"));
        q.push(make_queued(ch, "msg2"));
        assert_eq!(q.pending_count(), 2);

        let drained = q.drain_channel(ch);
        assert_eq!(drained.len(), 2);
        assert_eq!(q.pending_count(), 0);
    }

    #[test]
    fn test_drain_channel_does_not_affect_other_channels() {
        let mut q = EventQueue::new(DedupMode::Queue);
        let ch_a = Uuid::new_v4();
        let ch_b = Uuid::new_v4();

        q.push(make_queued(ch_a, "A"));
        q.push(make_queued(ch_b, "B"));

        let drained = q.drain_channel(ch_a);
        assert_eq!(drained.len(), 1);
        assert_eq!(q.pending_count(), 1); // ch_b still has 1
    }

    #[test]
    fn test_drain_channel_clears_retry_after() {
        let mut q = EventQueue::new(DedupMode::Queue);
        let ch = Uuid::new_v4();

        q.push(make_queued(ch, "msg"));
        let batch = q.flush_next().unwrap();
        q.requeue(batch); // sets retry_after
        q.mark_complete(ch);

        // Channel is throttled — verify drain clears it.
        assert!(!q.has_flushable_work());
        let drained = q.drain_channel(ch);
        assert_eq!(drained.len(), 1);
        assert_eq!(q.pending_count(), 0);
    }

    #[test]
    fn test_drain_channel_empty_returns_empty() {
        let mut q = EventQueue::new(DedupMode::Queue);
        let ch = Uuid::new_v4();
        assert!(q.drain_channel(ch).is_empty());
    }

    #[test]
    fn test_drain_channel_does_not_affect_in_flight() {
        let mut q = EventQueue::new(DedupMode::Queue);
        let ch = Uuid::new_v4();

        q.push(make_queued(ch, "msg1"));
        let _batch = q.flush_next().unwrap(); // now in-flight
        assert!(q.is_in_flight());

        // Push another event while in-flight.
        q.push(make_queued(ch, "msg2"));

        // drain_channel should only remove the queued event, not the in-flight one.
        let drained = q.drain_channel(ch);
        assert_eq!(drained.len(), 1);
        assert!(q.is_in_flight()); // in-flight unaffected
    }
}
