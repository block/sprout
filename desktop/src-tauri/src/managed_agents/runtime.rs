use std::collections::HashMap;

use tauri::AppHandle;

use crate::{
    managed_agents::{
        append_log_marker, managed_agent_log_path, missing_command_message, normalize_agent_args,
        open_log_file, resolve_command, ManagedAgentProcess, ManagedAgentRecord,
        ManagedAgentSummary,
    },
    util::now_iso,
};

/// Binary name fragments for all known agent/harness processes that Sprout
/// may spawn. Used by `process_belongs_to_us()` and the orphan sweep to
/// identify processes we should clean up. Both hyphenated and underscored
/// variants are listed because macOS `proc_name()` and Linux `/proc/comm`
/// may report either form depending on how the binary was built.
pub(crate) const KNOWN_AGENT_BINARIES: &[&str] = &[
    "sprout-acp",
    "sprout_acp",
    "claude-agent-acp",
    "claude_agent_acp",
    "claude-code-acp",
    "claude_code_acp",
    "codex-acp",
    "codex_acp",
    "goose",
    "sprout-mcp",
    "sprout_mcp",
];

/// Check if a process name matches any of our known agent binaries.
fn name_matches_known_binary(name: &str) -> bool {
    KNOWN_AGENT_BINARIES
        .iter()
        .any(|binary| name.contains(binary))
}

#[cfg(unix)]
pub(crate) fn process_is_running(pid: u32) -> bool {
    // Use libc::kill with signal 0 instead of forking a subprocess.
    // Returns true only if the process exists AND we can signal it.
    // Returns false for non-existent PIDs (ESRCH) and PIDs owned by
    // other users (EPERM) — callers should not interact with those.
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(not(unix))]
pub(crate) fn process_is_running(_pid: u32) -> bool {
    false
}

/// Check if a PID belongs to a known agent process we spawned.
/// Returns false for recycled PIDs that now belong to other processes.
#[cfg(target_os = "macos")]
pub(crate) fn process_belongs_to_us(pid: u32) -> bool {
    // Use proc_name() from libproc to get the process name without spawning
    // a subprocess.
    extern "C" {
        fn proc_name(pid: libc::c_int, buffer: *mut libc::c_void, buffersize: u32) -> libc::c_int;
    }
    let mut buf = [0u8; 1024];
    let len = unsafe {
        proc_name(
            pid as i32,
            buf.as_mut_ptr() as *mut libc::c_void,
            buf.len() as u32,
        )
    };
    if len <= 0 {
        return false;
    }
    let name = String::from_utf8_lossy(&buf[..len as usize]);
    name_matches_known_binary(&name)
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) fn process_belongs_to_us(pid: u32) -> bool {
    // On Linux, read /proc/<pid>/comm
    std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .map(|name| name_matches_known_binary(name.trim()))
        .unwrap_or(false)
}

#[cfg(not(unix))]
pub(crate) fn process_belongs_to_us(_pid: u32) -> bool {
    false
}

#[cfg(unix)]
pub(crate) fn terminate_process(pid: u32) -> Result<(), String> {
    // The child was spawned with process_group(0), so pid == pgid.
    // Kill the entire process group to avoid orphaning MCP servers
    // and agent subprocesses.
    let pgid = -(pid as i32);

    // Try graceful shutdown first (SIGTERM to the group).
    if unsafe { libc::kill(pgid, libc::SIGTERM) } != 0 {
        // ESRCH means the process is already gone — that's fine.
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() != Some(libc::ESRCH) && process_is_running(pid) {
            return Err(format!("failed to terminate process group {pid}: {err}"));
        }
        return Ok(());
    }

    // Wait up to 1s for graceful exit.
    for _ in 0..10 {
        if !process_is_running(pid) {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // Escalate to SIGKILL on the entire group.
    if unsafe { libc::kill(pgid, libc::SIGKILL) } != 0 {
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() != Some(libc::ESRCH) && process_is_running(pid) {
            return Err(format!("failed to kill process group {pid}: {err}"));
        }
    }

    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn terminate_process(_pid: u32) -> Result<(), String> {
    Err("managed agent shutdown after app restart is only supported on Unix".to_string())
}

/// Send SIGTERM to all given PIDs, wait 500ms, then SIGKILL any survivors.
#[cfg(unix)]
fn sigterm_then_sigkill(pids: &[i32]) {
    for &pid in pids {
        unsafe {
            libc::kill(pid, libc::SIGTERM);
        }
    }

    std::thread::sleep(std::time::Duration::from_millis(500));

    for &pid in pids {
        if process_is_running(pid as u32) {
            unsafe {
                libc::kill(pid, libc::SIGKILL);
            }
        }
    }
}

/// Enumerate all PIDs on the system that are owned by the current user and
/// match a known agent binary name, excluding `skip_pids`.
#[cfg(target_os = "macos")]
fn enumerate_orphaned_agent_pids(skip_pids: &[u32]) -> Vec<i32> {
    extern "C" {
        fn proc_listallpids(buffer: *mut libc::pid_t, buffersize: libc::c_int) -> libc::c_int;
    }

    let count = unsafe { proc_listallpids(std::ptr::null_mut(), 0) };
    if count <= 0 {
        return Vec::new();
    }

    let mut pids = vec![0i32; (count as usize) * 2]; // over-allocate for safety
    let actual =
        unsafe { proc_listallpids(pids.as_mut_ptr(), (pids.len() * size_of::<i32>()) as i32) };
    if actual <= 0 {
        return Vec::new();
    }
    pids.truncate(actual as usize);

    pids.into_iter()
        .filter(|&pid| {
            pid > 1 && {
                let pid_u32 = pid as u32;
                !skip_pids.contains(&pid_u32)
                    && process_is_running(pid_u32)
                    && process_belongs_to_us(pid_u32)
            }
        })
        .collect()
}

/// Enumerate all PIDs on the system that are owned by the current user and
/// match a known agent binary name, excluding `skip_pids`.
#[cfg(all(unix, not(target_os = "macos")))]
fn enumerate_orphaned_agent_pids(skip_pids: &[u32]) -> Vec<i32> {
    let my_uid = unsafe { libc::getuid() };
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };

    entries
        .flatten()
        .filter_map(|entry| {
            let pid = entry.file_name().to_str()?.parse::<u32>().ok()?;
            if pid <= 1 || skip_pids.contains(&pid) {
                return None;
            }
            let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
            let is_ours = status.lines().any(|line| {
                line.starts_with("Uid:")
                    && line
                        .split_whitespace()
                        .nth(1)
                        .and_then(|uid| uid.parse::<u32>().ok())
                        == Some(my_uid)
            });
            if is_ours && process_belongs_to_us(pid) {
                Some(pid as i32)
            } else {
                None
            }
        })
        .collect()
}

/// Sweep all processes owned by the current user and kill any whose binary
/// name matches a known agent binary. This catches processes that escaped
/// process-group kills (e.g. via `setsid()`) or weren't tracked in records.
///
/// `skip_pids` contains PIDs we've already handled — no need to signal them
/// again.
#[cfg(unix)]
pub(crate) fn sweep_orphaned_agent_processes(skip_pids: &[u32]) {
    let orphans = enumerate_orphaned_agent_pids(skip_pids);
    if !orphans.is_empty() {
        sigterm_then_sigkill(&orphans);
    }
}

#[cfg(not(unix))]
pub(crate) fn sweep_orphaned_agent_processes(_skip_pids: &[u32]) {
    // No-op on non-Unix platforms.
}

/// Kill stale agent processes from a previous session whose PID is still alive
/// but not tracked in the current `runtimes` map. Updates the record fields and
/// returns `true` if any records were modified.
pub fn kill_stale_tracked_processes(
    records: &mut [ManagedAgentRecord],
    runtimes: &HashMap<String, ManagedAgentProcess>,
) -> bool {
    use crate::managed_agents::BackendKind;

    let mut changed = false;
    for record in records.iter_mut() {
        if record.backend != BackendKind::Local {
            continue;
        }
        let Some(pid) = record.runtime_pid else {
            continue;
        };
        if !runtimes.contains_key(&record.pubkey) {
            if process_belongs_to_us(pid) {
                let _ = terminate_process(pid);
            }
            record.runtime_pid = None;
            record.last_stopped_at = Some(crate::util::now_iso());
            record.updated_at = crate::util::now_iso();
            changed = true;
        }
    }
    changed
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
        // Two-axis status model for remote agents:
        //
        //   Control-plane (this field): "deployed" = provider has been invoked and
        //   returned a backend_agent_id. "not_deployed" = no deploy call yet (or it
        //   failed). This axis tracks whether infrastructure *exists*, not whether
        //   the process is currently running.
        //
        //   Live axis (relay presence, polled by frontend): online/away/offline.
        //   Shown as a PresenceDot next to the agent name. This is the real-time
        //   signal for whether the harness is connected.
        //
        // After !shutdown the agent goes offline (presence) but stays "deployed"
        // (infrastructure still exists). This is intentional — the provider may
        // have allocated a VM/container that persists across process restarts.
        // A future provider `undeploy` operation (v2) will handle teardown.
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
        idle_timeout_seconds: record.idle_timeout_seconds,
        max_turn_duration_seconds: record.max_turn_duration_seconds,
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
    let agent_args = normalize_agent_args(&record.agent_command, record.agent_args.clone());
    let resolved_acp_command = resolve_command(&record.acp_command, Some(app))
        .ok_or_else(|| missing_command_message(&record.acp_command, "ACP harness command"))?;
    let resolved_mcp_command = resolve_command(&record.mcp_command, Some(app))
        .ok_or_else(|| missing_command_message(&record.mcp_command, "MCP server command"))?;

    let mut command = std::process::Command::new(&resolved_acp_command);
    if let Some(home) = super::default_agent_workdir() {
        command.current_dir(home);
    }
    command.stdin(std::process::Stdio::null());
    command.stdout(std::process::Stdio::from(stdout));
    command.stderr(std::process::Stdio::from(stderr));
    command.env("SPROUT_PRIVATE_KEY", &record.private_key_nsec);
    command.env("SPROUT_RELAY_URL", &record.relay_url);
    command.env("SPROUT_ACP_AGENT_COMMAND", &record.agent_command);
    command.env("SPROUT_ACP_AGENT_ARGS", agent_args.join(","));
    command.env("SPROUT_ACP_MCP_COMMAND", &resolved_mcp_command);
    // Timeout configuration: always set both IDLE_TIMEOUT and the deprecated TURN_TIMEOUT
    // so older harness binaries (which only read TURN_TIMEOUT) still get a value.
    if let Some(idle) = record.idle_timeout_seconds {
        command.env("SPROUT_ACP_IDLE_TIMEOUT", idle.to_string());
        // Mirror to deprecated var for older harness binaries.
        command.env("SPROUT_ACP_TURN_TIMEOUT", idle.to_string());
    } else {
        command.env(
            "SPROUT_ACP_TURN_TIMEOUT",
            record.turn_timeout_seconds.to_string(),
        );
    }

    let max_dur = record
        .max_turn_duration_seconds
        .unwrap_or(super::types::DEFAULT_AGENT_MAX_TURN_DURATION_SECONDS);
    command.env("SPROUT_ACP_MAX_TURN_DURATION", max_dur.to_string());
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

    // Spawn the harness in its own process group so we can kill the entire
    // tree (harness + MCP servers + agent subprocesses) on shutdown.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
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

    // Kill the entire process group (harness + MCP servers + agent
    // subprocesses). The child was spawned with process_group(0), so
    // its PID == its PGID.
    terminate_process(runtime.child.id())?;
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
