use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    sync::Mutex,
};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::Utc;
use nostr::{EventBuilder, JsonUtil, Keys, Kind, Tag, ToBech32};
use reqwest::Method;
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use sprout_core::PresenceStatus;
use tauri::{AppHandle, Manager};
use tauri_plugin_window_state::StateFlags;

pub struct AppState {
    pub keys: Mutex<Keys>,
    pub http_client: reqwest::Client,
    pub configured_api_token: Option<String>,
    pub session_token: Mutex<Option<String>>,
    pub managed_agents_store_lock: Mutex<()>,
    pub managed_agent_processes: Mutex<HashMap<String, ManagedAgentProcess>>,
}

#[derive(Serialize)]
pub struct IdentityInfo {
    pub pubkey: String,
    pub display_name: String,
}

#[derive(Serialize, Deserialize)]
pub struct ProfileInfo {
    pub pubkey: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub about: Option<String>,
    pub nip05_handle: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct UserProfileSummaryInfo {
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub nip05_handle: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct UsersBatchResponse {
    pub profiles: HashMap<String, UserProfileSummaryInfo>,
    pub missing: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct SetPresenceResponse {
    pub status: PresenceStatus,
    pub ttl_seconds: u64,
}

#[derive(Serialize, Deserialize)]
pub struct ChannelInfo {
    pub id: String,
    pub name: String,
    pub channel_type: String,
    pub visibility: String,
    #[serde(deserialize_with = "deserialize_null_string_as_empty")]
    pub description: String,
    pub topic: Option<String>,
    pub purpose: Option<String>,
    pub member_count: i64,
    pub last_message_at: Option<String>,
    pub archived_at: Option<String>,
    pub participants: Vec<String>,
    pub participant_pubkeys: Vec<String>,
    #[serde(default = "default_true")]
    pub is_member: bool,
}

#[derive(Serialize, Deserialize)]
pub struct ChannelDetailInfo {
    pub id: String,
    pub name: String,
    pub channel_type: String,
    pub visibility: String,
    #[serde(deserialize_with = "deserialize_null_string_as_empty")]
    pub description: String,
    pub topic: Option<String>,
    pub topic_set_by: Option<String>,
    pub topic_set_at: Option<String>,
    pub purpose: Option<String>,
    pub purpose_set_by: Option<String>,
    pub purpose_set_at: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
    pub member_count: i64,
    pub topic_required: bool,
    pub max_members: Option<i32>,
    pub nip29_group_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ChannelMemberInfo {
    pub pubkey: String,
    pub role: String,
    pub joined_at: String,
    pub display_name: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ChannelMembersResponse {
    pub members: Vec<ChannelMemberInfo>,
    pub next_cursor: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct AddMembersResponse {
    pub added: Vec<String>,
    pub errors: Vec<serde_json::Value>,
}

#[derive(Serialize)]
struct CreateChannelBody<'a> {
    name: &'a str,
    channel_type: &'a str,
    visibility: &'a str,
    description: Option<&'a str>,
}

#[derive(Serialize)]
struct UpdateChannelBody<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
}

#[derive(Serialize)]
struct SetTopicBody<'a> {
    topic: &'a str,
}

#[derive(Serialize)]
struct SetPurposeBody<'a> {
    purpose: &'a str,
}

#[derive(Serialize)]
struct AddMembersBody<'a> {
    pubkeys: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<&'a str>,
}

#[derive(Serialize)]
struct UpdateProfileBody<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    avatar_url: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    about: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    nip05_handle: Option<&'a str>,
}

#[derive(Serialize)]
struct SetPresenceBody {
    status: PresenceStatus,
}

#[derive(Serialize)]
struct GetFeedQuery<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    since: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    types: Option<&'a str>,
}

#[derive(Serialize)]
struct SearchQueryParams<'a> {
    q: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u32>,
}

#[derive(Serialize)]
struct SendChannelMessageBody<'a> {
    content: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_event_id: Option<&'a str>,
    broadcast_to_channel: bool,
}

#[derive(Serialize)]
struct AddReactionBody<'a> {
    emoji: &'a str,
}

#[derive(Serialize)]
struct MintTokenBody<'a> {
    name: &'a str,
    scopes: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    channel_ids: Option<&'a [String]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_in_days: Option<u32>,
}

#[derive(Serialize, Deserialize)]
pub struct MintTokenResponse {
    pub id: String,
    pub token: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub channel_ids: Vec<String>,
    pub created_at: String,
    pub expires_at: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct TokenInfo {
    pub id: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub channel_ids: Vec<String>,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
    pub revoked_at: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ListTokensResponse {
    pub tokens: Vec<TokenInfo>,
}

#[derive(Serialize, Deserialize)]
pub struct RevokeAllTokensResponse {
    pub revoked_count: u64,
}

#[derive(Serialize, Deserialize)]
pub struct FeedItemInfo {
    pub id: String,
    pub kind: u32,
    pub pubkey: String,
    pub content: String,
    pub created_at: u64,
    pub channel_id: Option<String>,
    pub channel_name: String,
    pub tags: Vec<Vec<String>>,
    pub category: String,
}

#[derive(Serialize, Deserialize)]
pub struct FeedSections {
    pub mentions: Vec<FeedItemInfo>,
    pub needs_action: Vec<FeedItemInfo>,
    pub activity: Vec<FeedItemInfo>,
    pub agent_activity: Vec<FeedItemInfo>,
}

#[derive(Serialize, Deserialize)]
pub struct FeedMeta {
    pub since: i64,
    pub total: u64,
    pub generated_at: i64,
}

#[derive(Serialize, Deserialize)]
pub struct FeedResponse {
    pub feed: FeedSections,
    pub meta: FeedMeta,
}

#[derive(Serialize, Deserialize)]
pub struct SearchHitInfo {
    pub event_id: String,
    pub content: String,
    pub kind: u32,
    pub pubkey: String,
    pub channel_id: String,
    pub channel_name: String,
    pub created_at: u64,
    pub score: f64,
}

#[derive(Serialize, Deserialize)]
pub struct SearchResponse {
    pub hits: Vec<SearchHitInfo>,
    pub found: u64,
}

#[derive(Serialize, Deserialize)]
pub struct SendChannelMessageResponse {
    pub event_id: String,
    pub parent_event_id: Option<String>,
    pub root_event_id: Option<String>,
    pub depth: u32,
    pub created_at: i64,
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
struct SproutAdminMintTokenJsonOutput {
    token_id: String,
    name: String,
    scopes: Vec<String>,
    pubkey: String,
    private_key_nsec: Option<String>,
    api_token: String,
}

struct KnownAcpProvider {
    id: &'static str,
    label: &'static str,
    command: &'static str,
    default_args: &'static [&'static str],
}

const DEFAULT_ACP_COMMAND: &str = "sprout-acp";
const DEFAULT_AGENT_COMMAND: &str = "goose";
const DEFAULT_MCP_COMMAND: &str = "sprout-mcp-server";
const DEFAULT_AGENT_ARG: &str = "acp";
const DEFAULT_AGENT_TURN_TIMEOUT_SECONDS: u64 = 300;
const COMMON_BINARY_PATHS: &[&str] = &[
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "/usr/bin",
    "/home/linuxbrew/.linuxbrew/bin",
];
const KNOWN_ACP_PROVIDERS: &[KnownAcpProvider] = &[
    KnownAcpProvider {
        id: "goose",
        label: "Goose",
        command: "goose",
        default_args: &[DEFAULT_AGENT_ARG],
    },
    KnownAcpProvider {
        id: "claude",
        label: "Claude Code",
        command: "claude-agent-acp",
        default_args: &[],
    },
    KnownAcpProvider {
        id: "codex",
        label: "Codex",
        command: "codex-acp",
        default_args: &[],
    },
];

fn deserialize_null_string_as_empty<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

fn default_true() -> bool {
    true
}

fn percent_encode(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len());

    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                let high = char::from_digit((byte >> 4) as u32, 16)
                    .expect("nibble 0-15 is always a valid hex digit")
                    .to_ascii_uppercase();
                let low = char::from_digit((byte & 0x0f) as u32, 16)
                    .expect("nibble 0-15 is always a valid hex digit")
                    .to_ascii_uppercase();
                encoded.push('%');
                encoded.push(high);
                encoded.push(low);
            }
        }
    }

    encoded
}

fn relay_ws_url() -> String {
    std::env::var("SPROUT_RELAY_URL").unwrap_or_else(|_| "ws://localhost:3000".to_string())
}

fn relay_api_base_url() -> String {
    if let Ok(base) = std::env::var("SPROUT_RELAY_HTTP") {
        return base;
    }

    relay_ws_url()
        .replace("wss://", "https://")
        .replace("ws://", "http://")
}

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

fn managed_agents_base_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("failed to resolve app data dir: {e}"))?
        .join("agents");
    fs::create_dir_all(&dir).map_err(|e| format!("failed to create agents dir: {e}"))?;
    Ok(dir)
}

fn managed_agents_store_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(managed_agents_base_dir(app)?.join("managed-agents.json"))
}

fn managed_agents_logs_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = managed_agents_base_dir(app)?.join("logs");
    fs::create_dir_all(&dir).map_err(|e| format!("failed to create logs dir: {e}"))?;
    Ok(dir)
}

fn managed_agent_log_path(app: &AppHandle, pubkey: &str) -> Result<PathBuf, String> {
    Ok(managed_agents_logs_dir(app)?.join(format!("{pubkey}.log")))
}

fn load_managed_agents(app: &AppHandle) -> Result<Vec<ManagedAgentRecord>, String> {
    let path = managed_agents_store_path(app)?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content =
        fs::read_to_string(&path).map_err(|e| format!("failed to read agent store: {e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("failed to parse agent store: {e}"))
}

fn save_managed_agents(app: &AppHandle, records: &[ManagedAgentRecord]) -> Result<(), String> {
    let mut sorted = records.to_vec();
    sorted.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.pubkey.cmp(&right.pubkey))
    });

    let path = managed_agents_store_path(app)?;
    let payload = serde_json::to_vec_pretty(&sorted)
        .map_err(|e| format!("failed to serialize agent store: {e}"))?;
    fs::write(&path, payload).map_err(|e| format!("failed to write agent store: {e}"))
}

fn workspace_root_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn command_looks_like_path(command: &str) -> bool {
    let path = Path::new(command);
    path.is_absolute() || path.components().count() > 1
}

fn executable_basename(command: &str) -> String {
    let suffix = std::env::consts::EXE_SUFFIX;
    if suffix.is_empty() || command.ends_with(suffix) {
        command.to_string()
    } else {
        format!("{command}{suffix}")
    }
}

fn command_search_dirs(app: Option<&AppHandle>) -> Vec<PathBuf> {
    let mut dirs = vec![
        workspace_root_dir().join("target/release"),
        workspace_root_dir().join("target/debug"),
    ];

    if let Ok(current_dir) = std::env::current_dir() {
        dirs.push(current_dir.join("target/release"));
        dirs.push(current_dir.join("target/debug"));
    }

    if app.is_some() {
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(parent) = exe_path.parent() {
                dirs.push(parent.to_path_buf());
            }
        }
    }

    let mut unique = Vec::new();
    for dir in dirs {
        if unique.iter().any(|candidate: &PathBuf| candidate == &dir) {
            continue;
        }
        unique.push(dir);
    }

    unique
}

fn resolve_workspace_command(command: &str, app: Option<&AppHandle>) -> Option<PathBuf> {
    if command_looks_like_path(command) {
        let path = PathBuf::from(command);
        return path.exists().then_some(path);
    }

    let file_name = executable_basename(command);
    command_search_dirs(app)
        .into_iter()
        .map(|dir| dir.join(&file_name))
        .find(|candidate| candidate.exists())
}

fn resolve_command(command: &str, app: Option<&AppHandle>) -> Option<PathBuf> {
    if let Some(path) = resolve_workspace_command(command, app) {
        return Some(path);
    }

    if command_looks_like_path(command) {
        let path = PathBuf::from(command);
        return path.exists().then_some(path);
    }

    for candidate in path_candidates_from_env(command) {
        if candidate.exists() {
            return Some(candidate);
        }
    }

    if let Some(path) = find_via_login_shell(command) {
        return Some(path);
    }

    for dir in COMMON_BINARY_PATHS {
        let candidate = PathBuf::from(dir).join(executable_basename(command));
        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

fn path_candidates_from_env(command: &str) -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths)
                .map(|dir| dir.join(executable_basename(command)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn find_via_login_shell(command: &str) -> Option<PathBuf> {
    let which_cmd = format!("command -v {command}");

    for shell in ["/bin/zsh", "/bin/bash"] {
        let Ok(output) = Command::new(shell)
            .args(["-l", "-c", &which_cmd])
            .output()
        else {
            continue;
        };

        if !output.status.success() {
            continue;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let Some(resolved) = stdout.lines().rfind(|line| !line.trim().is_empty()) else {
            continue;
        };
        let path = PathBuf::from(resolved.trim());
        if path.is_absolute() && path.exists() {
            return Some(path);
        }
    }

    None
}

fn find_command(command: &str) -> Option<PathBuf> {
    resolve_command(command, None)
}

fn command_availability(command: &str, app: Option<&AppHandle>) -> CommandAvailabilityInfo {
    let resolved_path = resolve_command(command, app).map(|path| path.display().to_string());
    CommandAvailabilityInfo {
        command: command.to_string(),
        available: resolved_path.is_some(),
        resolved_path,
    }
}

fn missing_command_message(command: &str, role: &str) -> String {
    if command_looks_like_path(command) {
        return format!("{role} `{command}` does not exist.");
    }

    format!(
        "{role} `{command}` was not found. Build the workspace binaries (`cargo build --release --workspace`) or add `target/release` to PATH as described in TESTING.md."
    )
}

fn discover_local_acp_providers() -> Vec<AcpProviderInfo> {
    KNOWN_ACP_PROVIDERS
        .iter()
        .filter_map(|provider| {
            find_command(provider.command).map(|binary_path| AcpProviderInfo {
                id: provider.id.to_string(),
                label: provider.label.to_string(),
                command: provider.command.to_string(),
                binary_path: binary_path.display().to_string(),
                default_args: provider
                    .default_args
                    .iter()
                    .map(|arg| (*arg).to_string())
                    .collect(),
            })
        })
        .collect()
}

fn admin_command() -> String {
    std::env::var("SPROUT_ADMIN_COMMAND").unwrap_or_else(|_| "sprout-admin".to_string())
}

fn default_token_scopes() -> Vec<String> {
    vec![
        "messages:read".to_string(),
        "messages:write".to_string(),
        "channels:read".to_string(),
    ]
}

fn open_log_file(path: &Path) -> Result<File, String> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("failed to open log file {}: {e}", path.display()))
}

fn append_log_marker(path: &Path, message: &str) -> Result<(), String> {
    let mut file = open_log_file(path)?;
    writeln!(file, "{message}").map_err(|e| format!("failed to write log marker: {e}"))
}

fn read_log_tail(path: &Path, max_lines: usize) -> Result<String, String> {
    if !path.exists() {
        return Ok(String::new());
    }

    let file = File::open(path)
        .map_err(|e| format!("failed to read log file {}: {e}", path.display()))?;
    let reader = BufReader::new(file);
    let lines = reader
        .lines()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("failed to read log lines: {e}"))?;
    let start = lines.len().saturating_sub(max_lines);
    Ok(lines[start..].join("\n"))
}

fn run_sprout_admin_command(
    program: &Path,
    pubkey: &str,
    owner_pubkey: &str,
    name: &str,
    scope_arg: &str,
    json: bool,
) -> Result<Output, String> {
    let mut command = Command::new(program);
    command.arg("mint-token");
    if json {
        command.arg("--json");
    }
    command.args([
        "--name",
        name,
        "--scopes",
        scope_arg,
        "--pubkey",
        pubkey,
        "--owner-pubkey",
        owner_pubkey,
    ]);

    command.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            format!(
                "failed to run `{}`: command not found. Build the workspace binaries from TESTING.md or set SPROUT_ADMIN_COMMAND explicitly.",
                program.display()
            )
        } else {
            format!("failed to run `{}`: {e}", program.display())
        }
    })
}

fn output_error_detail(program: &Path, output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() { stderr } else { stdout };
    format!("`{}` exited with {}: {detail}", program.display(), output.status)
}

fn boxed_line_content(line: &str) -> String {
    line.trim_matches(|ch: char| ch == '║' || ch.is_whitespace())
        .trim()
        .to_string()
}

fn parse_legacy_sprout_admin_mint_output(
    stdout: &str,
    pubkey: &str,
    name: &str,
    scopes: &[String],
) -> Result<SproutAdminMintTokenJsonOutput, String> {
    let contents: Vec<String> = stdout
        .lines()
        .map(boxed_line_content)
        .filter(|line| !line.is_empty())
        .collect();

    let token_id = contents
        .iter()
        .find_map(|line| line.strip_prefix("Token ID:").map(|value| value.trim().to_string()))
        .ok_or_else(|| "failed to parse legacy sprout-admin output: missing token id".to_string())?;

    let api_token_index = contents
        .iter()
        .position(|line| line == "API Token:")
        .ok_or_else(|| "failed to parse legacy sprout-admin output: missing API token label".to_string())?;
    let api_token = contents
        .iter()
        .skip(api_token_index + 1)
        .find(|line| !line.ends_with(':'))
        .cloned()
        .ok_or_else(|| "failed to parse legacy sprout-admin output: missing API token value".to_string())?;

    Ok(SproutAdminMintTokenJsonOutput {
        token_id,
        name: name.to_string(),
        scopes: scopes.to_vec(),
        pubkey: pubkey.to_string(),
        private_key_nsec: None,
        api_token,
    })
}

fn run_sprout_admin_mint_token(
    app: &AppHandle,
    pubkey: &str,
    owner_pubkey: &str,
    name: &str,
    scopes: &[String],
) -> Result<SproutAdminMintTokenJsonOutput, String> {
    let configured_program = admin_command();
    let program = resolve_command(&configured_program, Some(app))
        .ok_or_else(|| missing_command_message(&configured_program, "sprout-admin command"))?;
    let scope_arg = scopes.join(",");
    let output = run_sprout_admin_command(&program, pubkey, owner_pubkey, name, &scope_arg, true)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("unexpected argument '--json'") {
            let legacy_output =
                run_sprout_admin_command(&program, pubkey, owner_pubkey, name, &scope_arg, false)?;
            if !legacy_output.status.success() {
                return Err(output_error_detail(&program, &legacy_output));
            }

            return parse_legacy_sprout_admin_mint_output(
                &String::from_utf8_lossy(&legacy_output.stdout),
                pubkey,
                name,
                scopes,
            );
        }

        return Err(output_error_detail(&program, &output));
    }

    let parsed: SproutAdminMintTokenJsonOutput = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("failed to parse sprout-admin JSON output: {e}"))?;

    if parsed.pubkey.to_lowercase() != pubkey.to_lowercase() {
        return Err("sprout-admin returned a token for the wrong pubkey".to_string());
    }

    if parsed.token_id.trim().is_empty() {
        return Err("sprout-admin returned an empty token id".to_string());
    }

    if parsed.name.trim() != name.trim() {
        return Err("sprout-admin returned a token with the wrong name".to_string());
    }

    if parsed.scopes != scopes {
        return Err("sprout-admin returned a token with the wrong scopes".to_string());
    }

    if parsed.private_key_nsec.is_some() {
        return Err("sprout-admin unexpectedly returned a private key for an existing pubkey".to_string());
    }

    Ok(parsed)
}

fn sync_managed_agent_processes(
    records: &mut [ManagedAgentRecord],
    runtimes: &mut HashMap<String, ManagedAgentProcess>,
) -> bool {
    let mut changed = false;
    let mut exited = Vec::new();

    for (pubkey, runtime) in runtimes.iter_mut() {
        let status = match runtime.child.try_wait() {
            Ok(status) => status,
            Err(error) => {
                if let Some(record) = records.iter_mut().find(|record| record.pubkey == *pubkey) {
                    record.updated_at = now_iso();
                    record.last_error = Some(format!("failed to inspect process state: {error}"));
                }
                changed = true;
                exited.push(pubkey.clone());
                continue;
            }
        };

        let Some(status) = status else {
            continue;
        };

        if let Some(record) = records.iter_mut().find(|record| record.pubkey == *pubkey) {
            record.updated_at = now_iso();
            record.last_stopped_at = Some(now_iso());
            record.last_exit_code = status.code();
            record.last_error = if status.success() {
                None
            } else {
                Some(format!("harness exited with status {status}"))
            };
        }

        changed = true;
        exited.push(pubkey.clone());
    }

    for pubkey in exited {
        runtimes.remove(&pubkey);
    }

    changed
}

fn build_managed_agent_summary(
    app: &AppHandle,
    record: &ManagedAgentRecord,
    runtimes: &HashMap<String, ManagedAgentProcess>,
) -> Result<ManagedAgentSummary, String> {
    let (status, pid, log_path) = if let Some(runtime) = runtimes.get(&record.pubkey) {
        (
            "running".to_string(),
            Some(runtime.child.id()),
            runtime.log_path.display().to_string(),
        )
    } else {
        (
            "stopped".to_string(),
            None,
            managed_agent_log_path(app, &record.pubkey)?
                .display()
                .to_string(),
        )
    };

    Ok(ManagedAgentSummary {
        pubkey: record.pubkey.clone(),
        name: record.name.clone(),
        relay_url: record.relay_url.clone(),
        acp_command: record.acp_command.clone(),
        agent_command: record.agent_command.clone(),
        agent_args: record.agent_args.clone(),
        mcp_command: record.mcp_command.clone(),
        turn_timeout_seconds: record.turn_timeout_seconds,
        has_api_token: record.api_token.is_some(),
        status,
        pid,
        created_at: record.created_at.clone(),
        updated_at: record.updated_at.clone(),
        last_started_at: record.last_started_at.clone(),
        last_stopped_at: record.last_stopped_at.clone(),
        last_exit_code: record.last_exit_code,
        last_error: record.last_error.clone(),
        log_path,
    })
}

fn find_managed_agent_mut<'a>(
    records: &'a mut [ManagedAgentRecord],
    pubkey: &str,
) -> Result<&'a mut ManagedAgentRecord, String> {
    records
        .iter_mut()
        .find(|record| record.pubkey == pubkey)
        .ok_or_else(|| format!("agent {pubkey} not found"))
}

fn start_managed_agent_process(
    app: &AppHandle,
    record: &mut ManagedAgentRecord,
    runtimes: &mut HashMap<String, ManagedAgentProcess>,
) -> Result<(), String> {
    if let Some(runtime) = runtimes.get_mut(&record.pubkey) {
        if runtime
            .child
            .try_wait()
            .map_err(|e| format!("failed to inspect running process: {e}"))?
            .is_none()
        {
            return Ok(());
        }

        runtimes.remove(&record.pubkey);
    }

    let log_path = managed_agent_log_path(app, &record.pubkey)?;
    append_log_marker(
        &log_path,
        &format!(
            "\n=== starting {} ({}) at {} ===",
            record.name,
            record.pubkey,
            now_iso()
        ),
    )?;

    let stdout = open_log_file(&log_path)?;
    let stderr = stdout
        .try_clone()
        .map_err(|e| format!("failed to clone log handle: {e}"))?;
    let agent_args = if record.agent_args.is_empty() {
        vec![DEFAULT_AGENT_ARG.to_string()]
    } else {
        record.agent_args.clone()
    };
    let resolved_acp_command = resolve_command(&record.acp_command, Some(app))
        .ok_or_else(|| missing_command_message(&record.acp_command, "ACP harness command"))?;
    let resolved_mcp_command = resolve_command(&record.mcp_command, Some(app))
        .ok_or_else(|| missing_command_message(&record.mcp_command, "MCP server command"))?;

    let mut command = Command::new(&resolved_acp_command);
    command.stdin(Stdio::null());
    command.stdout(Stdio::from(stdout));
    command.stderr(Stdio::from(stderr));
    command.env("SPROUT_PRIVATE_KEY", &record.private_key_nsec);
    command.env("SPROUT_RELAY_URL", &record.relay_url);
    command.env("SPROUT_ACP_AGENT_COMMAND", &record.agent_command);
    command.env("SPROUT_ACP_AGENT_ARGS", agent_args.join(","));
    command.env("SPROUT_ACP_MCP_COMMAND", &resolved_mcp_command);
    command.env(
        "SPROUT_ACP_TURN_TIMEOUT",
        record.turn_timeout_seconds.to_string(),
    );
    command.env(
        "GOOSE_MODE",
        std::env::var("GOOSE_MODE").unwrap_or_else(|_| "auto".to_string()),
    );
    command.env_remove("SPROUT_ACP_PRIVATE_KEY");
    command.env_remove("SPROUT_ACP_API_TOKEN");

    if let Some(token) = &record.api_token {
        command.env("SPROUT_API_TOKEN", token);
    } else {
        command.env_remove("SPROUT_API_TOKEN");
    }

    let child = command.spawn().map_err(|e| {
        format!(
            "failed to spawn `{}` for agent {}: {e}",
            resolved_acp_command.display(),
            record.name
        )
    })?;

    let now = now_iso();
    record.updated_at = now.clone();
    record.last_started_at = Some(now);
    record.last_stopped_at = None;
    record.last_exit_code = None;
    record.last_error = None;

    runtimes.insert(record.pubkey.clone(), ManagedAgentProcess { child, log_path });
    Ok(())
}

fn stop_managed_agent_process(
    record: &mut ManagedAgentRecord,
    runtimes: &mut HashMap<String, ManagedAgentProcess>,
) -> Result<(), String> {
    let Some(mut runtime) = runtimes.remove(&record.pubkey) else {
        return Ok(());
    };

    let _ = runtime.child.kill();
    let status = runtime
        .child
        .wait()
        .map_err(|e| format!("failed to wait for agent shutdown: {e}"))?;
    let now = now_iso();
    record.updated_at = now.clone();
    record.last_stopped_at = Some(now);
    record.last_exit_code = status.code();
    record.last_error = None;

    append_log_marker(
        &runtime.log_path,
        &format!(
            "=== stopped {} ({}) at {} ===",
            record.name,
            record.pubkey,
            now_iso()
        ),
    )?;

    Ok(())
}

fn build_authed_request(
    client: &reqwest::Client,
    method: Method,
    path: &str,
    state: &AppState,
) -> Result<reqwest::RequestBuilder, String> {
    let url = format!("{}{}", relay_api_base_url(), path);
    let request = client.request(method, url);

    if let Some(token) = state.configured_api_token.as_deref() {
        return Ok(request.header("Authorization", format!("Bearer {token}")));
    }

    let pubkey_hex = auth_pubkey_header(state)?;
    Ok(request.header("X-Pubkey", pubkey_hex))
}

fn auth_pubkey_header(state: &AppState) -> Result<String, String> {
    let keys = state.keys.lock().map_err(|e| e.to_string())?;
    Ok(keys.public_key().to_hex())
}

fn session_api_token(state: &AppState) -> Result<Option<String>, String> {
    let token = state.session_token.lock().map_err(|e| e.to_string())?;
    Ok(token.clone())
}

fn build_token_management_request(
    client: &reqwest::Client,
    method: Method,
    path: &str,
    state: &AppState,
) -> Result<reqwest::RequestBuilder, String> {
    let url = format!("{}{}", relay_api_base_url(), path);
    let request = client.request(method, url);

    if let Some(token) = state.configured_api_token.as_deref() {
        return Ok(request.header("Authorization", format!("Bearer {token}")));
    }

    if let Some(token) = session_api_token(state)? {
        return Ok(request.header("Authorization", format!("Bearer {token}")));
    }

    let pubkey_hex = auth_pubkey_header(state)?;
    Ok(request.header("X-Pubkey", pubkey_hex))
}

fn build_nip98_auth_header(
    method: &Method,
    url: &str,
    body: &[u8],
    state: &AppState,
) -> Result<String, String> {
    let keys = state.keys.lock().map_err(|e| e.to_string())?;
    let payload_hash = format!("{:x}", Sha256::digest(body));
    let tags = vec![
        Tag::parse(vec!["u", url]).map_err(|e| format!("url tag failed: {e}"))?,
        Tag::parse(vec!["method", method.as_str()])
            .map_err(|e| format!("method tag failed: {e}"))?,
        Tag::parse(vec!["payload", &payload_hash])
            .map_err(|e| format!("payload tag failed: {e}"))?,
    ];

    let event = EventBuilder::new(Kind::HttpAuth, "")
        .tags(tags)
        .sign_with_keys(&keys)
        .map_err(|e| format!("sign failed: {e}"))?;

    Ok(format!("Nostr {}", BASE64.encode(event.as_json().as_bytes())))
}

async fn relay_error_message(response: reqwest::Response) -> String {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&body) {
        if let Some(message) = value.get("message").and_then(serde_json::Value::as_str) {
            return format!("relay returned {status}: {message}");
        }

        if let Some(error) = value.get("error").and_then(serde_json::Value::as_str) {
            return format!("relay returned {status}: {error}");
        }
    }

    format!("relay returned {status}: {body}")
}

async fn send_json_request<T>(request: reqwest::RequestBuilder) -> Result<T, String>
where
    T: DeserializeOwned,
{
    let response = request
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(relay_error_message(response).await);
    }

    response
        .json::<T>()
        .await
        .map_err(|e| format!("parse failed: {e}"))
}

async fn send_empty_request(request: reqwest::RequestBuilder) -> Result<(), String> {
    let response = request
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(relay_error_message(response).await);
    }

    Ok(())
}

#[tauri::command]
fn get_identity(state: tauri::State<'_, AppState>) -> Result<IdentityInfo, String> {
    let keys = state.keys.lock().map_err(|e| e.to_string())?;
    let pubkey = keys.public_key();
    let pubkey_hex = pubkey.to_hex();
    let bech32 = pubkey
        .to_bech32()
        .map_err(|e| format!("bech32 encode failed: {e}"))?;
    let display_name = if bech32.len() > 16 {
        format!("{}…{}", &bech32[..10], &bech32[bech32.len() - 4..])
    } else {
        bech32
    };

    Ok(IdentityInfo {
        pubkey: pubkey_hex,
        display_name,
    })
}

#[tauri::command]
fn get_relay_ws_url() -> String {
    relay_ws_url()
}

#[tauri::command]
fn discover_acp_providers() -> Vec<AcpProviderInfo> {
    discover_local_acp_providers()
}

#[tauri::command]
fn discover_managed_agent_prereqs(
    input: DiscoverManagedAgentPrereqsRequest,
    app: AppHandle,
) -> ManagedAgentPrereqsInfo {
    let acp_command = input
        .acp_command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_ACP_COMMAND);
    let mcp_command = input
        .mcp_command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_MCP_COMMAND);
    let admin_command = admin_command();

    ManagedAgentPrereqsInfo {
        admin: command_availability(&admin_command, Some(&app)),
        acp: command_availability(acp_command, Some(&app)),
        mcp: command_availability(mcp_command, Some(&app)),
    }
}

#[tauri::command]
async fn get_profile(state: tauri::State<'_, AppState>) -> Result<ProfileInfo, String> {
    let request = build_authed_request(
        &state.http_client,
        Method::GET,
        "/api/users/me/profile",
        &state,
    )?;
    send_json_request(request).await
}

#[tauri::command]
async fn update_profile(
    display_name: Option<String>,
    avatar_url: Option<String>,
    about: Option<String>,
    nip05_handle: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<ProfileInfo, String> {
    let request = build_authed_request(
        &state.http_client,
        Method::PUT,
        "/api/users/me/profile",
        &state,
    )?
    .json(&UpdateProfileBody {
        display_name: display_name.as_deref(),
        avatar_url: avatar_url.as_deref(),
        about: about.as_deref(),
        nip05_handle: nip05_handle.as_deref(),
    });
    send_empty_request(request).await?;

    let request = build_authed_request(
        &state.http_client,
        Method::GET,
        "/api/users/me/profile",
        &state,
    )?;
    send_json_request(request).await
}

#[tauri::command]
async fn get_user_profile(
    pubkey: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<ProfileInfo, String> {
    let path = match pubkey {
        Some(pubkey) => format!("/api/users/{pubkey}/profile"),
        None => "/api/users/me/profile".to_string(),
    };
    let request = build_authed_request(&state.http_client, Method::GET, &path, &state)?;
    send_json_request(request).await
}

#[derive(Serialize)]
struct GetUsersBatchBody<'a> {
    pubkeys: &'a [String],
}

#[tauri::command]
async fn get_users_batch(
    pubkeys: Vec<String>,
    state: tauri::State<'_, AppState>,
) -> Result<UsersBatchResponse, String> {
    let request = build_authed_request(
        &state.http_client,
        Method::POST,
        "/api/users/batch",
        &state,
    )?
    .json(&GetUsersBatchBody {
        pubkeys: pubkeys.as_slice(),
    });
    send_json_request(request).await
}

#[tauri::command]
async fn get_presence(
    pubkeys: Vec<String>,
    state: tauri::State<'_, AppState>,
) -> Result<HashMap<String, PresenceStatus>, String> {
    if pubkeys.is_empty() {
        return Ok(HashMap::new());
    }

    let request = build_authed_request(&state.http_client, Method::GET, "/api/presence", &state)?
        .query(&[("pubkeys", pubkeys.join(","))]);
    send_json_request(request).await
}

#[tauri::command]
async fn set_presence(
    status: PresenceStatus,
    state: tauri::State<'_, AppState>,
) -> Result<SetPresenceResponse, String> {
    let request = build_authed_request(&state.http_client, Method::PUT, "/api/presence", &state)?
        .json(&SetPresenceBody { status });
    send_json_request(request).await
}

#[tauri::command]
fn sign_event(
    kind: u16,
    content: String,
    tags: Vec<Vec<String>>,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let keys = state.keys.lock().map_err(|e| e.to_string())?;

    let nostr_tags = tags
        .into_iter()
        .map(|tag| Tag::parse(tag).map_err(|e| format!("invalid tag: {e}")))
        .collect::<Result<Vec<_>, _>>()?;

    let event = EventBuilder::new(Kind::Custom(kind), content)
        .tags(nostr_tags)
        .sign_with_keys(&keys)
        .map_err(|e| format!("sign failed: {e}"))?;

    Ok(event.as_json())
}

#[tauri::command]
fn create_auth_event(
    challenge: String,
    relay_url: String,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let keys = state.keys.lock().map_err(|e| e.to_string())?;

    let mut tags = vec![
        Tag::parse(vec!["relay", &relay_url]).map_err(|e| format!("relay tag failed: {e}"))?,
        Tag::parse(vec!["challenge", &challenge])
            .map_err(|e| format!("challenge tag failed: {e}"))?,
    ];

    if let Some(token) = state.configured_api_token.as_deref() {
        tags.push(
            Tag::parse(vec!["auth_token", token])
                .map_err(|e| format!("auth token tag failed: {e}"))?,
        );
    }

    let event = EventBuilder::new(Kind::Custom(22242), "")
        .tags(tags)
        .sign_with_keys(&keys)
        .map_err(|e| format!("sign failed: {e}"))?;

    Ok(event.as_json())
}

#[tauri::command]
async fn get_channels(state: tauri::State<'_, AppState>) -> Result<Vec<ChannelInfo>, String> {
    let request = build_authed_request(&state.http_client, Method::GET, "/api/channels", &state)?;
    send_json_request(request).await
}

#[tauri::command]
async fn create_channel(
    name: String,
    channel_type: String,
    visibility: String,
    description: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<ChannelInfo, String> {
    let request = build_authed_request(&state.http_client, Method::POST, "/api/channels", &state)?
        .json(&CreateChannelBody {
            name: &name,
            channel_type: &channel_type,
            visibility: &visibility,
            description: description.as_deref(),
        });

    send_json_request(request).await
}

#[tauri::command]
async fn get_channel_details(
    channel_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ChannelDetailInfo, String> {
    let path = format!("/api/channels/{channel_id}");
    let request = build_authed_request(&state.http_client, Method::GET, &path, &state)?;
    send_json_request(request).await
}

#[tauri::command]
async fn get_channel_members(
    channel_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<ChannelMembersResponse, String> {
    let path = format!("/api/channels/{channel_id}/members");
    let request = build_authed_request(&state.http_client, Method::GET, &path, &state)?;
    send_json_request(request).await
}

#[tauri::command]
async fn update_channel(
    channel_id: String,
    name: Option<String>,
    description: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<ChannelDetailInfo, String> {
    let path = format!("/api/channels/{channel_id}");
    let request = build_authed_request(&state.http_client, Method::PUT, &path, &state)?
        .json(&UpdateChannelBody {
            name: name.as_deref(),
            description: description.as_deref(),
        });

    send_json_request(request).await
}

#[tauri::command]
async fn set_channel_topic(
    channel_id: String,
    topic: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let path = format!("/api/channels/{channel_id}/topic");
    let request = build_authed_request(&state.http_client, Method::PUT, &path, &state)?
        .json(&SetTopicBody { topic: &topic });
    send_empty_request(request).await
}

#[tauri::command]
async fn set_channel_purpose(
    channel_id: String,
    purpose: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let path = format!("/api/channels/{channel_id}/purpose");
    let request = build_authed_request(&state.http_client, Method::PUT, &path, &state)?
        .json(&SetPurposeBody { purpose: &purpose });
    send_empty_request(request).await
}

#[tauri::command]
async fn archive_channel(
    channel_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let path = format!("/api/channels/{channel_id}/archive");
    let request = build_authed_request(&state.http_client, Method::POST, &path, &state)?;
    send_empty_request(request).await
}

#[tauri::command]
async fn unarchive_channel(
    channel_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let path = format!("/api/channels/{channel_id}/unarchive");
    let request = build_authed_request(&state.http_client, Method::POST, &path, &state)?;
    send_empty_request(request).await
}

#[tauri::command]
async fn delete_channel(
    channel_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let path = format!("/api/channels/{channel_id}");
    let request = build_authed_request(&state.http_client, Method::DELETE, &path, &state)?;
    send_empty_request(request).await
}

#[tauri::command]
async fn add_channel_members(
    channel_id: String,
    pubkeys: Vec<String>,
    role: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<AddMembersResponse, String> {
    let path = format!("/api/channels/{channel_id}/members");
    let request = build_authed_request(&state.http_client, Method::POST, &path, &state)?
        .json(&AddMembersBody {
            pubkeys: &pubkeys,
            role: role.as_deref(),
        });

    send_json_request(request).await
}

#[tauri::command]
async fn remove_channel_member(
    channel_id: String,
    pubkey: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let path = format!("/api/channels/{channel_id}/members/{pubkey}");
    let request = build_authed_request(&state.http_client, Method::DELETE, &path, &state)?;
    send_empty_request(request).await
}

#[tauri::command]
async fn join_channel(
    channel_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let path = format!("/api/channels/{channel_id}/join");
    let request = build_authed_request(&state.http_client, Method::POST, &path, &state)?;
    send_empty_request(request).await
}

#[tauri::command]
async fn leave_channel(
    channel_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let path = format!("/api/channels/{channel_id}/leave");
    let request = build_authed_request(&state.http_client, Method::POST, &path, &state)?;
    send_empty_request(request).await
}

#[tauri::command]
async fn get_feed(
    since: Option<i64>,
    limit: Option<u32>,
    types: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<FeedResponse, String> {
    let request = build_authed_request(&state.http_client, Method::GET, "/api/feed", &state)?
        .query(&GetFeedQuery {
            since,
            limit,
            types: types.as_deref(),
        });

    send_json_request(request).await
}

#[tauri::command]
async fn search_messages(
    q: String,
    limit: Option<u32>,
    state: tauri::State<'_, AppState>,
) -> Result<SearchResponse, String> {
    let request = build_authed_request(&state.http_client, Method::GET, "/api/search", &state)?
        .query(&SearchQueryParams {
            q: q.trim(),
            limit,
        });

    send_json_request(request).await
}

#[tauri::command]
async fn send_channel_message(
    channel_id: String,
    content: String,
    parent_event_id: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<SendChannelMessageResponse, String> {
    let path = format!("/api/channels/{channel_id}/messages");
    let request = build_authed_request(&state.http_client, Method::POST, &path, &state)?.json(
        &SendChannelMessageBody {
            content: content.trim(),
            parent_event_id: parent_event_id.as_deref(),
            broadcast_to_channel: false,
        },
    );

    send_json_request(request).await
}

#[tauri::command]
async fn add_reaction(
    event_id: String,
    emoji: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let path = format!("/api/messages/{event_id}/reactions");
    let request = build_authed_request(&state.http_client, Method::POST, &path, &state)?.json(
        &AddReactionBody {
            emoji: emoji.trim(),
        },
    );

    send_empty_request(request).await
}

#[tauri::command]
async fn remove_reaction(
    event_id: String,
    emoji: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let path = format!(
        "/api/messages/{event_id}/reactions/{}",
        percent_encode(emoji.trim())
    );
    let request = build_authed_request(&state.http_client, Method::DELETE, &path, &state)?;

    send_empty_request(request).await
}

#[tauri::command]
async fn get_event(event_id: String, state: tauri::State<'_, AppState>) -> Result<String, String> {
    let request = build_authed_request(
        &state.http_client,
        Method::GET,
        &format!("/api/events/{event_id}"),
        &state,
    )?;
    let response = request
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(relay_error_message(response).await);
    }

    response.text().await.map_err(|e| format!("parse failed: {e}"))
}

#[tauri::command]
async fn list_tokens(state: tauri::State<'_, AppState>) -> Result<ListTokensResponse, String> {
    let request =
        build_token_management_request(&state.http_client, Method::GET, "/api/tokens", &state)?;
    send_json_request(request).await
}

#[tauri::command]
async fn mint_token(
    name: String,
    scopes: Vec<String>,
    channel_ids: Option<Vec<String>>,
    expires_in_days: Option<u32>,
    state: tauri::State<'_, AppState>,
) -> Result<MintTokenResponse, String> {
    let body = MintTokenBody {
        name: &name,
        scopes: &scopes,
        channel_ids: channel_ids.as_deref(),
        expires_in_days,
    };
    let request = if state.configured_api_token.is_some() {
        build_authed_request(&state.http_client, Method::POST, "/api/tokens", &state)?.json(&body)
    } else {
        let url = format!("{}{}", relay_api_base_url(), "/api/tokens");
        let body_bytes =
            serde_json::to_vec(&body).map_err(|e| format!("serialize failed: {e}"))?;
        let auth_header = build_nip98_auth_header(&Method::POST, &url, &body_bytes, &state)?;
        let forwarded_proto = if url.starts_with("http://") {
            "http"
        } else {
            "https"
        };

        state
            .http_client
            .request(Method::POST, url)
            .header("Authorization", auth_header)
            .header("Content-Type", "application/json")
            .header("X-Forwarded-Proto", forwarded_proto)
            .body(body_bytes)
    };
    let response: MintTokenResponse = send_json_request(request).await?;

    if state.configured_api_token.is_none() {
        let mut token = state.session_token.lock().map_err(|e| e.to_string())?;
        *token = Some(response.token.clone());
    }

    Ok(response)
}

#[tauri::command]
async fn revoke_token(
    token_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let path = format!("/api/tokens/{token_id}");
    let request =
        build_token_management_request(&state.http_client, Method::DELETE, &path, &state)?;
    send_empty_request(request).await
}

#[tauri::command]
async fn revoke_all_tokens(
    state: tauri::State<'_, AppState>,
) -> Result<RevokeAllTokensResponse, String> {
    let request =
        build_token_management_request(&state.http_client, Method::DELETE, "/api/tokens", &state)?;
    send_json_request(request).await
}

#[tauri::command]
async fn list_relay_agents(state: tauri::State<'_, AppState>) -> Result<Vec<RelayAgentInfo>, String> {
    let request = build_authed_request(&state.http_client, Method::GET, "/api/agents", &state)?;
    send_json_request(request).await
}

#[tauri::command]
fn list_managed_agents(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ManagedAgentSummary>, String> {
    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    let mut records = load_managed_agents(&app)?;
    let mut runtimes = state
        .managed_agent_processes
        .lock()
        .map_err(|e| e.to_string())?;

    if sync_managed_agent_processes(&mut records, &mut runtimes) {
        save_managed_agents(&app, &records)?;
    }

    records
        .iter()
        .map(|record| build_managed_agent_summary(&app, record, &runtimes))
        .collect()
}

#[tauri::command]
fn create_managed_agent(
    input: CreateManagedAgentRequest,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<CreateManagedAgentResponse, String> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err("agent name is required".to_string());
    }

    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    let mut records = load_managed_agents(&app)?;
    let mut runtimes = state
        .managed_agent_processes
        .lock()
        .map_err(|e| e.to_string())?;

    if sync_managed_agent_processes(&mut records, &mut runtimes) {
        save_managed_agents(&app, &records)?;
    }

    let keys = Keys::generate();
    let pubkey = keys.public_key().to_hex();
    if records.iter().any(|record| record.pubkey == pubkey) {
        return Err(format!("agent {pubkey} already exists"));
    }

    let private_key_nsec = keys
        .secret_key()
        .to_bech32()
        .map_err(|e| format!("failed to encode private key: {e}"))?;
    let token_scopes = if input.mint_token {
        let requested = input
            .token_scopes
            .into_iter()
            .map(|scope| scope.trim().to_string())
            .filter(|scope| !scope.is_empty())
            .collect::<Vec<_>>();
        if requested.is_empty() {
            default_token_scopes()
        } else {
            requested
        }
    } else {
        Vec::new()
    };
    let minted = if input.mint_token {
        let owner_pubkey = auth_pubkey_header(&state)?;
        let token_name = input
            .token_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(name);
        Some(run_sprout_admin_mint_token(
            &app,
            &pubkey,
            &owner_pubkey,
            token_name,
            &token_scopes,
        )?)
    } else {
        None
    };
    let resolved_relay_url = input
        .relay_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(relay_ws_url);

    let mut record = ManagedAgentRecord {
        pubkey: pubkey.clone(),
        name: name.to_string(),
        private_key_nsec: private_key_nsec.clone(),
        api_token: minted.as_ref().map(|output| output.api_token.clone()),
        relay_url: resolved_relay_url,
        acp_command: input
            .acp_command
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_ACP_COMMAND)
            .to_string(),
        agent_command: input
            .agent_command
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_AGENT_COMMAND)
            .to_string(),
        agent_args: input
            .agent_args
            .into_iter()
            .map(|arg| arg.trim().to_string())
            .filter(|arg| !arg.is_empty())
            .collect::<Vec<_>>(),
        mcp_command: input
            .mcp_command
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_MCP_COMMAND)
            .to_string(),
        turn_timeout_seconds: input
            .turn_timeout_seconds
            .filter(|seconds| *seconds > 0)
            .unwrap_or(DEFAULT_AGENT_TURN_TIMEOUT_SECONDS),
        created_at: now_iso(),
        updated_at: now_iso(),
        last_started_at: None,
        last_stopped_at: None,
        last_exit_code: None,
        last_error: None,
    };

    if record.agent_args.is_empty() {
        record.agent_args.push(DEFAULT_AGENT_ARG.to_string());
    }

    records.push(record);

    let mut spawn_error = None;
    if input.spawn_after_create {
        let record = find_managed_agent_mut(&mut records, &pubkey)?;
        if let Err(error) = start_managed_agent_process(&app, record, &mut runtimes) {
            record.updated_at = now_iso();
            record.last_error = Some(error.clone());
            spawn_error = Some(error);
        }
    }

    save_managed_agents(&app, &records)?;

    let record = records
        .iter()
        .find(|record| record.pubkey == pubkey)
        .ok_or_else(|| "created agent disappeared unexpectedly".to_string())?;
    let agent = build_managed_agent_summary(&app, record, &runtimes)?;

    Ok(CreateManagedAgentResponse {
        agent,
        private_key_nsec,
        api_token: minted.map(|output| output.api_token),
        spawn_error,
    })
}

#[tauri::command]
fn start_managed_agent(
    pubkey: String,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<ManagedAgentSummary, String> {
    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    let mut records = load_managed_agents(&app)?;
    let mut runtimes = state
        .managed_agent_processes
        .lock()
        .map_err(|e| e.to_string())?;

    if sync_managed_agent_processes(&mut records, &mut runtimes) {
        save_managed_agents(&app, &records)?;
    }

    let record = find_managed_agent_mut(&mut records, &pubkey)?;
    start_managed_agent_process(&app, record, &mut runtimes)?;
    let _ = record;
    save_managed_agents(&app, &records)?;
    let record = records
        .iter()
        .find(|record| record.pubkey == pubkey)
        .ok_or_else(|| format!("agent {pubkey} not found"))?;
    build_managed_agent_summary(&app, record, &runtimes)
}

#[tauri::command]
fn stop_managed_agent(
    pubkey: String,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<ManagedAgentSummary, String> {
    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    let mut records = load_managed_agents(&app)?;
    let mut runtimes = state
        .managed_agent_processes
        .lock()
        .map_err(|e| e.to_string())?;

    if sync_managed_agent_processes(&mut records, &mut runtimes) {
        save_managed_agents(&app, &records)?;
    }

    let record = find_managed_agent_mut(&mut records, &pubkey)?;
    stop_managed_agent_process(record, &mut runtimes)?;
    let _ = record;
    save_managed_agents(&app, &records)?;
    let record = records
        .iter()
        .find(|record| record.pubkey == pubkey)
        .ok_or_else(|| format!("agent {pubkey} not found"))?;
    build_managed_agent_summary(&app, record, &runtimes)
}

#[tauri::command]
fn delete_managed_agent(
    pubkey: String,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    let mut records = load_managed_agents(&app)?;
    let mut runtimes = state
        .managed_agent_processes
        .lock()
        .map_err(|e| e.to_string())?;

    if sync_managed_agent_processes(&mut records, &mut runtimes) {
        save_managed_agents(&app, &records)?;
    }

    if let Some(record) = records.iter_mut().find(|record| record.pubkey == pubkey) {
        stop_managed_agent_process(record, &mut runtimes)?;
    }

    let initial_len = records.len();
    records.retain(|record| record.pubkey != pubkey);
    if records.len() == initial_len {
        return Err(format!("agent {pubkey} not found"));
    }

    save_managed_agents(&app, &records)
}

#[tauri::command]
fn mint_managed_agent_token(
    input: MintManagedAgentTokenRequest,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<MintManagedAgentTokenResponse, String> {
    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    let mut records = load_managed_agents(&app)?;
    let mut runtimes = state
        .managed_agent_processes
        .lock()
        .map_err(|e| e.to_string())?;

    if sync_managed_agent_processes(&mut records, &mut runtimes) {
        save_managed_agents(&app, &records)?;
    }

    let owner_pubkey = auth_pubkey_header(&state)?;
    let record = find_managed_agent_mut(&mut records, &input.pubkey)?;
    let scopes = input
        .scopes
        .into_iter()
        .map(|scope| scope.trim().to_string())
        .filter(|scope| !scope.is_empty())
        .collect::<Vec<_>>();
    let scopes = if scopes.is_empty() {
        default_token_scopes()
    } else {
        scopes
    };
    let token_name = input
        .token_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{}-token", record.name));
    let minted =
        run_sprout_admin_mint_token(&app, &record.pubkey, &owner_pubkey, &token_name, &scopes)?;

    record.api_token = Some(minted.api_token.clone());
    record.updated_at = now_iso();
    record.last_error = None;
    let pubkey = record.pubkey.clone();

    save_managed_agents(&app, &records)?;
    let record = records
        .iter()
        .find(|record| record.pubkey == pubkey)
        .ok_or_else(|| format!("agent {pubkey} not found"))?;
    let agent = build_managed_agent_summary(&app, record, &runtimes)?;

    Ok(MintManagedAgentTokenResponse {
        agent,
        token: minted.api_token,
    })
}

#[tauri::command]
fn get_managed_agent_log(
    pubkey: String,
    line_count: Option<u32>,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<ManagedAgentLogResponse, String> {
    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    let records = load_managed_agents(&app)?;
    if !records.iter().any(|record| record.pubkey == pubkey) {
        return Err(format!("agent {pubkey} not found"));
    }

    let log_path = managed_agent_log_path(&app, &pubkey)?;
    Ok(ManagedAgentLogResponse {
        content: read_log_tail(&log_path, line_count.unwrap_or(120) as usize)?,
        log_path: log_path.display().to_string(),
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // GUI app: warn on bad key but don't crash — fall back to ephemeral.
    // CLI crates (sprout-mcp, sprout-test-client) use fatal errors instead.
    let (keys, source) = match std::env::var("SPROUT_PRIVATE_KEY") {
        Ok(nsec) => match Keys::parse(nsec.trim()) {
            Ok(k) => (k, "configured"),
            Err(e) => {
                eprintln!("sprout-desktop: invalid SPROUT_PRIVATE_KEY: {e}");
                (Keys::generate(), "ephemeral")
            }
        },
        Err(std::env::VarError::NotUnicode(_)) => {
            eprintln!("sprout-desktop: SPROUT_PRIVATE_KEY contains invalid UTF-8");
            (Keys::generate(), "ephemeral")
        }
        Err(std::env::VarError::NotPresent) => (Keys::generate(), "ephemeral"),
    };

    eprintln!(
        "sprout-desktop: {source} identity pubkey {}",
        keys.public_key().to_hex()
    );

    let api_token = match std::env::var("SPROUT_API_TOKEN") {
        Ok(token) if !token.trim().is_empty() => Some(token),
        Ok(_) | Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(_)) => {
            eprintln!("sprout-desktop: SPROUT_API_TOKEN contains invalid UTF-8");
            None
        }
    };

    let app_state = AppState {
        keys: Mutex::new(keys),
        http_client: reqwest::Client::new(),
        configured_api_token: api_token,
        session_token: Mutex::new(None),
        managed_agents_store_lock: Mutex::new(()),
        managed_agent_processes: Mutex::new(HashMap::new()),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    StateFlags::all() & !StateFlags::VISIBLE,
                )
                .build(),
        )
        .plugin(tauri_plugin_websocket::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            get_identity,
            get_profile,
            update_profile,
            get_user_profile,
            get_users_batch,
            get_presence,
            set_presence,
            get_relay_ws_url,
            discover_acp_providers,
            discover_managed_agent_prereqs,
            sign_event,
            create_auth_event,
            get_channels,
            create_channel,
            get_channel_details,
            get_channel_members,
            update_channel,
            set_channel_topic,
            set_channel_purpose,
            archive_channel,
            unarchive_channel,
            delete_channel,
            add_channel_members,
            remove_channel_member,
            join_channel,
            leave_channel,
            get_feed,
            search_messages,
            send_channel_message,
            add_reaction,
            remove_reaction,
            get_event,
            list_tokens,
            mint_token,
            revoke_token,
            revoke_all_tokens,
            list_relay_agents,
            list_managed_agents,
            create_managed_agent,
            start_managed_agent,
            stop_managed_agent,
            delete_managed_agent,
            mint_managed_agent_token,
            get_managed_agent_log,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{percent_encode, ChannelInfo};

    #[test]
    fn channel_info_defaults_is_member_for_legacy_payloads() {
        let channel: ChannelInfo = serde_json::from_value(json!({
            "id": "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50",
            "name": "general",
            "channel_type": "stream",
            "visibility": "open",
            "description": "General discussion",
            "topic": null,
            "purpose": null,
            "member_count": 3,
            "last_message_at": null,
            "archived_at": null,
            "participants": [],
            "participant_pubkeys": []
        }))
        .expect("legacy payload should deserialize");

        assert!(channel.is_member);
    }

    #[test]
    fn percent_encode_leaves_unreserved_chars() {
        assert_eq!(
            percent_encode("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.~"),
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.~"
        );
    }

    #[test]
    fn percent_encode_escapes_unicode_and_reserved_chars() {
        assert_eq!(percent_encode("👍"), "%F0%9F%91%8D");
        assert_eq!(percent_encode("a/b?c"), "a%2Fb%3Fc");
    }
}
