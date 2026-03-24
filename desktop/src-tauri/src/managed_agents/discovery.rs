use std::path::{Path, PathBuf};
use std::process::Command;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use nostr::{EventBuilder, JsonUtil, Keys, Kind, Tag};
use reqwest::Method;
use sha2::{Digest, Sha256};
use tauri::AppHandle;

use crate::{
    app_state::AppState,
    managed_agents::{AcpProviderInfo, CommandAvailabilityInfo, DEFAULT_AGENT_ARG},
    models::MintTokenBody,
    relay::{relay_http_base_url, send_json_request},
};

struct KnownAcpProvider {
    id: &'static str,
    label: &'static str,
    command: &'static str,
    aliases: &'static [&'static str],
    default_args: &'static [&'static str],
    avatar_url: &'static str,
}

const GOOSE_AVATAR_URL: &str = "https://block.github.io/goose/img/logo_dark.png";
const CLAUDE_CODE_AVATAR_URL: &str = "https://anthropic.gallerycdn.vsassets.io/extensions/anthropic/claude-code/2.1.77/1773707456892/Microsoft.VisualStudio.Services.Icons.Default";
const CODEX_AVATAR_URL: &str = "https://openai.gallerycdn.vsassets.io/extensions/openai/chatgpt/26.5313.41514/1773706730621/Microsoft.VisualStudio.Services.Icons.Default";

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
        aliases: &[],
        default_args: &[DEFAULT_AGENT_ARG],
        avatar_url: GOOSE_AVATAR_URL,
    },
    KnownAcpProvider {
        id: "claude",
        label: "Claude Code",
        command: "claude-agent-acp",
        aliases: &["claude-code", "claudecode"],
        default_args: &[],
        avatar_url: CLAUDE_CODE_AVATAR_URL,
    },
    KnownAcpProvider {
        id: "codex",
        label: "Codex",
        command: "codex-acp",
        aliases: &[],
        default_args: &[],
        avatar_url: CODEX_AVATAR_URL,
    },
];

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

fn normalize_command_identity(command: &str) -> String {
    let normalized = command.trim().replace('\\', "/");
    let basename = normalized.rsplit('/').next().unwrap_or(normalized.as_str());
    let lower = basename
        .chars()
        .map(|character| match character {
            ' ' | '_' => '-',
            _ => character.to_ascii_lowercase(),
        })
        .collect::<String>();
    let lower = lower.strip_suffix(".exe").unwrap_or(&lower).to_string();

    if let Some(suffix) = std::env::consts::EXE_SUFFIX.strip_prefix('.') {
        return lower
            .strip_suffix(&format!(".{suffix}"))
            .unwrap_or(&lower)
            .to_string();
    }

    if !std::env::consts::EXE_SUFFIX.is_empty() {
        return lower
            .strip_suffix(std::env::consts::EXE_SUFFIX)
            .unwrap_or(&lower)
            .to_string();
    }

    lower
}

fn known_acp_provider(command: &str) -> Option<&'static KnownAcpProvider> {
    let normalized = normalize_command_identity(command);

    KNOWN_ACP_PROVIDERS.iter().find(|provider| {
        normalized == provider.id
            || normalized == normalize_command_identity(provider.command)
            || provider.aliases.iter().any(|alias| normalized == *alias)
    })
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

pub fn resolve_command(command: &str, app: Option<&AppHandle>) -> Option<PathBuf> {
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
        let Ok(output) = Command::new(shell).args(["-l", "-c", &which_cmd]).output() else {
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

pub fn command_availability(command: &str, app: Option<&AppHandle>) -> CommandAvailabilityInfo {
    let resolved_path = resolve_command(command, app).map(|path| path.display().to_string());
    CommandAvailabilityInfo {
        command: command.to_string(),
        available: resolved_path.is_some(),
        resolved_path,
    }
}

pub fn missing_command_message(command: &str, role: &str) -> String {
    if command_looks_like_path(command) {
        return format!("{role} `{command}` does not exist.");
    }

    format!(
        "{role} `{command}` was not found. Build the workspace binaries (`cargo build --release --workspace`) or add `target/release` to PATH as described in TESTING.md."
    )
}

pub fn discover_local_acp_providers() -> Vec<AcpProviderInfo> {
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

pub fn managed_agent_avatar_url(command: &str) -> Option<String> {
    let provider = known_acp_provider(command)?;
    Some(provider.avatar_url.to_string())
}

pub fn default_token_scopes() -> Vec<String> {
    vec![
        "messages:read".to_string(),
        "messages:write".to_string(),
        "channels:read".to_string(),
        "users:read".to_string(),
        "users:write".to_string(),
    ]
}

/// Mint an API token for `agent_keys` by signing a NIP-98 auth event with the
/// agent's own keypair and posting to `POST /api/tokens` on the relay.
///
/// The relay mints the token for the signing pubkey, so using the agent's keys
/// here ensures the token is bound to the agent's identity — not the desktop
/// user's. No `sprout-admin` binary or database access is required.
pub async fn mint_token_via_api(
    state: &AppState,
    agent_keys: &Keys,
    relay_url: &str,
    name: &str,
    scopes: &[String],
    owner_pubkey: Option<&str>,
) -> Result<String, String> {
    let http_base = relay_http_base_url(relay_url);
    let url = format!("{http_base}/api/tokens");

    let body = MintTokenBody {
        name,
        scopes,
        channel_ids: None,
        expires_in_days: None,
        owner_pubkey,
    };
    let body_bytes =
        serde_json::to_vec(&body).map_err(|e| format!("serialize mint body failed: {e}"))?;

    // Build NIP-98 auth header signed by the AGENT's keys (not the desktop user's).
    let payload_hash = format!("{:x}", Sha256::digest(&body_bytes));
    let forwarded_proto = if url.starts_with("http://") {
        "http"
    } else {
        "https"
    };
    let tags = vec![
        Tag::parse(vec!["u", &url]).map_err(|e| format!("url tag failed: {e}"))?,
        Tag::parse(vec!["method", "POST"]).map_err(|e| format!("method tag failed: {e}"))?,
        Tag::parse(vec!["payload", &payload_hash])
            .map_err(|e| format!("payload tag failed: {e}"))?,
    ];
    let event = EventBuilder::new(Kind::HttpAuth, "")
        .tags(tags)
        .sign_with_keys(agent_keys)
        .map_err(|e| format!("sign failed: {e}"))?;
    let auth_header = format!("Nostr {}", BASE64.encode(event.as_json().as_bytes()));

    let request = state
        .http_client
        .request(Method::POST, &url)
        .header("Authorization", auth_header)
        .header("Content-Type", "application/json")
        .header("X-Forwarded-Proto", forwarded_proto)
        .body(body_bytes);

    let response: crate::models::MintTokenResponse = send_json_request(request).await?;

    Ok(response.token)
}

#[cfg(test)]
mod tests {
    use super::{
        managed_agent_avatar_url, CLAUDE_CODE_AVATAR_URL, CODEX_AVATAR_URL, GOOSE_AVATAR_URL,
    };

    #[test]
    fn resolves_known_avatar_for_bare_command() {
        let avatar_url = managed_agent_avatar_url("goose").expect("goose avatar should resolve");

        assert_eq!(avatar_url, GOOSE_AVATAR_URL);
    }

    #[test]
    fn resolves_known_avatar_for_command_paths_and_aliases() {
        assert_eq!(
            managed_agent_avatar_url("/usr/local/bin/codex-acp"),
            Some(CODEX_AVATAR_URL.to_string())
        );
        assert_eq!(
            managed_agent_avatar_url("Claude Code"),
            Some(CLAUDE_CODE_AVATAR_URL.to_string())
        );
        assert_eq!(
            managed_agent_avatar_url(r"C:\Tools\claude-agent-acp.exe"),
            Some(CLAUDE_CODE_AVATAR_URL.to_string())
        );
    }

    #[test]
    fn returns_none_for_unknown_commands() {
        assert!(managed_agent_avatar_url("custom-agent").is_none());
    }
}
