use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

/// Invoke a provider binary: write JSON to stdin, read JSON from stdout.
///
/// Reader threads stream lines/chunks over channels so the caller can receive
/// data as it arrives and time-box the wait. No `read_to_end` — if a provider
/// daemonizes or leaves descendants holding pipes open, the caller still gets
/// all data written before the child exited and returns without leaking threads
/// (the readers drop naturally when the sender is gone and the pipe closes or
/// the desktop process exits).
pub fn invoke_provider(
    binary: &Path,
    request: &serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    let request_bytes =
        format!("{}\n", serde_json::to_string(request).map_err(|e| e.to_string())?);

    let mut child = std::process::Command::new(binary)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn {}: {e}", binary.display()))?;

    // Write request and close stdin immediately so the provider sees EOF.
    let stdin_result = if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(request_bytes.as_bytes())
    } else {
        Ok(())
    };

    // Stream stdout lines over a channel. The reader thread sends each line
    // as it arrives, so the caller can capture the JSON response without
    // waiting for EOF. If a descendant holds the pipe open after the provider
    // exits, the thread blocks on the next read — but the caller already has
    // the response and proceeds. The thread is not joined; it terminates when
    // the pipe eventually closes or the process exits.
    let (stdout_tx, stdout_rx) = mpsc::channel::<String>();
    if let Some(stdout) = child.stdout.take() {
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(l) => {
                        if stdout_tx.send(l).is_err() {
                            break; // receiver dropped
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }

    // Drain stderr into a bounded buffer via channel. Same non-blocking
    // pattern — the thread is not joined; it dies when the pipe closes.
    let (stderr_tx, stderr_rx) = mpsc::channel::<Vec<u8>>();
    if let Some(stderr) = child.stderr.take() {
        std::thread::spawn(move || {
            let mut buf = vec![0u8; 8192];
            let mut reader = BufReader::new(stderr);
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if stderr_tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }

    // Bail early if stdin write failed — child may be in a bad state.
    if let Err(e) = stdin_result {
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!("stdin write failed: {e}"));
    }

    // Poll try_wait with a deadline, collecting stdout lines as they arrive.
    // Once the child exits we drain any remaining buffered lines.
    let timeout_secs = timeout.as_secs();
    let deadline = std::time::Instant::now() + timeout;
    let mut stdout_lines: Vec<String> = Vec::new();
    let mut exit_status = None;

    loop {
        // Drain any stdout lines available right now (non-blocking).
        while let Ok(line) = stdout_rx.try_recv() {
            stdout_lines.push(line);
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                exit_status = Some(status);
                break;
            }
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("provider timed out after {timeout_secs}s"));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("wait error: {e}"));
            }
        }
    }

    // Drain remaining stdout lines buffered between last poll and child exit.
    // Brief timeout: the child has exited, so the reader should finish quickly
    // unless a descendant holds the pipe. 2s is generous.
    let drain_deadline = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < drain_deadline {
        match stdout_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(line) => stdout_lines.push(line),
            Err(mpsc::RecvTimeoutError::Timeout) => break,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    // Collect stderr (non-blocking drain of whatever arrived).
    let mut stderr_bytes = Vec::new();
    while let Ok(chunk) = stderr_rx.try_recv() {
        stderr_bytes.extend_from_slice(&chunk);
        if stderr_bytes.len() > 65536 {
            break; // cap stderr capture
        }
    }

    let stderr = String::from_utf8_lossy(&stderr_bytes);
    let stderr_redacted = redact_secrets(&stderr);

    let exit_info = exit_status
        .map(|s| {
            s.code()
                .map(|c| format!("exit code {c}"))
                .unwrap_or_else(|| "killed by signal".to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());

    // Fail on non-zero exit regardless of stdout content. A provider that
    // crashes mid-deploy may flush partial JSON before dying — trusting that
    // output would be worse than surfacing the failure.
    let exited_ok = exit_status.map_or(false, |s| s.success());
    if !exited_ok {
        let stderr_snippet = &stderr_redacted[..stderr_redacted.len().min(4096)];
        if stderr_snippet.is_empty() {
            return Err(format!("provider failed ({exit_info}, empty stderr)"));
        } else {
            return Err(format!(
                "provider failed ({exit_info}). stderr: {stderr_snippet}"
            ));
        }
    }

    let response: serde_json::Value = stdout_lines
        .iter()
        .find_map(|line| serde_json::from_str(line).ok())
        .ok_or_else(|| {
            let stderr_snippet = &stderr_redacted[..stderr_redacted.len().min(4096)];
            if stderr_snippet.is_empty() {
                format!("provider produced no JSON response ({exit_info}, empty stderr)")
            } else {
                format!(
                    "provider produced no JSON response ({exit_info}). stderr: {stderr_snippet}"
                )
            }
        })?;

    if response.get("ok").and_then(|v| v.as_bool()) == Some(false) {
        let error = response["error"].as_str().unwrap_or("unknown error");
        return Err(redact_secrets(error));
    }

    Ok(response)
}

/// Split a config key into lowercase words on `_`, `-`, `.`, and camelCase boundaries.
///
/// Handles acronyms: consecutive uppercase runs stay together until a lowercase follows.
/// "apiKey" → ["api", "key"], "apiKEY" → ["api", "key"], "APIKey" → ["api", "key"],
/// "access_token" → ["access", "token"], "keyboard" → ["keyboard"],
/// "clientSecret" → ["client", "secret"].
fn split_config_key(key: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = key.chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        if ch == '_' || ch == '-' || ch == '.' {
            if !current.is_empty() {
                words.push(current.to_lowercase());
                current.clear();
            }
        } else if ch.is_uppercase() {
            // Start a new word on: (a) transition from lowercase to uppercase, or
            // (b) uppercase followed by lowercase (end of acronym run, e.g. "APIKey" → "API" + "Key").
            let prev_lower = !current.is_empty() && current.chars().last().map_or(false, |c| c.is_lowercase());
            let acronym_end = !current.is_empty()
                && current.chars().last().map_or(false, |c| c.is_uppercase())
                && chars.get(i + 1).map_or(false, |c| c.is_lowercase());
            if prev_lower || acronym_end {
                words.push(current.to_lowercase());
                current.clear();
            }
            current.push(ch);
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        words.push(current.to_lowercase());
    }
    words
}

fn redact_secrets(s: &str) -> String {
    let mut result = s.to_string();
    for prefix in &["nsec1", "sprt_tok_"] {
        while let Some(pos) = result.find(prefix) {
            let end = result[pos..]
                .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
                .map(|i| pos + i)
                .unwrap_or(result.len());
            result.replace_range(pos..end, "[REDACTED]");
        }
    }
    result
}

/// Deploy an agent via provider binary. Returns the provider-assigned agent_id.
///
/// `request_id` is included for provider-side logging/correlation but is not
/// validated in the response — the stdin→stdout exchange is 1:1 per process.
pub fn provider_deploy(
    binary: &Path,
    agent: &serde_json::Value,
    provider_config: &serde_json::Value,
) -> Result<String, String> {
    let request = serde_json::json!({
        "op": "deploy",
        "request_id": uuid::Uuid::new_v4().to_string(),
        "agent": agent,
        "provider_config": provider_config,
    });
    let resp = invoke_provider(binary, &request, Duration::from_secs(600))?;
    resp["agent_id"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| "deploy response missing agent_id".to_string())
}

/// Validate provider_config: flat object, scalar values, no secret-like keys.
pub fn validate_provider_config(config: &serde_json::Value) -> Result<(), String> {
    let obj = config
        .as_object()
        .ok_or("provider_config must be a JSON object")?;
    if obj.len() > 20 {
        return Err("provider_config: max 20 fields".to_string());
    }
    let json_str = serde_json::to_string(config).unwrap_or_default();
    if json_str.len() > 65536 {
        return Err("provider_config: max 64KB".to_string());
    }
    // Split on separators AND camelCase boundaries, then check each word.
    // Catches: api_key, apiKey, access-token, clientSecret, etc.
    // Allows: keyboard, monkey_wrench (no forbidden word as a segment).
    let forbidden = ["secret", "password", "token", "key", "credential"];
    for (k, v) in obj {
        let words = split_config_key(k);
        for f in &forbidden {
            if words.iter().any(|w| w == f) {
                return Err(format!("provider_config: key '{}' looks like a secret", k));
            }
        }
        if v.is_object() || v.is_array() {
            return Err(format!(
                "provider_config: value for '{}' must be a scalar",
                k
            ));
        }
    }
    Ok(())
}

/// Enumerate PATH for sprout-backend-* executables. Returns (id, path) pairs.
/// Only includes files that are executable. Does NOT execute any binaries.
pub fn discover_provider_candidates() -> Vec<(String, PathBuf)> {
    let prefix = "sprout-backend-";
    let mut seen = std::collections::HashSet::new();
    let mut results = Vec::new();

    let path_var = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path_var) {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(id) = name.strip_prefix(prefix) {
                if !id.is_empty() && !seen.contains(&name) && is_executable(&entry.path()) {
                    seen.insert(name.clone());
                    results.push((id.to_string(), entry.path()));
                }
            }
        }
    }
    results
}

/// Check if a file is executable (Unix: mode bits; other platforms: always true).
fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        true
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendProviderInfo {
    pub id: String,
    pub binary_path: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_secrets_replaces_nsec() {
        let s = "key=nsec1abc123def456 other";
        let r = redact_secrets(s);
        assert!(r.contains("[REDACTED]"));
        assert!(!r.contains("nsec1abc123def456"));
    }

    #[test]
    fn redact_secrets_replaces_token() {
        let s = r#"{"token":"sprt_tok_xyz789"}"#;
        let r = redact_secrets(s);
        assert!(r.contains("[REDACTED]"));
        assert!(!r.contains("sprt_tok_xyz789"));
    }

    #[test]
    fn validate_provider_config_rejects_secret_key() {
        let cfg = serde_json::json!({"api_key": "val"});
        assert!(validate_provider_config(&cfg).is_err());
    }

    #[test]
    fn validate_provider_config_rejects_nested() {
        let cfg = serde_json::json!({"region": {"us": "east"}});
        assert!(validate_provider_config(&cfg).is_err());
    }

    #[test]
    fn validate_provider_config_accepts_scalars() {
        let cfg = serde_json::json!({"region": "us-east-1", "tier": "standard"});
        assert!(validate_provider_config(&cfg).is_ok());
    }

    #[test]
    fn validate_provider_config_allows_key_as_substring() {
        // "keyboard", "monkey" contain "key" as substring but not as a word segment.
        let cfg = serde_json::json!({"keyboard_layout": "us", "monkey_wrench": "tight"});
        assert!(validate_provider_config(&cfg).is_ok());
    }

    #[test]
    fn validate_provider_config_rejects_camel_case_secrets() {
        assert!(validate_provider_config(&serde_json::json!({"apiKey": "val"})).is_err());
        assert!(validate_provider_config(&serde_json::json!({"accessToken": "val"})).is_err());
        assert!(validate_provider_config(&serde_json::json!({"clientSecret": "val"})).is_err());
        // ALL-CAPS variants
        assert!(validate_provider_config(&serde_json::json!({"apiKEY": "val"})).is_err());
        assert!(validate_provider_config(&serde_json::json!({"accessTOKEN": "val"})).is_err());
    }

    #[test]
    fn split_config_key_handles_all_styles() {
        assert_eq!(split_config_key("apiKey"), vec!["api", "key"]);
        assert_eq!(split_config_key("access_token"), vec!["access", "token"]);
        assert_eq!(split_config_key("keyboard"), vec!["keyboard"]);
        assert_eq!(split_config_key("client-secret"), vec!["client", "secret"]);
        // Acronym runs stay together
        assert_eq!(split_config_key("APIKey"), vec!["api", "key"]);
        assert_eq!(split_config_key("apiKEY"), vec!["api", "key"]);
        assert_eq!(split_config_key("accessTOKEN"), vec!["access", "token"]);
        assert_eq!(split_config_key("MyAPIKey"), vec!["my", "api", "key"]);
    }
}
