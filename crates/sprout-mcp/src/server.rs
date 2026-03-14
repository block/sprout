use nostr::{EventBuilder, Kind, Tag};
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

/// Validate that `s` is a well-formed UUID (any version/variant).
/// Returns `Ok(())` on success, or an error string on failure.
fn validate_uuid(s: &str) -> Result<(), String> {
    uuid::Uuid::parse_str(s).map_err(|_| format!("invalid UUID: {s}"))?;
    Ok(())
}

/// Maximum allowed content size for a single message (64 KiB).
const MAX_CONTENT_BYTES: usize = 65_536;

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
    /// Optional parent event ID. If provided, sends a reply via REST instead of WebSocket.
    #[serde(default)]
    pub parent_event_id: Option<String>,
}
fn default_kind() -> Option<u16> {
    Some(sprout_core::kind::KIND_STREAM_MESSAGE as u16)
}

/// Parameters for the `get_channel_history` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GetChannelHistoryParams {
    /// UUID of the channel to fetch history from.
    pub channel_id: String,
    /// Maximum number of messages to return (default 50, max 200).
    #[serde(default)]
    pub limit: Option<u32>,
    /// If true, fetch messages with thread metadata via REST instead of WebSocket.
    #[serde(default)]
    pub with_threads: Option<bool>,
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

/// Parameters for the `approve_workflow_step` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ApproveWorkflowStepParams {
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

// ── Thread tool parameter structs ─────────────────────────────────────────────

/// Parameters for the `send_reply` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SendReplyParams {
    /// UUID of the channel containing the parent message.
    pub channel_id: String,
    /// Event ID of the message being replied to.
    pub parent_event_id: String,
    /// Reply message body text.
    pub content: String,
    /// If true, the reply is also broadcast to the main channel timeline.
    #[serde(default)]
    pub broadcast_to_channel: Option<bool>,
}

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

/// Parameters for the `get_user_profile` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GetUserProfileParams {
    /// Hex-encoded pubkey to look up. Omit to get your own profile.
    #[serde(default)]
    pub pubkey: Option<String>,
}

/// Parameters for the `get_users_batch` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GetUsersBatchParams {
    /// List of hex-encoded pubkeys to look up (max 200).
    pub pubkeys: Vec<String>,
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

/// Parameters for the `get_feed_mentions` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GetFeedMentionsParams {
    /// Only return mentions newer than this Unix timestamp.
    /// Defaults to now - 7 days if omitted.
    #[serde(default)]
    pub since: Option<i64>,
    /// Maximum items to return. Default 50, max 50.
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Parameters for the `get_feed_actions` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GetFeedActionsParams {
    /// Only return action items newer than this Unix timestamp.
    /// Defaults to now - 7 days if omitted.
    #[serde(default)]
    pub since: Option<i64>,
    /// Maximum items to return. Default 50, max 50.
    #[serde(default)]
    pub limit: Option<u32>,
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

/// The MCP server that exposes Sprout relay functionality as tools.
#[derive(Clone)]
pub struct SproutMcpServer {
    client: RelayClient,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl SproutMcpServer {
    /// Create a new [`SproutMcpServer`] backed by the given relay client.
    pub fn new(client: RelayClient) -> Self {
        Self {
            client,
            tool_router: Self::tool_router(),
        }
    }

    /// Send a message to a Sprout channel.
    #[tool(
        name = "send_message",
        description = "Send a message to a Sprout channel. Optionally supply parent_event_id to send as a threaded reply via REST."
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

        // Use a user-signed WebSocket event for top-level messages so downstream
        // clients see the agent pubkey directly rather than the relay pubkey.
        // Threaded replies still go through REST because that path handles the
        // reply ancestry tags and DB bookkeeping for us.
        if p.parent_event_id.is_none() {
            let kind = Kind::from(
                p.kind
                    .unwrap_or(sprout_core::kind::KIND_STREAM_MESSAGE as u16),
            );
            let tags = vec![match Tag::parse(&["h", &p.channel_id]) {
                Ok(tag) => tag,
                Err(e) => return format!("Error: failed to build channel tag: {e}"),
            }];

            let event =
                match EventBuilder::new(kind, p.content, tags).sign_with_keys(self.client.keys()) {
                    Ok(event) => event,
                    Err(e) => return format!("Error: failed to sign message event: {e}"),
                };

            return match self.client.send_event(event).await {
                Ok(ok) => serde_json::json!({
                    "event_id": ok.event_id,
                    "accepted": ok.accepted,
                    "message": ok.message,
                })
                .to_string(),
                Err(e) => format!("Error: {e}"),
            };
        }

        let mut body = serde_json::json!({
            "content": p.content,
        });
        if let Some(ref parent_id) = p.parent_event_id {
            body["parent_event_id"] = serde_json::Value::String(parent_id.clone());
        }
        if let Some(kind) = p.kind {
            body["kind"] = serde_json::json!(kind);
        }
        match self
            .client
            .post(&format!("/api/channels/{}/messages", p.channel_id), &body)
            .await
        {
            Ok(b) => b,
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Send a code diff to a Sprout channel as kind:40008.
    #[tool(
        name = "send_diff_message",
        description = "Send a code diff to a Sprout channel with syntax highlighting and structured metadata. The diff is rendered with GitHub-quality visualization in the desktop client."
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

        // 1. Truncate diff at 60KB (UTF-8 safe)
        let (diff_content, truncated) = truncate_diff(&diff, 60 * 1024);

        // 2. Infer language from file extension if not provided
        let lang = language.or_else(|| file_path.as_deref().and_then(infer_language));

        // 3. Build NIP-31 alt tag
        let alt_text = match &description {
            Some(desc) => format!(
                "Diff: {} — {}",
                file_path.as_deref().unwrap_or("diff"),
                desc
            ),
            None => format!("Diff: {}", file_path.as_deref().unwrap_or("diff")),
        };

        // 4. Build JSON body for REST endpoint
        let mut body = serde_json::json!({
            "content": diff_content,
            "kind": 40008_u32,
            "broadcast_to_channel": false,
            "diff_repo_url": repo_url,
            "diff_commit_sha": commit_sha,
            "diff_alt": alt_text,
        });
        if let Some(ref parent) = parent_event_id {
            body["parent_event_id"] = serde_json::Value::String(parent.clone());
        }
        if let Some(ref file) = file_path {
            body["diff_file_path"] = serde_json::Value::String(file.clone());
        }
        if let Some(ref sha) = parent_commit_sha {
            body["diff_parent_commit_sha"] = serde_json::Value::String(sha.clone());
        }
        // Branch metadata — both source and target must be provided together
        match (&source_branch, &target_branch) {
            (Some(ref src), Some(ref tgt)) => {
                body["diff_source_branch"] = serde_json::Value::String(src.clone());
                body["diff_target_branch"] = serde_json::Value::String(tgt.clone());
            }
            (Some(_), None) | (None, Some(_)) => {
                // Warn caller that partial branch metadata is discarded
                tracing::warn!("send_diff_message: only one of source_branch/target_branch provided — both required, branch metadata omitted");
            }
            (None, None) => {} // Both absent — no branch tag
        }
        if let Some(pr) = pr_number {
            body["diff_pr_number"] = serde_json::json!(pr);
        }
        if let Some(ref l) = lang {
            body["diff_language"] = serde_json::Value::String(l.clone());
        }
        if let Some(ref desc) = description {
            body["diff_description"] = serde_json::Value::String(desc.clone());
        }
        if truncated {
            body["diff_truncated"] = serde_json::json!(true);
        }

        match self
            .client
            .post(&format!("/api/channels/{}/messages", channel_id), &body)
            .await
        {
            Ok(b) => b,
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Get recent messages from a Sprout channel.
    #[tool(
        name = "get_channel_history",
        description = "Get recent messages from a Sprout channel. Set with_threads=true to include thread metadata via REST."
    )]
    pub async fn get_channel_history(
        &self,
        Parameters(p): Parameters<GetChannelHistoryParams>,
    ) -> String {
        if let Err(e) = validate_uuid(&p.channel_id) {
            return format!("Error: {e}");
        }

        const MAX_HISTORY_LIMIT: u32 = 200;
        let limit = p.limit.unwrap_or(50).min(MAX_HISTORY_LIMIT);

        // Use the REST endpoint so callers get the canonical history payload,
        // including thread metadata when requested.
        let with_threads = p.with_threads.unwrap_or(false);
        let path = if with_threads {
            format!(
                "/api/channels/{}/messages?with_threads=true&limit={}",
                p.channel_id, limit
            )
        } else {
            format!("/api/channels/{}/messages?limit={}", p.channel_id, limit)
        };
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
        let body = serde_json::json!({
            "name": p.name,
            "channel_type": p.channel_type,
            "visibility": p.visibility,
            "description": p.description,
        });
        match self.client.post("/api/channels", &body).await {
            Ok(b) => b,
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
        match self.client.set_canvas(&p.channel_id, &p.content).await {
            Ok(body) => {
                // Parse REST JSON; return a clean confirmation string.
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
                    if v.get("ok").and_then(|o| o.as_bool()) == Some(true) {
                        return "Canvas updated.".to_string();
                    }
                    if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
                        return format!("Error: {err}");
                    }
                }
                body
            }
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
        name = "approve_workflow_step",
        description = "Approve or deny a pending workflow approval step"
    )]
    pub async fn approve_workflow_step(
        &self,
        Parameters(p): Parameters<ApproveWorkflowStepParams>,
    ) -> String {
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

    /// Get only @mentions for this agent from the Sprout relay.
    #[tool(
        name = "get_feed_mentions",
        description = "Get only @mentions for this agent from the Sprout relay. \
                       Returns events where the agent's pubkey appears in a p-tag. \
                       Equivalent to the @Mentions tab on the Home feed."
    )]
    pub async fn get_feed_mentions(
        &self,
        Parameters(p): Parameters<GetFeedMentionsParams>,
    ) -> String {
        const MAX_FEED_LIMIT: u32 = 50;
        let mut url = format!("{}/api/feed?types=mentions", self.client.relay_http_url());
        if let Some(since) = p.since {
            url = format!("{url}&since={since}");
        }
        if let Some(limit) = p.limit {
            url = format!("{url}&limit={}", limit.min(MAX_FEED_LIMIT));
        }
        match self.client.get_api(&url).await {
            Ok(body) => body,
            Err(e) => format!("Error fetching mentions: {e}"),
        }
    }

    /// Get items that require action from this agent.
    #[tool(
        name = "get_feed_actions",
        description = "Get items that require action from this agent: approval requests (kind 46010) \
                       and reminders (kind 40007) addressed to the agent's pubkey. \
                       Equivalent to the 'Needs Action' section on the Home feed."
    )]
    pub async fn get_feed_actions(
        &self,
        Parameters(p): Parameters<GetFeedActionsParams>,
    ) -> String {
        const MAX_FEED_LIMIT: u32 = 50;
        let mut url = format!(
            "{}/api/feed?types=needs_action",
            self.client.relay_http_url()
        );
        if let Some(since) = p.since {
            url = format!("{url}&since={since}");
        }
        if let Some(limit) = p.limit {
            url = format!("{url}&limit={}", limit.min(MAX_FEED_LIMIT));
        }
        match self.client.get_api(&url).await {
            Ok(body) => body,
            Err(e) => format!("Error fetching action items: {e}"),
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
        let body = serde_json::json!({
            "pubkeys": [p.pubkey],
            "role": p.role.unwrap_or_else(|| "member".to_string()),
        });
        match self
            .client
            .post(&format!("/api/channels/{}/members", p.channel_id), &body)
            .await
        {
            Ok(b) => b,
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
        let encoded_pubkey = percent_encode(&p.pubkey);
        match self
            .client
            .delete(&format!(
                "/api/channels/{}/members/{}",
                p.channel_id, encoded_pubkey
            ))
            .await
        {
            Ok(_) => "Member removed.".to_string(),
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
        let body = serde_json::json!({});
        match self
            .client
            .post(&format!("/api/channels/{}/join", p.channel_id), &body)
            .await
        {
            Ok(b) => b,
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
        let body = serde_json::json!({});
        match self
            .client
            .post(&format!("/api/channels/{}/leave", p.channel_id), &body)
            .await
        {
            Ok(b) => b,
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
        let body = serde_json::json!({
            "name": p.name,
            "description": p.description,
        });
        match self
            .client
            .put(&format!("/api/channels/{}", p.channel_id), &body)
            .await
        {
            Ok(b) => b,
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
        let body = serde_json::json!({ "topic": p.topic });
        match self
            .client
            .put(&format!("/api/channels/{}/topic", p.channel_id), &body)
            .await
        {
            Ok(b) => b,
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
        let body = serde_json::json!({ "purpose": p.purpose });
        match self
            .client
            .put(&format!("/api/channels/{}/purpose", p.channel_id), &body)
            .await
        {
            Ok(b) => b,
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
        let body = serde_json::json!({});
        match self
            .client
            .post(&format!("/api/channels/{}/archive", p.channel_id), &body)
            .await
        {
            Ok(b) => b,
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
        let body = serde_json::json!({});
        match self
            .client
            .post(&format!("/api/channels/{}/unarchive", p.channel_id), &body)
            .await
        {
            Ok(b) => b,
            Err(e) => format!("Error: {e}"),
        }
    }

    // ── Thread tools ──────────────────────────────────────────────────────────

    /// Send a reply to a message in a thread.
    #[tool(
        name = "send_reply",
        description = "Send a reply to a message in a Sprout channel thread. \
                       Optionally set broadcast_to_channel=true to also surface the reply in the main channel timeline."
    )]
    pub async fn send_reply(&self, Parameters(p): Parameters<SendReplyParams>) -> String {
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

        let body = serde_json::json!({
            "content": p.content,
            "parent_event_id": p.parent_event_id,
            "broadcast_to_channel": p.broadcast_to_channel.unwrap_or(false),
        });
        match self
            .client
            .post(&format!("/api/channels/{}/messages", p.channel_id), &body)
            .await
        {
            Ok(b) => b,
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Get a message thread (replies to a message).
    #[tool(
        name = "get_thread",
        description = "Get a message thread from a Sprout channel. Returns the root message and all nested replies."
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

    // ── Reaction tools ────────────────────────────────────────────────────────

    /// Add an emoji reaction to a message.
    #[tool(
        name = "add_reaction",
        description = "Add an emoji reaction to a Sprout message."
    )]
    pub async fn add_reaction(&self, Parameters(p): Parameters<AddReactionParams>) -> String {
        let body = serde_json::json!({ "emoji": p.emoji });
        let encoded_event_id = percent_encode(&p.event_id);
        match self
            .client
            .post(
                &format!("/api/messages/{}/reactions", encoded_event_id),
                &body,
            )
            .await
        {
            Ok(b) => b,
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Remove an emoji reaction from a message.
    #[tool(
        name = "remove_reaction",
        description = "Remove an emoji reaction from a Sprout message."
    )]
    pub async fn remove_reaction(&self, Parameters(p): Parameters<RemoveReactionParams>) -> String {
        let encoded_event_id = percent_encode(&p.event_id);
        let encoded_emoji = percent_encode(&p.emoji);
        match self
            .client
            .delete(&format!(
                "/api/messages/{}/reactions/{}",
                encoded_event_id, encoded_emoji
            ))
            .await
        {
            Ok(_) => "Reaction removed.".to_string(),
            Err(e) => format!("Error: {e}"),
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
        let body = serde_json::json!({
            "display_name": p.display_name,
            "avatar_url": p.avatar_url,
            "about": p.about,
            "nip05_handle": p.nip05_handle,
        });
        match self.client.put("/api/users/me/profile", &body).await {
            Ok(b) => b,
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Read a user's profile by pubkey.
    #[tool(
        name = "get_user_profile",
        description = "Get a user's profile by pubkey. Omit pubkey to get your own profile. Returns display name, avatar URL, about text, and NIP-05 handle."
    )]
    pub async fn get_user_profile(
        &self,
        Parameters(p): Parameters<GetUserProfileParams>,
    ) -> String {
        let path = match p.pubkey {
            None => "/api/users/me/profile".to_string(),
            Some(pk) => format!("/api/users/{}/profile", pk),
        };
        match self.client.get(&path).await {
            Ok(body) => body,
            Err(e) => format!("Error fetching profile: {e}"),
        }
    }

    /// Resolve display names for multiple pubkeys.
    #[tool(
        name = "get_users_batch",
        description = "Resolve display names and NIP-05 handles for multiple pubkeys at once. Returns a map of pubkey to profile info, plus a list of unknown pubkeys. Useful for identifying message senders in bulk."
    )]
    pub async fn get_users_batch(&self, Parameters(p): Parameters<GetUsersBatchParams>) -> String {
        let body = serde_json::json!({ "pubkeys": p.pubkeys });
        match self.client.post("/api/users/batch", &body).await {
            Ok(resp) => resp,
            Err(e) => format!("Error fetching profiles: {e}"),
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
