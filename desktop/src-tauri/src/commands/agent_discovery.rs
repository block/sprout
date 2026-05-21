use tauri::{AppHandle, State};

use crate::{
    app_state::AppState,
    managed_agents::{
        command_availability, discover_local_acp_providers, AcpProviderCatalogEntry, AcpProviderInfo,
        DiscoverManagedAgentPrereqsRequest, InstallRuntimeResult, InstallStepResult,
        ManagedAgentPrereqsInfo, RelayAgentInfo, DEFAULT_ACP_COMMAND, DEFAULT_MCP_COMMAND,
    },
    nostr_convert,
    relay::query_relay,
};

#[tauri::command]
pub fn discover_acp_providers() -> Vec<AcpProviderInfo> {
    discover_local_acp_providers()
}

#[tauri::command]
pub fn discover_all_acp_providers() -> Vec<AcpProviderCatalogEntry> {
    crate::managed_agents::discover_all_acp_providers()
}

#[tauri::command]
pub async fn install_acp_runtime(provider_id: String) -> Result<InstallRuntimeResult, String> {
    tokio::task::spawn_blocking(move || install_acp_runtime_blocking(&provider_id))
        .await
        .map_err(|e| format!("install task panicked: {e}"))?
}

fn install_acp_runtime_blocking(provider_id: &str) -> Result<InstallRuntimeResult, String> {
    let provider = crate::managed_agents::known_acp_provider(provider_id)
        .ok_or_else(|| format!("unknown provider: {provider_id}"))?;

    let mut steps = Vec::new();

    // Phase 1: Install CLI if missing and commands are available.
    if let Some(cli) = provider.underlying_cli {
        if crate::managed_agents::resolve_command(cli, None).is_none() {
            for cmd in provider.cli_install_commands {
                let result = run_install_command("cli", cmd);
                let success = result.success;
                steps.push(result);
                if !success {
                    return Ok(InstallRuntimeResult {
                        success: false,
                        steps,
                    });
                }
            }
        }
    }

    // Phase 2: Install adapter if missing and commands are available.
    let adapter_found = provider
        .commands
        .iter()
        .any(|cmd| crate::managed_agents::resolve_command(cmd, None).is_some());
    if !adapter_found {
        for cmd in provider.adapter_install_commands {
            let result = run_install_command("adapter", cmd);
            let success = result.success;
            steps.push(result);
            if !success {
                return Ok(InstallRuntimeResult {
                    success: false,
                    steps,
                });
            }
        }
    }

    // Clear the resolve cache so the next discovery picks up new binaries.
    crate::managed_agents::clear_resolve_cache();

    Ok(InstallRuntimeResult {
        success: true,
        steps,
    })
}

fn run_install_command(step: &str, command: &str) -> InstallStepResult {
    let shell_path = crate::managed_agents::login_shell_path();
    let shell = if std::path::Path::new("/bin/zsh").exists() {
        "/bin/zsh"
    } else {
        "/bin/bash"
    };

    let mut cmd = std::process::Command::new(shell);
    cmd.args(["-l", "-c", command]);

    if let Some(ref path) = shell_path {
        cmd.env("PATH", path);
    }

    let mut child = match cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            return InstallStepResult {
                step: step.to_string(),
                command: command.to_string(),
                success: false,
                stdout: String::new(),
                stderr: format!("failed to spawn shell: {e}"),
                exit_code: None,
            };
        }
    };

    // 5-minute timeout for install commands.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(300);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = child
                    .stdout
                    .take()
                    .map(|mut s| {
                        let mut buf = String::new();
                        let _ = std::io::Read::read_to_string(&mut s, &mut buf);
                        buf
                    })
                    .unwrap_or_default();
                let stderr_raw = child
                    .stderr
                    .take()
                    .map(|mut s| {
                        let mut buf = String::new();
                        let _ = std::io::Read::read_to_string(&mut s, &mut buf);
                        buf
                    })
                    .unwrap_or_default();
                let stderr = truncate_output(stderr_raw);
                return InstallStepResult {
                    step: step.to_string(),
                    command: command.to_string(),
                    success: status.success(),
                    stdout: truncate_output(stdout),
                    stderr,
                    exit_code: status.code(),
                };
            }
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return InstallStepResult {
                        step: step.to_string(),
                        command: command.to_string(),
                        success: false,
                        stdout: String::new(),
                        stderr: "install command timed out after 5 minutes".to_string(),
                        exit_code: None,
                    };
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            Err(e) => {
                return InstallStepResult {
                    step: step.to_string(),
                    command: command.to_string(),
                    success: false,
                    stdout: String::new(),
                    stderr: format!("failed to check process status: {e}"),
                    exit_code: None,
                };
            }
        }
    }
}

/// Cap output at 2 KB to avoid flooding the UI with large error dumps.
fn truncate_output(s: String) -> String {
    if s.len() > 2048 {
        format!("{}... (truncated)", &s[..2048])
    } else {
        s
    }
}

#[tauri::command]
pub fn discover_managed_agent_prereqs(
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

    ManagedAgentPrereqsInfo {
        acp: command_availability(acp_command, Some(&app)),
        mcp: command_availability(mcp_command, Some(&app)),
    }
}

#[tauri::command]
pub async fn list_relay_agents(state: State<'_, AppState>) -> Result<Vec<RelayAgentInfo>, String> {
    // Query kind:10100 agent profile events from the relay.
    let events = query_relay(
        &state,
        &[serde_json::json!({
            "kinds": [10100],
        })],
    )
    .await?;

    // The convert helper returns `{"agents": [...]}`. Extract and re-deserialize
    // into the strongly-typed `Vec<RelayAgentInfo>` the frontend expects.
    let value = nostr_convert::agents_from_events(&events);
    let agents = value
        .get("agents")
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));
    serde_json::from_value(agents).map_err(|e| format!("agent parse failed: {e}"))
}
