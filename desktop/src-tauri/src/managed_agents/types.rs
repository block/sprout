use std::{path::PathBuf, process::Child};

use serde::{Deserialize, Serialize};

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
    pub private_key_nsec: String,
    pub api_token: Option<String>,
    pub relay_url: String,
    pub acp_command: String,
    pub agent_command: String,
    pub agent_args: Vec<String>,
    pub mcp_command: String,
    pub turn_timeout_seconds: u64,
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
    pub relay_url: String,
    pub acp_command: String,
    pub agent_command: String,
    pub agent_args: Vec<String>,
    pub mcp_command: String,
    pub turn_timeout_seconds: u64,
    pub has_api_token: bool,
    pub status: String,
    pub pid: Option<u32>,
    pub created_at: String,
    pub updated_at: String,
    pub last_started_at: Option<String>,
    pub last_stopped_at: Option<String>,
    pub last_exit_code: Option<i32>,
    pub last_error: Option<String>,
    pub log_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateManagedAgentRequest {
    pub name: String,
    pub relay_url: Option<String>,
    pub acp_command: Option<String>,
    pub agent_command: Option<String>,
    #[serde(default)]
    pub agent_args: Vec<String>,
    pub mcp_command: Option<String>,
    pub turn_timeout_seconds: Option<u64>,
    #[serde(default)]
    pub mint_token: bool,
    #[serde(default)]
    pub token_scopes: Vec<String>,
    pub token_name: Option<String>,
    #[serde(default)]
    pub spawn_after_create: bool,
}

#[derive(Debug, Serialize)]
pub struct CreateManagedAgentResponse {
    pub agent: ManagedAgentSummary,
    pub private_key_nsec: String,
    pub api_token: Option<String>,
    pub spawn_error: Option<String>,
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

#[derive(Debug, Deserialize)]
pub struct SproutAdminMintTokenJsonOutput {
    pub token_id: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub pubkey: String,
    pub private_key_nsec: Option<String>,
    pub api_token: String,
}

pub const DEFAULT_ACP_COMMAND: &str = "sprout-acp";
pub const DEFAULT_AGENT_COMMAND: &str = "goose";
pub const DEFAULT_MCP_COMMAND: &str = "sprout-mcp-server";
pub const DEFAULT_AGENT_ARG: &str = "acp";
pub const DEFAULT_AGENT_TURN_TIMEOUT_SECONDS: u64 = 300;
