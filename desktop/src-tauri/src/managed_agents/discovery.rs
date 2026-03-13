use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

use tauri::AppHandle;

use crate::managed_agents::{
    AcpProviderInfo, CommandAvailabilityInfo, SproutAdminMintTokenJsonOutput, DEFAULT_AGENT_ARG,
};

struct KnownAcpProvider {
    id: &'static str,
    label: &'static str,
    command: &'static str,
    default_args: &'static [&'static str],
}

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

pub fn admin_command() -> String {
    std::env::var("SPROUT_ADMIN_COMMAND").unwrap_or_else(|_| "sprout-admin".to_string())
}

pub fn default_token_scopes() -> Vec<String> {
    vec![
        "messages:read".to_string(),
        "messages:write".to_string(),
        "channels:read".to_string(),
    ]
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

    command.output().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            format!(
                "failed to run `{}`: command not found. Build the workspace binaries from TESTING.md or set SPROUT_ADMIN_COMMAND explicitly.",
                program.display()
            )
        } else {
            format!("failed to run `{}`: {error}", program.display())
        }
    })
}

fn output_error_detail(program: &Path, output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() { stderr } else { stdout };
    format!(
        "`{}` exited with {}: {detail}",
        program.display(),
        output.status
    )
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
        .find_map(|line| {
            line.strip_prefix("Token ID:")
                .map(|value| value.trim().to_string())
        })
        .ok_or_else(|| {
            "failed to parse legacy sprout-admin output: missing token id".to_string()
        })?;

    let api_token_index = contents
        .iter()
        .position(|line| line == "API Token:")
        .ok_or_else(|| {
            "failed to parse legacy sprout-admin output: missing API token label".to_string()
        })?;
    let api_token = contents
        .iter()
        .skip(api_token_index + 1)
        .find(|line| !line.ends_with(':'))
        .cloned()
        .ok_or_else(|| {
            "failed to parse legacy sprout-admin output: missing API token value".to_string()
        })?;

    Ok(SproutAdminMintTokenJsonOutput {
        token_id,
        name: name.to_string(),
        scopes: scopes.to_vec(),
        pubkey: pubkey.to_string(),
        private_key_nsec: None,
        api_token,
    })
}

pub fn run_sprout_admin_mint_token(
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
        .map_err(|error| format!("failed to parse sprout-admin JSON output: {error}"))?;

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
        return Err(
            "sprout-admin unexpectedly returned a private key for an existing pubkey".to_string(),
        );
    }

    Ok(parsed)
}
