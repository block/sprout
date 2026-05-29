use std::{
    fs::{self, File, OpenOptions},
    io::{Read as _, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use tauri::{AppHandle, Manager};

use crate::managed_agents::{ManagedAgentRecord, PersonaRecord};

pub fn managed_agents_base_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("failed to resolve app data dir: {error}"))?
        .join("agents");
    fs::create_dir_all(&dir).map_err(|error| format!("failed to create agents dir: {error}"))?;
    Ok(dir)
}

fn managed_agents_store_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(managed_agents_base_dir(app)?.join("managed-agents.json"))
}

fn managed_agents_logs_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = managed_agents_base_dir(app)?.join("logs");
    fs::create_dir_all(&dir).map_err(|error| format!("failed to create logs dir: {error}"))?;
    Ok(dir)
}

pub fn managed_agent_log_path(app: &AppHandle, pubkey: &str) -> Result<PathBuf, String> {
    Ok(managed_agents_logs_dir(app)?.join(format!("{pubkey}.log")))
}

pub fn load_managed_agents(app: &AppHandle) -> Result<Vec<ManagedAgentRecord>, String> {
    let path = managed_agents_store_path(app)?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("failed to read agent store: {error}"))?;
    let records: Vec<ManagedAgentRecord> = serde_json::from_str(&content)
        .map_err(|error| format!("failed to parse agent store: {error}"))?;

    Ok(records)
}

pub fn save_managed_agents(app: &AppHandle, records: &[ManagedAgentRecord]) -> Result<(), String> {
    let mut sorted = records.to_vec();
    sorted.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.pubkey.cmp(&right.pubkey))
    });

    let path = managed_agents_store_path(app)?;
    let payload = serde_json::to_vec_pretty(&sorted)
        .map_err(|error| format!("failed to serialize agent store: {error}"))?;

    atomic_write_json(&path, &payload)
}

/// Atomic, symlink-preserving JSON write.
/// Resolves symlinks so the tmp+rename happens at the real target path,
/// preserving any symlink at `path`.
pub(crate) fn atomic_write_json(path: &Path, payload: &[u8]) -> Result<(), String> {
    let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let tmp = resolved.with_extension("json.tmp");
    std::fs::write(&tmp, payload).map_err(|e| format!("failed to write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &resolved)
        .map_err(|e| format!("failed to rename {}: {e}", resolved.display()))
}

/// Maximum log file size before rotation (10 MB).
const MAX_LOG_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// If `path` exceeds [`MAX_LOG_FILE_SIZE`], rotate it to `<path>.1`.
fn maybe_rotate_log(path: &Path) {
    let size = match fs::metadata(path) {
        Ok(m) => m.len(),
        Err(_) => return,
    };
    if size <= MAX_LOG_FILE_SIZE {
        return;
    }
    let mut rotated = path.as_os_str().to_owned();
    rotated.push(".1");
    let _ = fs::rename(path, &rotated);
}

pub(crate) fn open_log_file(path: &Path) -> Result<File, String> {
    maybe_rotate_log(path);
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("failed to open log file {}: {error}", path.display()))
}

pub(crate) fn append_log_marker(path: &Path, message: &str) -> Result<(), String> {
    let mut file = open_log_file(path)?;
    writeln!(file, "{message}").map_err(|error| format!("failed to write log marker: {error}"))
}

fn agent_pids_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = managed_agents_base_dir(app)?.join("agent-pids");
    fs::create_dir_all(&dir)
        .map_err(|error| format!("failed to create agent-pids dir: {error}"))?;
    Ok(dir)
}

/// Write a PID file for a spawned agent. The PID equals the PGID since we
/// spawn with `process_group(0)`.
pub fn write_agent_pid_file(app: &AppHandle, pubkey: &str, pid: u32) -> Result<(), String> {
    let path = agent_pids_dir(app)?.join(format!("{pubkey}.pid"));
    fs::write(&path, pid.to_string())
        .map_err(|error| format!("failed to write PID file {}: {error}", path.display()))
}

/// Remove the PID file for an agent (e.g. on normal stop).
pub fn remove_agent_pid_file(app: &AppHandle, pubkey: &str) {
    if let Ok(dir) = agent_pids_dir(app) {
        let _ = fs::remove_file(dir.join(format!("{pubkey}.pid")));
    }
}

/// Read all PID files from `agent-pids/`, returning `(pubkey, pid)` pairs.
pub fn read_all_agent_pid_files(app: &AppHandle) -> Vec<(String, u32)> {
    let Ok(dir) = agent_pids_dir(app) else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str()?;
            let pubkey = name.strip_suffix(".pid")?;
            let pid: u32 = fs::read_to_string(entry.path()).ok()?.trim().parse().ok()?;
            Some((pubkey.to_string(), pid))
        })
        .collect()
}

pub fn read_log_tail(path: &Path, max_lines: usize) -> Result<String, String> {
    if !path.exists() {
        return Ok(String::new());
    }

    let mut file = File::open(path)
        .map_err(|error| format!("failed to read log file {}: {error}", path.display()))?;

    let file_len = file
        .seek(SeekFrom::End(0))
        .map_err(|error| format!("failed to seek log file: {error}"))?;

    if file_len == 0 {
        return Ok(String::new());
    }

    // Read backward in chunks to find enough newlines.
    const CHUNK_SIZE: u64 = 8 * 1024;
    let mut buf = Vec::new();
    let mut remaining = file_len;
    let mut newline_count: usize = 0;
    // We need max_lines + 1 newlines to delimit max_lines lines (the trailing
    // newline of the last line counts as one).
    let target_newlines = max_lines + 1;

    while remaining > 0 && newline_count < target_newlines {
        let chunk = remaining.min(CHUNK_SIZE);
        remaining -= chunk;
        file.seek(SeekFrom::Start(remaining))
            .map_err(|error| format!("failed to seek log file: {error}"))?;

        let mut tmp = vec![0u8; chunk as usize];
        file.read_exact(&mut tmp)
            .map_err(|error| format!("failed to read log chunk: {error}"))?;

        // Prepend this chunk so buf always has the tail of the file.
        tmp.append(&mut buf);
        buf = tmp;

        newline_count = bytecount_newlines(&buf);
    }

    // Strip ANSI escapes here (not in the harness) so the desktop log view
    // renders cleanly while terminals and other tools still get the colors
    // sprout-acp emits.
    let cleaned = strip_ansi_escapes::strip_str(String::from_utf8_lossy(&buf));
    let lines: Vec<&str> = cleaned.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    Ok(lines[start..].join("\n"))
}

fn bytecount_newlines(buf: &[u8]) -> usize {
    buf.iter().filter(|&&b| b == b'\n').count()
}

/// Clear `system_prompt` and `model` on persona-backed agents only when the
/// stored value matches the persona's current default. Returns `true` if any
/// record was modified.
pub(crate) fn migrate_clear_persona_defaults(
    records: &mut [ManagedAgentRecord],
    personas: &[PersonaRecord],
) -> bool {
    let mut changed = false;
    for record in records.iter_mut() {
        if let Some(persona_id) = record.persona_id.as_deref() {
            let persona = personas.iter().find(|p| p.id == persona_id);
            if let Some(persona) = persona {
                if let Some(ref prompt) = record.system_prompt {
                    if prompt == &persona.system_prompt {
                        record.system_prompt = None;
                        changed = true;
                    }
                }
                if let Some(ref model) = record.model {
                    if persona.model.as_deref() == Some(model.as_str()) {
                        record.model = None;
                        changed = true;
                    }
                }
            }
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed_agents::types::PersonaRecord;

    #[test]
    fn strips_ansi_from_typical_tracing_line() {
        let input = "\x1b[2m2026-05-27T15:16:32\x1b[0m \x1b[32m INFO\x1b[0m \x1b[2msprout_acp\x1b[0m\x1b[2m:\x1b[0m starting";
        assert_eq!(
            strip_ansi_escapes::strip_str(input),
            "2026-05-27T15:16:32  INFO sprout_acp: starting"
        );
    }

    fn persona(id: &str, prompt: &str, model: Option<&str>) -> PersonaRecord {
        PersonaRecord {
            id: id.to_string(),
            display_name: "Test".into(),
            avatar_url: None,
            system_prompt: prompt.to_string(),
            provider: None,
            model: model.map(|s| s.to_string()),
            name_pool: vec![],
            is_builtin: false,
            is_active: true,
            source_pack: None,
            source_pack_persona_slug: None,
            env_vars: Default::default(),
            created_at: "now".into(),
            updated_at: "now".into(),
        }
    }

    fn agent(
        persona_id: Option<&str>,
        prompt: Option<&str>,
        model: Option<&str>,
    ) -> ManagedAgentRecord {
        ManagedAgentRecord {
            pubkey: "p".into(),
            name: "n".into(),
            persona_id: persona_id.map(|s| s.to_string()),
            private_key_nsec: "nsec1fake".into(),
            auth_tag: None,
            relay_url: "ws://localhost:3000".into(),
            acp_command: "sprout-acp".into(),
            agent_command: "goose".into(),
            agent_args: vec![],
            mcp_command: "sprout-mcp-server".into(),
            turn_timeout_seconds: 320,
            idle_timeout_seconds: None,
            max_turn_duration_seconds: None,
            parallelism: 1,
            system_prompt: prompt.map(|s| s.to_string()),
            model: model.map(|s| s.to_string()),
            mcp_toolsets: None,
            env_vars: std::collections::BTreeMap::new(),
            start_on_app_launch: false,
            runtime_pid: None,
            backend: Default::default(),
            backend_agent_id: None,
            provider_binary_path: None,
            persona_pack_path: None,
            persona_name_in_pack: None,
            created_at: "now".into(),
            updated_at: "now".into(),
            last_started_at: None,
            last_stopped_at: None,
            last_exit_code: None,
            last_error: None,
            respond_to: Default::default(),
            respond_to_allowlist: vec![],
        }
    }

    #[test]
    fn migration_clears_matching_defaults() {
        let personas = vec![persona("p1", "default prompt", Some("gpt-4o"))];
        let mut records = vec![agent(Some("p1"), Some("default prompt"), Some("gpt-4o"))];

        let changed = migrate_clear_persona_defaults(&mut records, &personas);

        assert!(changed);
        assert_eq!(records[0].system_prompt, None);
        assert_eq!(records[0].model, None);
    }

    #[test]
    fn migration_preserves_user_overrides() {
        let personas = vec![persona("p1", "default prompt", Some("gpt-4o"))];
        let mut records = vec![agent(
            Some("p1"),
            Some("my custom prompt"),
            Some("claude-sonnet"),
        )];

        let changed = migrate_clear_persona_defaults(&mut records, &personas);

        assert!(!changed);
        assert_eq!(
            records[0].system_prompt.as_deref(),
            Some("my custom prompt")
        );
        assert_eq!(records[0].model.as_deref(), Some("claude-sonnet"));
    }

    #[test]
    fn migration_skips_agents_without_persona() {
        let personas = vec![persona("p1", "default prompt", Some("gpt-4o"))];
        let mut records = vec![agent(None, Some("standalone prompt"), Some("gpt-4o"))];

        let changed = migrate_clear_persona_defaults(&mut records, &personas);

        assert!(!changed);
        assert_eq!(
            records[0].system_prompt.as_deref(),
            Some("standalone prompt")
        );
        assert_eq!(records[0].model.as_deref(), Some("gpt-4o"));
    }

    #[test]
    fn migration_partial_match_clears_only_matching_field() {
        let personas = vec![persona("p1", "default prompt", Some("gpt-4o"))];
        // prompt matches default, model is user-overridden
        let mut records = vec![agent(
            Some("p1"),
            Some("default prompt"),
            Some("claude-sonnet"),
        )];

        let changed = migrate_clear_persona_defaults(&mut records, &personas);

        assert!(changed);
        assert_eq!(records[0].system_prompt, None);
        assert_eq!(records[0].model.as_deref(), Some("claude-sonnet"));
    }

    #[test]
    fn migration_handles_persona_with_no_model() {
        let personas = vec![persona("p1", "default prompt", None)];
        let mut records = vec![agent(
            Some("p1"),
            Some("default prompt"),
            Some("user-model"),
        )];

        let changed = migrate_clear_persona_defaults(&mut records, &personas);

        assert!(changed); // prompt cleared
        assert_eq!(records[0].system_prompt, None);
        // model stays — persona has no model, so it can't match
        assert_eq!(records[0].model.as_deref(), Some("user-model"));
    }
}
