use nostr::EventId;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router, ServerHandler,
};
use serde::{Deserialize, Serialize};

use crate::relay_client::RelayClient;
use sprout_core::PresenceStatus;

/// Percent-encode a string for safe inclusion in a URL query parameter value.
/// Encodes all characters except unreserved ones (A-Z a-z 0-9 - _ . ~).
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                // SAFETY: nibble values 0–15 are always valid hex digits.
                let hi = char::from_digit((byte >> 4) as u32, 16)
                    .expect("nibble 0-15 is always a valid hex digit")
                    .to_ascii_uppercase();
                let lo = char::from_digit((byte & 0xf) as u32, 16)
                    .expect("nibble 0-15 is always a valid hex digit")
                    .to_ascii_uppercase();
                out.push('%');
                out.push(hi);
                out.push(lo);
            }
        }
    }
    out
}

/// Validate that a string is exactly 64 hex characters.
fn validate_hex64(s: &str, label: &str) -> Result<(), String> {
    if s.len() != 64 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        Err(format!(
            "Error: {label} must be exactly 64 hex characters, got len={}",
            s.len()
        ))
    } else {
        Ok(())
    }
}

/// Validate that `s` is a well-formed UUID (any version/variant).
/// Returns `Ok(())` on success, or an error string on failure.
fn validate_uuid(s: &str) -> Result<(), String> {
    uuid::Uuid::parse_str(s).map_err(|_| format!("invalid UUID: {s}"))?;
    Ok(())
}

/// Extract the thread root event ID from a serialized Nostr tag array.
///
/// Parses `"e"` tags with NIP-10 markers:
/// - If a `"root"` marker exists, returns that event ID.
/// - If only a `"reply"` marker exists, returns the reply target (it IS the root
///   for a direct reply — needed so nested replies can supply the correct root).
/// - If no thread markers exist, returns `None` (top-level message).
fn find_root_from_tags(tags: &serde_json::Value) -> Option<String> {
    let arr = tags.as_array()?;
    let mut root = None;
    let mut reply = None;
    for tag in arr {
        let Some(parts) = tag.as_array() else {
            continue;
        };
        if parts.len() >= 4 && parts[0].as_str() == Some("e") {
            match parts[3].as_str() {
                Some("root") => root = parts[1].as_str().map(|s| s.to_string()),
                Some("reply") => reply = parts[1].as_str().map(|s| s.to_string()),
                _ => {}
            }
        }
    }
    root.or(reply)
}

/// Maximum allowed content size for a single message (64 KiB).
const MAX_CONTENT_BYTES: usize = 65_536;

/// Extract @mention names from message content.
/// Returns lowercased names found after `@` tokens.
/// Only matches `@word` preceded by whitespace or start-of-string.
/// Characters allowed in names: alphanumeric, `.`, `-`, `_`.
fn extract_at_names(content: &str) -> Vec<String> {
    if content.is_empty() || !content.contains('@') {
        return vec![];
    }
    let mut names: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let chars: Vec<char> = content.chars().collect();
    let len = chars.len();
    let mut i = 0;
    while i < len {
        if chars[i] == '@' {
            // Must be at start-of-string or preceded by whitespace
            let preceded_by_ws = i == 0 || chars[i - 1].is_ascii_whitespace();
            if preceded_by_ws && i + 1 < len {
                // Capture the name token: [a-zA-Z0-9._-]+
                let start = i + 1;
                let mut end = start;
                while end < len {
                    let c = chars[end];
                    if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                        end += 1;
                    } else {
                        break;
                    }
                }
                if end > start {
                    let name: String = chars[start..end].iter().collect();
                    let lower = name.to_ascii_lowercase();
                    if seen.insert(lower.clone()) {
                        names.push(lower);
                    }
                }
            }
        }
        i += 1;
    }
    names
}

/// Resolve @names in content against channel members.
/// Returns matching pubkeys. On any error, returns empty vec — never blocks a send.
async fn resolve_content_mentions(
    client: &RelayClient,
    channel_id: &str,
    content: &str,
) -> Vec<String> {
    let names = extract_at_names(content);
    if names.is_empty() {
        return vec![];
    }
    let body = client
        .get(&format!("/api/channels/{channel_id}/members"))
        .await
        .unwrap_or_default();
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
    let Some(members) = parsed["members"].as_array() else {
        return vec![];
    };
    let mut pubkeys = Vec::new();
    for m in members {
        let Some(dn) = m["display_name"].as_str() else {
            continue;
        };
        if names.iter().any(|n| n.eq_ignore_ascii_case(dn)) {
            if let Some(pk) = m["pubkey"].as_str() {
                pubkeys.push(pk.to_ascii_lowercase());
            }
        }
    }
    pubkeys
}

/// Parameters for the `send_message` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SendMessageParams {
    /// UUID of the channel to post to.
    pub channel_id: String,
    /// Message body text.
    pub content: String,
    /// Nostr event kind. Defaults to KIND_STREAM_MESSAGE (NIP-29 group chat message).
    #[serde(default = "default_kind")]
    pub kind: Option<u16>,
    /// Optional parent event ID for threading. If provided, NIP-10 reply tags are added.
    #[serde(default)]
    pub parent_event_id: Option<String>,
    /// If true and parent_event_id is set, surface the reply in the main channel timeline.
    #[serde(default)]
    pub broadcast_to_channel: Option<bool>,
    /// Pubkeys to @mention in the message.
    #[serde(default)]
    pub mention_pubkeys: Option<Vec<String>>,
    /// Optional file paths to upload and attach as media. Each file is uploaded
    /// to the relay and included as an imeta tag + markdown image in the message.
    #[serde(default)]
    pub file_paths: Option<Vec<String>>,
}
fn default_kind() -> Option<u16> {
    Some(sprout_core::kind::KIND_STREAM_MESSAGE as u16)
}

/// Parameters for the `get_messages` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GetMessagesParams {
    /// UUID of the channel to fetch history from.
    pub channel_id: String,
    /// Maximum number of messages to return (default 50, max 200).
    #[serde(default)]
    pub limit: Option<u32>,
    /// Legacy parameter (thread summaries are now always included). Kept for backward compatibility.
    #[serde(default)]
    pub with_threads: Option<bool>,
    /// Unix timestamp cursor for pagination. Returns messages before this time.
    #[serde(default)]
    pub before: Option<i64>,
    /// Unix timestamp cursor. Returns messages created strictly after this time.
    /// When used without `before`, results are ordered oldest-first (chronological).
    /// Useful for polling: pass the timestamp of the last seen message to get only newer ones.
    #[serde(default)]
    pub since: Option<i64>,
    /// Comma-separated event kind numbers to filter by (e.g. "45001" for forum posts,
    /// "45002" for votes). When omitted, all kinds are returned.
    #[serde(default)]
    pub kinds: Option<String>,
}

/// Parameters for the `list_channels` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ListChannelsParams {
    /// Optional visibility filter: `"open"` or `"private"`.
    #[serde(default)]
    pub visibility: Option<String>,
}

/// Parameters for the `create_channel` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateChannelParams {
    /// Display name for the new channel.
    pub name: String,
    /// Channel type: `"stream"` (real-time chat) or `"forum"` (threaded discussions).
    pub channel_type: String,
    /// Channel visibility: `"open"` (anyone can join) or `"private"` (invite-only).
    pub visibility: String,
    /// Optional human-readable description of the channel's purpose.
    #[serde(default)]
    pub description: Option<String>,
}

/// Parameters for the `get_canvas` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GetCanvasParams {
    /// UUID of the channel whose canvas to retrieve.
    pub channel_id: String,
}

/// Parameters for the `set_canvas` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SetCanvasParams {
    /// UUID of the channel whose canvas to update.
    pub channel_id: String,
    /// New canvas content (replaces any existing canvas).
    pub content: String,
}

// ── Workflow tool parameter structs ──────────────────────────────────────────

/// Parameters for the `list_workflows` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ListWorkflowsParams {
    /// UUID of the channel whose workflows to list.
    pub channel_id: String,
}

/// Parameters for the `create_workflow` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateWorkflowParams {
    /// UUID of the channel to own this workflow.
    pub channel_id: String,
    /// Full workflow definition in YAML format. Required fields: name (string), trigger (object with
    /// 'on' field: 'message_posted', 'diff_posted', 'reaction_added', or 'webhook'), steps (array).
    /// Each step needs: id (alphanumeric/underscore), action (e.g. 'send_message'), and action-specific
    /// fields as direct properties (NOT nested under 'params'). Example:
    /// ```yaml
    /// name: My Workflow
    /// trigger:
    ///   on: message_posted
    /// steps:
    ///   - id: notify
    ///     action: send_message
    ///     text: Hello from workflow!
    /// ```
    pub yaml_definition: String,
}

/// Parameters for the `update_workflow` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateWorkflowParams {
    /// UUID of the workflow to update.
    pub workflow_id: String,
    /// Replacement YAML definition.
    pub yaml_definition: String,
}

/// Parameters for the `delete_workflow` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeleteWorkflowParams {
    /// UUID of the workflow to delete.
    pub workflow_id: String,
}

/// Parameters for the `trigger_workflow` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TriggerWorkflowParams {
    /// UUID of the workflow to trigger.
    pub workflow_id: String,
    /// Optional JSON object of input variables passed to the workflow.
    #[serde(default)]
    pub inputs: Option<serde_json::Value>,
}

/// Parameters for the `get_workflow_runs` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GetWorkflowRunsParams {
    /// UUID of the workflow whose run history to fetch.
    pub workflow_id: String,
    /// Maximum number of runs to return. Default 20, max 100.
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Parameters for the `approve_step` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ApproveStepParams {
    /// Opaque approval token from the kind:46010 event.
    pub approval_token: String,
    /// true = approve, false = deny.
    pub approved: bool,
    /// Optional human-readable note to attach to the decision.
    #[serde(default)]
    pub note: Option<String>,
}

// ── Feed tool parameter structs ───────────────────────────────────────────────

// ── Membership tool parameter structs ────────────────────────────────────────

/// Parameters for the `add_channel_member` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AddChannelMemberParams {
    /// UUID of the channel.
    pub channel_id: String,
    /// Hex-encoded public key of the user to add.
    pub pubkey: String,
    /// Role to assign: `"member"` (default) or `"admin"`.
    #[serde(default)]
    pub role: Option<String>,
}

/// Parameters for the `remove_channel_member` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RemoveChannelMemberParams {
    /// UUID of the channel.
    pub channel_id: String,
    /// Hex-encoded public key of the user to remove.
    pub pubkey: String,
}

/// Parameters for the `list_channel_members` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ListChannelMembersParams {
    /// UUID of the channel whose members to list.
    pub channel_id: String,
}

/// Parameters for the `join_channel` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct JoinChannelParams {
    /// UUID of the channel to join.
    pub channel_id: String,
}

/// Parameters for the `leave_channel` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LeaveChannelParams {
    /// UUID of the channel to leave.
    pub channel_id: String,
}

/// Parameters for the `get_channel` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GetChannelParams {
    /// UUID of the channel to retrieve.
    pub channel_id: String,
}

// ── Metadata tool parameter structs ──────────────────────────────────────────

/// Parameters for the `update_channel` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateChannelParams {
    /// UUID of the channel to update.
    pub channel_id: String,
    /// New display name for the channel.
    #[serde(default)]
    pub name: Option<String>,
    /// New description for the channel.
    #[serde(default)]
    pub description: Option<String>,
}

/// Parameters for the `set_channel_topic` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SetChannelTopicParams {
    /// UUID of the channel.
    pub channel_id: String,
    /// New topic string.
    pub topic: String,
}

/// Parameters for the `set_channel_purpose` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SetChannelPurposeParams {
    /// UUID of the channel.
    pub channel_id: String,
    /// New purpose string.
    pub purpose: String,
}

/// Parameters for the `archive_channel` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ArchiveChannelParams {
    /// UUID of the channel to archive.
    pub channel_id: String,
}

/// Parameters for the `unarchive_channel` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UnarchiveChannelParams {
    /// UUID of the channel to unarchive.
    pub channel_id: String,
}

/// Parameters for the `delete_channel` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeleteChannelParams {
    /// UUID of the channel to permanently delete.
    pub channel_id: String,
}

// ── Thread tool parameter structs ─────────────────────────────────────────────

/// Parameters for the `get_thread` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GetThreadParams {
    /// UUID of the channel containing the thread.
    pub channel_id: String,
    /// Event ID of the root (or any ancestor) message of the thread.
    pub event_id: String,
    /// Maximum nesting depth to return (default: unlimited).
    #[serde(default)]
    pub depth_limit: Option<u32>,
    /// Maximum number of replies to return (default 50).
    #[serde(default)]
    pub limit: Option<u32>,
}

// ── DM tool parameter structs ─────────────────────────────────────────────────

/// Parameters for the `open_dm` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OpenDmParams {
    /// Hex-encoded public keys of the other participants (1–8).
    pub pubkeys: Vec<String>,
}

/// Parameters for the `add_dm_member` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AddDmMemberParams {
    /// UUID of the DM channel.
    pub channel_id: String,
    /// Hex-encoded public key of the user to add.
    pub pubkey: String,
}

/// Parameters for the `hide_dm` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct HideDmParams {
    /// UUID of the DM channel to hide.
    pub channel_id: String,
}

// ── Reaction tool parameter structs ──────────────────────────────────────────

/// Parameters for the `add_reaction` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AddReactionParams {
    /// Event ID of the message to react to.
    pub event_id: String,
    /// Emoji to react with (e.g. `"👍"` or `":thumbsup:"`).
    pub emoji: String,
}

/// Parameters for the `remove_reaction` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RemoveReactionParams {
    /// Event ID of the message whose reaction to remove.
    pub event_id: String,
    /// Emoji to remove.
    pub emoji: String,
}

/// Parameters for the `get_reactions` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GetReactionsParams {
    /// Event ID of the message whose reactions to fetch.
    pub event_id: String,
}

// ── User profile tool parameter structs ──────────────────────────────────────

/// Parameters for the `set_profile` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SetProfileParams {
    /// New display name for the agent's profile.
    #[serde(default)]
    pub display_name: Option<String>,
    /// URL of the agent's avatar image.
    #[serde(default)]
    pub avatar_url: Option<String>,
    /// Short bio or description.
    #[serde(default)]
    pub about: Option<String>,
    /// NIP-05 identifier (e.g. "alice@example.com"), or None to leave unchanged.
    #[serde(default)]
    pub nip05_handle: Option<String>,
}

/// Parameters for the `get_users` tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetUsersParams {
    /// Pubkey(s) to look up. Omit for your own profile. Provide one hex pubkey
    /// for a single user, or multiple for batch lookup (max 200).
    pub pubkeys: Option<Vec<String>>,
}

/// Parameters for the `search` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SearchParams {
    /// Full-text search query string.
    pub q: String,
    /// Maximum results to return (default 20, max 100).
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Parameters for the `get_presence` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GetPresenceParams {
    /// Comma-separated hex-encoded public keys to look up presence for (max 200).
    pub pubkeys: String,
}

/// Parameters for the `set_presence` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SetPresenceParams {
    /// Presence status to set.
    pub status: PresenceStatus,
}

/// Parameters for the `set_channel_add_policy` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SetChannelAddPolicyParams {
    /// Channel add policy: "anyone" (default), "owner_only", or "nobody".
    /// - "anyone": any authenticated user can add you to open channels.
    /// - "owner_only": only your provisioned owner can add you.
    /// - "nobody": no one can add you; you must self-join channels.
    pub policy: String,
}

/// Parameters for the `get_feed` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GetFeedParams {
    /// Only return feed items newer than this Unix timestamp.
    /// Defaults to now - 7 days if omitted.
    #[serde(default)]
    pub since: Option<i64>,
    /// Maximum items per category. Default 50, max 50.
    #[serde(default)]
    pub limit: Option<u32>,
    /// Comma-separated category filter: "mentions,needs_action,activity,agent_activity".
    /// Omit to return all categories.
    #[serde(default)]
    pub types: Option<String>,
}

/// Parameters for the `edit_message` tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EditMessageParams {
    /// Channel ID (UUID) containing the message to edit.
    pub channel_id: String,
    /// Event ID (64-char hex) of the message to edit.
    pub event_id: String,
    /// New content for the message.
    pub content: String,
}

/// Parameters for the `delete_message` tool.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DeleteMessageParams {
    /// Event ID (64-char hex) of the message to delete.
    pub event_id: String,
}

/// Parameters for the `send_diff_message` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SendDiffMessageParams {
    /// UUID of the channel to post to.
    pub channel_id: String,
    /// Unified diff content (git diff format).
    pub diff: String,
    /// URL of the source repository (e.g. "https://github.com/org/repo").
    pub repo_url: String,
    /// Full commit SHA this diff applies to.
    pub commit_sha: String,
    /// Optional file path within the repo (used for language inference and display).
    #[serde(default)]
    pub file_path: Option<String>,
    /// Optional parent commit SHA (the base of the diff).
    #[serde(default)]
    pub parent_commit_sha: Option<String>,
    /// Optional source branch name (e.g. "feat/my-feature").
    #[serde(default)]
    pub source_branch: Option<String>,
    /// Optional target branch name (e.g. "main").
    #[serde(default)]
    pub target_branch: Option<String>,
    /// Optional pull request number associated with this diff.
    #[serde(default)]
    pub pr_number: Option<u32>,
    /// Optional language hint for syntax highlighting (e.g. "rust", "typescript").
    /// Inferred from file_path extension if omitted.
    #[serde(default)]
    pub language: Option<String>,
    /// Optional human-readable description of the change.
    #[serde(default)]
    pub description: Option<String>,
    /// Optional parent event ID. If provided, sends the diff as a threaded reply.
    #[serde(default)]
    pub parent_event_id: Option<String>,
}

/// Vote direction for forum posts.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VoteDirection {
    /// Upvote.
    Up,
    /// Downvote.
    Down,
}

/// Parameters for the `vote_on_post` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VoteOnPostParams {
    /// UUID of the forum channel.
    pub channel_id: String,
    /// 64-character hex event ID of the post or comment being voted on.
    pub event_id: String,
    /// Vote direction.
    pub direction: VoteDirection,
}

/// Parameters for [`SproutServer::publish_note`].
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PublishNoteParams {
    /// Text content of the note (max 64 KiB).
    pub content: String,
    /// 64-char hex event ID to reply to. Adds a single e-tag with "reply" marker.
    #[serde(default)]
    pub reply_to_event_id: Option<String>,
}

/// A single contact entry for [`SetContactListParams`].
///
/// Kept local to MCP — not part of sprout-sdk. The SDK builder takes primitive slices.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ContactEntry {
    /// 64-char hex pubkey (any case accepted, normalized to lowercase).
    pub pubkey: String,
    /// Optional relay URL hint (NIP-02). Empty string if omitted.
    #[serde(default)]
    pub relay_url: Option<String>,
    /// Optional petname / display alias.
    #[serde(default)]
    pub petname: Option<String>,
}

/// Parameters for [`SproutServer::set_contact_list`].
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SetContactListParams {
    /// Replaces the **entire** contact list. Call `get_contact_list` first for delta updates.
    pub contacts: Vec<ContactEntry>,
}

/// Parameters for [`SproutServer::get_event`].
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GetEventParams {
    /// 64-char hex event ID.
    pub event_id: String,
}

/// Parameters for [`SproutServer::get_user_notes`].
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GetUserNotesParams {
    /// 64-char hex pubkey of the author.
    pub pubkey: String,
    /// Maximum number of notes to return (default 50, max 100).
    #[serde(default)]
    pub limit: Option<u32>,
    /// Unix timestamp cursor — return notes created before this time.
    /// Use with `before_id` for stable composite cursor pagination.
    #[serde(default)]
    pub before: Option<i64>,
    /// Hex event ID cursor for composite keyset pagination. Use together with
    /// `before` to avoid skipping same-second events. Pass the `before_id` value
    /// from the previous page's `next_cursor` response.
    #[serde(default)]
    pub before_id: Option<String>,
}

/// Parameters for [`SproutServer::get_contact_list`].
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GetContactListParams {
    /// 64-char hex pubkey of the user whose contact list to fetch.
    pub pubkey: String,
}

// ── Diff utility functions ────────────────────────────────────────────────────

// Truncation notice appended when a diff is cut. This constant is used to
// reserve space so the final result never exceeds max_bytes.
// NOTE: This function is only called with max_bytes = 60 * 1024, so the
// hardcoded "60KB" in the notice is intentional and always accurate.
const TRUNCATION_NOTICE: &str =
    "\n\\ Diff truncated at 60KB. Full diff available at the source repository.";

/// Truncate a diff to at most `max_bytes` bytes, cutting at a hunk boundary
/// where possible. Returns the (possibly truncated) string and a flag indicating
/// whether truncation occurred.
///
/// The truncation notice is included within the `max_bytes` budget — the
/// returned string is guaranteed to be `<= max_bytes` in length.
fn truncate_diff(diff: &str, max_bytes: usize) -> (String, bool) {
    debug_assert!(
        max_bytes >= TRUNCATION_NOTICE.len(),
        "max_bytes ({max_bytes}) must be >= TRUNCATION_NOTICE length ({})",
        TRUNCATION_NOTICE.len()
    );

    if diff.len() <= max_bytes {
        return (diff.to_string(), false);
    }

    // Reserve space for the truncation notice so the final result stays within max_bytes.
    let effective_limit = max_bytes.saturating_sub(TRUNCATION_NOTICE.len());

    // Step 1: Find the last UTF-8 char boundary at or before effective_limit
    let utf8_boundary = diff
        .char_indices()
        .map(|(i, _)| i)
        .take_while(|&i| i <= effective_limit)
        .last()
        .unwrap_or(0);

    // Step 2: Within safe prefix, find last complete hunk boundary
    let safe_prefix = &diff[..utf8_boundary];
    let last_hunk_start = safe_prefix.rfind("\n@@");

    let cut_point = match last_hunk_start {
        Some(pos) if pos > 0 => pos,
        _ => safe_prefix.rfind('\n').unwrap_or(utf8_boundary),
    };

    let mut result = diff[..cut_point].to_string();
    result.push_str(TRUNCATION_NOTICE);
    (result, true)
}

/// Infer a language name from a file path's extension for syntax highlighting.
/// Returns `None` if the extension is unknown or absent.
fn infer_language(file_path: &str) -> Option<String> {
    // Note: rsplit always yields at least one element (the full string if no '.' found),
    // so .next() always returns Some. The ? is effectively a no-op here.
    let ext = file_path.rsplit('.').next()?;
    let lang = match ext {
        "rs" => "rust",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" => "javascript",
        "py" => "python",
        "go" => "go",
        "java" => "java",
        "rb" => "ruby",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" => "cpp",
        "cs" => "csharp",
        "swift" => "swift",
        "kt" | "kts" => "kotlin",
        "scala" => "scala",
        "sh" | "bash" | "zsh" => "bash",
        "sql" => "sql",
        "html" | "htm" => "html",
        "css" | "scss" | "sass" => "css",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "xml" => "xml",
        "md" | "markdown" => "markdown",
        "dockerfile" => "dockerfile",
        _ => return None,
    };
    Some(lang.to_string())
}

/// Parameters for the `upload_file` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UploadFileParams {
    /// Local filesystem path to the file to upload.
    pub file_path: String,
}

/// The MCP server that exposes Sprout relay functionality as tools.
#[derive(Clone)]
pub struct SproutMcpServer {
    client: RelayClient,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl SproutMcpServer {
    /// Create a new [`SproutMcpServer`] backed by the given relay client.
    ///
    /// Pass `tools_to_remove` to filter out tools by name (e.g. from toolset config).
    pub fn new(
        client: RelayClient,
        tools_to_remove: Option<std::collections::HashSet<&'static str>>,
    ) -> Self {
        let mut tool_router = Self::tool_router();
        if let Some(ref remove) = tools_to_remove {
            for name in remove {
                tool_router.remove_route(name);
            }
        }

        Self {
            client,
            tool_router,
        }
    }

    /// Resolve a `ThreadRef` for SDK builders by fetching the parent event.
    ///
    /// Determines root vs. parent for NIP-10 markers:
    /// - Direct reply: root == parent
    /// - Nested reply: root is the thread root, parent is the immediate reply target
    async fn resolve_thread_ref(
        &self,
        parent_event_id: &str,
        parent_eid: EventId,
    ) -> Result<sprout_sdk::ThreadRef, String> {
        let resp = self
            .client
            .get(&format!("/api/events/{}", parent_event_id))
            .await
            .map_err(|e| format!("failed to fetch parent event: {e}"))?;

        let event_json: serde_json::Value = serde_json::from_str(&resp)
            .map_err(|e| format!("failed to parse parent event: {e}"))?;

        let root_eid = match find_root_from_tags(&event_json["tags"]) {
            Some(root_hex) if root_hex != parent_event_id => EventId::from_hex(&root_hex)
                .map_err(|e| format!("failed to parse root event id: {e}"))?,
            _ => parent_eid,
        };

        Ok(sprout_sdk::ThreadRef {
            root_event_id: root_eid,
            parent_event_id: parent_eid,
        })
    }

    /// Send a message to a Sprout channel.
    #[tool(
        name = "send_message",
        description = "Send a message to a Sprout channel. Include `parent_event_id` to reply in a thread. \
Set `broadcast_to_channel` to also surface the reply in the main channel timeline. \
For forum channels, set `kind` to 45001 (post) or 45003 (comment with `parent_event_id`). \
Default kind is 9 (stream message)."
    )]
    pub async fn send_message(&self, Parameters(p): Parameters<SendMessageParams>) -> String {
        if let Err(e) = validate_uuid(&p.channel_id) {
            return format!("Error: {e}");
        }
        if p.content.len() > MAX_CONTENT_BYTES {
            return format!(
                "Error: content exceeds maximum size of {} bytes (got {})",
                MAX_CONTENT_BYTES,
                p.content.len()
            );
        }
        if let Some(ref parent_id) = p.parent_event_id {
            if parent_id.len() != 64 || !parent_id.chars().all(|c| c.is_ascii_hexdigit()) {
                return format!(
                    "Error: parent_event_id must be a 64-character hex string (got {:?})",
                    parent_id
                );
            }
        }
        if let Some(ref mentions) = p.mention_pubkeys {
            for pk in mentions {
                if pk.len() != 64 || !pk.chars().all(|c| c.is_ascii_hexdigit()) {
                    return format!(
                        "Error: mention_pubkeys entry must be a 64-character hex string (got {:?})",
                        pk
                    );
                }
            }
        }

        let channel_uuid = match uuid::Uuid::parse_str(&p.channel_id) {
            Ok(u) => u,
            Err(_) => return format!("Error: invalid UUID: {}", p.channel_id),
        };
        let kind_num = p
            .kind
            .unwrap_or(sprout_core::kind::KIND_STREAM_MESSAGE as u16);
        // Collect explicit pubkeys, dedup case-insensitively.
        let mut seen = std::collections::HashSet::new();
        let mut mentions: Vec<String> = p
            .mention_pubkeys
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|s| s.to_ascii_lowercase())
            .filter(|s| seen.insert(s.clone()))
            .collect();

        // Auto-resolve @names in content and merge, up to SDK cap of 50.
        let auto = resolve_content_mentions(&self.client, &p.channel_id, &p.content).await;
        let budget = 50usize.saturating_sub(mentions.len());
        let mut added = 0usize;
        for pk in &auto {
            if added >= budget {
                break;
            }
            if !mentions.contains(pk) {
                mentions.push(pk.clone());
                added += 1;
            }
        }

        let mention_refs: Vec<&str> = mentions.iter().map(String::as_str).collect();
        let broadcast = p.broadcast_to_channel.unwrap_or(false);

        // Upload files and build media tags
        let mut media_tags: Vec<Vec<String>> = Vec::new();
        let mut media_content = String::new();
        if let Some(ref paths) = p.file_paths {
            for path in paths {
                match crate::upload::upload_file(
                    self.client.http_client(),
                    self.client.keys(),
                    &self.client.relay_http_url(),
                    self.client.api_token(),
                    self.client.server_domain().as_deref(),
                    path,
                )
                .await
                {
                    Ok(desc) => {
                        media_tags.push(crate::upload::build_imeta_tag(&desc));
                        if desc.mime_type.starts_with("video/") {
                            media_content.push_str("\n![video](");
                        } else {
                            media_content.push_str("\n![image](");
                        }
                        media_content.push_str(&desc.url);
                        media_content.push(')');
                    }
                    Err(e) => {
                        return format!("Error uploading {}: {e}", path);
                    }
                }
            }
        }
        let final_content = if media_content.is_empty() {
            p.content.clone()
        } else {
            format!("{}{}", p.content, media_content)
        };

        // Build the event builder via SDK, routing by kind.
        let builder = match kind_num as u32 {
            sprout_core::kind::KIND_FORUM_POST => {
                // kind 45001: forum post (no thread ref, no broadcast)
                match sprout_sdk::build_forum_post(
                    channel_uuid,
                    &final_content,
                    &mention_refs,
                    &media_tags,
                ) {
                    Ok(b) => b,
                    Err(e) => return format!("Error: {e}"),
                }
            }
            sprout_core::kind::KIND_FORUM_COMMENT => {
                // kind 45003: forum comment — requires parent_event_id
                let parent_id = match p.parent_event_id.as_deref() {
                    Some(id) => id,
                    None => return "Error: kind 45003 requires parent_event_id".to_string(),
                };
                let parent_eid = match EventId::from_hex(parent_id) {
                    Ok(id) => id,
                    Err(e) => return format!("Error: invalid parent_event_id: {e}"),
                };
                // Fetch parent to resolve thread root for NIP-10 markers.
                let thread_ref = match self.resolve_thread_ref(parent_id, parent_eid).await {
                    Ok(tr) => tr,
                    Err(e) => return format!("Error: {e}"),
                };
                match sprout_sdk::build_forum_comment(
                    channel_uuid,
                    &final_content,
                    &thread_ref,
                    &mention_refs,
                    &media_tags,
                ) {
                    Ok(b) => b,
                    Err(e) => return format!("Error: {e}"),
                }
            }
            _ => {
                // kind 9 (default) and any other stream message kinds.
                let thread_ref = if let Some(ref parent_id) = p.parent_event_id {
                    let parent_eid = match EventId::from_hex(parent_id) {
                        Ok(id) => id,
                        Err(e) => return format!("Error: invalid parent_event_id: {e}"),
                    };
                    match self.resolve_thread_ref(parent_id, parent_eid).await {
                        Ok(tr) => Some(tr),
                        Err(e) => return format!("Error: {e}"),
                    }
                } else {
                    None
                };
                match sprout_sdk::build_message(
                    channel_uuid,
                    &final_content,
                    thread_ref.as_ref(),
                    &mention_refs,
                    broadcast && p.parent_event_id.is_some(),
                    &media_tags,
                ) {
                    Ok(b) => b,
                    Err(e) => return format!("Error: {e}"),
                }
            }
        };

        let event = match self.client.sign_event(builder) {
            Ok(e) => e,
            Err(e) => return format!("Error: failed to sign message event: {e}"),
        };

        match self.client.send_event(event).await {
            Ok(ok) => serde_json::json!({
                "event_id": ok.event_id,
                "accepted": ok.accepted,
                "message": ok.message,
            })
            .to_string(),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Send a code diff to a Sprout channel as kind:40008.
    #[tool(
        name = "send_diff_message",
        description = "Send a code diff to a Sprout channel with syntax highlighting and structured metadata. \
Include `parent_event_id` to post the diff as a thread reply. \
The diff is rendered with GitHub-quality visualization in the desktop client."
    )]
    pub async fn send_diff_message(
        &self,
        Parameters(p): Parameters<SendDiffMessageParams>,
    ) -> String {
        let SendDiffMessageParams {
            channel_id,
            diff,
            repo_url,
            commit_sha,
            file_path,
            parent_commit_sha,
            source_branch,
            target_branch,
            pr_number,
            language,
            description,
            parent_event_id,
        } = p;

        if let Err(e) = validate_uuid(&channel_id) {
            return format!("Error: {e}");
        }
        let channel_uuid = match uuid::Uuid::parse_str(&channel_id) {
            Ok(u) => u,
            Err(_) => return format!("Error: invalid UUID: {channel_id}"),
        };

        // 1. Truncate diff at 60KB (UTF-8 safe)
        let (diff_content, truncated) = truncate_diff(&diff, 60 * 1024);

        // 2. Infer language from file extension if not provided
        let lang = language.or_else(|| file_path.as_deref().and_then(infer_language));

        // 3. Build NIP-31 alt text
        let alt_text = match &description {
            Some(desc) => format!(
                "Diff: {} — {}",
                file_path.as_deref().unwrap_or("diff"),
                desc
            ),
            None => format!("Diff: {}", file_path.as_deref().unwrap_or("diff")),
        };

        // 4. Warn on partial branch metadata (both or neither required)
        match (&source_branch, &target_branch) {
            (Some(_), None) | (None, Some(_)) => {
                tracing::warn!("send_diff_message: only one of source_branch/target_branch provided — both required, branch metadata omitted");
            }
            _ => {}
        }
        let branch = match (source_branch, target_branch) {
            (Some(src), Some(tgt)) => Some((src, tgt)),
            _ => None,
        };

        // 5. Resolve optional thread ref
        let thread_ref = if let Some(ref parent_id) = parent_event_id {
            let parent_eid = match EventId::from_hex(parent_id) {
                Ok(id) => id,
                Err(e) => return format!("Error: invalid parent_event_id: {e}"),
            };
            match self.resolve_thread_ref(parent_id, parent_eid).await {
                Ok(tr) => Some(tr),
                Err(e) => return format!("Error: {e}"),
            }
        } else {
            None
        };

        // 6. Build signed event via SDK
        let diff_meta = sprout_sdk::DiffMeta {
            repo_url,
            commit_sha,
            file_path,
            parent_commit: parent_commit_sha,
            branch,
            pr_number,
            language: lang,
            description,
            truncated,
            alt_text: Some(alt_text),
        };
        let builder = match sprout_sdk::build_diff_message(
            channel_uuid,
            &diff_content,
            &diff_meta,
            thread_ref.as_ref(),
        ) {
            Ok(b) => b,
            Err(e) => return format!("Error: {e}"),
        };
        let event = match self.client.sign_event(builder) {
            Ok(e) => e,
            Err(e) => return format!("Error: failed to sign diff event: {e}"),
        };
        match self.client.send_event(event).await {
            Ok(ok) => serde_json::json!({
                "event_id": ok.event_id,
                "accepted": ok.accepted,
                "message": ok.message,
            })
            .to_string(),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Edit a message you previously sent.
    #[tool(
        name = "edit_message",
        description = "Edit a message you previously sent. Creates an edit event (kind 40003) referencing the original."
    )]
    pub async fn edit_message(&self, Parameters(p): Parameters<EditMessageParams>) -> String {
        if let Err(e) = validate_uuid(&p.channel_id) {
            return format!("Error: {e}");
        }
        if p.event_id.len() != 64 || !p.event_id.chars().all(|c| c.is_ascii_hexdigit()) {
            return format!(
                "Error: event_id must be a 64-character hex string (got {:?})",
                p.event_id
            );
        }
        if p.content.len() > MAX_CONTENT_BYTES {
            return format!(
                "Error: content exceeds maximum size of {} bytes (got {})",
                MAX_CONTENT_BYTES,
                p.content.len()
            );
        }

        let channel_uuid = match uuid::Uuid::parse_str(&p.channel_id) {
            Ok(u) => u,
            Err(_) => return format!("Error: invalid UUID: {}", p.channel_id),
        };
        let target_eid = match EventId::from_hex(&p.event_id) {
            Ok(id) => id,
            Err(e) => return format!("Error: invalid event_id: {e}"),
        };

        let builder = match sprout_sdk::build_edit(channel_uuid, target_eid, &p.content) {
            Ok(b) => b,
            Err(e) => return format!("Error: {e}"),
        };
        let event = match self.client.sign_event(builder) {
            Ok(e) => e,
            Err(e) => return format!("Error: failed to sign edit event: {e}"),
        };
        match self.client.send_event(event).await {
            Ok(ok) => serde_json::json!({
                "event_id": ok.event_id,
                "accepted": ok.accepted,
                "message": ok.message,
            })
            .to_string(),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Delete a message.
    #[tool(
        name = "delete_message",
        description = "Delete a message. You must be the message author or a channel owner/admin."
    )]
    pub async fn delete_message(&self, Parameters(p): Parameters<DeleteMessageParams>) -> String {
        if p.event_id.len() != 64 || !p.event_id.chars().all(|c| c.is_ascii_hexdigit()) {
            return format!(
                "Error: event_id must be a 64-character hex string (got {:?})",
                p.event_id
            );
        }
        let target_eid = match EventId::from_hex(&p.event_id) {
            Ok(id) => id,
            Err(e) => return format!("Error: invalid event_id: {e}"),
        };

        // Fetch the event to extract its channel_id (h-tag) — required by build_delete_message.
        let resp = match self
            .client
            .get(&format!("/api/events/{}", p.event_id))
            .await
        {
            Ok(r) => r,
            Err(e) => return format!("Error: failed to fetch event: {e}"),
        };
        let event_json: serde_json::Value = match serde_json::from_str(&resp) {
            Ok(v) => v,
            Err(e) => return format!("Error: failed to parse event: {e}"),
        };
        let channel_id_str = match event_json["tags"].as_array().and_then(|tags| {
            tags.iter().find_map(|t| {
                let parts = t.as_array()?;
                if parts.first()?.as_str() == Some("h") {
                    parts.get(1)?.as_str().map(|s| s.to_string())
                } else {
                    None
                }
            })
        }) {
            Some(id) => id,
            None => return "Error: could not find channel_id (h-tag) on event".to_string(),
        };
        let channel_uuid = match uuid::Uuid::parse_str(&channel_id_str) {
            Ok(u) => u,
            Err(_) => return format!("Error: event h-tag is not a valid UUID: {channel_id_str}"),
        };

        let builder = match sprout_sdk::build_delete_message(channel_uuid, target_eid) {
            Ok(b) => b,
            Err(e) => return format!("Error: {e}"),
        };
        let event = match self.client.sign_event(builder) {
            Ok(e) => e,
            Err(e) => return format!("Error: failed to sign delete event: {e}"),
        };
        match self.client.send_event(event).await {
            Ok(ok) => serde_json::json!({
                "event_id": ok.event_id,
                "accepted": ok.accepted,
                "message": ok.message,
            })
            .to_string(),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Get recent messages from a Sprout channel.
    #[tool(
        name = "get_messages",
        description = "Fetch recent top-level messages from a Sprout channel. Use `before` for backward \
pagination and `since` for forward pagination (both Unix timestamps). When `since` is \
provided without `before`, results are ordered oldest-first — useful for polling new \
messages. Use `kinds` to filter by event type (e.g. \"45001\" for forum posts, \
\"45002\" for votes). Thread summaries are included automatically. Threaded replies \
are not returned — use `get_thread` to fetch the full reply tree for a specific message."
    )]
    pub async fn get_messages(&self, Parameters(p): Parameters<GetMessagesParams>) -> String {
        if let Err(e) = validate_uuid(&p.channel_id) {
            return format!("Error: {e}");
        }

        const MAX_HISTORY_LIMIT: u32 = 200;
        let limit = p.limit.unwrap_or(50).min(MAX_HISTORY_LIMIT);

        // Use the REST endpoint so callers get the canonical history payload.
        // Note: with_threads is legacy — summaries are always included server-side.
        let with_threads = p.with_threads.unwrap_or(false);
        let mut query_parts: Vec<String> = Vec::new();
        if with_threads {
            query_parts.push("with_threads=true".to_string());
        }
        query_parts.push(format!("limit={limit}"));
        if let Some(before) = p.before {
            query_parts.push(format!("before={before}"));
        }
        if let Some(since) = p.since {
            query_parts.push(format!("since={since}"));
        }
        if let Some(ref kinds) = p.kinds {
            query_parts.push(format!("kinds={}", percent_encode(kinds)));
        }
        let path = format!(
            "/api/channels/{}/messages?{}",
            p.channel_id,
            query_parts.join("&")
        );
        match self.client.get(&path).await {
            Ok(body) => body,
            Err(e) => format!("Error: {e}"),
        }
    }

    /// List Sprout channels accessible to this agent.
    #[tool(
        name = "list_channels",
        description = "List Sprout channels accessible to this agent"
    )]
    pub async fn list_channels(&self, Parameters(p): Parameters<ListChannelsParams>) -> String {
        // Use the REST endpoint — faster and simpler than a WebSocket subscription.
        let path = if let Some(ref vis) = p.visibility {
            // percent-encode the visibility value to prevent query-string injection
            let encoded = percent_encode(vis);
            format!("/api/channels?visibility={encoded}")
        } else {
            "/api/channels".to_string()
        };
        match self.client.get(&path).await {
            Ok(body) => body,
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Create a new Sprout channel.
    #[tool(
        name = "create_channel",
        description = "Create a new Sprout channel. channel_type must be 'stream' or 'forum'. visibility must be 'open' or 'private'."
    )]
    pub async fn create_channel(&self, Parameters(p): Parameters<CreateChannelParams>) -> String {
        let channel_uuid = uuid::Uuid::new_v4();
        let visibility = match p.visibility.as_str() {
            "open" => sprout_sdk::Visibility::Open,
            "private" => sprout_sdk::Visibility::Private,
            other => {
                return format!(
                    "Error: invalid visibility: {other:?} (must be 'open' or 'private')"
                )
            }
        };
        let channel_type = match p.channel_type.as_str() {
            "stream" => sprout_sdk::ChannelKind::Stream,
            "forum" => sprout_sdk::ChannelKind::Forum,
            other => {
                return format!(
                    "Error: invalid channel_type: {other:?} (must be 'stream' or 'forum')"
                )
            }
        };
        let builder = match sprout_sdk::build_create_channel(
            channel_uuid,
            &p.name,
            Some(visibility),
            Some(channel_type),
            p.description.as_deref(),
        ) {
            Ok(b) => b,
            Err(e) => return format!("Error: {e}"),
        };
        let event = match self.client.sign_event(builder) {
            Ok(e) => e,
            Err(e) => return format!("Error: failed to sign create_channel event: {e}"),
        };
        match self.client.send_event(event).await {
            Ok(ok) => serde_json::json!({
                "event_id": ok.event_id,
                "channel_id": channel_uuid.to_string(),
                "accepted": ok.accepted,
                "message": ok.message,
            })
            .to_string(),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Get the canvas (shared document) for a channel.
    #[tool(
        name = "get_canvas",
        description = "Get the canvas (shared document) for a channel"
    )]
    pub async fn get_canvas(&self, Parameters(p): Parameters<GetCanvasParams>) -> String {
        if let Err(e) = validate_uuid(&p.channel_id) {
            return format!("Error: {e}");
        }
        match self.client.get_canvas(&p.channel_id).await {
            Ok(body) => {
                // Parse REST JSON and return just the content string.
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
                    match v.get("content").and_then(|c| c.as_str()) {
                        Some(content) => content.to_string(),
                        None => "No canvas set for this channel.".to_string(),
                    }
                } else {
                    body
                }
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Set or update the canvas (shared document) for a channel.
    #[tool(
        name = "set_canvas",
        description = "Set or update the canvas (shared document) for a channel"
    )]
    pub async fn set_canvas(&self, Parameters(p): Parameters<SetCanvasParams>) -> String {
        if let Err(e) = validate_uuid(&p.channel_id) {
            return format!("Error: {e}");
        }
        let channel_uuid = match uuid::Uuid::parse_str(&p.channel_id) {
            Ok(u) => u,
            Err(_) => return format!("Error: invalid UUID: {}", p.channel_id),
        };
        let builder = match sprout_sdk::build_set_canvas(channel_uuid, &p.content) {
            Ok(b) => b,
            Err(e) => return format!("Error: {e}"),
        };
        let event = match self.client.sign_event(builder) {
            Ok(e) => e,
            Err(e) => return format!("Error: failed to sign set_canvas event: {e}"),
        };
        match self.client.send_event(event).await {
            Ok(ok) => serde_json::json!({
                "event_id": ok.event_id,
                "accepted": ok.accepted,
                "message": ok.message,
            })
            .to_string(),
            Err(e) => format!("Error: {e}"),
        }
    }

    // ── Workflow tools ────────────────────────────────────────────────────────

    /// List workflows defined in a Sprout channel.
    #[tool(
        name = "list_workflows",
        description = "List workflows defined in a Sprout channel"
    )]
    pub async fn list_workflows(&self, Parameters(p): Parameters<ListWorkflowsParams>) -> String {
        if uuid::Uuid::parse_str(&p.channel_id).is_err() {
            return format!("Error: channel_id '{}' is not a valid UUID", p.channel_id);
        }
        match self
            .client
            .get(&format!("/api/channels/{}/workflows", p.channel_id))
            .await
        {
            Ok(body) => body,
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Create a new workflow in a channel from a YAML definition.
    #[tool(
        name = "create_workflow",
        description = "Create a new workflow from a YAML definition. Steps need 'id' (not 'name'), and action fields are direct properties (not nested under 'params'). Triggers: message_posted, reaction_added, webhook."
    )]
    pub async fn create_workflow(&self, Parameters(p): Parameters<CreateWorkflowParams>) -> String {
        if uuid::Uuid::parse_str(&p.channel_id).is_err() {
            return format!("Error: channel_id '{}' is not a valid UUID", p.channel_id);
        }
        let body = serde_json::json!({ "yaml_definition": p.yaml_definition });
        match self
            .client
            .post(&format!("/api/channels/{}/workflows", p.channel_id), &body)
            .await
        {
            Ok(b) => b,
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Replace a workflow's YAML definition.
    #[tool(
        name = "update_workflow",
        description = "Replace a workflow's YAML definition"
    )]
    pub async fn update_workflow(&self, Parameters(p): Parameters<UpdateWorkflowParams>) -> String {
        if uuid::Uuid::parse_str(&p.workflow_id).is_err() {
            return format!("Error: workflow_id '{}' is not a valid UUID", p.workflow_id);
        }
        let body = serde_json::json!({ "yaml_definition": p.yaml_definition });
        match self
            .client
            .put(&format!("/api/workflows/{}", p.workflow_id), &body)
            .await
        {
            Ok(b) => b,
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Delete a workflow by ID.
    #[tool(name = "delete_workflow", description = "Delete a workflow by ID")]
    pub async fn delete_workflow(&self, Parameters(p): Parameters<DeleteWorkflowParams>) -> String {
        if uuid::Uuid::parse_str(&p.workflow_id).is_err() {
            return format!("Error: workflow_id '{}' is not a valid UUID", p.workflow_id);
        }
        match self
            .client
            .delete(&format!("/api/workflows/{}", p.workflow_id))
            .await
        {
            Ok(_) => "Workflow deleted.".to_string(),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Manually trigger a workflow with optional input variables.
    #[tool(
        name = "trigger_workflow",
        description = "Manually trigger a workflow with optional input variables"
    )]
    pub async fn trigger_workflow(
        &self,
        Parameters(p): Parameters<TriggerWorkflowParams>,
    ) -> String {
        if uuid::Uuid::parse_str(&p.workflow_id).is_err() {
            return format!("Error: workflow_id '{}' is not a valid UUID", p.workflow_id);
        }
        let body = serde_json::json!({
            "inputs": p.inputs.unwrap_or(serde_json::Value::Object(Default::default()))
        });
        match self
            .client
            .post(&format!("/api/workflows/{}/trigger", p.workflow_id), &body)
            .await
        {
            Ok(b) => b,
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Get execution history for a workflow.
    #[tool(
        name = "get_workflow_runs",
        description = "Get execution history for a workflow"
    )]
    pub async fn get_workflow_runs(
        &self,
        Parameters(p): Parameters<GetWorkflowRunsParams>,
    ) -> String {
        if uuid::Uuid::parse_str(&p.workflow_id).is_err() {
            return format!("Error: workflow_id '{}' is not a valid UUID", p.workflow_id);
        }
        let limit = p.limit.unwrap_or(20).min(100);
        match self
            .client
            .get(&format!(
                "/api/workflows/{}/runs?limit={}",
                p.workflow_id, limit
            ))
            .await
        {
            Ok(b) => b,
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Approve or deny a pending workflow approval step.
    #[tool(
        name = "approve_step",
        description = "Approve or deny a pending workflow approval step"
    )]
    pub async fn approve_step(&self, Parameters(p): Parameters<ApproveStepParams>) -> String {
        if uuid::Uuid::parse_str(&p.approval_token).is_err() {
            return format!(
                "Error: approval_token '{}' is not a valid UUID",
                p.approval_token
            );
        }
        let route = if p.approved {
            format!("/api/approvals/{}/grant", p.approval_token)
        } else {
            format!("/api/approvals/{}/deny", p.approval_token)
        };
        let body = serde_json::json!({ "note": p.note });
        match self.client.post(&route, &body).await {
            Ok(b) => b,
            Err(e) => format!("Error: {e}"),
        }
    }

    // ── Feed tools ────────────────────────────────────────────────────────────

    /// Get the agent's personalized home feed from the Sprout relay.
    #[tool(
        name = "get_feed",
        description = "Get the agent's personalized home feed from the Sprout relay. \
                       Returns mentions, needs-action items, channel activity, and agent activity. \
                       Equivalent to what a human sees on the Home tab in the desktop app."
    )]
    pub async fn get_feed(&self, Parameters(p): Parameters<GetFeedParams>) -> String {
        const MAX_FEED_LIMIT: u32 = 50;
        let base = format!("{}/api/feed", self.client.relay_http_url());
        let mut query_parts: Vec<String> = Vec::new();
        if let Some(since) = p.since {
            query_parts.push(format!("since={since}"));
        }
        if let Some(limit) = p.limit {
            query_parts.push(format!("limit={}", limit.min(MAX_FEED_LIMIT)));
        }
        if let Some(types) = &p.types {
            // percent-encode to prevent query-string injection (e.g. values containing & or ?)
            query_parts.push(format!("types={}", percent_encode(types)));
        }
        let url = if query_parts.is_empty() {
            base
        } else {
            format!("{base}?{}", query_parts.join("&"))
        };
        match self.client.get_api(&url).await {
            Ok(body) => body,
            Err(e) => format!("Error fetching feed: {e}"),
        }
    }

    // ── Membership tools ──────────────────────────────────────────────────────

    /// Add a member to a channel.
    #[tool(
        name = "add_channel_member",
        description = "Add a member to a Sprout channel. Optionally specify a role (default: \"member\")."
    )]
    pub async fn add_channel_member(
        &self,
        Parameters(p): Parameters<AddChannelMemberParams>,
    ) -> String {
        if let Err(e) = validate_uuid(&p.channel_id) {
            return format!("Error: {e}");
        }
        let channel_uuid = match uuid::Uuid::parse_str(&p.channel_id) {
            Ok(u) => u,
            Err(_) => return format!("Error: invalid UUID: {}", p.channel_id),
        };
        let role = match p.role.as_deref() {
            None => None,
            Some("owner") => Some(sprout_sdk::MemberRole::Owner),
            Some("admin") => Some(sprout_sdk::MemberRole::Admin),
            Some("member") => Some(sprout_sdk::MemberRole::Member),
            Some("guest") => Some(sprout_sdk::MemberRole::Guest),
            Some("bot") => Some(sprout_sdk::MemberRole::Bot),
            Some(other) => {
                return format!(
                    "Error: invalid role: {other:?} (must be owner/admin/member/guest/bot)"
                )
            }
        };
        let builder = match sprout_sdk::build_add_member(channel_uuid, &p.pubkey, role) {
            Ok(b) => b,
            Err(e) => return format!("Error: {e}"),
        };
        let event = match self.client.sign_event(builder) {
            Ok(e) => e,
            Err(e) => return format!("Error: failed to sign add_member event: {e}"),
        };
        match self.client.send_event(event).await {
            Ok(ok) => serde_json::json!({
                "event_id": ok.event_id,
                "accepted": ok.accepted,
                "message": ok.message,
            })
            .to_string(),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Remove a member from a channel.
    #[tool(
        name = "remove_channel_member",
        description = "Remove a member from a Sprout channel by their public key."
    )]
    pub async fn remove_channel_member(
        &self,
        Parameters(p): Parameters<RemoveChannelMemberParams>,
    ) -> String {
        if let Err(e) = validate_uuid(&p.channel_id) {
            return format!("Error: {e}");
        }
        let channel_uuid = match uuid::Uuid::parse_str(&p.channel_id) {
            Ok(u) => u,
            Err(_) => return format!("Error: invalid UUID: {}", p.channel_id),
        };
        let builder = match sprout_sdk::build_remove_member(channel_uuid, &p.pubkey) {
            Ok(b) => b,
            Err(e) => return format!("Error: {e}"),
        };
        let event = match self.client.sign_event(builder) {
            Ok(e) => e,
            Err(e) => return format!("Error: failed to sign remove_member event: {e}"),
        };
        match self.client.send_event(event).await {
            Ok(ok) => serde_json::json!({
                "event_id": ok.event_id,
                "accepted": ok.accepted,
                "message": ok.message,
            })
            .to_string(),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// List all members of a channel.
    #[tool(
        name = "list_channel_members",
        description = "List all members of a Sprout channel."
    )]
    pub async fn list_channel_members(
        &self,
        Parameters(p): Parameters<ListChannelMembersParams>,
    ) -> String {
        if let Err(e) = validate_uuid(&p.channel_id) {
            return format!("Error: {e}");
        }
        match self
            .client
            .get(&format!("/api/channels/{}/members", p.channel_id))
            .await
        {
            Ok(b) => b,
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Join a channel (add yourself as a member).
    #[tool(
        name = "join_channel",
        description = "Join a Sprout channel (adds the agent as a member)."
    )]
    pub async fn join_channel(&self, Parameters(p): Parameters<JoinChannelParams>) -> String {
        if let Err(e) = validate_uuid(&p.channel_id) {
            return format!("Error: {e}");
        }
        let channel_uuid = match uuid::Uuid::parse_str(&p.channel_id) {
            Ok(u) => u,
            Err(_) => return format!("Error: invalid UUID: {}", p.channel_id),
        };
        let builder = match sprout_sdk::build_join(channel_uuid) {
            Ok(b) => b,
            Err(e) => return format!("Error: {e}"),
        };
        let event = match self.client.sign_event(builder) {
            Ok(e) => e,
            Err(e) => return format!("Error: failed to sign join event: {e}"),
        };
        match self.client.send_event(event).await {
            Ok(ok) => serde_json::json!({
                "event_id": ok.event_id,
                "accepted": ok.accepted,
                "message": ok.message,
            })
            .to_string(),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Leave a channel (remove yourself as a member).
    #[tool(
        name = "leave_channel",
        description = "Leave a Sprout channel (removes the agent as a member)."
    )]
    pub async fn leave_channel(&self, Parameters(p): Parameters<LeaveChannelParams>) -> String {
        if let Err(e) = validate_uuid(&p.channel_id) {
            return format!("Error: {e}");
        }
        let channel_uuid = match uuid::Uuid::parse_str(&p.channel_id) {
            Ok(u) => u,
            Err(_) => return format!("Error: invalid UUID: {}", p.channel_id),
        };
        let builder = match sprout_sdk::build_leave(channel_uuid) {
            Ok(b) => b,
            Err(e) => return format!("Error: {e}"),
        };
        let event = match self.client.sign_event(builder) {
            Ok(e) => e,
            Err(e) => return format!("Error: failed to sign leave event: {e}"),
        };
        match self.client.send_event(event).await {
            Ok(ok) => serde_json::json!({
                "event_id": ok.event_id,
                "accepted": ok.accepted,
                "message": ok.message,
            })
            .to_string(),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Get details for a single channel.
    #[tool(
        name = "get_channel",
        description = "Get metadata and details for a single Sprout channel by ID."
    )]
    pub async fn get_channel(&self, Parameters(p): Parameters<GetChannelParams>) -> String {
        if let Err(e) = validate_uuid(&p.channel_id) {
            return format!("Error: {e}");
        }
        match self
            .client
            .get(&format!("/api/channels/{}", p.channel_id))
            .await
        {
            Ok(b) => b,
            Err(e) => format!("Error: {e}"),
        }
    }

    // ── Metadata tools ────────────────────────────────────────────────────────

    /// Update a channel's name and/or description.
    #[tool(
        name = "update_channel",
        description = "Update a Sprout channel's name and/or description."
    )]
    pub async fn update_channel(&self, Parameters(p): Parameters<UpdateChannelParams>) -> String {
        if let Err(e) = validate_uuid(&p.channel_id) {
            return format!("Error: {e}");
        }
        let channel_uuid = match uuid::Uuid::parse_str(&p.channel_id) {
            Ok(u) => u,
            Err(_) => return format!("Error: invalid UUID: {}", p.channel_id),
        };
        let builder = match sprout_sdk::build_update_channel(
            channel_uuid,
            p.name.as_deref(),
            p.description.as_deref(),
        ) {
            Ok(b) => b,
            Err(e) => return format!("Error: {e}"),
        };
        let event = match self.client.sign_event(builder) {
            Ok(e) => e,
            Err(e) => return format!("Error: failed to sign update_channel event: {e}"),
        };
        match self.client.send_event(event).await {
            Ok(ok) => serde_json::json!({
                "event_id": ok.event_id,
                "accepted": ok.accepted,
                "message": ok.message,
            })
            .to_string(),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Set the topic for a channel.
    #[tool(
        name = "set_channel_topic",
        description = "Set the topic for a Sprout channel."
    )]
    pub async fn set_channel_topic(
        &self,
        Parameters(p): Parameters<SetChannelTopicParams>,
    ) -> String {
        if let Err(e) = validate_uuid(&p.channel_id) {
            return format!("Error: {e}");
        }
        let channel_uuid = match uuid::Uuid::parse_str(&p.channel_id) {
            Ok(u) => u,
            Err(_) => return format!("Error: invalid UUID: {}", p.channel_id),
        };
        let builder = match sprout_sdk::build_set_topic(channel_uuid, &p.topic) {
            Ok(b) => b,
            Err(e) => return format!("Error: {e}"),
        };
        let event = match self.client.sign_event(builder) {
            Ok(e) => e,
            Err(e) => return format!("Error: failed to sign set_topic event: {e}"),
        };
        match self.client.send_event(event).await {
            Ok(ok) => serde_json::json!({
                "event_id": ok.event_id,
                "accepted": ok.accepted,
                "message": ok.message,
            })
            .to_string(),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Set the purpose for a channel.
    #[tool(
        name = "set_channel_purpose",
        description = "Set the purpose for a Sprout channel."
    )]
    pub async fn set_channel_purpose(
        &self,
        Parameters(p): Parameters<SetChannelPurposeParams>,
    ) -> String {
        if let Err(e) = validate_uuid(&p.channel_id) {
            return format!("Error: {e}");
        }
        let channel_uuid = match uuid::Uuid::parse_str(&p.channel_id) {
            Ok(u) => u,
            Err(_) => return format!("Error: invalid UUID: {}", p.channel_id),
        };
        let builder = match sprout_sdk::build_set_purpose(channel_uuid, &p.purpose) {
            Ok(b) => b,
            Err(e) => return format!("Error: {e}"),
        };
        let event = match self.client.sign_event(builder) {
            Ok(e) => e,
            Err(e) => return format!("Error: failed to sign set_purpose event: {e}"),
        };
        match self.client.send_event(event).await {
            Ok(ok) => serde_json::json!({
                "event_id": ok.event_id,
                "accepted": ok.accepted,
                "message": ok.message,
            })
            .to_string(),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Archive a channel (makes it read-only).
    #[tool(
        name = "archive_channel",
        description = "Archive a Sprout channel, making it read-only."
    )]
    pub async fn archive_channel(&self, Parameters(p): Parameters<ArchiveChannelParams>) -> String {
        if let Err(e) = validate_uuid(&p.channel_id) {
            return format!("Error: {e}");
        }
        let channel_uuid = match uuid::Uuid::parse_str(&p.channel_id) {
            Ok(u) => u,
            Err(_) => return format!("Error: invalid UUID: {}", p.channel_id),
        };
        let builder = match sprout_sdk::build_archive(channel_uuid) {
            Ok(b) => b,
            Err(e) => return format!("Error: {e}"),
        };
        let event = match self.client.sign_event(builder) {
            Ok(e) => e,
            Err(e) => return format!("Error: failed to sign archive event: {e}"),
        };
        match self.client.send_event(event).await {
            Ok(ok) => serde_json::json!({
                "event_id": ok.event_id,
                "accepted": ok.accepted,
                "message": ok.message,
            })
            .to_string(),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Unarchive a channel (restores it to active).
    #[tool(
        name = "unarchive_channel",
        description = "Unarchive a Sprout channel, restoring it to active status."
    )]
    pub async fn unarchive_channel(
        &self,
        Parameters(p): Parameters<UnarchiveChannelParams>,
    ) -> String {
        if let Err(e) = validate_uuid(&p.channel_id) {
            return format!("Error: {e}");
        }
        let channel_uuid = match uuid::Uuid::parse_str(&p.channel_id) {
            Ok(u) => u,
            Err(_) => return format!("Error: invalid UUID: {}", p.channel_id),
        };
        let builder = match sprout_sdk::build_unarchive(channel_uuid) {
            Ok(b) => b,
            Err(e) => return format!("Error: {e}"),
        };
        let event = match self.client.sign_event(builder) {
            Ok(e) => e,
            Err(e) => return format!("Error: failed to sign unarchive event: {e}"),
        };
        match self.client.send_event(event).await {
            Ok(ok) => serde_json::json!({
                "event_id": ok.event_id,
                "accepted": ok.accepted,
                "message": ok.message,
            })
            .to_string(),
            Err(e) => format!("Error: {e}"),
        }
    }

    // ── Thread tools ──────────────────────────────────────────────────────────

    /// Get a message thread (replies to a message).
    #[tool(
        name = "get_thread",
        description = "Fetch a full thread tree rooted at an event. Returns the root message and all nested \
replies. Works for both stream message threads and forum post threads (kind:45001 root \
with kind:45003 comments)."
    )]
    pub async fn get_thread(&self, Parameters(p): Parameters<GetThreadParams>) -> String {
        if let Err(e) = validate_uuid(&p.channel_id) {
            return format!("Error: {e}");
        }

        let mut query_parts: Vec<String> = Vec::new();
        if let Some(depth) = p.depth_limit {
            query_parts.push(format!("depth_limit={depth}"));
        }
        if let Some(limit) = p.limit {
            query_parts.push(format!("limit={}", limit.min(200)));
        }

        let encoded_event_id = percent_encode(&p.event_id);
        let path = if query_parts.is_empty() {
            format!(
                "/api/channels/{}/threads/{}",
                p.channel_id, encoded_event_id
            )
        } else {
            format!(
                "/api/channels/{}/threads/{}?{}",
                p.channel_id,
                encoded_event_id,
                query_parts.join("&")
            )
        };

        match self.client.get(&path).await {
            Ok(b) => b,
            Err(e) => format!("Error: {e}"),
        }
    }

    // ── DM tools ──────────────────────────────────────────────────────────────

    /// Open or retrieve a direct message channel with one or more participants.
    #[tool(
        name = "open_dm",
        description = "Open (or retrieve an existing) direct message channel with 1–8 other participants. \
                       Returns the DM channel details including its ID."
    )]
    pub async fn open_dm(&self, Parameters(p): Parameters<OpenDmParams>) -> String {
        if p.pubkeys.is_empty() {
            return "Error: pubkeys must contain at least one participant".to_string();
        }
        if p.pubkeys.len() > 8 {
            return format!(
                "Error: too many participants (max 8, got {})",
                p.pubkeys.len()
            );
        }
        let body = serde_json::json!({ "pubkeys": p.pubkeys });
        match self.client.post("/api/dms", &body).await {
            Ok(b) => b,
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Add a participant to an existing DM channel.
    #[tool(
        name = "add_dm_member",
        description = "Add a participant to an existing Sprout DM channel."
    )]
    pub async fn add_dm_member(&self, Parameters(p): Parameters<AddDmMemberParams>) -> String {
        if let Err(e) = validate_uuid(&p.channel_id) {
            return format!("Error: {e}");
        }
        let body = serde_json::json!({ "pubkeys": [p.pubkey] });
        match self
            .client
            .post(&format!("/api/dms/{}/members", p.channel_id), &body)
            .await
        {
            Ok(b) => b,
            Err(e) => format!("Error: {e}"),
        }
    }

    /// List all DM channels the agent is a participant in.
    #[tool(
        name = "list_dms",
        description = "List all direct message channels the agent is a participant in."
    )]
    pub async fn list_dms(&self) -> String {
        match self.client.get("/api/dms").await {
            Ok(b) => b,
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Hide a DM channel from the agent's DM list.
    #[tool(
        name = "hide_dm",
        description = "Hide a direct message channel from the agent's DM list. The DM can be restored by opening a new DM with the same participants."
    )]
    pub async fn hide_dm(&self, Parameters(p): Parameters<HideDmParams>) -> String {
        if let Err(e) = validate_uuid(&p.channel_id) {
            return format!("Error: {e}");
        }
        match self
            .client
            .post(
                &format!("/api/dms/{}/hide", p.channel_id),
                &serde_json::json!({}),
            )
            .await
        {
            Ok(b) => {
                if b.is_empty() {
                    "DM hidden successfully.".to_string()
                } else {
                    b
                }
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    // ── Reaction tools ────────────────────────────────────────────────────────

    /// Add an emoji reaction to a message.
    #[tool(
        name = "add_reaction",
        description = "Add an emoji reaction to a Sprout message."
    )]
    pub async fn add_reaction(&self, Parameters(p): Parameters<AddReactionParams>) -> String {
        let target_eid = match EventId::from_hex(&p.event_id) {
            Ok(id) => id,
            Err(e) => return format!("Error: invalid event_id: {e}"),
        };
        let builder = match sprout_sdk::build_reaction(target_eid, &p.emoji) {
            Ok(b) => b,
            Err(e) => return format!("Error: {e}"),
        };
        let event = match self.client.sign_event(builder) {
            Ok(e) => e,
            Err(e) => return format!("Error: failed to sign reaction event: {e}"),
        };
        match self.client.send_event(event).await {
            Ok(ok) => serde_json::json!({
                "event_id": ok.event_id,
                "accepted": ok.accepted,
                "message": ok.message,
            })
            .to_string(),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Remove an emoji reaction from a message.
    #[tool(
        name = "remove_reaction",
        description = "Remove an emoji reaction from a Sprout message."
    )]
    pub async fn remove_reaction(&self, Parameters(p): Parameters<RemoveReactionParams>) -> String {
        // Fetch the reactions list to find the current user's reaction event ID for this emoji.
        let encoded_event_id = percent_encode(&p.event_id);
        let my_pubkey = self.client.pubkey_hex();
        let reactions_resp = match self
            .client
            .get(&format!("/api/messages/{}/reactions", encoded_event_id))
            .await
        {
            Ok(r) => r,
            Err(e) => return format!("Error: failed to fetch reactions: {e}"),
        };
        let reactions: serde_json::Value = match serde_json::from_str(&reactions_resp) {
            Ok(v) => v,
            Err(e) => return format!("Error: failed to parse reactions: {e}"),
        };

        // Parse the grouped response: { "reactions": [ { "emoji": "...", "users": [ { "pubkey": "...", "reaction_event_id": "..." } ] } ] }
        let reaction_event_id_hex = reactions
            .get("reactions")
            .and_then(|r| r.as_array())
            .and_then(|groups| {
                groups.iter().find_map(|group| {
                    let group_emoji = group.get("emoji")?.as_str()?;
                    if group_emoji != p.emoji {
                        return None;
                    }
                    group.get("users")?.as_array()?.iter().find_map(|user| {
                        let pubkey = user.get("pubkey")?.as_str()?;
                        if pubkey != my_pubkey {
                            return None;
                        }
                        user.get("reaction_event_id")?
                            .as_str()
                            .map(|s| s.to_string())
                    })
                })
            });

        match reaction_event_id_hex {
            Some(hex) => {
                let reaction_eid = match EventId::from_hex(&hex) {
                    Ok(id) => id,
                    Err(e) => return format!("Error: invalid reaction event_id: {e}"),
                };
                let builder = match sprout_sdk::build_remove_reaction(reaction_eid) {
                    Ok(b) => b,
                    Err(e) => return format!("Error: {e}"),
                };
                let event = match self.client.sign_event(builder) {
                    Ok(e) => e,
                    Err(e) => return format!("Error: failed to sign remove_reaction event: {e}"),
                };
                match self.client.send_event(event).await {
                    Ok(ok) => serde_json::json!({
                        "event_id": ok.event_id,
                        "accepted": ok.accepted,
                        "message": ok.message,
                    })
                    .to_string(),
                    Err(e) => format!("Error: {e}"),
                }
            }
            None => "Error: could not find your reaction event ID for this emoji. \
                 The reaction may not exist or the relay has not recorded the event ID."
                .to_string(),
        }
    }

    /// Get all reactions for a message.
    #[tool(
        name = "get_reactions",
        description = "Get all emoji reactions for a Sprout message."
    )]
    pub async fn get_reactions(&self, Parameters(p): Parameters<GetReactionsParams>) -> String {
        let encoded_event_id = percent_encode(&p.event_id);
        match self
            .client
            .get(&format!("/api/messages/{}/reactions", encoded_event_id))
            .await
        {
            Ok(b) => b,
            Err(e) => format!("Error: {e}"),
        }
    }

    // ── User profile tools ────────────────────────────────────────────────────

    /// Update the agent's user profile.
    #[tool(
        name = "set_profile",
        description = "Update the agent's user profile (display name, about, avatar URL, and/or NIP-05 handle)."
    )]
    pub async fn set_profile(&self, Parameters(p): Parameters<SetProfileParams>) -> String {
        // Read-merge-write: fetch current profile, merge desired changes, sign kind:0.
        let current_profile: serde_json::Value =
            match self.client.get("/api/users/me/profile").await {
                Ok(body) => serde_json::from_str(&body)
                    .unwrap_or(serde_json::Value::Object(Default::default())),
                Err(_) => serde_json::Value::Object(Default::default()),
            };

        // Resolve each field: use new value if provided, else keep existing.
        let display_name = p
            .display_name
            .as_deref()
            .or_else(|| current_profile.get("display_name").and_then(|v| v.as_str()));
        let name = current_profile.get("name").and_then(|v| v.as_str());
        let picture = p
            .avatar_url
            .as_deref()
            .or_else(|| current_profile.get("avatar_url").and_then(|v| v.as_str()));
        let about = p
            .about
            .as_deref()
            .or_else(|| current_profile.get("about").and_then(|v| v.as_str()));
        let nip05 = p
            .nip05_handle
            .as_deref()
            .or_else(|| current_profile.get("nip05_handle").and_then(|v| v.as_str()));

        let builder = match sprout_sdk::build_profile(display_name, name, picture, about, nip05) {
            Ok(b) => b,
            Err(e) => return format!("Error: {e}"),
        };
        let event = match self.client.sign_event(builder) {
            Ok(e) => e,
            Err(e) => return format!("Error: failed to sign profile event: {e}"),
        };
        match self.client.send_event(event).await {
            Ok(ok) => serde_json::json!({
                "event_id": ok.event_id,
                "accepted": ok.accepted,
                "message": ok.message,
            })
            .to_string(),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Get user profile(s) by pubkey.
    #[tool(
        name = "get_users",
        description = "Get user profile(s). Omit pubkeys for your own profile, provide one for a specific user, or provide multiple for batch lookup (max 200)."
    )]
    pub async fn get_users(&self, Parameters(p): Parameters<GetUsersParams>) -> String {
        let pubkeys = p.pubkeys.unwrap_or_default();
        if pubkeys.len() > 200 {
            return "Error: max 200 pubkeys for batch lookup".to_string();
        }
        for pk in &pubkeys {
            if pk.len() != 64 || !pk.chars().all(|c| c.is_ascii_hexdigit()) {
                return format!(
                    "Error: pubkey must be a 64-character hex string (got {:?})",
                    pk
                );
            }
        }
        match pubkeys.len() {
            0 => match self.client.get("/api/users/me/profile").await {
                Ok(body) => body,
                Err(e) => format!("Error fetching profile: {e}"),
            },
            1 => {
                let path = format!("/api/users/{}/profile", percent_encode(&pubkeys[0]));
                match self.client.get(&path).await {
                    Ok(body) => body,
                    Err(e) => format!("Error fetching profile: {e}"),
                }
            }
            _ => {
                let body = serde_json::json!({ "pubkeys": pubkeys });
                match self.client.post("/api/users/batch", &body).await {
                    Ok(resp) => resp,
                    Err(e) => format!("Error fetching profiles: {e}"),
                }
            }
        }
    }

    /// Full-text search across messages.
    #[tool(
        name = "search",
        description = "Full-text search across messages in accessible channels. Returns matching messages with channel context. Powered by Typesense."
    )]
    pub async fn search(&self, Parameters(p): Parameters<SearchParams>) -> String {
        let limit = p.limit.unwrap_or(20).min(100);
        let path = format!("/api/search?q={}&limit={}", percent_encode(&p.q), limit);
        match self.client.get(&path).await {
            Ok(body) => body,
            Err(e) => format!("Error searching: {e}"),
        }
    }

    /// Get presence status for one or more users.
    #[tool(
        name = "get_presence",
        description = "Get presence status (online/away/offline) for one or more users by pubkey. Pass comma-separated hex pubkeys."
    )]
    pub async fn get_presence(&self, Parameters(p): Parameters<GetPresenceParams>) -> String {
        let path = format!("/api/presence?pubkeys={}", percent_encode(&p.pubkeys));
        match self.client.get(&path).await {
            Ok(body) => body,
            Err(e) => format!("Error fetching presence: {e}"),
        }
    }

    /// Set the agent's presence status.
    #[tool(
        name = "set_presence",
        description = "Set the agent's presence status. Valid values: 'online', 'away', 'offline'. Presence auto-expires after 90 seconds — call periodically to stay online."
    )]
    pub async fn set_presence(&self, Parameters(p): Parameters<SetPresenceParams>) -> String {
        let body = serde_json::json!({ "status": p.status });
        match self.client.put("/api/presence", &body).await {
            Ok(b) => b,
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Set this agent's channel addition policy.
    #[tool(
        name = "set_channel_add_policy",
        description = "Set your channel addition policy. 'anyone' = any authenticated user can add you to open channels (default). 'owner_only' = only your provisioned owner can add you. 'nobody' = no one can add you; you may self-join open channels via join_channel, but private channels are inaccessible until a consent flow is implemented."
    )]
    pub async fn set_channel_add_policy(
        &self,
        Parameters(p): Parameters<SetChannelAddPolicyParams>,
    ) -> String {
        if !matches!(p.policy.as_str(), "anyone" | "owner_only" | "nobody") {
            return format!(
                "Error: invalid policy {:?} — must be 'anyone', 'owner_only', or 'nobody'",
                p.policy
            );
        }
        let body = serde_json::json!({ "channel_add_policy": p.policy });
        match self
            .client
            .put("/api/users/me/channel-add-policy", &body)
            .await
        {
            Ok(b) => b,
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Vote on a forum post or comment (kind:45002).
    #[tool(
        name = "vote_on_post",
        description = "Vote on a forum post or comment. Creates a kind:45002 event. \
                       Each vote is a separate event — vote deduplication is not yet enforced."
    )]
    pub async fn vote_on_post(&self, Parameters(p): Parameters<VoteOnPostParams>) -> String {
        if let Err(e) = validate_uuid(&p.channel_id) {
            return format!("Error: {e}");
        }
        if p.event_id.len() != 64 || !p.event_id.chars().all(|c| c.is_ascii_hexdigit()) {
            return format!(
                "Error: event_id must be a 64-character hex string (got {:?})",
                p.event_id
            );
        }
        let channel_uuid = match uuid::Uuid::parse_str(&p.channel_id) {
            Ok(u) => u,
            Err(_) => return format!("Error: invalid UUID: {}", p.channel_id),
        };
        let target_eid = match EventId::from_hex(&p.event_id) {
            Ok(id) => id,
            Err(e) => return format!("Error: invalid event_id: {e}"),
        };
        let direction = match p.direction {
            VoteDirection::Up => sprout_sdk::VoteDirection::Up,
            VoteDirection::Down => sprout_sdk::VoteDirection::Down,
        };
        let builder = match sprout_sdk::build_vote(channel_uuid, target_eid, direction) {
            Ok(b) => b,
            Err(e) => return format!("Error: {e}"),
        };
        let event = match self.client.sign_event(builder) {
            Ok(e) => e,
            Err(e) => return format!("Error: failed to sign vote event: {e}"),
        };
        match self.client.send_event(event).await {
            Ok(ok) => serde_json::json!({
                "event_id": ok.event_id,
                "accepted": ok.accepted,
                "message": ok.message,
            })
            .to_string(),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Permanently delete a Sprout channel.
    #[tool(
        name = "delete_channel",
        description = "Permanently delete a Sprout channel. You must be the channel owner. This action is irreversible."
    )]
    pub async fn delete_channel(&self, Parameters(p): Parameters<DeleteChannelParams>) -> String {
        if let Err(e) = validate_uuid(&p.channel_id) {
            return format!("Error: {e}");
        }
        let channel_uuid = match uuid::Uuid::parse_str(&p.channel_id) {
            Ok(u) => u,
            Err(_) => return format!("Error: invalid UUID: {}", p.channel_id),
        };
        let builder = match sprout_sdk::build_delete_channel(channel_uuid) {
            Ok(b) => b,
            Err(e) => return format!("Error: {e}"),
        };
        let event = match self.client.sign_event(builder) {
            Ok(e) => e,
            Err(e) => return format!("Error: failed to sign delete_channel event: {e}"),
        };
        match self.client.send_event(event).await {
            Ok(ok) => serde_json::json!({
                "event_id": ok.event_id,
                "accepted": ok.accepted,
                "message": ok.message,
            })
            .to_string(),
            Err(e) => format!("Error: {e}"),
        }
    }

    // ── Social tools ─────────────────────────────────────────────────────────

    /// Publish a kind:1 text note (global, no channel scope).
    #[tool(
        name = "publish_note",
        description = "Publish a short text note (kind:1) to the global feed. Optionally reply to another note by event ID."
    )]
    pub async fn publish_note(&self, Parameters(p): Parameters<PublishNoteParams>) -> String {
        let reply_id = match p.reply_to_event_id.as_deref() {
            Some(hex) => match EventId::from_hex(hex) {
                Ok(id) => Some(id),
                Err(e) => return format!("Error: invalid reply_to_event_id: {e}"),
            },
            None => None,
        };

        if p.content.len() > 64 * 1024 {
            return format!(
                "Error: content exceeds 64 KiB limit ({} bytes)",
                p.content.len()
            );
        }

        let builder = match sprout_sdk::build_note(&p.content, reply_id) {
            Ok(b) => b,
            Err(e) => return format!("Error: {e}"),
        };

        let event = match self.client.sign_event(builder) {
            Ok(e) => e,
            Err(e) => return format!("Error: failed to sign event: {e}"),
        };

        match self.client.send_event(event).await {
            Ok(ok) => serde_json::json!({
                "event_id": ok.event_id,
                "accepted": ok.accepted,
                "message": ok.message,
            })
            .to_string(),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Replace the authenticated user's contact list (kind:3).
    #[tool(
        name = "set_contact_list",
        description = "Set the authenticated user's contact/follow list (kind:3). Replaces the entire list. Call get_contact_list first for delta updates."
    )]
    pub async fn set_contact_list(
        &self,
        Parameters(p): Parameters<SetContactListParams>,
    ) -> String {
        let contacts: Vec<(&str, Option<&str>, Option<&str>)> = p
            .contacts
            .iter()
            .map(|c| {
                (
                    c.pubkey.as_str(),
                    c.relay_url.as_deref(),
                    c.petname.as_deref(),
                )
            })
            .collect();

        let builder = match sprout_sdk::build_contact_list(&contacts) {
            Ok(b) => b,
            Err(e) => return format!("Error: {e}"),
        };

        let event = match self.client.sign_event(builder) {
            Ok(e) => e,
            Err(e) => return format!("Error: failed to sign event: {e}"),
        };

        match self.client.send_event(event).await {
            Ok(ok) => serde_json::json!({
                "event_id": ok.event_id,
                "accepted": ok.accepted,
                "message": ok.message,
            })
            .to_string(),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Fetch a single event by event ID.
    #[tool(
        name = "get_event",
        description = "Fetch a single event by its 64-char hex event ID. For global events: kind:0 profiles and kind:3 contacts require UsersRead scope; kind:1 notes and kind:30023 articles require MessagesRead scope. For channel events: requires MessagesRead scope and channel membership. Unknown kinds return 404."
    )]
    pub async fn get_event(&self, Parameters(p): Parameters<GetEventParams>) -> String {
        if let Err(e) = validate_hex64(&p.event_id, "event_id") {
            return e;
        }
        let path = format!("/api/events/{}", p.event_id);
        match self.client.get(&path).await {
            Ok(body) => body,
            Err(e) => format!("Error: {e}"),
        }
    }

    /// List notes by a specific user.
    #[tool(
        name = "get_user_notes",
        description = "List kind:1 text notes by a specific user (by hex pubkey). Returns id, pubkey, created_at, and content per note (tags and sig omitted — use get_event for full events). Supports composite cursor pagination via `before` (Unix timestamp) and `before_id` (hex event ID)."
    )]
    pub async fn get_user_notes(&self, Parameters(p): Parameters<GetUserNotesParams>) -> String {
        if let Err(e) = validate_hex64(&p.pubkey, "pubkey") {
            return e;
        }
        if let Some(ref bid) = p.before_id {
            if let Err(e) = validate_hex64(bid, "before_id") {
                return e;
            }
        }
        if p.before_id.is_some() && p.before.is_none() {
            return "Error: before_id requires before".to_string();
        }
        let mut url = format!("/api/users/{}/notes", p.pubkey);
        let mut query_parts = vec![];
        if let Some(limit) = p.limit {
            query_parts.push(format!("limit={limit}"));
        }
        if let Some(before) = p.before {
            query_parts.push(format!("before={before}"));
        }
        if let Some(ref before_id) = p.before_id {
            query_parts.push(format!("before_id={before_id}"));
        }
        if !query_parts.is_empty() {
            url.push('?');
            url.push_str(&query_parts.join("&"));
        }
        match self.client.get(&url).await {
            Ok(body) => body,
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Get a user's contact/follow list.
    #[tool(
        name = "get_contact_list",
        description = "Get a user's contact/follow list (kind:3) by hex pubkey. Returns the latest replaceable event."
    )]
    pub async fn get_contact_list(
        &self,
        Parameters(p): Parameters<GetContactListParams>,
    ) -> String {
        if let Err(e) = validate_hex64(&p.pubkey, "pubkey") {
            return e;
        }
        let path = format!("/api/users/{}/contact-list", p.pubkey);
        match self.client.get(&path).await {
            Ok(body) => body,
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Upload a local file to the Sprout relay.
    #[tool(
        name = "upload_file",
        description = "Upload a local file (image or video) to the Sprout relay. \
Returns a BlobDescriptor with the URL, hash, dimensions, and other metadata. \
Supported types: JPEG, PNG, GIF, WebP, MP4. \
The returned URL can be included in messages, or use the file_paths parameter \
on send_message to upload and attach in one step."
    )]
    pub async fn upload_file(&self, Parameters(p): Parameters<UploadFileParams>) -> String {
        match crate::upload::upload_file(
            self.client.http_client(),
            self.client.keys(),
            &self.client.relay_http_url(),
            self.client.api_token(),
            self.client.server_domain().as_deref(),
            &p.file_path,
        )
        .await
        {
            Ok(desc) => {
                serde_json::to_string_pretty(&desc).unwrap_or_else(|e| format!("Error: {e}"))
            }
            Err(e) => format!("Error: {e}"),
        }
    }
}

#[tool_handler]
impl ServerHandler for SproutMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(rmcp::model::Implementation::new(
                "sprout-mcp",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "Sprout MCP server — interact with the Sprout relay. \
                 Send messages, read channel history, create channels, \
                 manage canvases, create and manage workflows, \
                 and read your personalized home feed."
                    .to_string(),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── percent_encode ────────────────────────────────────────────────────────

    #[test]
    fn percent_encode_empty_string() {
        assert_eq!(percent_encode(""), "");
    }

    #[test]
    fn percent_encode_already_safe_chars() {
        // Unreserved chars (RFC 3986) must pass through unchanged.
        let safe = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.~";
        assert_eq!(percent_encode(safe), safe);
    }

    #[test]
    fn percent_encode_space() {
        assert_eq!(percent_encode(" "), "%20");
    }

    #[test]
    fn percent_encode_special_chars() {
        assert_eq!(percent_encode("hello world"), "hello%20world");
        assert_eq!(percent_encode("a&b=c"), "a%26b%3Dc");
        assert_eq!(percent_encode("foo?bar"), "foo%3Fbar");
    }

    #[test]
    fn percent_encode_slash() {
        assert_eq!(percent_encode("/"), "%2F");
    }

    #[test]
    fn percent_encode_unicode_multibyte() {
        // "é" is 0xC3 0xA9 in UTF-8.
        assert_eq!(percent_encode("é"), "%C3%A9");
    }

    // ── validate_uuid ─────────────────────────────────────────────────────────

    #[test]
    fn validate_uuid_valid() {
        assert!(validate_uuid("550e8400-e29b-41d4-a716-446655440000").is_ok());
    }

    #[test]
    fn validate_uuid_valid_v4() {
        assert!(validate_uuid("f47ac10b-58cc-4372-a567-0e02b2c3d479").is_ok());
    }

    #[test]
    fn validate_uuid_invalid_string() {
        let result = validate_uuid("not-a-uuid");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid UUID"));
    }

    #[test]
    fn validate_uuid_empty_string() {
        let result = validate_uuid("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid UUID"));
    }

    #[test]
    fn validate_uuid_almost_valid() {
        // Missing one character in the last group.
        let result = validate_uuid("550e8400-e29b-41d4-a716-44665544000");
        assert!(result.is_err());
    }

    // ── MAX_CONTENT_BYTES ─────────────────────────────────────────────────────

    #[test]
    fn max_content_bytes_value() {
        assert_eq!(MAX_CONTENT_BYTES, 65_536);
    }

    // ── VoteDirection serde ───────────────────────────────────────────────────

    #[test]
    fn vote_direction_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&VoteDirection::Up).unwrap(), "\"up\"");
        assert_eq!(
            serde_json::to_string(&VoteDirection::Down).unwrap(),
            "\"down\""
        );
    }

    #[test]
    fn vote_direction_deserializes_lowercase() {
        assert!(matches!(
            serde_json::from_str::<VoteDirection>("\"up\"").unwrap(),
            VoteDirection::Up
        ));
        assert!(matches!(
            serde_json::from_str::<VoteDirection>("\"down\"").unwrap(),
            VoteDirection::Down
        ));
    }

    #[test]
    fn vote_direction_rejects_invalid() {
        assert!(serde_json::from_str::<VoteDirection>("\"sideways\"").is_err());
        assert!(serde_json::from_str::<VoteDirection>("\"UP\"").is_err());
        assert!(serde_json::from_str::<VoteDirection>("\"\"").is_err());
    }

    #[test]
    fn vote_on_post_params_round_trip() {
        let params = VoteOnPostParams {
            channel_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            event_id: "a".repeat(64),
            direction: VoteDirection::Up,
        };
        let json = serde_json::to_string(&params).unwrap();
        let parsed: VoteOnPostParams = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.channel_id, params.channel_id);
        assert_eq!(parsed.event_id, params.event_id);
        assert!(matches!(parsed.direction, VoteDirection::Up));
    }

    // ── extract_at_names ──────────────────────────────────────────────────────

    #[test]
    fn extract_at_names_matches() {
        // basic, start-of-string, dedup, newline, dots/hyphens/underscores
        assert_eq!(extract_at_names("Hello @Tyler"), vec!["tyler"]);
        assert_eq!(extract_at_names("@Tyler are you there?"), vec!["tyler"]);
        assert_eq!(
            extract_at_names("Hey @Alice and @alice, meet @Bob"),
            vec!["alice", "bob"]
        );
        assert_eq!(extract_at_names("first line\n@Tyler second"), vec!["tyler"]);
        assert_eq!(
            extract_at_names("@john.doe @mary_jane @bob-smith"),
            vec!["john.doe", "mary_jane", "bob-smith"]
        );
    }

    #[test]
    fn extract_at_names_rejects() {
        // empty, no @, email, bare @, @ at EOF
        assert!(extract_at_names("").is_empty());
        assert!(extract_at_names("no mentions").is_empty());
        assert!(extract_at_names("user@example.com").is_empty());
        assert!(extract_at_names("hello @ world").is_empty());
        assert!(extract_at_names("hello @").is_empty());
    }
}

#[cfg(test)]
mod diff_tests {
    use super::*;

    #[test]
    fn truncate_diff_small_passes_through() {
        let diff = "--- a/file\n+++ b/file\n@@ -1,3 +1,3 @@\n context\n-old\n+new\n";
        let (result, truncated) = truncate_diff(diff, 60 * 1024);
        assert_eq!(result, diff);
        assert!(!truncated);
    }

    #[test]
    fn truncate_diff_cuts_at_hunk_boundary() {
        // Build a diff large enough that truncation is meaningful.
        // Repeat the first hunk many times so the total is well above any
        // reasonable max_bytes, then append a second hunk we want excluded.
        let hunk_unit = "--- a/file\n+++ b/file\n@@ -1,3 +1,3 @@\n context\n-old\n+new\n";
        let mut diff = hunk_unit.repeat(20); // ~1140 bytes of first-hunk content
        diff.push_str("@@ -10,3 +10,3 @@\n more context\n-old2\n+new2\n");

        // max_bytes sits inside the repeated first-hunk region (well below total)
        // but above TRUNCATION_NOTICE.len() so effective_limit > 0.
        // effective_limit = max_bytes - TRUNCATION_NOTICE.len() ≈ 500 - 72 = 428,
        // which lands inside the repeated first-hunk block.
        let max_bytes = 500;
        let (result, truncated) = truncate_diff(&diff, max_bytes);
        assert!(truncated);
        assert!(
            result.contains("context"),
            "should contain first-hunk content"
        );
        assert!(result.contains("Diff truncated"));
        assert!(
            !result.contains("@@ -10,3"),
            "second hunk should be excluded"
        );
        // Result must not exceed max_bytes.
        assert!(
            result.len() <= max_bytes,
            "truncated result ({}) exceeds max_bytes ({})",
            result.len(),
            max_bytes
        );
    }

    #[test]
    fn truncate_diff_utf8_safe() {
        // Create a diff with multi-byte chars near the boundary
        let mut diff = String::from("--- a/file\n+++ b/file\n@@ -1,1 +1,1 @@\n-");
        // Add enough content to exceed a small limit, with multi-byte chars
        for _ in 0..100 {
            diff.push('日'); // 3-byte UTF-8 char
        }
        diff.push('\n');
        let (result, truncated) = truncate_diff(&diff, 80);
        assert!(truncated);
        // Must not panic and must produce valid UTF-8
        assert!(result.is_char_boundary(result.len()));
    }

    #[test]
    fn truncate_diff_result_within_limit() {
        let mut diff = String::new();
        for i in 0..2000 {
            diff.push_str(&format!(
                "@@ -{i},1 +{i},1 @@\n-old line {i}\n+new line {i}\n"
            ));
        }
        let max = 1024;
        let (result, truncated) = truncate_diff(&diff, max);
        assert!(truncated);
        assert!(
            result.len() <= max,
            "truncated result ({}) exceeds max_bytes ({})",
            result.len(),
            max
        );
    }

    #[test]
    fn infer_language_known_extensions() {
        assert_eq!(infer_language("src/main.rs"), Some("rust".to_string()));
        assert_eq!(infer_language("app.tsx"), Some("typescript".to_string()));
        assert_eq!(infer_language("script.py"), Some("python".to_string()));
        assert_eq!(infer_language("Makefile"), None);
    }

    #[test]
    fn infer_language_no_extension() {
        assert_eq!(infer_language("Dockerfile"), None);
        // But "foo.dockerfile" should match
        assert_eq!(
            infer_language("foo.dockerfile"),
            Some("dockerfile".to_string())
        );
    }
}
