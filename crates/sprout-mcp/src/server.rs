use sprout_core::kind::{event_kind_u32, KIND_CANVAS};

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router, ServerHandler,
};
use serde::{Deserialize, Serialize};

use crate::relay_client::RelayClient;

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
    /// Nostr event kind. Defaults to 40001 (channel message).
    #[serde(default = "default_kind")]
    pub kind: Option<u16>,
}
fn default_kind() -> Option<u16> {
    Some(40001)
}

/// Parameters for the `get_channel_history` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GetChannelHistoryParams {
    /// UUID of the channel to fetch history from.
    pub channel_id: String,
    /// Maximum number of messages to return (default 50, max 200).
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Parameters for the `list_channels` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ListChannelsParams {
    /// Optional visibility filter: `"public"` or `"private"`.
    #[serde(default)]
    pub visibility: Option<String>,
}

/// Parameters for the `create_channel` tool.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateChannelParams {
    /// Display name for the new channel.
    pub name: String,
    /// Channel type identifier (e.g. `"text"`, `"voice"`).
    pub channel_type: String,
    /// Visibility of the channel: `"public"` or `"private"`.
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
    /// Full workflow definition in YAML format.
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
        description = "Send a message to a Sprout channel"
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

        let kind = p.kind.unwrap_or(40001);

        let e_tag = match nostr::Tag::parse(&["e", &p.channel_id]) {
            Ok(t) => t,
            Err(e) => return format!("Error building tag: {e}"),
        };

        let keys = self.client.keys().clone();
        let event = match nostr::EventBuilder::new(nostr::Kind::Custom(kind), &p.content, [e_tag])
            .sign_with_keys(&keys)
        {
            Ok(e) => e,
            Err(e) => return format!("Error signing event: {e}"),
        };

        match self.client.send_event(event).await {
            Ok(ok) if ok.accepted => format!("Message sent. Event ID: {}", ok.event_id),
            Ok(ok) => format!("Message rejected: {}", ok.message),
            Err(e) => format!("Relay error: {e}"),
        }
    }

    /// Get recent messages from a Sprout channel.
    #[tool(
        name = "get_channel_history",
        description = "Get recent messages from a Sprout channel"
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

        let filter = nostr::Filter::new()
            .custom_tag(
                nostr::SingleLetterTag::lowercase(nostr::Alphabet::E),
                [p.channel_id.as_str()],
            )
            .limit(limit as usize);

        let sub_id = format!("history-{}", uuid::Uuid::new_v4());
        let events = match self.client.subscribe(&sub_id, vec![filter]).await {
            Ok(e) => e,
            Err(e) => return format!("Subscribe error: {e}"),
        };
        let _ = self.client.close_subscription(&sub_id).await;

        let messages: Vec<serde_json::Value> = events
            .iter()
            .map(|event| {
                serde_json::json!({
                    "id": event.id.to_hex(),
                    "pubkey": event.pubkey.to_hex(),
                    "content": event.content,
                    "kind": event_kind_u32(event),
                    "created_at": event.created_at.as_u64(),
                })
            })
            .collect();

        serde_json::to_string_pretty(&messages).unwrap_or_default()
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
    #[tool(name = "create_channel", description = "Create a new Sprout channel")]
    pub async fn create_channel(&self, Parameters(p): Parameters<CreateChannelParams>) -> String {
        let keys = self.client.keys().clone();

        let metadata = serde_json::json!({
            "name": p.name,
            "channel_type": p.channel_type,
            "visibility": p.visibility,
            "description": p.description,
        });

        let event =
            match nostr::EventBuilder::new(nostr::Kind::Custom(40), metadata.to_string(), [])
                .sign_with_keys(&keys)
            {
                Ok(e) => e,
                Err(e) => return format!("Error signing event: {e}"),
            };

        match self.client.send_event(event).await {
            Ok(ok) if ok.accepted => format!("Channel created. Event ID: {}", ok.event_id),
            Ok(ok) => format!("Channel creation rejected: {}", ok.message),
            Err(e) => format!("Relay error: {e}"),
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

        let filter = nostr::Filter::new()
            .custom_tag(
                nostr::SingleLetterTag::lowercase(nostr::Alphabet::E),
                [p.channel_id.as_str()],
            )
            .kind(nostr::Kind::Custom(KIND_CANVAS as u16))
            .limit(1);

        let sub_id = format!("canvas-{}", uuid::Uuid::new_v4());
        let events = match self.client.subscribe(&sub_id, vec![filter]).await {
            Ok(e) => e,
            Err(e) => return format!("Error: {e}"),
        };
        let _ = self.client.close_subscription(&sub_id).await;

        if let Some(event) = events.last() {
            event.content.clone()
        } else {
            "No canvas set for this channel.".to_string()
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

        let keys = self.client.keys().clone();

        let e_tag = match nostr::Tag::parse(&["e", &p.channel_id]) {
            Ok(t) => t,
            Err(e) => return format!("Error building tag: {e}"),
        };

        let event = match nostr::EventBuilder::new(
            nostr::Kind::Custom(KIND_CANVAS as u16),
            &p.content,
            [e_tag],
        )
        .sign_with_keys(&keys)
        {
            Ok(e) => e,
            Err(e) => return format!("Error signing event: {e}"),
        };

        match self.client.send_event(event).await {
            Ok(ok) if ok.accepted => "Canvas updated.".to_string(),
            Ok(ok) => format!("Canvas update rejected: {}", ok.message),
            Err(e) => format!("Relay error: {e}"),
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
        description = "Create a new workflow in a channel from a YAML definition"
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
