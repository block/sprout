//! Sprout V2 kind number registry.
//!
//! Authoritative source: RESEARCH/SPROUT_KIND_REGISTRY_V2.md
//! All constants are `u32` — NIP-01 specifies kind as an unsigned integer,
//! and u32 covers the full range without truncation.

// Standard NIP kinds
/// NIP-01: User profile metadata.
pub const KIND_PROFILE: u32 = 0;
/// NIP-02: Contact list / follow list.
pub const KIND_CONTACT_LIST: u32 = 3;
/// NIP-09: Event deletion request.
pub const KIND_DELETION: u32 = 5;
/// NIP-25: Content is emoji char or `+`/`-`.
pub const KIND_REACTION: u32 = 7;
/// NIP-17: Outer envelope for private DMs — hides sender, content, timestamp.
pub const KIND_GIFT_WRAP: u32 = 1059;
/// NIP-94: File metadata attachment.
pub const KIND_FILE_METADATA: u32 = 1063;
/// NIP-42 auth event — never stored (carries bearer tokens).
pub const KIND_AUTH: u32 = 22242;

// NIP-29 group admin events
/// NIP-29: Add a user to a group.
pub const KIND_NIP29_PUT_USER: u32 = 9000;
/// NIP-29: Remove a user from a group.
pub const KIND_NIP29_REMOVE_USER: u32 = 9001;
/// NIP-29: Edit group metadata.
pub const KIND_NIP29_EDIT_METADATA: u32 = 9002;
/// NIP-29: Delete an event from a group.
pub const KIND_NIP29_DELETE_EVENT: u32 = 9005;
/// NIP-29: Create a new group.
pub const KIND_NIP29_CREATE_GROUP: u32 = 9007;
/// NIP-29: Delete a group.
pub const KIND_NIP29_DELETE_GROUP: u32 = 9008;
/// NIP-29: Create an invite to a group.
pub const KIND_NIP29_CREATE_INVITE: u32 = 9009;
/// NIP-29: Request to join a group.
pub const KIND_NIP29_JOIN_REQUEST: u32 = 9021;
/// NIP-29: Request to leave a group.
pub const KIND_NIP29_LEAVE_REQUEST: u32 = 9022;

// System / admin (9031–9999)
/// V1 used kind:9001 — moved here due to NIP-29 conflict.
pub const KIND_SYSTEM_TIMER_FIRED: u32 = 9100;
/// V1 used kind:9010 — moved here for NIP-29 range safety.
pub const KIND_SYSTEM_SLASH_COMMAND: u32 = 9110;
/// Internal system flag event for admin tooling.
pub const KIND_SYSTEM_FLAG: u32 = 9900;

// NIP-29 group state (addressable range 39000–39003)
/// NIP-29: Addressable group metadata state.
pub const KIND_NIP29_GROUP_METADATA: u32 = 39000;
/// NIP-29: Addressable group admins list.
pub const KIND_NIP29_GROUP_ADMINS: u32 = 39001;
/// NIP-29: Addressable group members list.
pub const KIND_NIP29_GROUP_MEMBERS: u32 = 39002;
/// NIP-29: Addressable group roles definition.
pub const KIND_NIP29_GROUP_ROLES: u32 = 39003;

/// Lower bound of the ephemeral event range (20000–29999). Never stored.
pub const EPHEMERAL_KIND_MIN: u32 = 20000;
/// Upper bound of the ephemeral event range (20000–29999). Never stored.
pub const EPHEMERAL_KIND_MAX: u32 = 29999;

// Ephemeral events (20000–29999) — Redis pub/sub only, never stored.
/// Ephemeral: user presence update (online/away/offline).
pub const KIND_PRESENCE_UPDATE: u32 = 20001;
/// Ephemeral: typing indicator for a channel.
pub const KIND_TYPING_INDICATOR: u32 = 20002;

// Stream messaging (40000–40999)
/// V1 used kind:10001 (replaceable range — wrong).
pub const KIND_STREAM_MESSAGE: u32 = 40001;
/// V1 used kind:10002 (replaceable range — wrong).
pub const KIND_STREAM_MESSAGE_V2: u32 = 40002;
/// V1 used kind:10004 (replaceable range + NIP-51 collision — wrong).
pub const KIND_STREAM_MESSAGE_EDIT: u32 = 40003;
/// A stream message that has been pinned in a channel.
pub const KIND_STREAM_MESSAGE_PINNED: u32 = 40004;
/// A stream message that has been bookmarked by a user.
pub const KIND_STREAM_MESSAGE_BOOKMARKED: u32 = 40005;
/// A stream message scheduled for future delivery.
pub const KIND_STREAM_MESSAGE_SCHEDULED: u32 = 40006;
/// A reminder attached to a stream message or time.
pub const KIND_STREAM_REMINDER: u32 = 40007;
/// Canvas (shared document) for a channel.
pub const KIND_CANVAS: u32 = 40100;

// Direct messages (41000–41999)
/// A new direct-message conversation was created.
pub const KIND_DM_CREATED: u32 = 41001;
/// A member was added to a DM conversation.
pub const KIND_DM_MEMBER_ADDED: u32 = 41002;
/// A member was removed from a DM conversation.
pub const KIND_DM_MEMBER_REMOVED: u32 = 41003;

// Channel / topic management (42000–42999)
/// A new channel topic was created.
pub const KIND_TOPIC_CREATED: u32 = 42001;
/// An existing channel topic was updated.
pub const KIND_TOPIC_UPDATED: u32 = 42002;
/// A channel topic was archived.
pub const KIND_TOPIC_ARCHIVED: u32 = 42003;

// Agent job protocol (43000–43999)
// Not using NIP-90 kinds (5000–6999) — Sprout requires auth chains (depth ≤ 3, breadth ≤ 10).
/// An agent job was requested.
pub const KIND_JOB_REQUEST: u32 = 43001;
/// An agent accepted a job request.
pub const KIND_JOB_ACCEPTED: u32 = 43002;
/// Progress update for an in-flight agent job.
pub const KIND_JOB_PROGRESS: u32 = 43003;
/// Final result of a completed agent job.
pub const KIND_JOB_RESULT: u32 = 43004;
/// A job cancellation was requested.
pub const KIND_JOB_CANCEL: u32 = 43005;
/// An agent job failed with an error.
pub const KIND_JOB_ERROR: u32 = 43006;

// Subscription system (44000–44999)
/// A new event subscription was created.
pub const KIND_SUBSCRIPTION_CREATED: u32 = 44001;
/// An event matched an active subscription.
pub const KIND_SUBSCRIPTION_MATCHED: u32 = 44002;
/// A subscription was paused.
pub const KIND_SUBSCRIPTION_PAUSED: u32 = 44003;
/// A paused subscription was resumed.
pub const KIND_SUBSCRIPTION_RESUMED: u32 = 44004;

// Forum / social (45000–45999)
// V1 used addressable range (30001–30003) — wrong.
/// A forum post (thread root).
pub const KIND_FORUM_POST: u32 = 45001;
/// A vote on a forum post.
pub const KIND_FORUM_VOTE: u32 = 45002;
/// A comment reply on a forum post.
pub const KIND_FORUM_COMMENT: u32 = 45003;

// Workflow engine (46000–46999)
/// A workflow was triggered by a matching event.
pub const KIND_WORKFLOW_TRIGGERED: u32 = 46001;
/// A workflow step began execution.
pub const KIND_WORKFLOW_STEP_STARTED: u32 = 46002;
/// A workflow step completed successfully.
pub const KIND_WORKFLOW_STEP_COMPLETED: u32 = 46003;
/// A workflow step failed.
pub const KIND_WORKFLOW_STEP_FAILED: u32 = 46004;
/// The entire workflow completed successfully.
pub const KIND_WORKFLOW_COMPLETED: u32 = 46005;
/// The entire workflow failed.
pub const KIND_WORKFLOW_FAILED: u32 = 46006;
/// The workflow was cancelled before completion.
pub const KIND_WORKFLOW_CANCELLED: u32 = 46007;
/// A workflow step is waiting for human approval.
pub const KIND_WORKFLOW_APPROVAL_REQUESTED: u32 = 46010;
/// A pending workflow approval was granted.
pub const KIND_WORKFLOW_APPROVAL_GRANTED: u32 = 46011;
/// A pending workflow approval was denied.
pub const KIND_WORKFLOW_APPROVAL_DENIED: u32 = 46012;

// User groups (47000–47999)
/// A new user group was created.
pub const KIND_USER_GROUP_CREATED: u32 = 47001;
/// An existing user group was updated.
pub const KIND_USER_GROUP_UPDATED: u32 = 47002;
/// A user group was deleted.
pub const KIND_USER_GROUP_DELETED: u32 = 47003;

// System / admin custom range (48000–48999)
/// An audit log entry was recorded.
pub const KIND_AUDIT_ENTRY: u32 = 48001;
/// A compliance export was initiated.
pub const KIND_COMPLIANCE_EXPORT: u32 = 48002;
/// A knowledge crystal was created.
pub const KIND_KNOWLEDGE_CRYSTAL_CREATED: u32 = 48003;
/// A knowledge crystal was approved.
pub const KIND_KNOWLEDGE_CRYSTAL_APPROVED: u32 = 48004;
/// A knowledge crystal was updated.
pub const KIND_KNOWLEDGE_CRYSTAL_UPDATED: u32 = 48005;
/// A huddle (audio/video session) was started.
pub const KIND_HUDDLE_STARTED: u32 = 48100;
/// A participant joined a huddle.
pub const KIND_HUDDLE_PARTICIPANT_JOINED: u32 = 48101;
/// A participant left a huddle.
pub const KIND_HUDDLE_PARTICIPANT_LEFT: u32 = 48102;
/// A huddle ended.
pub const KIND_HUDDLE_ENDED: u32 = 48103;
/// A media track was published in a huddle.
pub const KIND_HUDDLE_TRACK_PUBLISHED: u32 = 48104;
/// A huddle recording became available.
pub const KIND_HUDDLE_RECORDING_AVAILABLE: u32 = 48105;

/// All registered kind constants — used for duplicate detection and iteration.
pub const ALL_KINDS: &[u32] = &[
    KIND_PROFILE,
    KIND_CONTACT_LIST,
    KIND_DELETION,
    KIND_REACTION,
    KIND_GIFT_WRAP,
    KIND_FILE_METADATA,
    KIND_NIP29_PUT_USER,
    KIND_NIP29_REMOVE_USER,
    KIND_NIP29_EDIT_METADATA,
    KIND_NIP29_DELETE_EVENT,
    KIND_NIP29_CREATE_GROUP,
    KIND_NIP29_DELETE_GROUP,
    KIND_NIP29_CREATE_INVITE,
    KIND_NIP29_JOIN_REQUEST,
    KIND_NIP29_LEAVE_REQUEST,
    KIND_SYSTEM_TIMER_FIRED,
    KIND_SYSTEM_SLASH_COMMAND,
    KIND_SYSTEM_FLAG,
    KIND_NIP29_GROUP_METADATA,
    KIND_NIP29_GROUP_ADMINS,
    KIND_NIP29_GROUP_MEMBERS,
    KIND_NIP29_GROUP_ROLES,
    KIND_PRESENCE_UPDATE,
    KIND_TYPING_INDICATOR,
    KIND_STREAM_MESSAGE,
    KIND_STREAM_MESSAGE_V2,
    KIND_STREAM_MESSAGE_EDIT,
    KIND_STREAM_MESSAGE_PINNED,
    KIND_STREAM_MESSAGE_BOOKMARKED,
    KIND_STREAM_MESSAGE_SCHEDULED,
    KIND_STREAM_REMINDER,
    KIND_CANVAS,
    KIND_DM_CREATED,
    KIND_DM_MEMBER_ADDED,
    KIND_DM_MEMBER_REMOVED,
    KIND_TOPIC_CREATED,
    KIND_TOPIC_UPDATED,
    KIND_TOPIC_ARCHIVED,
    KIND_JOB_REQUEST,
    KIND_JOB_ACCEPTED,
    KIND_JOB_PROGRESS,
    KIND_JOB_RESULT,
    KIND_JOB_CANCEL,
    KIND_JOB_ERROR,
    KIND_SUBSCRIPTION_CREATED,
    KIND_SUBSCRIPTION_MATCHED,
    KIND_SUBSCRIPTION_PAUSED,
    KIND_SUBSCRIPTION_RESUMED,
    KIND_FORUM_POST,
    KIND_FORUM_VOTE,
    KIND_FORUM_COMMENT,
    KIND_WORKFLOW_TRIGGERED,
    KIND_WORKFLOW_STEP_STARTED,
    KIND_WORKFLOW_STEP_COMPLETED,
    KIND_WORKFLOW_STEP_FAILED,
    KIND_WORKFLOW_COMPLETED,
    KIND_WORKFLOW_FAILED,
    KIND_WORKFLOW_CANCELLED,
    KIND_WORKFLOW_APPROVAL_REQUESTED,
    KIND_WORKFLOW_APPROVAL_GRANTED,
    KIND_WORKFLOW_APPROVAL_DENIED,
    KIND_USER_GROUP_CREATED,
    KIND_USER_GROUP_UPDATED,
    KIND_USER_GROUP_DELETED,
    KIND_AUDIT_ENTRY,
    KIND_COMPLIANCE_EXPORT,
    KIND_KNOWLEDGE_CRYSTAL_CREATED,
    KIND_KNOWLEDGE_CRYSTAL_APPROVED,
    KIND_KNOWLEDGE_CRYSTAL_UPDATED,
    KIND_HUDDLE_STARTED,
    KIND_HUDDLE_PARTICIPANT_JOINED,
    KIND_HUDDLE_PARTICIPANT_LEFT,
    KIND_HUDDLE_ENDED,
    KIND_HUDDLE_TRACK_PUBLISHED,
    KIND_HUDDLE_RECORDING_AVAILABLE,
];

/// Returns `true` if `kind` is in the ephemeral range (20000–29999).
pub const fn is_ephemeral(kind: u32) -> bool {
    kind >= EPHEMERAL_KIND_MIN && kind <= EPHEMERAL_KIND_MAX
}

/// Returns `true` if `kind` is a workflow execution event (46001–46012).
/// These must not trigger workflows (prevents infinite loops).
pub const fn is_workflow_execution_kind(kind: u32) -> bool {
    kind >= KIND_WORKFLOW_TRIGGERED && kind <= KIND_WORKFLOW_APPROVAL_DENIED
}

/// Extract the kind from a nostr Event as u32.
/// NIP-01 specifies kind as an unsigned integer; u32 covers the full range.
pub fn event_kind_u32(event: &nostr::Event) -> u32 {
    event.kind.as_u16() as u32
}

/// Extract the kind from a nostr Event as i32 (for MySQL INT columns).
/// Safe: all Sprout kinds fit in i32 (max 65535 < i32::MAX).
pub fn event_kind_i32(event: &nostr::Event) -> i32 {
    event.kind.as_u16() as i32
}

// Compile-time: all Sprout kind constants fit in nostr's u16-backed Kind.
const _: () = assert!(KIND_AUTH <= u16::MAX as u32);
const _: () = assert!(KIND_CANVAS <= u16::MAX as u32);
const _: () = assert!(KIND_HUDDLE_RECORDING_AVAILABLE <= u16::MAX as u32);
const _: () = assert!(EPHEMERAL_KIND_MIN < EPHEMERAL_KIND_MAX);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_duplicate_kind_values() {
        let mut seen = std::collections::HashSet::new();
        for &k in ALL_KINDS {
            assert!(seen.insert(k), "duplicate kind value: {k}");
        }
    }
}
