use std::{collections::HashMap, process::Command};

use tauri::AppHandle;

use crate::{
    managed_agents::{
        append_log_marker, managed_agent_log_path, missing_command_message, open_log_file,
        resolve_command, ManagedAgentProcess, ManagedAgentRecord, ManagedAgentSummary,
        DEFAULT_AGENT_ARG,
    },
    util::now_iso,
};

#[cfg(unix)]
fn process_is_running(pid: u32) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn process_is_running(_pid: u32) -> bool {
    false
}

#[cfg(unix)]
fn terminate_process(pid: u32) -> Result<(), String> {
    let pid_arg = pid.to_string();
    let status = Command::new("kill")
        .arg("-TERM")
        .arg(&pid_arg)
        .status()
        .map_err(|error| format!("failed to terminate process {pid}: {error}"))?;
    if !status.success() && process_is_running(pid) {
        return Err(format!("failed to terminate process {pid}: signal was rejected"));
    }

    for _ in 0..10 {
        if !process_is_running(pid) {
            return Ok(());
        }

        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    let kill_status = Command::new("kill")
        .arg("-KILL")
        .arg(&pid_arg)
        .status()
        .map_err(|error| format!("failed to kill process {pid}: {error}"))?;
    if !kill_status.success() && process_is_running(pid) {
        return Err(format!("failed to kill process {pid}: signal was rejected"));
    }

    Ok(())
}

#[cfg(not(unix))]
fn terminate_process(_pid: u32) -> Result<(), String> {
    Err("managed agent shutdown after app restart is only supported on Unix".to_string())
}

pub fn sync_managed_agent_processes(
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
            record.runtime_pid = None;
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

    for record in records.iter_mut() {
        if runtimes.contains_key(&record.pubkey) {
            continue;
        }

        let Some(pid) = record.runtime_pid else {
            continue;
        };

        if process_is_running(pid) {
            continue;
        }

        record.runtime_pid = None;
        record.updated_at = now_iso();
        if record.last_stopped_at.is_none() {
            record.last_stopped_at = Some(now_iso());
        }
        changed = true;
    }

    changed
}

pub fn build_managed_agent_summary(
    app: &AppHandle,
    record: &ManagedAgentRecord,
    runtimes: &HashMap<String, ManagedAgentProcess>,
) -> Result<ManagedAgentSummary, String> {
    use crate::managed_agents::BackendKind;

    let (status, pid, log_path) = if record.backend != BackendKind::Local {
        // Provider-backed (remote) agents: no local process to check.
        // Status is "deployed" if we have a backend_agent_id, "not_deployed" otherwise.
        // Actual online/offline comes from relay presence (polled separately by the
        // frontend). The desktop does NOT track remote stop state — presence is truth.
        let status = if record.backend_agent_id.is_some() {
            "deployed".to_string()
        } else {
            "not_deployed".to_string()
        };
        (status, None, String::new())
    } else {
        let persisted_pid = record.runtime_pid.filter(|pid| process_is_running(*pid));
        if let Some(runtime) = runtimes.get(&record.pubkey) {
            (
                "running".to_string(),
                Some(runtime.child.id()),
                runtime.log_path.display().to_string(),
            )
        } else if let Some(pid) = persisted_pid {
            (
                "running".to_string(),
                Some(pid),
                managed_agent_log_path(app, &record.pubkey)?
                    .display()
                    .to_string(),
            )
        } else {
            (
                "stopped".to_string(),
                None,
                managed_agent_log_path(app, &record.pubkey)?
                    .display()
                    .to_string(),
            )
        }
    };

    Ok(ManagedAgentSummary {
        pubkey: record.pubkey.clone(),
        name: record.name.clone(),
        persona_id: record.persona_id.clone(),
        relay_url: record.relay_url.clone(),
        acp_command: record.acp_command.clone(),
        agent_command: record.agent_command.clone(),
        agent_args: record.agent_args.clone(),
        mcp_command: record.mcp_command.clone(),
        turn_timeout_seconds: record.turn_timeout_seconds,
        parallelism: record.parallelism,
        system_prompt: record.system_prompt.clone(),
        model: record.model.clone(),
        has_api_token: record.api_token.is_some(),
        backend: record.backend.clone(),
        backend_agent_id: record.backend_agent_id.clone(),
        status,
        pid,
        created_at: record.created_at.clone(),
        updated_at: record.updated_at.clone(),
        last_started_at: record.last_started_at.clone(),
        last_stopped_at: record.last_stopped_at.clone(),
        last_exit_code: record.last_exit_code,
        last_error: record.last_error.clone(),
        start_on_app_launch: record.start_on_app_launch,
        log_path,
    })
}

pub fn find_managed_agent_mut<'a>(
    records: &'a mut [ManagedAgentRecord],
    pubkey: &str,
) -> Result<&'a mut ManagedAgentRecord, String> {
    records
        .iter_mut()
        .find(|record| record.pubkey == pubkey)
        .ok_or_else(|| format!("agent {pubkey} not found"))
}

pub fn start_managed_agent_process(
    app: &AppHandle,
    record: &mut ManagedAgentRecord,
    runtimes: &mut HashMap<String, ManagedAgentProcess>,
) -> Result<(), String> {
    if let Some(runtime) = runtimes.get_mut(&record.pubkey) {
        if runtime
            .child
            .try_wait()
            .map_err(|error| format!("failed to inspect running process: {error}"))?
            .is_none()
        {
            return Ok(());
        }

        runtimes.remove(&record.pubkey);
    }

    if let Some(pid) = record.runtime_pid {
        if process_is_running(pid) {
            record.updated_at = now_iso();
            record.last_error = None;
            return Ok(());
        }

        record.runtime_pid = None;
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
        .map_err(|error| format!("failed to clone log handle: {error}"))?;
    let agent_args = if record.agent_args.is_empty() {
        vec![DEFAULT_AGENT_ARG.to_string()]
    } else {
        record.agent_args.clone()
    };
    let resolved_acp_command = resolve_command(&record.acp_command, Some(app))
        .ok_or_else(|| missing_command_message(&record.acp_command, "ACP harness command"))?;
    let resolved_mcp_command = resolve_command(&record.mcp_command, Some(app))
        .ok_or_else(|| missing_command_message(&record.mcp_command, "MCP server command"))?;

    let mut command = std::process::Command::new(&resolved_acp_command);
    command.stdin(std::process::Stdio::null());
    command.stdout(std::process::Stdio::from(stdout));
    command.stderr(std::process::Stdio::from(stderr));
    command.env("SPROUT_PRIVATE_KEY", &record.private_key_nsec);
    command.env("SPROUT_RELAY_URL", &record.relay_url);
    command.env("SPROUT_ACP_AGENT_COMMAND", &record.agent_command);
    command.env("SPROUT_ACP_AGENT_ARGS", agent_args.join(","));
    command.env("SPROUT_ACP_MCP_COMMAND", &resolved_mcp_command);
    command.env(
        "SPROUT_ACP_TURN_TIMEOUT",
        record.turn_timeout_seconds.to_string(),
    );
    command.env("SPROUT_ACP_AGENTS", record.parallelism.to_string());
    command.env(
        "GOOSE_MODE",
        std::env::var("GOOSE_MODE").unwrap_or_else(|_| "auto".to_string()),
    );
    if let Some(system_prompt) = &record.system_prompt {
        command.env("SPROUT_ACP_SYSTEM_PROMPT", system_prompt);
    } else {
        command.env_remove("SPROUT_ACP_SYSTEM_PROMPT");
    }
    if let Some(model) = &record.model {
        command.env("SPROUT_ACP_MODEL", model);
    } else {
        command.env_remove("SPROUT_ACP_MODEL");
    }
    command.env_remove("SPROUT_ACP_PRIVATE_KEY");
    command.env_remove("SPROUT_ACP_API_TOKEN");

    if let Some(token) = &record.api_token {
        command.env("SPROUT_API_TOKEN", token);
    } else {
        command.env_remove("SPROUT_API_TOKEN");
    }

    let child = command.spawn().map_err(|error| {
        format!(
            "failed to spawn `{}` for agent {}: {error}",
            resolved_acp_command.display(),
            record.name
        )
    })?;

    let now = now_iso();
    record.updated_at = now.clone();
    record.runtime_pid = Some(child.id());
    record.last_started_at = Some(now);
    record.last_stopped_at = None;
    record.last_exit_code = None;
    record.last_error = None;

    runtimes.insert(
        record.pubkey.clone(),
        ManagedAgentProcess { child, log_path },
    );
    Ok(())
}

pub fn stop_managed_agent_process(
    record: &mut ManagedAgentRecord,
    runtimes: &mut HashMap<String, ManagedAgentProcess>,
) -> Result<(), String> {
    let Some(mut runtime) = runtimes.remove(&record.pubkey) else {
        if let Some(pid) = record.runtime_pid {
            if process_is_running(pid) {
                terminate_process(pid)?;
            }

            let now = now_iso();
            record.runtime_pid = None;
            record.updated_at = now.clone();
            record.last_stopped_at = Some(now);
            record.last_exit_code = None;
            record.last_error = None;
        }
        return Ok(());
    };

    let _ = runtime.child.kill();
    let status = runtime
        .child
        .wait()
        .map_err(|error| format!("failed to wait for agent shutdown: {error}"))?;
    let now = now_iso();
    record.runtime_pid = None;
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
