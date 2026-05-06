use std::time::Duration;

pub const PROTOCOL_VERSION: u32 = 1;

pub const MAX_PROMPT_BYTES: usize = 1024 * 1024;
pub const MAX_TOOL_RESULT_BYTES: usize = 256 * 1024;
pub const MAX_TOOL_CALLS_PER_TURN: usize = 64;

/// Trigger an internal handoff once history exceeds this fraction of
/// `max_history_bytes`. Hardcoded — leaves headroom for the summary call.
pub const HANDOFF_THRESHOLD: f64 = 0.75;

/// Output token budget for the handoff summary LLM call.
pub const HANDOFF_MAX_OUTPUT_TOKENS: u32 = 8192;

/// Number of trailing history items to include in the handoff prompt.
pub const HANDOFF_TAIL_ITEMS: usize = 5;

/// Cap on the original task text included in the handoff prompt. The full
/// original prompt may be up to MAX_PROMPT_BYTES (1MB); we only need enough
/// to remind the summarizer of the goal.
pub const HANDOFF_ORIGINAL_TASK_MAX_BYTES: usize = 16 * 1024;

/// Hard cap on the assembled handoff prompt sent to the summarizer. If the
/// prompt would exceed this, history snippets are dropped (oldest first) —
/// the original task is never truncated past HANDOFF_ORIGINAL_TASK_MAX_BYTES.
pub const HANDOFF_PROMPT_MAX_BYTES: usize = 32 * 1024;

/// Cap on the number of tool names listed in the handoff prompt. With a
/// sprawling MCP registry (100+ tools), listing them all is wasteful and
/// adds nothing the summarizer can act on.
pub const HANDOFF_MAX_TOOL_NAMES: usize = 20;

const DEFAULT_SYSTEM_PROMPT: &str =
    "You are sprout-agent. Use the provided tools to act. Tool calls are your only output.";

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
    pub mcp_init_timeout: Duration,
    pub max_line_bytes: usize,
    pub max_history_bytes: usize,
    pub max_handoffs: usize,
    pub api_key: String,
    pub model: String,
    pub base_url: String,
    pub anthropic_api_version: String,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let provider = match req("SPROUT_AGENT_PROVIDER")?.to_ascii_lowercase().as_str() {
            "anthropic" => Provider::Anthropic,
            "openai" | "openai-compat" => Provider::OpenAi,
            o => return Err(format!("config: SPROUT_AGENT_PROVIDER={o} not supported")),
        };
        let (api_key, model, base_url) = match provider {
            Provider::Anthropic => (
                req("ANTHROPIC_API_KEY")?,
                req("ANTHROPIC_MODEL")?,
                env_or("ANTHROPIC_BASE_URL", "https://api.anthropic.com"),
            ),
            Provider::OpenAi => (
                req("OPENAI_COMPAT_API_KEY")?,
                req("OPENAI_COMPAT_MODEL")?,
                env_or("OPENAI_COMPAT_BASE_URL", "https://api.openai.com/v1"),
            ),
        };
        let system_prompt = match (env("SPROUT_AGENT_SYSTEM_PROMPT"), env("SPROUT_AGENT_SYSTEM_PROMPT_FILE")) {
            (Some(_), Some(_)) => return Err(
                "config: SPROUT_AGENT_SYSTEM_PROMPT and SPROUT_AGENT_SYSTEM_PROMPT_FILE are mutually exclusive".into()),
            (Some(s), _) => s,
            (_, Some(p)) => std::fs::read_to_string(&p).map_err(|e| format!("config: read {p}: {e}"))?,
            _ => DEFAULT_SYSTEM_PROMPT.to_owned(),
        };
        let cfg = Config {
            provider,
            system_prompt,
            api_key,
            model,
            base_url,
            anthropic_api_version: env_or("ANTHROPIC_API_VERSION", "2023-06-01"),
            max_rounds: parse_env("SPROUT_AGENT_MAX_ROUNDS", 16)?,
            max_output_tokens: parse_env("SPROUT_AGENT_MAX_OUTPUT_TOKENS", 4096)?,
            llm_timeout: Duration::from_secs(parse_env("SPROUT_AGENT_LLM_TIMEOUT_SECS", 120)?),
            tool_timeout: Duration::from_secs(parse_env("SPROUT_AGENT_TOOL_TIMEOUT_SECS", 120)?),
            mcp_init_timeout: Duration::from_secs(parse_env(
                "SPROUT_AGENT_MCP_INIT_TIMEOUT_SECS",
                30,
            )?),
            max_line_bytes: parse_env("SPROUT_AGENT_MAX_LINE_BYTES", 4 * 1024 * 1024)?,
            max_history_bytes: parse_env("SPROUT_AGENT_MAX_HISTORY_BYTES", 1024 * 1024)?,
            max_handoffs: parse_env("SPROUT_AGENT_MAX_HANDOFFS", 5)?,
        };
        cfg.validate()?;
        Ok(cfg)
    }

    /// Reject zero/near-zero values that would render the agent useless or
    /// thrash the LLM. Floors are conservative — they catch obvious misuse
    /// (`MAX_ROUNDS=0`) without forbidding tight test configs.
    fn validate(&self) -> Result<(), String> {
        const MIN_HISTORY_BYTES: usize = 4096;
        const MIN_LINE_BYTES: usize = 1024;
        const MIN_TIMEOUT: Duration = Duration::from_secs(1);
        if self.max_rounds < 1 {
            return Err("config: SPROUT_AGENT_MAX_ROUNDS must be >= 1".into());
        }
        if self.max_output_tokens < 1 {
            return Err("config: SPROUT_AGENT_MAX_OUTPUT_TOKENS must be >= 1".into());
        }
        if self.max_history_bytes < MIN_HISTORY_BYTES {
            return Err(format!(
                "config: SPROUT_AGENT_MAX_HISTORY_BYTES must be >= {MIN_HISTORY_BYTES}"
            ));
        }
        if self.max_line_bytes < MIN_LINE_BYTES {
            return Err(format!(
                "config: SPROUT_AGENT_MAX_LINE_BYTES must be >= {MIN_LINE_BYTES}"
            ));
        }
        if self.llm_timeout < MIN_TIMEOUT {
            return Err("config: SPROUT_AGENT_LLM_TIMEOUT_SECS must be >= 1".into());
        }
        if self.tool_timeout < MIN_TIMEOUT {
            return Err("config: SPROUT_AGENT_TOOL_TIMEOUT_SECS must be >= 1".into());
        }
        if self.mcp_init_timeout < MIN_TIMEOUT {
            return Err("config: SPROUT_AGENT_MCP_INIT_TIMEOUT_SECS must be >= 1".into());
        }
        Ok(())
    }
}

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok()
}

fn env_or(k: &str, d: &str) -> String {
    env(k).unwrap_or_else(|| d.into())
}

fn req(k: &str) -> Result<String, String> {
    env(k).ok_or_else(|| format!("config: {k} required"))
}

fn parse_env<T: std::str::FromStr>(key: &str, default: T) -> Result<T, String>
where
    T::Err: std::fmt::Display,
{
    env(key)
        .map(|v| v.parse().map_err(|e| format!("config: {key}: {e}")))
        .unwrap_or(Ok(default))
}
