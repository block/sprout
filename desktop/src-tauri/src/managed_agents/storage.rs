use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

use tauri::{AppHandle, Manager};

use crate::managed_agents::ManagedAgentRecord;

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
    serde_json::from_str(&content).map_err(|error| format!("failed to parse agent store: {error}"))
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
    fs::write(&path, payload).map_err(|error| format!("failed to write agent store: {error}"))
}

pub(crate) fn open_log_file(path: &Path) -> Result<File, String> {
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

pub fn read_log_tail(path: &Path, max_lines: usize) -> Result<String, String> {
    if !path.exists() {
        return Ok(String::new());
    }

    let file = File::open(path)
        .map_err(|error| format!("failed to read log file {}: {error}", path.display()))?;
    let reader = BufReader::new(file);
    let lines = reader
        .lines()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read log lines: {error}"))?;
    let start = lines.len().saturating_sub(max_lines);
    Ok(lines[start..].join("\n"))
}
