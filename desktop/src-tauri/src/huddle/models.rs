//! Model download manager for STT (Moonshine) and TTS (Kokoro) models.
//!
//! Mental model:
//!   app launch → start_moonshine_download (background) → ~/.sprout/models/moonshine-tiny/
//!   app launch → start_kokoro_download (background)    → ~/.sprout/models/kokoro/
//!   STT pipeline → is_moonshine_ready() → moonshine_model_dir() → run inference
//!   TTS pipeline → is_kokoro_ready()    → kokoro_model_dir()    → run synthesis
//!
//! Models are downloaded once and cached. No versioning in MVP — presence of
//! all expected files is sufficient to consider the model ready.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};

// ── Constants ─────────────────────────────────────────────────────────────────

const MOONSHINE_DOWNLOAD_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/\
     sherpa-onnx-moonshine-tiny-en-int8.tar.bz2";

/// Subdirectory name produced by `tar xjf` on the archive.
const MOONSHINE_ARCHIVE_SUBDIR: &str = "sherpa-onnx-moonshine-tiny-en-int8";

/// Final directory name under `~/.sprout/models/`.
const MOONSHINE_MODEL_DIR_NAME: &str = "moonshine-tiny";

/// All files that must be present for the model to be considered ready.
const MOONSHINE_EXPECTED_FILES: &[&str] = &[
    "preprocessor.onnx",
    "encoder.onnx",
    "merged_decoder.onnx",
    "tokens.txt",
];

const KOKORO_MODEL_URL: &str =
    "https://huggingface.co/hexgrad/Kokoro-82M/resolve/main/kokoro-v0_19.onnx";

const KOKORO_VOICES_URL: &str =
    "https://huggingface.co/hexgrad/Kokoro-82M/resolve/main/voices.bin";

/// Final directory name under `~/.sprout/models/`.
const KOKORO_MODEL_DIR_NAME: &str = "kokoro";

/// All files that must be present for Kokoro to be considered ready.
const KOKORO_EXPECTED_FILES: &[&str] = &["kokoro-v0_19.onnx", "voices.bin"];

// ── Status types ──────────────────────────────────────────────────────────────

/// Download/readiness status for a single model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelStatus {
    NotDownloaded,
    Downloading { progress_percent: u8 },
    Ready,
    Error(String),
}

/// Combined status for all voice models (returned to the frontend).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceModelStatus {
    pub moonshine: ModelStatus,
    pub kokoro: ModelStatus,
}

// ── Platform-gated tar extraction ─────────────────────────────────────────────

/// Extract a `.tar.bz2` archive into `dest_dir` using the system `tar`.
///
/// Only available on Unix — on other platforms this returns an error at
/// compile time so the caller can handle it gracefully.
#[cfg(unix)]
fn extract_archive(archive_path: &Path, dest_dir: &Path) -> Result<(), String> {
    let status = std::process::Command::new("tar")
        .args([
            "xjf",
            &archive_path.to_string_lossy(),
            "-C",
            &dest_dir.to_string_lossy(),
        ])
        .status()
        .map_err(|e| format!("tar execution failed: {e}"))?;
    if !status.success() {
        return Err(format!("tar exited with status {status}"));
    }
    Ok(())
}

#[cfg(not(unix))]
fn extract_archive(_archive_path: &Path, _dest_dir: &Path) -> Result<(), String> {
    Err("Model download is not yet supported on this platform".to_string())
}

// ── ModelManager ──────────────────────────────────────────────────────────────

/// Manages download and location of STT/TTS model files.
///
/// Cheap to clone — the inner status is behind an `Arc<Mutex<>>`.
#[derive(Clone)]
pub struct ModelManager {
    /// `~/.sprout/models/`
    models_dir: PathBuf,
    moonshine_status: Arc<Mutex<ModelStatus>>,
    kokoro_status: Arc<Mutex<ModelStatus>>,
}

impl ModelManager {
    /// Create a new `ModelManager` rooted at `~/.sprout/models/`.
    ///
    /// Returns `None` if the home directory cannot be resolved.
    pub fn new() -> Option<Self> {
        let models_dir = dirs::home_dir()?.join(".sprout").join("models");
        Some(Self {
            models_dir,
            moonshine_status: Arc::new(Mutex::new(ModelStatus::NotDownloaded)),
            kokoro_status: Arc::new(Mutex::new(ModelStatus::NotDownloaded)),
        })
    }

    /// Returns the path to the Moonshine model directory, or `None` if not ready.
    pub fn moonshine_model_dir(&self) -> Option<PathBuf> {
        if self.is_moonshine_ready() {
            Some(self.models_dir.join(MOONSHINE_MODEL_DIR_NAME))
        } else {
            None
        }
    }

    /// Returns `true` if all expected Moonshine model files are present on disk.
    pub fn is_moonshine_ready(&self) -> bool {
        let dir = self.models_dir.join(MOONSHINE_MODEL_DIR_NAME);
        MOONSHINE_EXPECTED_FILES
            .iter()
            .all(|f| dir.join(f).is_file())
    }

    /// Current Moonshine download status.
    pub fn moonshine_status(&self) -> ModelStatus {
        self.moonshine_status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Returns the path to the Kokoro model directory, or `None` if not ready.
    pub fn kokoro_model_dir(&self) -> Option<PathBuf> {
        if self.is_kokoro_ready() {
            Some(self.models_dir.join(KOKORO_MODEL_DIR_NAME))
        } else {
            None
        }
    }

    /// Returns `true` if all expected Kokoro model files are present on disk.
    pub fn is_kokoro_ready(&self) -> bool {
        let dir = self.models_dir.join(KOKORO_MODEL_DIR_NAME);
        KOKORO_EXPECTED_FILES.iter().all(|f| dir.join(f).is_file())
    }

    /// Current Kokoro download status.
    pub fn kokoro_status(&self) -> ModelStatus {
        self.kokoro_status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Trigger a background download of the Kokoro TTS model (~92 MB).
    ///
    /// Returns immediately. Progress is tracked via `kokoro_status()`.
    /// No-op if the model is already ready or a download is already running.
    pub fn start_kokoro_download(&self, http_client: reqwest::Client) {
        // Fast path: already on disk — sync the status and return.
        if self.is_kokoro_ready() {
            *self.kokoro_status.lock().unwrap_or_else(|e| e.into_inner()) = ModelStatus::Ready;
            return;
        }

        // Atomically check-and-set before spawning.
        {
            let mut status = self.kokoro_status.lock().unwrap_or_else(|e| e.into_inner());
            match *status {
                ModelStatus::Downloading { .. } | ModelStatus::Ready => return,
                _ => {}
            }
            *status = ModelStatus::Downloading {
                progress_percent: 0,
            };
        }

        let manager = self.clone();
        tokio::spawn(async move {
            if let Err(e) = manager.download_kokoro_model(http_client).await {
                eprintln!("sprout-desktop: kokoro download failed: {e}");
                *manager
                    .kokoro_status
                    .lock()
                    .unwrap_or_else(|e2| e2.into_inner()) = ModelStatus::Error(e);
            }
        });
    }

    /// Trigger a background download of the Moonshine model.
    ///
    /// Returns immediately. Progress is tracked via `moonshine_status()`.
    /// No-op if the model is already ready or a download is already running.
    ///
    /// Fix: status is set to `Downloading` synchronously *before* the task is
    /// spawned, eliminating the race where two concurrent callers both see
    /// `NotDownloaded` and each spawn a download.
    pub fn start_moonshine_download(&self, http_client: reqwest::Client) {
        // Fast path: already on disk — sync the status and return.
        if self.is_moonshine_ready() {
            *self
                .moonshine_status
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = ModelStatus::Ready;
            return;
        }

        // Atomically check-and-set: if already Downloading or Ready, bail out.
        // Setting the status here (before spawn) prevents a second caller from
        // racing through this check while the first caller's task hasn't started.
        {
            let mut status = self
                .moonshine_status
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            match *status {
                ModelStatus::Downloading { .. } | ModelStatus::Ready => return,
                _ => {}
            }
            *status = ModelStatus::Downloading {
                progress_percent: 0,
            };
        } // lock released before spawn

        let manager = self.clone();
        tokio::spawn(async move {
            if let Err(e) = manager.download_moonshine_model(http_client).await {
                eprintln!("sprout-desktop: moonshine download failed: {e}");
                *manager
                    .moonshine_status
                    .lock()
                    .unwrap_or_else(|e2| e2.into_inner()) = ModelStatus::Error(e);
            }
        });
    }

    // ── Private ───────────────────────────────────────────────────────────────

    fn set_status(&self, status: ModelStatus) {
        *self
            .moonshine_status
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = status;
    }

    fn set_kokoro_status(&self, status: ModelStatus) {
        *self.kokoro_status.lock().unwrap_or_else(|e| e.into_inner()) = status;
    }

    /// Download, extract, and verify the Moonshine model archive.
    ///
    /// Extraction is atomic: we extract into a temp directory, verify all
    /// expected files, then rename into place. A failed extraction leaves any
    /// previously working model untouched.
    async fn download_moonshine_model(&self, http_client: reqwest::Client) -> Result<(), String> {
        // 1. Ensure models directory exists.
        fs::create_dir_all(&self.models_dir).map_err(|e| format!("create models dir: {e}"))?;

        self.set_status(ModelStatus::Downloading {
            progress_percent: 0,
        });

        let archive_path = self.models_dir.join("moonshine-tiny.tar.bz2");

        eprintln!("sprout-desktop: downloading Moonshine model from {MOONSHINE_DOWNLOAD_URL}");

        // 2. Fetch the archive.
        let response = http_client
            .get(MOONSHINE_DOWNLOAD_URL)
            .send()
            .await
            .map_err(|e| format!("download request failed: {e}"))?;

        if !response.status().is_success() {
            return Err(format!(
                "download HTTP {}: {}",
                response.status().as_u16(),
                response.status().canonical_reason().unwrap_or("unknown"),
            ));
        }

        let content_length = response.content_length();

        // 3. Stream bytes to disk with progress updates.
        {
            use tokio::io::AsyncWriteExt;

            let body = response
                .bytes()
                .await
                .map_err(|e| format!("download stream error: {e}"))?;

            // Report progress based on content-length (if known).
            if let Some(total) = content_length {
                if total > 0 {
                    let pct = ((body.len() as u64 * 100) / total).min(89) as u8;
                    self.set_status(ModelStatus::Downloading {
                        progress_percent: pct,
                    });
                }
            }

            eprintln!("sprout-desktop: downloaded {} bytes, writing…", body.len());

            let mut file = tokio::fs::File::create(&archive_path)
                .await
                .map_err(|e| format!("create archive file: {e}"))?;
            file.write_all(&body)
                .await
                .map_err(|e| format!("write archive: {e}"))?;
            file.flush()
                .await
                .map_err(|e| format!("flush archive: {e}"))?;
        }

        self.set_status(ModelStatus::Downloading {
            progress_percent: 90,
        });

        // 4. Extract into a temp directory so that a failure does not destroy
        //    any previously working model.
        let temp_dir = self.models_dir.join("moonshine-tiny.tmp");
        let final_dir = self.models_dir.join(MOONSHINE_MODEL_DIR_NAME);

        // Clean up any leftover temp dir from a prior failed attempt.
        if temp_dir.exists() {
            fs::remove_dir_all(&temp_dir).map_err(|e| format!("remove stale temp dir: {e}"))?;
        }
        fs::create_dir_all(&temp_dir).map_err(|e| format!("create temp dir: {e}"))?;

        eprintln!("sprout-desktop: extracting Moonshine archive…");
        extract_archive(&archive_path, &temp_dir)?;

        // 5. Locate the extracted subdirectory inside the temp dir.
        let extracted_subdir = temp_dir.join(MOONSHINE_ARCHIVE_SUBDIR);
        if !extracted_subdir.is_dir() {
            // Clean up before returning the error.
            let _ = fs::remove_dir_all(&temp_dir);
            return Err(format!(
                "expected subdir '{}' not found after extraction",
                MOONSHINE_ARCHIVE_SUBDIR,
            ));
        }

        // 6. Verify all expected files are present before touching the live dir.
        let missing: Vec<&str> = MOONSHINE_EXPECTED_FILES
            .iter()
            .filter(|&&f| !extracted_subdir.join(f).is_file())
            .copied()
            .collect();

        if !missing.is_empty() {
            let _ = fs::remove_dir_all(&temp_dir);
            return Err(format!(
                "model verification failed — missing: {}",
                missing.join(", "),
            ));
        }

        // 7. Atomic swap: rename old out of the way first, then bring new in.
        //    This ensures a rename failure cannot destroy the previously working model.
        let backup_dir = final_dir.with_extension("old");
        if final_dir.exists() {
            // Remove any stale backup from a prior interrupted swap.
            if backup_dir.exists() {
                let _ = fs::remove_dir_all(&backup_dir);
            }
            fs::rename(&final_dir, &backup_dir).map_err(|e| format!("backup old model: {e}"))?;
        }

        // Bring new model into place; restore backup on failure.
        if let Err(e) = fs::rename(&extracted_subdir, &final_dir) {
            if backup_dir.exists() {
                let _ = fs::rename(&backup_dir, &final_dir);
            }
            return Err(format!("install new model: {e}"));
        }

        // 8. Clean up backup, temp dir (now empty after rename), and archive.
        let _ = fs::remove_dir_all(&backup_dir);
        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::remove_file(&archive_path);

        eprintln!(
            "sprout-desktop: Moonshine model ready at {}",
            final_dir.display()
        );

        self.set_status(ModelStatus::Ready);
        Ok(())
    }

    /// Download and verify the Kokoro TTS model files directly from HuggingFace.
    ///
    /// Downloads `kokoro-v0_19.onnx` and `voices.bin` into `~/.sprout/models/kokoro/`.
    /// Files are written to a temp directory first, then moved atomically.
    async fn download_kokoro_model(&self, http_client: reqwest::Client) -> Result<(), String> {
        fs::create_dir_all(&self.models_dir).map_err(|e| format!("create models dir: {e}"))?;

        self.set_kokoro_status(ModelStatus::Downloading {
            progress_percent: 0,
        });

        let final_dir = self.models_dir.join(KOKORO_MODEL_DIR_NAME);
        let temp_dir = self.models_dir.join("kokoro.tmp");

        // Clean up any leftover temp dir from a prior failed attempt.
        if temp_dir.exists() {
            fs::remove_dir_all(&temp_dir).map_err(|e| format!("remove stale temp dir: {e}"))?;
        }
        fs::create_dir_all(&temp_dir).map_err(|e| format!("create temp dir: {e}"))?;

        // Download each file individually.
        let downloads: &[(&str, &str)] = &[
            (KOKORO_MODEL_URL, "kokoro-v0_19.onnx"),
            (KOKORO_VOICES_URL, "voices.bin"),
        ];

        for (i, (url, filename)) in downloads.iter().enumerate() {
            eprintln!("sprout-desktop: downloading Kokoro {filename} from {url}");

            let response = http_client
                .get(*url)
                .send()
                .await
                .map_err(|e| format!("download {filename} request failed: {e}"))?;

            if !response.status().is_success() {
                let _ = fs::remove_dir_all(&temp_dir);
                return Err(format!(
                    "download {filename} HTTP {}: {}",
                    response.status().as_u16(),
                    response.status().canonical_reason().unwrap_or("unknown"),
                ));
            }

            let body = response
                .bytes()
                .await
                .map_err(|e| format!("download {filename} stream error: {e}"))?;

            eprintln!(
                "sprout-desktop: downloaded {} bytes ({}), writing…",
                body.len(),
                filename
            );

            // Progress: split evenly across the two files (0–44 for first, 45–89 for second).
            let pct = (((i as u8) * 45) + 10).min(89);
            self.set_kokoro_status(ModelStatus::Downloading {
                progress_percent: pct,
            });

            use tokio::io::AsyncWriteExt;
            let dest = temp_dir.join(filename);
            let mut file = tokio::fs::File::create(&dest)
                .await
                .map_err(|e| format!("create {filename}: {e}"))?;
            file.write_all(&body)
                .await
                .map_err(|e| format!("write {filename}: {e}"))?;
            file.flush()
                .await
                .map_err(|e| format!("flush {filename}: {e}"))?;
        }

        self.set_kokoro_status(ModelStatus::Downloading {
            progress_percent: 90,
        });

        // Verify all expected files landed in the temp dir.
        let missing: Vec<&str> = KOKORO_EXPECTED_FILES
            .iter()
            .filter(|&&f| !temp_dir.join(f).is_file())
            .copied()
            .collect();

        if !missing.is_empty() {
            let _ = fs::remove_dir_all(&temp_dir);
            return Err(format!(
                "kokoro model verification failed — missing: {}",
                missing.join(", "),
            ));
        }

        // Atomic swap: backup old dir, move new dir into place.
        let backup_dir = final_dir.with_extension("old");
        if final_dir.exists() {
            if backup_dir.exists() {
                let _ = fs::remove_dir_all(&backup_dir);
            }
            fs::rename(&final_dir, &backup_dir)
                .map_err(|e| format!("backup old kokoro model: {e}"))?;
        }

        if let Err(e) = fs::rename(&temp_dir, &final_dir) {
            if backup_dir.exists() {
                let _ = fs::rename(&backup_dir, &final_dir);
            }
            return Err(format!("install new kokoro model: {e}"));
        }

        let _ = fs::remove_dir_all(&backup_dir);

        eprintln!(
            "sprout-desktop: Kokoro model ready at {}",
            final_dir.display()
        );

        self.set_kokoro_status(ModelStatus::Ready);
        Ok(())
    }
}

// ── Process-global singleton ──────────────────────────────────────────────────

/// Process-global `ModelManager`. Initialized on first access.
///
/// `None` only if the home directory cannot be resolved (extremely rare).
static GLOBAL_MODEL_MANAGER: OnceLock<Option<ModelManager>> = OnceLock::new();

/// Return a reference to the process-global `ModelManager`.
pub fn global_model_manager() -> Option<&'static ModelManager> {
    GLOBAL_MODEL_MANAGER.get_or_init(ModelManager::new).as_ref()
}

// ── Standalone helpers (used by the STT pipeline) ────────────────────────────

/// Path to the Moonshine model directory, or `None` if not ready.
pub fn moonshine_model_dir() -> Option<PathBuf> {
    global_model_manager()?.moonshine_model_dir()
}

/// `true` if all expected Moonshine model files are present on disk.
pub fn is_moonshine_ready() -> bool {
    global_model_manager()
        .map(|m| m.is_moonshine_ready())
        .unwrap_or(false)
}

/// Path to the Kokoro model directory, or `None` if not ready.
pub fn kokoro_model_dir() -> Option<PathBuf> {
    global_model_manager()?.kokoro_model_dir()
}

/// `true` if all expected Kokoro model files are present on disk.
pub fn is_kokoro_ready() -> bool {
    global_model_manager()
        .map(|m| m.is_kokoro_ready())
        .unwrap_or(false)
}
