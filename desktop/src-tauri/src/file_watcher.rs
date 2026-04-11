//! Local file watcher for computing diffs when agents modify files.
//!
//! Watches a per-channel project directory, debounces file changes,
//! computes unified diffs, and emits Tauri events to the frontend.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use notify::{
    event::{CreateKind, ModifyKind},
    EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use serde::{Deserialize, Serialize};
use similar::TextDiff;
use tauri::{AppHandle, Emitter, Manager};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Payload emitted to the frontend via the `"file-diff"` Tauri event.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiffEvent {
    pub channel_id: String,
    pub file_path: String,
    pub unified_diff: String,
    pub timestamp: u64,
}

/// Per-channel project directory config, persisted to a local JSON file.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectDirConfig {
    /// channel_id → absolute directory path
    pub channels: HashMap<String, String>,
}

/// Runtime state for an active file watcher.
struct WatcherRuntime {
    _watcher: RecommendedWatcher,
}

/// App-level state that holds all active file watchers.
pub struct FileWatcherState {
    /// channel_id → watcher runtime
    watchers: Mutex<HashMap<String, WatcherRuntime>>,
    /// channel_id → (file_path → last known content)
    snapshots: Arc<Mutex<HashMap<String, HashMap<PathBuf, String>>>>,
}

impl FileWatcherState {
    pub fn new() -> Self {
        Self {
            watchers: Mutex::new(HashMap::new()),
            snapshots: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

// ---------------------------------------------------------------------------
// Config persistence
// ---------------------------------------------------------------------------

fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app data dir: {e}"))?;
    fs::create_dir_all(&dir).map_err(|e| format!("create app data dir: {e}"))?;
    Ok(dir.join("project-dirs.json"))
}

fn load_config(app: &AppHandle) -> Result<ProjectDirConfig, String> {
    let path = config_path(app)?;
    if !path.exists() {
        return Ok(ProjectDirConfig::default());
    }
    let content =
        fs::read_to_string(&path).map_err(|e| format!("read project-dirs.json: {e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("parse project-dirs.json: {e}"))
}

fn save_config(app: &AppHandle, config: &ProjectDirConfig) -> Result<(), String> {
    let path = config_path(app)?;
    let payload =
        serde_json::to_vec_pretty(config).map_err(|e| format!("serialize config: {e}"))?;
    fs::write(&path, payload).map_err(|e| format!("write project-dirs.json: {e}"))
}

// ---------------------------------------------------------------------------
// Path filtering
// ---------------------------------------------------------------------------

/// Directories to always ignore.
const IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    ".next",
    "dist",
    "build",
    "__pycache__",
    ".turbo",
];

/// File extensions to skip (binary / large generated files).
const IGNORED_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "ico", "webp", "svg", "bmp", "tiff", "mp4", "mov", "avi",
    "mp3", "wav", "ogg", "woff", "woff2", "ttf", "eot", "otf", "zip", "tar", "gz", "bz2",
    "xz", "7z", "rar", "exe", "dll", "so", "dylib", "o", "a", "class", "jar", "pyc", "pyo",
    "wasm", "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "db", "sqlite", "sqlite3",
    "lock",
];

fn should_ignore_path(path: &Path, project_dir: &Path) -> bool {
    // Check directory components.
    if let Ok(relative) = path.strip_prefix(project_dir) {
        for component in relative.components() {
            if let std::path::Component::Normal(name) = component {
                let name_str = name.to_string_lossy();
                if IGNORED_DIRS.iter().any(|d| *d == name_str.as_ref()) {
                    return true;
                }
            }
        }
    }

    // Check extension.
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let ext_lower = ext.to_lowercase();
        if IGNORED_EXTENSIONS.iter().any(|e| *e == ext_lower.as_str()) {
            return true;
        }
    }

    false
}

fn is_regular_file(path: &Path) -> bool {
    path.is_file() && !path.is_symlink()
}

// ---------------------------------------------------------------------------
// Diff computation
// ---------------------------------------------------------------------------

/// Compute a unified diff between `old` and `new` content for a given file path.
fn compute_unified_diff(file_path: &str, old: &str, new: &str) -> String {
    let diff = TextDiff::from_lines(old, new);
    diff.unified_diff()
        .context_radius(3)
        .header(&format!("a/{file_path}"), &format!("b/{file_path}"))
        .to_string()
}

// ---------------------------------------------------------------------------
// Snapshot management
// ---------------------------------------------------------------------------

/// Take a snapshot of all text files in the project directory.
fn snapshot_directory(project_dir: &Path) -> HashMap<PathBuf, String> {
    let mut snapshots = HashMap::new();

    if !project_dir.is_dir() {
        return snapshots;
    }

    fn walk_dir(dir: &Path, project_dir: &Path, snapshots: &mut HashMap<PathBuf, String>) {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();

            if should_ignore_path(&path, project_dir) {
                continue;
            }

            if path.is_dir() {
                walk_dir(&path, project_dir, snapshots);
            } else if is_regular_file(&path) {
                // Only snapshot files under 1MB to avoid memory issues.
                if let Ok(meta) = path.metadata() {
                    if meta.len() > 1_048_576 {
                        continue;
                    }
                }
                if let Ok(content) = fs::read_to_string(&path) {
                    snapshots.insert(path, content);
                }
            }
        }
    }

    walk_dir(project_dir, project_dir, &mut snapshots);
    snapshots
}

// ---------------------------------------------------------------------------
// Watcher logic
// ---------------------------------------------------------------------------

/// Start watching a project directory for a given channel.
fn start_watcher(
    app: &AppHandle,
    channel_id: &str,
    project_dir: &Path,
) -> Result<WatcherRuntime, String> {
    let fw_state = app.state::<FileWatcherState>();

    // Take initial snapshot.
    let initial_snapshot = snapshot_directory(project_dir);
    {
        let mut snapshots = fw_state.snapshots.lock().map_err(|e| e.to_string())?;
        snapshots.insert(channel_id.to_string(), initial_snapshot);
    }

    let app_handle = app.clone();
    let channel_id_owned = channel_id.to_string();
    let project_dir_owned = project_dir.to_path_buf();
    let snapshots_ref = Arc::clone(&fw_state.snapshots);

    // Debounce state: track last event time per file.
    let pending: Arc<Mutex<HashMap<PathBuf, Instant>>> = Arc::new(Mutex::new(HashMap::new()));
    let pending_clone = Arc::clone(&pending);

    // Spawn a debounce processor that runs every 250ms.
    let app_for_timer = app_handle.clone();
    let channel_for_timer = channel_id_owned.clone();
    let project_dir_for_timer = project_dir_owned.clone();
    let snapshots_for_timer = Arc::clone(&snapshots_ref);

    tauri::async_runtime::spawn(async move {
        let debounce_delay = Duration::from_millis(500);
        loop {
            tokio::time::sleep(Duration::from_millis(250)).await;

            let ready_paths: Vec<PathBuf> = {
                let mut pending_guard = match pending_clone.lock() {
                    Ok(g) => g,
                    Err(_) => continue,
                };
                let now = Instant::now();
                let mut ready = Vec::new();
                pending_guard.retain(|path, last_event| {
                    if now.duration_since(*last_event) >= debounce_delay {
                        ready.push(path.clone());
                        false
                    } else {
                        true
                    }
                });
                ready
            };

            for path in ready_paths {
                if should_ignore_path(&path, &project_dir_for_timer) {
                    continue;
                }

                let relative = path
                    .strip_prefix(&project_dir_for_timer)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string();

                // Get old content from snapshot.
                let old_content = {
                    let snapshots = match snapshots_for_timer.lock() {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                    snapshots
                        .get(&channel_for_timer)
                        .and_then(|m| m.get(&path))
                        .cloned()
                        .unwrap_or_default()
                };

                // Read new content.
                let new_content = if path.exists() && is_regular_file(&path) {
                    // Skip files over 1MB.
                    match path.metadata() {
                        Ok(meta) if meta.len() <= 1_048_576 => {
                            fs::read_to_string(&path).unwrap_or_default()
                        }
                        _ => continue,
                    }
                } else {
                    // File was deleted.
                    String::new()
                };

                if old_content == new_content {
                    continue;
                }

                let diff = compute_unified_diff(&relative, &old_content, &new_content);

                if diff.is_empty() {
                    continue;
                }

                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                let event = FileDiffEvent {
                    channel_id: channel_for_timer.clone(),
                    file_path: relative,
                    unified_diff: diff,
                    timestamp,
                };

                let _ = app_for_timer.emit("file-diff", &event);

                // Update snapshot.
                if let Ok(mut snapshots) = snapshots_for_timer.lock() {
                    let channel_snaps = snapshots
                        .entry(channel_for_timer.clone())
                        .or_insert_with(HashMap::new);
                    if new_content.is_empty() {
                        channel_snaps.remove(&path);
                    } else {
                        channel_snaps.insert(path.clone(), new_content);
                    }
                }
            }
        }
    });

    let mut watcher = notify::recommended_watcher(move |res: Result<notify::Event, _>| {
        let event = match res {
            Ok(e) => e,
            Err(_) => return,
        };

        // Only care about creates and modifies.
        match event.kind {
            EventKind::Create(CreateKind::File)
            | EventKind::Modify(ModifyKind::Data(_))
            | EventKind::Modify(ModifyKind::Name(_))
            | EventKind::Remove(_) => {}
            _ => return,
        }

        if let Ok(mut pending_guard) = pending.lock() {
            for path in event.paths {
                if should_ignore_path(&path, &project_dir_owned) {
                    continue;
                }
                pending_guard.insert(path, Instant::now());
            }
        }
    })
    .map_err(|e| format!("create file watcher: {e}"))?;

    watcher
        .watch(project_dir, RecursiveMode::Recursive)
        .map_err(|e| format!("watch directory: {e}"))?;

    Ok(WatcherRuntime {
        _watcher: watcher,
    })
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_project_dir(app: AppHandle, channel_id: String) -> Result<Option<String>, String> {
    let config = load_config(&app)?;
    Ok(config.channels.get(&channel_id).cloned())
}

#[tauri::command]
pub async fn set_project_dir(
    app: AppHandle,
    channel_id: String,
    path: Option<String>,
) -> Result<(), String> {
    let mut config = load_config(&app)?;

    if let Some(dir) = &path {
        let dir_path = PathBuf::from(dir);
        if !dir_path.is_dir() {
            return Err(format!("Not a directory: {dir}"));
        }
        config.channels.insert(channel_id.clone(), dir.clone());
    } else {
        config.channels.remove(&channel_id);
    }

    save_config(&app, &config)?;

    // Stop existing watcher if any, start new one if path is set.
    let fw_state = app.state::<FileWatcherState>();
    {
        let mut watchers = fw_state.watchers.lock().map_err(|e| e.to_string())?;
        watchers.remove(&channel_id);
    }

    if let Some(dir) = &path {
        let runtime = start_watcher(&app, &channel_id, &PathBuf::from(dir))?;
        let mut watchers = fw_state.watchers.lock().map_err(|e| e.to_string())?;
        watchers.insert(channel_id, runtime);
    }

    Ok(())
}

#[tauri::command]
pub async fn start_file_watcher(app: AppHandle, channel_id: String) -> Result<(), String> {
    let config = load_config(&app)?;
    let dir = config
        .channels
        .get(&channel_id)
        .ok_or_else(|| format!("No project directory configured for channel {channel_id}"))?;

    let dir_path = PathBuf::from(dir);
    if !dir_path.is_dir() {
        return Err(format!("Project directory does not exist: {dir}"));
    }

    let fw_state = app.state::<FileWatcherState>();
    {
        let watchers = fw_state.watchers.lock().map_err(|e| e.to_string())?;
        if watchers.contains_key(&channel_id) {
            return Ok(()); // Already watching.
        }
    }

    let runtime = start_watcher(&app, &channel_id, &dir_path)?;
    let mut watchers = fw_state.watchers.lock().map_err(|e| e.to_string())?;
    watchers.insert(channel_id, runtime);
    Ok(())
}

#[tauri::command]
pub async fn stop_file_watcher(app: AppHandle, channel_id: String) -> Result<(), String> {
    let fw_state = app.state::<FileWatcherState>();
    let mut watchers = fw_state.watchers.lock().map_err(|e| e.to_string())?;
    watchers.remove(&channel_id);

    // Also clear snapshots.
    let mut snapshots = fw_state.snapshots.lock().map_err(|e| e.to_string())?;
    snapshots.remove(&channel_id);

    Ok(())
}

/// Re-snapshot the project directory (e.g. after the user manually resets).
#[tauri::command]
pub async fn resnapshot_project_dir(app: AppHandle, channel_id: String) -> Result<(), String> {
    let config = load_config(&app)?;
    let dir = config
        .channels
        .get(&channel_id)
        .ok_or_else(|| format!("No project directory configured for channel {channel_id}"))?;

    let dir_path = PathBuf::from(dir);
    if !dir_path.is_dir() {
        return Err(format!("Project directory does not exist: {dir}"));
    }

    let new_snapshot = snapshot_directory(&dir_path);
    let fw_state = app.state::<FileWatcherState>();
    let mut snapshots = fw_state.snapshots.lock().map_err(|e| e.to_string())?;
    snapshots.insert(channel_id, new_snapshot);

    Ok(())
}
