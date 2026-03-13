//! Configuration for the sprout-acp harness.
//!
//! CLI-first: every option is a CLI flag with env var fallback.
//! Config file (TOML) for complex subscription rules.

use std::collections::HashMap;
use std::path::PathBuf;

use clap::Parser;
use nostr::Keys;
use thiserror::Error;
use uuid::Uuid;

use crate::filter::SubscriptionRule;

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to parse nostr keys: {0}")]
    KeyParse(#[from] nostr::key::Error),

    #[error("failed to read file: {0}")]
    Io(#[from] std::io::Error),

    #[error("config file error: {0}")]
    ConfigFile(String),
}

// ── Enums ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum SubscribeMode {
    Mentions,
    All,
    Config,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum DedupMode {
    Drop,
    Queue,
}

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Parser)]
#[command(
    name = "sprout-acp",
    about = "ACP harness that bridges Sprout events to AI agents"
)]
pub struct CliArgs {
    #[arg(long, env = "SPROUT_RELAY_URL", default_value = "ws://localhost:3000")]
    pub relay_url: String,

    #[arg(long, env = "SPROUT_PRIVATE_KEY")]
    pub private_key: String,

    #[arg(long, env = "SPROUT_API_TOKEN")]
    pub api_token: Option<String>,

    #[arg(long, env = "SPROUT_ACP_AGENT_COMMAND", default_value = "goose")]
    pub agent_command: String,

    #[arg(
        long,
        env = "SPROUT_ACP_AGENT_ARGS",
        default_value = "acp",
        value_delimiter = ','
    )]
    pub agent_args: Vec<String>,

    #[arg(
        long,
        env = "SPROUT_ACP_MCP_COMMAND",
        default_value = "sprout-mcp-server"
    )]
    pub mcp_command: String,

    #[arg(long, env = "SPROUT_ACP_TURN_TIMEOUT", default_value = "300")]
    pub turn_timeout: u64,

    #[arg(
        long,
        env = "SPROUT_ACP_SYSTEM_PROMPT",
        conflicts_with = "system_prompt_file"
    )]
    pub system_prompt: Option<String>,

    #[arg(
        long,
        env = "SPROUT_ACP_SYSTEM_PROMPT_FILE",
        conflicts_with = "system_prompt"
    )]
    pub system_prompt_file: Option<PathBuf>,

    #[arg(long, env = "SPROUT_ACP_INITIAL_MESSAGE")]
    pub initial_message: Option<String>,

    #[arg(
        long,
        env = "SPROUT_ACP_SUBSCRIBE",
        default_value = "mentions",
        value_enum
    )]
    pub subscribe: SubscribeMode,

    #[arg(long, env = "SPROUT_ACP_KINDS", value_delimiter = ',')]
    pub kinds: Option<Vec<u32>>,

    #[arg(long, env = "SPROUT_ACP_CHANNELS", value_delimiter = ',')]
    pub channels: Option<Vec<String>>,

    #[arg(long, env = "SPROUT_ACP_NO_MENTION_FILTER")]
    pub no_mention_filter: bool,

    #[arg(long, env = "SPROUT_ACP_CONFIG", default_value = "./sprout-acp.toml")]
    pub config: PathBuf,

    #[arg(long, env = "SPROUT_ACP_DEDUP", default_value = "drop", value_enum)]
    pub dedup: DedupMode,

    #[arg(long, env = "SPROUT_ACP_NO_IGNORE_SELF")]
    pub no_ignore_self: bool,
}

// ── Merged NIP-01 filter ──────────────────────────────────────────────────────

/// Merged NIP-01 subscription filter for a single channel.
#[derive(Debug, Clone)]
pub struct ChannelFilter {
    /// Event kinds to subscribe to. None = wildcard (all kinds).
    pub kinds: Option<Vec<u32>>,
    /// Whether to include `#p` tag filter for agent pubkey.
    pub require_mention: bool,
}

// ── Resolved config ───────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct Config {
    pub keys: Keys,
    pub api_token: Option<String>,
    pub relay_url: String,
    pub agent_command: String,
    pub agent_args: Vec<String>,
    pub mcp_command: String,
    pub turn_timeout_secs: u64,
    pub system_prompt: Option<String>,
    pub initial_message: Option<String>,
    pub subscribe_mode: SubscribeMode,
    pub dedup_mode: DedupMode,
    pub ignore_self: bool,
    pub kinds_override: Option<Vec<u32>>,
    pub channels_override: Option<Vec<String>>,
    pub no_mention_filter: bool,
    pub config_path: PathBuf,
}

impl Config {
    pub fn from_cli() -> Result<Self, ConfigError> {
        let args = CliArgs::parse();
        let keys = Keys::parse(&args.private_key)?;

        let system_prompt = if let Some(text) = args.system_prompt {
            Some(text)
        } else if let Some(ref path) = args.system_prompt_file {
            Some(std::fs::read_to_string(path)?)
        } else {
            None
        };

        if matches!(args.subscribe, SubscribeMode::Config) {
            if args.kinds.is_some() {
                tracing::warn!("--kinds is ignored in config mode");
            }
            if args.channels.is_some() {
                tracing::warn!("--channels is ignored in config mode");
            }
            if args.no_mention_filter {
                tracing::warn!("--no-mention-filter is ignored in config mode");
            }
        }

        Ok(Config {
            keys,
            api_token: args.api_token,
            relay_url: args.relay_url,
            agent_command: args.agent_command,
            agent_args: args.agent_args,
            mcp_command: args.mcp_command,
            turn_timeout_secs: args.turn_timeout,
            system_prompt,
            initial_message: args.initial_message,
            subscribe_mode: args.subscribe,
            dedup_mode: args.dedup,
            ignore_self: !args.no_ignore_self,
            kinds_override: args.kinds,
            channels_override: args.channels,
            no_mention_filter: args.no_mention_filter,
            config_path: args.config,
        })
    }

    /// Human-readable summary (no secrets).
    pub fn summary(&self) -> String {
        format!(
            "relay={} pubkey={} agent_cmd={} {} mcp_cmd={} timeout={}s subscribe={:?} dedup={:?} ignore_self={}",
            self.relay_url,
            self.keys.public_key().to_hex(),
            self.agent_command,
            self.agent_args.join(" "),
            self.mcp_command,
            self.turn_timeout_secs,
            self.subscribe_mode,
            self.dedup_mode,
            self.ignore_self,
        )
    }
}

// ── TOML config file ──────────────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
struct TomlConfig {
    #[serde(default)]
    rules: Vec<SubscriptionRule>,
}

pub fn load_rules(path: &std::path::Path) -> Result<Vec<SubscriptionRule>, ConfigError> {
    let content = std::fs::read_to_string(path)?;
    let config: TomlConfig =
        toml::from_str(&content).map_err(|e| ConfigError::ConfigFile(e.to_string()))?;

    if config.rules.len() > 100 {
        return Err(ConfigError::ConfigFile(format!(
            "too many rules ({}, max 100)",
            config.rules.len()
        )));
    }

    let mut seen_names = std::collections::HashSet::new();
    for rule in &config.rules {
        if rule.name.trim().is_empty() {
            return Err(ConfigError::ConfigFile(
                "rule name must not be empty".into(),
            ));
        }
        if !seen_names.insert(&rule.name) {
            return Err(ConfigError::ConfigFile(format!(
                "duplicate rule name: {}",
                rule.name
            )));
        }
        if let Some(ref expr) = rule.filter {
            if expr.len() > 4096 {
                return Err(ConfigError::ConfigFile(format!(
                    "rule '{}': filter too long ({} bytes, max 4096)",
                    rule.name,
                    expr.len()
                )));
            }
        }
    }

    Ok(config.rules)
}

// ── Subscription resolution ───────────────────────────────────────────────────

/// Resolve per-channel NIP-01 filters from config + discovered channels.
pub fn resolve_channel_filters(
    config: &Config,
    discovered_channels: &[Uuid],
    rules: &[SubscriptionRule],
) -> HashMap<Uuid, ChannelFilter> {
    use sprout_core::kind::{
        KIND_STREAM_MESSAGE, KIND_STREAM_REMINDER, KIND_WORKFLOW_APPROVAL_REQUESTED,
    };

    let target_channels: Vec<Uuid> = if let Some(ref overrides) = config.channels_override {
        overrides
            .iter()
            .filter_map(|s| s.parse::<Uuid>().ok())
            .filter(|id| discovered_channels.contains(id))
            .collect()
    } else {
        discovered_channels.to_vec()
    };

    let mut result = HashMap::new();

    match config.subscribe_mode {
        SubscribeMode::Mentions => {
            let kinds = config.kinds_override.clone().unwrap_or_else(|| {
                vec![
                    KIND_STREAM_MESSAGE,
                    KIND_WORKFLOW_APPROVAL_REQUESTED,
                    KIND_STREAM_REMINDER,
                ]
            });
            let require_mention = !config.no_mention_filter;
            for ch in &target_channels {
                result.insert(
                    *ch,
                    ChannelFilter {
                        kinds: Some(kinds.clone()),
                        require_mention,
                    },
                );
            }
        }
        SubscribeMode::All => {
            for ch in &target_channels {
                result.insert(
                    *ch,
                    ChannelFilter {
                        kinds: config.kinds_override.clone(),
                        require_mention: false,
                    },
                );
            }
        }
        SubscribeMode::Config => {
            for ch in discovered_channels {
                let mut merged_kinds: Option<Vec<u32>> = Some(vec![]);
                let mut require_mention = true;
                let mut has_rule = false;

                for rule in rules {
                    if !rule_applies_to_channel(rule, *ch) {
                        continue;
                    }
                    has_rule = true;
                    if rule.kinds.is_empty() {
                        merged_kinds = None;
                    } else if let Some(ref mut kinds) = merged_kinds {
                        for k in &rule.kinds {
                            if !kinds.contains(k) {
                                kinds.push(*k);
                            }
                        }
                    }
                    if !rule.require_mention {
                        require_mention = false;
                    }
                }

                if has_rule {
                    result.insert(
                        *ch,
                        ChannelFilter {
                            kinds: merged_kinds,
                            require_mention,
                        },
                    );
                }
            }
        }
    }

    result
}

fn rule_applies_to_channel(rule: &SubscriptionRule, channel_id: Uuid) -> bool {
    use crate::filter::ChannelScope;
    match &rule.channels {
        ChannelScope::All(s) if s == "all" => true,
        ChannelScope::List(ids) => ids
            .iter()
            .any(|id| id.parse::<Uuid>().ok() == Some(channel_id)),
        _ => false,
    }
}
