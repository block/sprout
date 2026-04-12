//! Model download manager for STT (Moonshine) and TTS (Supertonic) models.
//!
//! Mental model:
//!   app launch → start_moonshine_download (background) → ~/.sprout/models/moonshine-tiny/
//!   app launch → start_supertonic_download (background) → ~/.sprout/models/supertonic/
//!   STT pipeline → is_moonshine_ready() → moonshine_model_dir() → run inference
//!   TTS pipeline → is_supertonic_ready() → supertonic_model_dir() → run synthesis
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
    "preprocess.onnx",
    "encode.int8.onnx",
    "cached_decode.int8.onnx",
    "uncached_decode.int8.onnx",
    "tokens.txt",
];

/// HuggingFace base URL for Supertonic model files.
const SUPERTONIC_HF_BASE: &str =
    "https://huggingface.co/Supertone/supertonic-2/resolve/main";

/// Final directory name under `~/.sprout/models/`.
const SUPERTONIC_MODEL_DIR_NAME: &str = "supertonic";

/// All files that must be present for Supertonic to be considered ready.
const SUPERTONIC_EXPECTED_FILES: &[&str] = &[
    "duration_predictor.onnx",
    "text_encoder.onnx",
    "vector_estimator.onnx",
    "vocoder.onnx",
    "tts.json",
    "unicode_indexer.json",
    "F1.json",
];

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
    pub supertonic: ModelStatus,
}

// ── Platform-gated tar extraction ─────────────────────────────────────────────

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
    supertonic_status: Arc<Mutex<ModelStatus>>,
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
            supertonic_status: Arc::new(Mutex::new(ModelStatus::NotDownloaded)),
        })
    }

    // ── Moonshine ─────────────────────────────────────────────────────────────

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

    // ── Supertonic ────────────────────────────────────────────────────────────

    /// Returns the path to the Supertonic model directory, or `None` if not ready.
    pub fn supertonic_model_dir(&self) -> Option<PathBuf> {
        if self.is_supertonic_ready() {
            Some(self.models_dir.join(SUPERTONIC_MODEL_DIR_NAME))
        } else {
            None
        }
    }

    /// Returns `true` if all expected Supertonic model files are present on disk.
    pub fn is_supertonic_ready(&self) -> bool {
        let dir = self.models_dir.join(SUPERTONIC_MODEL_DIR_NAME);
        SUPERTONIC_EXPECTED_FILES.iter().all(|f| dir.join(f).is_file())
    }

    /// Current Supertonic download status.
    pub fn supertonic_status(&self) -> ModelStatus {
        self.supertonic_status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Trigger a background download of the Supertonic TTS model (~253 MB total).
    ///
    /// Returns immediately. Progress is tracked via `supertonic_status()`.
    /// No-op if the model is already ready or a download is already running.
    pub fn start_supertonic_download(&self, http_client: reqwest::Client) {
        if self.is_supertonic_ready() {
            *self.supertonic_status.lock().unwrap_or_else(|e| e.into_inner()) =
                ModelStatus::Ready;
            return;
        }

        {
            let mut status = self.supertonic_status.lock().unwrap_or_else(|e| e.into_inner());
            match *status {
                ModelStatus::Downloading { .. } | ModelStatus::Ready => return,
                _ => {}
            }
            *status = ModelStatus::Downloading { progress_percent: 0 };
        }

        let manager = self.clone();
        tokio::spawn(async move {
            if let Err(e) = manager.download_supertonic_model(http_client).await {
                eprintln!("sprout-desktop: supertonic download failed: {e}");
                *manager
                    .supertonic_status
                    .lock()
                    .unwrap_or_else(|e2| e2.into_inner()) = ModelStatus::Error(e);
            }
        });
    }

    /// Trigger a background download of the Moonshine model.
    ///
    /// Returns immediately. Progress is tracked via `moonshine_status()`.
    /// No-op if the model is already ready or a download is already running.
    pub fn start_moonshine_download(&self, http_client: reqwest::Client) {
        if self.is_moonshine_ready() {
            *self
                .moonshine_status
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = ModelStatus::Ready;
            return;
        }

        {
            let mut status = self
                .moonshine_status
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            match *status {
                ModelStatus::Downloading { .. } | ModelStatus::Ready => return,
                _ => {}
            }
            *status = ModelStatus::Downloading { progress_percent: 0 };
        }

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

    fn set_moonshine_status(&self, status: ModelStatus) {
        *self
            .moonshine_status
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = status;
    }

    fn set_supertonic_status(&self, status: ModelStatus) {
        *self.supertonic_status.lock().unwrap_or_else(|e| e.into_inner()) = status;
    }

    /// Download, extract, and verify the Moonshine model archive.
    async fn download_moonshine_model(&self, http_client: reqwest::Client) -> Result<(), String> {
        fs::create_dir_all(&self.models_dir).map_err(|e| format!("create models dir: {e}"))?;

        self.set_moonshine_status(ModelStatus::Downloading { progress_percent: 0 });

        let archive_path = self.models_dir.join("moonshine-tiny.tar.bz2");

        eprintln!("sprout-desktop: downloading Moonshine model from {MOONSHINE_DOWNLOAD_URL}");

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

        {
            use tokio::io::AsyncWriteExt;

            let body = response
                .bytes()
                .await
                .map_err(|e| format!("download stream error: {e}"))?;

            if let Some(total) = content_length {
                if total > 0 {
                    let pct = ((body.len() as u64 * 100) / total).min(89) as u8;
                    self.set_moonshine_status(ModelStatus::Downloading { progress_percent: pct });
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

        self.set_moonshine_status(ModelStatus::Downloading { progress_percent: 90 });

        let temp_dir = self.models_dir.join("moonshine-tiny.tmp");
        let final_dir = self.models_dir.join(MOONSHINE_MODEL_DIR_NAME);

        if temp_dir.exists() {
            fs::remove_dir_all(&temp_dir).map_err(|e| format!("remove stale temp dir: {e}"))?;
        }
        fs::create_dir_all(&temp_dir).map_err(|e| format!("create temp dir: {e}"))?;

        eprintln!("sprout-desktop: extracting Moonshine archive…");
        extract_archive(&archive_path, &temp_dir)?;

        let extracted_subdir = temp_dir.join(MOONSHINE_ARCHIVE_SUBDIR);
        if !extracted_subdir.is_dir() {
            let _ = fs::remove_dir_all(&temp_dir);
            return Err(format!(
                "expected subdir '{}' not found after extraction",
                MOONSHINE_ARCHIVE_SUBDIR,
            ));
        }

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

        let backup_dir = final_dir.with_extension("old");
        if final_dir.exists() {
            if backup_dir.exists() {
                let _ = fs::remove_dir_all(&backup_dir);
            }
            fs::rename(&final_dir, &backup_dir).map_err(|e| format!("backup old model: {e}"))?;
        }

        if let Err(e) = fs::rename(&extracted_subdir, &final_dir) {
            if backup_dir.exists() {
                let _ = fs::rename(&backup_dir, &final_dir);
            }
            return Err(format!("install new model: {e}"));
        }

        let _ = fs::remove_dir_all(&backup_dir);
        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::remove_file(&archive_path);

        eprintln!(
            "sprout-desktop: Moonshine model ready at {}",
            final_dir.display()
        );

        self.set_moonshine_status(ModelStatus::Ready);
        Ok(())
    }

    /// Download and verify the Supertonic TTS model files from HuggingFace.
    ///
    /// Downloads 7 files into `~/.sprout/models/supertonic/`.
    /// Files are written to a temp directory first, then moved atomically.
    async fn download_supertonic_model(&self, http_client: reqwest::Client) -> Result<(), String> {
        fs::create_dir_all(&self.models_dir).map_err(|e| format!("create models dir: {e}"))?;

        self.set_supertonic_status(ModelStatus::Downloading { progress_percent: 0 });

        let final_dir = self.models_dir.join(SUPERTONIC_MODEL_DIR_NAME);
        let temp_dir = self.models_dir.join("supertonic.tmp");

        if temp_dir.exists() {
            fs::remove_dir_all(&temp_dir).map_err(|e| format!("remove stale temp dir: {e}"))?;
        }
        fs::create_dir_all(&temp_dir).map_err(|e| format!("create temp dir: {e}"))?;

        // (url_suffix, local_filename)
        let downloads: &[(&str, &str)] = &[
            ("onnx/duration_predictor.onnx", "duration_predictor.onnx"),
            ("onnx/text_encoder.onnx",       "text_encoder.onnx"),
            ("onnx/vector_estimator.onnx",   "vector_estimator.onnx"),
            ("onnx/vocoder.onnx",            "vocoder.onnx"),
            ("onnx/tts.json",                "tts.json"),
            ("onnx/unicode_indexer.json",    "unicode_indexer.json"),
            ("voice_styles/F1.json",         "F1.json"),
        ];

        let total_files = downloads.len() as u8;

        for (i, (url_suffix, filename)) in downloads.iter().enumerate() {
            let url = format!("{SUPERTONIC_HF_BASE}/{url_suffix}");
            eprintln!("sprout-desktop: downloading Supertonic {filename} from {url}");

            let response = http_client
                .get(&url)
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

            // Progress: spread 0–89% across all files.
            let pct = (((i as u8 + 1) * 89) / total_files).min(89);
            self.set_supertonic_status(ModelStatus::Downloading { progress_percent: pct });

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

        self.set_supertonic_status(ModelStatus::Downloading { progress_percent: 90 });

        // Verify all expected files landed in the temp dir.
        let missing: Vec<&str> = SUPERTONIC_EXPECTED_FILES
            .iter()
            .filter(|&&f| !temp_dir.join(f).is_file())
            .copied()
            .collect();

        if !missing.is_empty() {
            let _ = fs::remove_dir_all(&temp_dir);
            return Err(format!(
                "supertonic model verification failed — missing: {}",
                missing.join(", "),
            ));
        }

        // Atomic swap.
        let backup_dir = final_dir.with_extension("old");
        if final_dir.exists() {
            if backup_dir.exists() {
                let _ = fs::remove_dir_all(&backup_dir);
            }
            fs::rename(&final_dir, &backup_dir)
                .map_err(|e| format!("backup old supertonic model: {e}"))?;
        }

        if let Err(e) = fs::rename(&temp_dir, &final_dir) {
            if backup_dir.exists() {
                let _ = fs::rename(&backup_dir, &final_dir);
            }
            return Err(format!("install new supertonic model: {e}"));
        }

        let _ = fs::remove_dir_all(&backup_dir);

        eprintln!(
            "sprout-desktop: Supertonic model ready at {}",
            final_dir.display()
        );

        self.set_supertonic_status(ModelStatus::Ready);
        Ok(())
    }
}

// ── Process-global singleton ──────────────────────────────────────────────────

static GLOBAL_MODEL_MANAGER: OnceLock<Option<ModelManager>> = OnceLock::new();

/// Return a reference to the process-global `ModelManager`.
pub fn global_model_manager() -> Option<&'static ModelManager> {
    GLOBAL_MODEL_MANAGER.get_or_init(ModelManager::new).as_ref()
}

// ── Standalone helpers ────────────────────────────────────────────────────────

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

/// Path to the Supertonic model directory, or `None` if not ready.
pub fn supertonic_model_dir() -> Option<PathBuf> {
    global_model_manager()?.supertonic_model_dir()
}

/// `true` if all expected Supertonic model files are present on disk.
pub fn is_supertonic_ready() -> bool {
    global_model_manager()
        .map(|m| m.is_supertonic_ready())
        .unwrap_or(false)
}

/// Path to a specific voice style JSON, or `None` if not downloaded.
pub fn voice_style_path(voice_name: &str) -> Option<PathBuf> {
    let dir = supertonic_model_dir()?;
    let path = dir.join(format!("{voice_name}.json"));
    if path.is_file() { Some(path) } else { None }
}
