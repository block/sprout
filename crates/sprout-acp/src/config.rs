use nostr::Keys;
use thiserror::Error;

/// Errors that can occur when loading configuration from environment variables.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("required environment variable {0} is not set")]
    MissingVar(&'static str),

    #[error("failed to parse nostr keys from {var}: {source}")]
    KeyParse {
        var: &'static str,
        #[source]
        source: nostr::key::Error,
    },
}

/// Configuration for the sprout-acp harness.
#[derive(Debug)]
pub struct Config {
    // --- Harness identity (relay WebSocket connection) ---
    /// Harness's nostr keypair, parsed from `SPROUT_ACP_PRIVATE_KEY`.
    pub harness_keys: Keys,
    /// Harness's API token (`SPROUT_ACP_API_TOKEN`), optional.
    pub harness_api_token: Option<String>,
    /// Relay WebSocket URL (`SPROUT_RELAY_URL`). Default: `ws://localhost:3000`.
    pub relay_url: String,

    // --- Agent identity (passed to MCP server via env vars) ---
    /// Agent's nostr keypair, parsed from `SPROUT_AGENT_PRIVATE_KEY`.
    pub agent_keys: Keys,
    /// Agent's API token (`SPROUT_AGENT_API_TOKEN`), optional.
    pub agent_api_token: Option<String>,

    // --- Agent binary ---
    /// Agent command (`SPROUT_ACP_AGENT_COMMAND`). Default: `goose`.
    pub agent_command: String,
    /// Agent arguments (`SPROUT_ACP_AGENT_ARGS`, comma-separated). Default: `["acp"]`.
    pub agent_args: Vec<String>,

    // --- MCP server ---
    /// MCP server binary path (`SPROUT_ACP_MCP_COMMAND`). Default: `sprout-mcp-server`.
    pub mcp_command: String,

    // --- Tuning ---
    /// Maximum turn duration in seconds (`SPROUT_ACP_TURN_TIMEOUT`). Default: 300.
    pub turn_timeout_secs: u64,
}

/// Parse a nostr `Keys` from the named environment variable.
fn parse_keys_var(var: &'static str) -> Result<Keys, ConfigError> {
    let nsec = std::env::var(var).map_err(|_| ConfigError::MissingVar(var))?;
    Keys::parse(&nsec).map_err(|e| ConfigError::KeyParse { var, source: e })
}

impl Config {
    /// Load configuration from environment variables.
    pub fn from_env() -> Result<Self, ConfigError> {
        let harness_keys = parse_keys_var("SPROUT_ACP_PRIVATE_KEY")?;

        let harness_api_token = std::env::var("SPROUT_ACP_API_TOKEN").ok();

        let relay_url = std::env::var("SPROUT_RELAY_URL")
            .unwrap_or_else(|_| "ws://localhost:3000".to_string());

        let agent_keys = parse_keys_var("SPROUT_AGENT_PRIVATE_KEY")?;

        let agent_api_token = std::env::var("SPROUT_AGENT_API_TOKEN").ok();

        let agent_command = std::env::var("SPROUT_ACP_AGENT_COMMAND")
            .unwrap_or_else(|_| "goose".to_string());

        let agent_args = std::env::var("SPROUT_ACP_AGENT_ARGS")
            .map(|s| s.split(',').map(|a| a.trim().to_string()).collect())
            .unwrap_or_else(|_| vec!["acp".to_string()]);

        let mcp_command = std::env::var("SPROUT_ACP_MCP_COMMAND")
            .unwrap_or_else(|_| "sprout-mcp-server".to_string());

        let turn_timeout_secs = std::env::var("SPROUT_ACP_TURN_TIMEOUT")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(300);

        Ok(Config {
            harness_keys,
            harness_api_token,
            relay_url,
            agent_keys,
            agent_api_token,
            agent_command,
            agent_args,
            mcp_command,
            turn_timeout_secs,
        })
    }

    /// Return a human-readable summary (no secrets).
    pub fn summary(&self) -> String {
        format!(
            "relay={} harness_pubkey={} agent_pubkey={} agent_cmd={} {} mcp_cmd={} turn_timeout={}s",
            self.relay_url,
            self.harness_keys.public_key().to_hex(),
            self.agent_keys.public_key().to_hex(),
            self.agent_command,
            self.agent_args.join(" "),
            self.mcp_command,
            self.turn_timeout_secs,
        )
    }
}
