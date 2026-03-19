use std::{path::PathBuf, process::Child};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaRecord {
    pub id: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub system_prompt: String,
    #[serde(default)]
    pub is_builtin: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayAgentInfo {
    pub pubkey: String,
    pub name: String,
    pub agent_type: String,
    pub channels: Vec<String>,
    pub capabilities: Vec<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedAgentRecord {
    pub pubkey: String,
    pub name: String,
    #[serde(default)]
    pub persona_id: Option<String>,
    pub private_key_nsec: String,
    pub api_token: Option<String>,
    pub relay_url: String,
    pub acp_command: String,
    pub agent_command: String,
    pub agent_args: Vec<String>,
    pub mcp_command: String,
    pub turn_timeout_seconds: u64,
    #[serde(default = "default_agent_parallelism")]
    pub parallelism: u32,
    pub system_prompt: Option<String>,
    /// Desired LLM model ID. Matches AgentModelInfo.id from discovery.
    /// The harness re-discovers the correct ACP switching metadata at session
    /// creation by matching this ID against the fresh session/new response.
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default = "default_start_on_app_launch")]
    pub start_on_app_launch: bool,
    #[serde(default)]
    pub runtime_pid: Option<u32>,
    pub created_at: String,
    pub updated_at: String,
    pub last_started_at: Option<String>,
    pub last_stopped_at: Option<String>,
    pub last_exit_code: Option<i32>,
    pub last_error: Option<String>,
}

#[derive(Debug)]
pub struct ManagedAgentProcess {
    pub child: Child,
    pub log_path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManagedAgentSummary {
    pub pubkey: String,
    pub name: String,
    pub persona_id: Option<String>,
    pub relay_url: String,
    pub acp_command: String,
    pub agent_command: String,
    pub agent_args: Vec<String>,
    pub mcp_command: String,
    pub turn_timeout_seconds: u64,
    pub parallelism: u32,
    pub system_prompt: Option<String>,
    pub model: Option<String>,
    pub has_api_token: bool,
    pub status: String,
    pub pid: Option<u32>,
    pub created_at: String,
    pub updated_at: String,
    pub last_started_at: Option<String>,
    pub last_stopped_at: Option<String>,
    pub last_exit_code: Option<i32>,
    pub last_error: Option<String>,
    pub start_on_app_launch: bool,
    pub log_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateManagedAgentRequest {
    pub name: String,
    #[serde(default)]
    pub persona_id: Option<String>,
    pub relay_url: Option<String>,
    pub acp_command: Option<String>,
    pub agent_command: Option<String>,
    #[serde(default)]
    pub agent_args: Vec<String>,
    pub mcp_command: Option<String>,
    pub turn_timeout_seconds: Option<u64>,
    pub parallelism: Option<u32>,
    pub system_prompt: Option<String>,
    pub avatar_url: Option<String>,
    pub model: Option<String>,
    #[serde(default)]
    pub mint_token: bool,
    #[serde(default)]
    pub token_scopes: Vec<String>,
    pub token_name: Option<String>,
    #[serde(default)]
    pub spawn_after_create: bool,
    #[serde(default = "default_start_on_app_launch")]
    pub start_on_app_launch: bool,
}

#[derive(Debug, Serialize)]
pub struct CreateManagedAgentResponse {
    pub agent: ManagedAgentSummary,
    pub private_key_nsec: String,
    pub api_token: Option<String>,
    pub profile_sync_error: Option<String>,
    pub spawn_error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePersonaRequest {
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub system_prompt: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePersonaRequest {
    pub id: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub system_prompt: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MintManagedAgentTokenRequest {
    pub pubkey: String,
    pub token_name: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct MintManagedAgentTokenResponse {
    pub agent: ManagedAgentSummary,
    pub token: String,
}

#[derive(Debug, Serialize)]
pub struct ManagedAgentLogResponse {
    pub content: String,
    pub log_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AcpProviderInfo {
    pub id: String,
    pub label: String,
    pub command: String,
    pub binary_path: String,
    pub default_args: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommandAvailabilityInfo {
    pub command: String,
    pub resolved_path: Option<String>,
    pub available: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverManagedAgentPrereqsRequest {
    pub acp_command: Option<String>,
    pub mcp_command: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManagedAgentPrereqsInfo {
    pub admin: CommandAvailabilityInfo,
    pub acp: CommandAvailabilityInfo,
    pub mcp: CommandAvailabilityInfo,
}

/// Patch request for updating a managed agent's mutable fields.
///
/// Tri-state nullable semantics via `Option<Option<T>>`:
/// - Field absent in JSON → `None` (don't touch)
/// - `"field": null` → `Some(None)` (clear to default)
/// - `"field": "value"` → `Some(Some("value"))` (set)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateManagedAgentRequest {
    pub pubkey: String,
    /// Absent = don't touch. null = clear to agent default. "id" = set.
    #[serde(default)]
    pub model: Option<Option<String>>,
    #[serde(default)]
    pub system_prompt: Option<Option<String>>,
}

/// Response from `get_agent_models` — normalized model info for the frontend.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentModelsResponse {
    pub agent_name: String,
    pub agent_version: String,
    /// Unified model list (merged from both ACP paths, deduplicated by ID).
    pub models: Vec<AgentModelInfo>,
    /// The agent's default model for a fresh session.
    pub agent_default_model: Option<String>,
    /// The user's persisted model selection (from ManagedAgentRecord.model).
    pub selected_model: Option<String>,
    /// Whether this agent supports model switching.
    pub supports_switching: bool,
}

/// A single model available from an agent.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentModelInfo {
    /// Canonical ID used for persistence and round-tripping.
    pub id: String,
    pub name: Option<String>,
    pub description: Option<String>,
}

pub const DEFAULT_ACP_COMMAND: &str = "sprout-acp";
pub const DEFAULT_AGENT_COMMAND: &str = "goose";
pub const DEFAULT_MCP_COMMAND: &str = "sprout-mcp-server";
pub const DEFAULT_AGENT_ARG: &str = "acp";
pub const DEFAULT_AGENT_TURN_TIMEOUT_SECONDS: u64 = 300;
pub const DEFAULT_AGENT_PARALLELISM: u32 = 1;

fn default_agent_parallelism() -> u32 {
    DEFAULT_AGENT_PARALLELISM
}

fn default_start_on_app_launch() -> bool {
    true
}
