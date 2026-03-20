use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Invoke a provider binary: write JSON to stdin, read JSON from stdout.
///
/// Uses `wait_with_output` on a dedicated thread with a timeout to avoid pipe
/// deadlocks — stdout and stderr are drained concurrently by the OS while the
/// child runs, so a chatty provider won't block on a full pipe buffer.
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
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(request_bytes.as_bytes())
            .map_err(|e| format!("stdin write failed: {e}"))?;
    }

    // wait_with_output drains stdout+stderr concurrently, preventing pipe deadlocks.
    // We run it on a thread so we can enforce a timeout. On timeout we kill the
    // child by PID to avoid orphaning a provider process that may mutate remote state.
    let timeout_secs = timeout.as_secs();
    let child_pid = child.id();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });

    let output = match rx.recv_timeout(timeout) {
        Ok(result) => result.map_err(|e| format!("wait failed: {e}"))?,
        Err(_) => {
            // Timeout — kill the child process so it doesn't keep running.
            #[cfg(unix)]
            unsafe {
                libc::kill(child_pid as i32, libc::SIGKILL);
            }
            #[cfg(not(unix))]
            {
                // On non-Unix, try to terminate via taskkill (best effort).
                let _ = std::process::Command::new("taskkill")
                    .args(["/F", "/PID", &child_pid.to_string()])
                    .output();
            }
            return Err(format!("provider timed out after {timeout_secs}s"));
        }
    };

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr_redacted = redact_secrets(&stderr);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let response: serde_json::Value = stdout
        .lines()
        .find_map(|line| serde_json::from_str(line).ok())
        .ok_or_else(|| {
            format!(
                "no JSON response. stderr: {}",
                &stderr_redacted[..stderr_redacted.len().min(4096)]
            )
        })?;

    if response.get("ok").and_then(|v| v.as_bool()) == Some(false) {
        let error = response["error"].as_str().unwrap_or("unknown error");
        return Err(redact_secrets(error));
    }

    Ok(response)
}

/// Split a config key into lowercase words on `_`, `-`, `.`, and camelCase boundaries.
/// "apiKey" → ["api", "key"], "access_token" → ["access", "token"],
/// "keyboard" → ["keyboard"], "clientSecret" → ["client", "secret"].
fn split_config_key(key: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    for ch in key.chars() {
        if ch == '_' || ch == '-' || ch == '.' {
            if !current.is_empty() {
                words.push(current.to_lowercase());
                current.clear();
            }
        } else if ch.is_uppercase() && !current.is_empty() {
            words.push(current.to_lowercase());
            current.clear();
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
/// Does NOT execute any binaries.
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
                if !id.is_empty() && !seen.contains(&name) {
                    seen.insert(name.clone());
                    results.push((id.to_string(), entry.path()));
                }
            }
        }
    }
    results
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
    }

    #[test]
    fn split_config_key_handles_all_styles() {
        assert_eq!(split_config_key("apiKey"), vec!["api", "key"]);
        assert_eq!(split_config_key("access_token"), vec!["access", "token"]);
        assert_eq!(split_config_key("keyboard"), vec!["keyboard"]);
        assert_eq!(split_config_key("client-secret"), vec!["client", "secret"]);
        assert_eq!(split_config_key("MyAPIKey"), vec!["my", "a", "p", "i", "key"]);
    }
}
