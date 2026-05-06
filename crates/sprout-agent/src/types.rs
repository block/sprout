//! Typed core for sprout-agent.
//!
//! `HistoryItem` is the append-only conversation. It encodes the
//! tool_use→tool_result handshake as a typed invariant: every
//! `Assistant.tool_calls[i].provider_id` must reappear in a following
//! `ToolResult.provider_id`. The agent loop maintains this by construction.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ─── History ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum HistoryItem {
    User {
        text: String,
    },
    Assistant {
        text: String,
        tool_calls: Vec<ToolCall>,
    },
    ToolResult(ToolResult),
}

// ─── Tool calls & results ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ToolCall {
    /// Anthropic `id` or OpenAI `tool_calls[].id`. Pairing key.
    pub provider_id: String,
    /// Qualified name `{server}__{tool}`.
    pub name: String,
    /// Already parsed (OpenAI's string form is decoded before reaching here).
    pub arguments: Value,
}

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub provider_id: String,
    pub name: String,
    pub content: Vec<McpContent>,
    pub is_error: bool,
    /// True → ACP `tool_call_update.status = failed`. False → `completed`.
    pub infrastructure_failed: bool,
    /// Whether result was truncated to fit `max_tool_result_bytes`.
    /// Currently informational; future rounds may surface to the model.
    #[allow(dead_code)]
    pub truncated: bool,
}

impl ToolResult {
    pub fn synthetic(call: &ToolCall, msg: impl Into<String>, infra: bool) -> Self {
        Self {
            provider_id: call.provider_id.clone(),
            name: call.name.clone(),
            content: vec![McpContent::Text { text: msg.into() }],
            is_error: true,
            infrastructure_failed: infra,
            truncated: false,
        }
    }

    /// One-line summary for ACP error reporting.
    pub fn summary(&self) -> String {
        for c in &self.content {
            if let McpContent::Text { text } = c {
                return text.lines().next().unwrap_or("").to_owned();
            }
        }
        format!(
            "{} returned {} content blocks",
            self.name,
            self.content.len()
        )
    }
}

#[derive(Debug, Clone)]
pub enum McpContent {
    Text { text: String },
    Image { data: String, mime_type: String },
    Audio { data: String, mime_type: String },
    ResourceLink { uri: String },
    Other(Value),
}

// ─── LLM-side ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub stop_reason: ProviderStop,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProviderStop {
    EndTurn,
    ToolUse,
    MaxTokens,
    Refusal,
    Other,
}

#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
}

// ─── ACP DTOs (inbound) ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[allow(dead_code)] // schema-complete; we accept and ignore client capabilities
pub struct InitializeParams {
    #[serde(default)]
    pub protocol_version: Option<u32>,
    #[serde(default)]
    pub client_capabilities: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionNewParams {
    /// Required by ACP spec; we don't use it (MCP servers are launched with
    /// inherited environment, not a chdir).
    #[allow(dead_code)]
    pub cwd: String,
    #[serde(default)]
    pub mcp_servers: Vec<McpServerStdio>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct McpServerStdio {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: Vec<EnvVar>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct EnvVar {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPromptParams {
    pub session_id: String,
    pub prompt: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCancelParams {
    pub session_id: String,
}

// ─── Stop reasons (outbound) ────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StopReason {
    EndTurn,
    Cancelled,
    MaxTokens,
    MaxTurnRequests,
    Refusal,
}

impl StopReason {
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::EndTurn => "end_turn",
            Self::Cancelled => "cancelled",
            Self::MaxTokens => "max_tokens",
            Self::MaxTurnRequests => "max_turn_requests",
            Self::Refusal => "refusal",
        }
    }
}

// ─── Errors ─────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum AgentError {
    /// Authentication failure (401/403). Maps to JSON-RPC -32000.
    LlmAuth(String),
    /// Transport / 5xx after retry. Maps to JSON-RPC -32000.
    LlmHttp(String),
    /// Other LLM problem (parse, etc.). Maps to JSON-RPC -32000.
    Llm(String),
    /// MCP startup or call failure. Maps to JSON-RPC -32000 in session/new;
    /// inside the loop becomes a synthetic ToolResult.
    Mcp(String),
    /// Bad params. Maps to JSON-RPC -32602.
    InvalidParams(String),
    /// Internal logic bug. Maps to JSON-RPC -32603.
    Internal(String),
    /// I/O error talking to ACP client.
    Io(std::io::Error),
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LlmAuth(s) => write!(f, "llm authentication failed: {s}"),
            Self::LlmHttp(s) => write!(f, "llm transport: {s}"),
            Self::Llm(s) => write!(f, "llm: {s}"),
            Self::Mcp(s) => write!(f, "mcp: {s}"),
            Self::InvalidParams(s) => write!(f, "invalid params: {s}"),
            Self::Internal(s) => write!(f, "internal: {s}"),
            Self::Io(e) => write!(f, "io: {e}"),
        }
    }
}

impl std::error::Error for AgentError {}

impl From<std::io::Error> for AgentError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl AgentError {
    pub fn json_rpc_code(&self) -> i32 {
        match self {
            Self::InvalidParams(_) => -32602,
            Self::Internal(_) => -32603,
            _ => -32000,
        }
    }
}

// ─── Config ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct Config {
    pub provider: ProviderKind,
    pub system_prompt: String,
    pub max_rounds: u32,
    pub max_output_tokens: u32,
    pub llm_timeout: Duration,
    pub tool_timeout: Duration,
    pub max_line_bytes: usize,
    pub max_prompt_bytes: usize,
    pub max_tool_result_bytes: usize,

    // Anthropic
    pub anthropic_api_key: Option<String>,
    pub anthropic_model: Option<String>,
    pub anthropic_base_url: String,
    pub anthropic_api_version: String,

    // OpenAI-compat
    pub openai_api_key: Option<String>,
    pub openai_model: Option<String>,
    pub openai_base_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Anthropic,
    OpenAi,
}

const DEFAULT_SYSTEM_PROMPT: &str =
    "You are sprout-agent, an autonomous agent. Use the provided tools to act. \
     Tool calls are your only user-visible output.";

impl Config {
    pub fn from_env() -> Result<Self, String> {
        use std::env::var;

        let provider = match var("ACP_SEED_PROVIDER")
            .map_err(|_| "ACP_SEED_PROVIDER not set (anthropic|openai)".to_string())?
            .to_ascii_lowercase()
            .as_str()
        {
            "anthropic" => ProviderKind::Anthropic,
            "openai" | "openai-compat" => ProviderKind::OpenAi,
            other => return Err(format!("ACP_SEED_PROVIDER={other} not supported")),
        };

        let system_prompt = match (
            var("ACP_SEED_SYSTEM_PROMPT"),
            var("ACP_SEED_SYSTEM_PROMPT_FILE"),
        ) {
            (Ok(_), Ok(_)) => {
                return Err(
                    "ACP_SEED_SYSTEM_PROMPT and ACP_SEED_SYSTEM_PROMPT_FILE are mutually exclusive"
                        .into(),
                );
            }
            (Ok(s), _) => s,
            (_, Ok(p)) => std::fs::read_to_string(&p).map_err(|e| format!("read {p}: {e}"))?,
            _ => DEFAULT_SYSTEM_PROMPT.to_owned(),
        };

        Ok(Config {
            provider,
            system_prompt,
            max_rounds: parse_env("ACP_SEED_MAX_ROUNDS", 16)?,
            max_output_tokens: parse_env("ACP_SEED_MAX_OUTPUT_TOKENS", 4096)?,
            llm_timeout: Duration::from_secs(parse_env("ACP_SEED_LLM_TIMEOUT_SECS", 120)?),
            tool_timeout: Duration::from_secs(parse_env("ACP_SEED_TOOL_TIMEOUT_SECS", 120)?),
            max_line_bytes: parse_env("ACP_SEED_MAX_LINE_BYTES", 4 * 1024 * 1024)?,
            max_prompt_bytes: parse_env("ACP_SEED_MAX_PROMPT_BYTES", 1024 * 1024)?,
            max_tool_result_bytes: parse_env("ACP_SEED_MAX_TOOL_RESULT_BYTES", 256 * 1024)?,

            anthropic_api_key: var("ANTHROPIC_API_KEY").ok(),
            anthropic_model: var("ANTHROPIC_MODEL").ok(),
            anthropic_base_url: var("ANTHROPIC_BASE_URL")
                .unwrap_or_else(|_| "https://api.anthropic.com".into()),
            anthropic_api_version: var("ANTHROPIC_API_VERSION")
                .unwrap_or_else(|_| "2023-06-01".into()),

            openai_api_key: var("OPENAI_COMPAT_API_KEY").ok(),
            openai_model: var("OPENAI_COMPAT_MODEL").ok(),
            openai_base_url: var("OPENAI_COMPAT_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".into()),
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        match self.provider {
            ProviderKind::Anthropic => {
                if self.anthropic_api_key.as_deref().unwrap_or("").is_empty() {
                    return Err("ANTHROPIC_API_KEY required".into());
                }
                if self.anthropic_model.as_deref().unwrap_or("").is_empty() {
                    return Err("ANTHROPIC_MODEL required".into());
                }
            }
            ProviderKind::OpenAi => {
                if self.openai_api_key.as_deref().unwrap_or("").is_empty() {
                    return Err("OPENAI_COMPAT_API_KEY required".into());
                }
                if self.openai_model.as_deref().unwrap_or("").is_empty() {
                    return Err("OPENAI_COMPAT_MODEL required".into());
                }
            }
        }
        Ok(())
    }
}

fn parse_env<T: std::str::FromStr>(key: &str, default: T) -> Result<T, String>
where
    T::Err: std::fmt::Display,
{
    match std::env::var(key) {
        Ok(v) => v.parse().map_err(|e| format!("{key}: {e}")),
        Err(_) => Ok(default),
    }
}
