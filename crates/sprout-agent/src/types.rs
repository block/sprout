use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone)]
pub enum HistoryItem {
    User(String),
    Assistant { text: String, tool_calls: Vec<ToolCall> },
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

#[derive(Debug, Clone)]
pub struct LlmResponse {
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

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
    ResourceLink { uri: String },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCancelParams {
    pub session_id: String,
}

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
    fn from(e: std::io::Error) -> Self { Self::Io(e) }
}
impl AgentError {
    pub fn json_rpc_code(&self) -> i32 {
        match self {
            Self::InvalidParams(_) => -32602,
            _ => -32000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Provider {
    Anthropic,
    OpenAi,
}

pub const MAX_PROMPT_BYTES: usize = 1024 * 1024;
pub const MAX_TOOL_RESULT_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone)]
pub struct Config {
    pub provider: Provider,
    pub system_prompt: String,
    pub max_rounds: u32,
    pub max_output_tokens: u32,
    pub llm_timeout: Duration,
    pub tool_timeout: Duration,
    pub max_line_bytes: usize,
    pub max_history_bytes: usize,
    pub api_key: String,
    pub model: String,
    pub base_url: String,
    pub anthropic_api_version: String,
}

const DEFAULT_SYSTEM_PROMPT: &str =
    "You are sprout-agent. Use the provided tools to act. Tool calls are your only output.";

fn env(k: &str) -> Option<String> { std::env::var(k).ok() }
fn env_or(k: &str, d: &str) -> String { env(k).unwrap_or_else(|| d.into()) }
fn req(k: &str) -> Result<String, String> { env(k).ok_or_else(|| format!("{k} required")) }
fn parse_env<T: std::str::FromStr>(key: &str, default: T) -> Result<T, String>
where T::Err: std::fmt::Display
{
    env(key).map(|v| v.parse().map_err(|e| format!("{key}: {e}"))).unwrap_or(Ok(default))
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let provider = match env("ACP_SEED_PROVIDER")
            .ok_or("ACP_SEED_PROVIDER required (anthropic|openai)")?
            .to_ascii_lowercase().as_str()
        {
            "anthropic" => Provider::Anthropic,
            "openai" | "openai-compat" => Provider::OpenAi,
            o => return Err(format!("ACP_SEED_PROVIDER={o} not supported")),
        };
        let (api_key, model, base_url) = match provider {
            Provider::Anthropic => (req("ANTHROPIC_API_KEY")?, req("ANTHROPIC_MODEL")?,
                env_or("ANTHROPIC_BASE_URL", "https://api.anthropic.com")),
            Provider::OpenAi => (req("OPENAI_COMPAT_API_KEY")?, req("OPENAI_COMPAT_MODEL")?,
                env_or("OPENAI_COMPAT_BASE_URL", "https://api.openai.com/v1")),
        };
        let system_prompt = match (env("ACP_SEED_SYSTEM_PROMPT"), env("ACP_SEED_SYSTEM_PROMPT_FILE")) {
            (Some(_), Some(_)) => return Err(
                "ACP_SEED_SYSTEM_PROMPT and ACP_SEED_SYSTEM_PROMPT_FILE are mutually exclusive".into()),
            (Some(s), _) => s,
            (_, Some(p)) => std::fs::read_to_string(&p).map_err(|e| format!("read {p}: {e}"))?,
            _ => DEFAULT_SYSTEM_PROMPT.to_owned(),
        };
        Ok(Config {
            provider, system_prompt, api_key, model, base_url,
            max_rounds: parse_env("ACP_SEED_MAX_ROUNDS", 16)?,
            max_output_tokens: parse_env("ACP_SEED_MAX_OUTPUT_TOKENS", 4096)?,
            llm_timeout: Duration::from_secs(parse_env("ACP_SEED_LLM_TIMEOUT_SECS", 120)?),
            tool_timeout: Duration::from_secs(parse_env("ACP_SEED_TOOL_TIMEOUT_SECS", 120)?),
            max_line_bytes: parse_env("ACP_SEED_MAX_LINE_BYTES", 4 * 1024 * 1024)?,
            max_history_bytes: parse_env("ACP_SEED_MAX_HISTORY_BYTES", 1024 * 1024)?,
            anthropic_api_version: env_or("ANTHROPIC_API_VERSION", "2023-06-01"),
        })
    }
}

pub fn clamp(mut s: String, max: usize) -> String {
    if s.len() <= max { return s; }
    const MARKER: &str = "\n[truncated]";
    let budget = max.saturating_sub(MARKER.len());
    let mut cut = budget;
    while cut > 0 && !s.is_char_boundary(cut) { cut -= 1; }
    s.truncate(cut);
    if max >= MARKER.len() { s.push_str(MARKER); }
    s
}
