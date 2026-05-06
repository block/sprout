//! Core types: history, tool calls, config, errors.

use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

// ─── History ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum HistoryItem {
    User(String),
    Assistant {
        text: String,
        tool_calls: Vec<ToolCall>,
    },
    ToolResult(ToolResult),
}

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub provider_id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub provider_id: String,
    pub text: String,
    pub is_error: bool,
}

// ─── LLM ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct LlmResponse {
    /// Assistant text content. Empty when the model returned only tool calls.
    /// Preserved in history so the next turn doesn't serialize as
    /// `content: null`/`[]`, which is invalid for both providers.
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub stop: ProviderStop,
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
    pub description: String,
    pub input_schema: Value,
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

// ─── ACP DTOs (inbound params) ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionNewParams {
    #[serde(default)]
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

/// Text and ResourceLink survive; other blocks degrade to a marker so we
/// never silently drop input.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ResourceLink {
        uri: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCancelParams {
    pub session_id: String,
}

// ─── Errors ─────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum AgentError {
    InvalidParams(String),
    Llm(String),
    LlmAuth(String),
    Mcp(String),
    Io(std::io::Error),
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidParams(s) => write!(f, "invalid params: {s}"),
            Self::Llm(s) => write!(f, "llm: {s}"),
            Self::LlmAuth(s) => write!(f, "llm auth: {s}"),
            Self::Mcp(s) => write!(f, "mcp: {s}"),
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
            _ => -32000,
        }
    }
}

// ─── Config ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Provider {
    Anthropic,
    OpenAi,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub provider: Provider,
    pub system_prompt: String,
    pub max_rounds: u32,
    pub max_output_tokens: u32,
    pub llm_timeout: Duration,
    pub tool_timeout: Duration,
    pub max_line_bytes: usize,
    pub max_prompt_bytes: usize,
    pub max_tool_result_bytes: usize,
    pub max_history_bytes: usize,
    pub api_key: String,
    pub model: String,
    pub base_url: String,
    pub anthropic_api_version: String,
}

const DEFAULT_SYSTEM_PROMPT: &str =
    "You are sprout-agent. Use the provided tools to act. Tool calls are your only output.";

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let env = |k: &str| std::env::var(k).ok();
        let req = |k: &str| env(k).ok_or_else(|| format!("{k} required"));

        let provider = match env("ACP_SEED_PROVIDER")
            .ok_or("ACP_SEED_PROVIDER required (anthropic|openai)")?
            .to_ascii_lowercase()
            .as_str()
        {
            "anthropic" => Provider::Anthropic,
            "openai" | "openai-compat" => Provider::OpenAi,
            o => return Err(format!("ACP_SEED_PROVIDER={o} not supported")),
        };

        let (api_key, model, base_url) = match provider {
            Provider::Anthropic => (
                req("ANTHROPIC_API_KEY")?,
                req("ANTHROPIC_MODEL")?,
                env("ANTHROPIC_BASE_URL").unwrap_or_else(|| "https://api.anthropic.com".into()),
            ),
            Provider::OpenAi => (
                req("OPENAI_COMPAT_API_KEY")?,
                req("OPENAI_COMPAT_MODEL")?,
                env("OPENAI_COMPAT_BASE_URL").unwrap_or_else(|| "https://api.openai.com/v1".into()),
            ),
        };

        let system_prompt = match (
            env("ACP_SEED_SYSTEM_PROMPT"),
            env("ACP_SEED_SYSTEM_PROMPT_FILE"),
        ) {
            (Some(_), Some(_)) => {
                return Err(
                    "ACP_SEED_SYSTEM_PROMPT and ACP_SEED_SYSTEM_PROMPT_FILE are mutually exclusive"
                        .into(),
                )
            }
            (Some(s), _) => s,
            (_, Some(p)) => std::fs::read_to_string(&p).map_err(|e| format!("read {p}: {e}"))?,
            _ => DEFAULT_SYSTEM_PROMPT.to_owned(),
        };

        Ok(Config {
            provider,
            system_prompt,
            api_key,
            model,
            base_url,
            max_rounds: parse_env("ACP_SEED_MAX_ROUNDS", 16)?,
            max_output_tokens: parse_env("ACP_SEED_MAX_OUTPUT_TOKENS", 4096)?,
            llm_timeout: Duration::from_secs(parse_env("ACP_SEED_LLM_TIMEOUT_SECS", 120)?),
            tool_timeout: Duration::from_secs(parse_env("ACP_SEED_TOOL_TIMEOUT_SECS", 120)?),
            max_line_bytes: parse_env("ACP_SEED_MAX_LINE_BYTES", 4 * 1024 * 1024)?,
            max_prompt_bytes: parse_env("ACP_SEED_MAX_PROMPT_BYTES", 1024 * 1024)?,
            max_tool_result_bytes: parse_env("ACP_SEED_MAX_TOOL_RESULT_BYTES", 256 * 1024)?,
            max_history_bytes: parse_env("ACP_SEED_MAX_HISTORY_BYTES", 1024 * 1024)?,
            anthropic_api_version: env("ANTHROPIC_API_VERSION")
                .unwrap_or_else(|| "2023-06-01".into()),
        })
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

/// Reject empty strings as hard errors so we never paper over malformed input.
pub fn nonempty(s: String, field: &str) -> Result<String, AgentError> {
    if s.is_empty() {
        Err(AgentError::Llm(format!("{field} is empty")))
    } else {
        Ok(s)
    }
}

/// Truncate text to `max` bytes (utf-8 safe). Always returns `<= max` bytes.
/// Appends a marker when there's room; otherwise just truncates.
pub fn clamp(mut s: String, max: usize) -> String {
    if s.len() <= max {
        return s;
    }
    const MARKER: &str = "\n[truncated]";
    if max < MARKER.len() {
        // No room for the marker — truncate to max on a char boundary.
        let mut cut = max;
        while cut > 0 && !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
        return s;
    }
    let mut cut = max - MARKER.len();
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    s.truncate(cut);
    s.push_str(MARKER);
    s
}
