use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
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

/// Read the last `max_lines` lines from a log file by seeking from the end.
///
/// Instead of loading the entire file into memory, we read backwards in chunks
/// until we've found enough newline characters (or reached the start of the file).
/// This keeps memory usage proportional to the returned tail, not the file size.
pub fn read_log_tail(path: &Path, max_lines: usize) -> Result<String, String> {
    if max_lines == 0 || !path.exists() {
        return Ok(String::new());
    }

    let mut file = File::open(path)
        .map_err(|error| format!("failed to read log file {}: {error}", path.display()))?;

    let file_size = file
        .seek(SeekFrom::End(0))
        .map_err(|error| format!("failed to seek log file: {error}"))?;

    if file_size == 0 {
        return Ok(String::new());
    }

    // Read backwards in 8 KB chunks, collecting bytes until we have enough newlines.
    const CHUNK_SIZE: u64 = 8 * 1024;
    let mut buf = Vec::new();
    let mut remaining = file_size;
    let mut newline_count: usize = 0;
    // We need max_lines+1 newlines to delimit max_lines lines (the leading newline
    // before the first returned line acts as the boundary).
    let target_newlines = max_lines + 1;

    while remaining > 0 {
        let chunk_len = remaining.min(CHUNK_SIZE);
        remaining -= chunk_len;

        file.seek(SeekFrom::Start(remaining))
            .map_err(|error| format!("failed to seek log file: {error}"))?;

        let mut chunk = vec![0u8; chunk_len as usize];
        file.read_exact(&mut chunk)
            .map_err(|error| format!("failed to read log chunk: {error}"))?;

        // Count newlines in this chunk (iterate backwards so we can bail early).
        for &byte in chunk.iter().rev() {
            if byte == b'\n' {
                newline_count += 1;
                if newline_count >= target_newlines {
                    break;
                }
            }
        }

        // Prepend this chunk to our buffer.
        chunk.append(&mut buf);
        buf = chunk;

        if newline_count >= target_newlines {
            break;
        }
    }

    // Convert to a string. If we landed on a UTF-8 boundary mid-character,
    // skip forward to the next valid boundary.
    let text = match String::from_utf8(buf) {
        Ok(s) => s,
        Err(err) => {
            let bytes = err.into_bytes();
            // Find the first valid UTF-8 start by skipping continuation bytes.
            let start = bytes
                .iter()
                .position(|b| (*b as i8) >= -64) // not a continuation byte (0b10xxxxxx)
                .unwrap_or(0);
            String::from_utf8_lossy(&bytes[start..]).into_owned()
        }
    };

    // Extract the last max_lines lines from the text we read.
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    Ok(lines[start..].join("\n"))
}
