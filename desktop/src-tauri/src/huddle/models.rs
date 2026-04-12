//! Model download manager for STT (Moonshine) and TTS (Supertonic) models.
//!
//! Mental model:
//!   app launch → start_moonshine_download (background) → ~/.sprout/models/moonshine-tiny/
//!   app launch → start_supertonic_download (background) → ~/.sprout/models/supertonic/
//!   STT pipeline → is_moonshine_ready() → moonshine_model_dir() → run inference
//!   TTS pipeline → is_supertonic_ready() → supertonic_model_dir() → run synthesis
//!
//! Models are downloaded once and cached. A version manifest (`.sprout-model-manifest`)
//! is written alongside model files — if the on-disk version doesn't match the
//! compiled-in version, the model is re-downloaded.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// ── Integrity verification ────────────────────────────────────────────────────
//
// All model artifacts are verified against pinned SHA-256 hashes before
// installation. This is defense-in-depth: HTTPS protects the transport,
// hashes protect the content.
//
// To recompute hashes: download each file, run `shasum -a 256 <file>`, and
// update the corresponding constant.

/// SHA-256 hash of the Moonshine archive (sherpa-onnx-moonshine-tiny-en-int8.tar.bz2).
/// Computed from a known-good download. Update when upgrading model versions.
const MOONSHINE_ARCHIVE_SHA256: &str =
    "d5fe6ec4334fef36255b2a4010412cad4c007e33103fec62fb5d17cad88086f2";

/// SHA-256 hashes for individual Supertonic model files.
/// Computed from known-good downloads. Update when upgrading model versions.
const SUPERTONIC_FILE_HASHES: &[(&str, &str)] = &[
    (
        "duration_predictor.onnx",
        "6d556b3691165c364be91dc0bd894656b5949f5acd2750d8ec2f954010845011",
    ),
    (
        "text_encoder.onnx",
        "dd5f535ed629f7df86071043e15f541ce1b2ab7f1bdbce4c7892b307bca79fa3",
    ),
    (
        "vector_estimator.onnx",
        "105e9d66fd8756876b210a6b4aa03fc393b1eaca3a8dadcc8d9a3bc785c86a35",
    ),
    (
        "vocoder.onnx",
        "19bd51f47a186069c752403518a40f7ea4c647455056d2511f7249691ecddf7c",
    ),
    (
        "tts.json",
        "ee531d9af9b80438a2ed703e22155ee6c83b12595ab22fd3bb6de94c7502fe96",
    ),
    (
        "unicode_indexer.json",
        "b7662a73a0703f43b97c0f2e089f8e8325e26f5d841aca393b5a54c509c92df1",
    ),
    (
        "F1.json",
        "6106950ebeb8a5da29ea22075f605db659cd07dbc288a68292543d9129aa250f",
    ),
];

// ── Model versioning ──────────────────────────────────────────────────────────
//
// A version manifest is written alongside model files after successful download.
// If the on-disk manifest doesn't match the compiled-in version, the model is
// considered stale and re-downloaded. Increment when upgrading model files.

/// Model manifest version for Moonshine. Increment when upgrading model files.
const MOONSHINE_MODEL_VERSION: &str = "1";

/// Model manifest version for Supertonic. Increment when upgrading model files.
const SUPERTONIC_MODEL_VERSION: &str = "1";

/// Filename for the version manifest written alongside model files.
const MANIFEST_FILENAME: &str = ".sprout-model-manifest";

// ── Constants ─────────────────────────────────────────────────────────────────

/// Maximum expected Moonshine archive size (200 MB — actual is ~50 MB).
const MAX_MOONSHINE_DOWNLOAD_BYTES: u64 = 200 * 1024 * 1024;

/// Maximum expected Supertonic file size (150 MB per file).
const MAX_SUPERTONIC_FILE_BYTES: u64 = 150 * 1024 * 1024;

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
const SUPERTONIC_HF_BASE: &str = "https://huggingface.co/Supertone/supertonic-2/resolve/main";

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

// ── Safe archive extraction ───────────────────────────────────────────────────

/// Extract a .tar.bz2 archive safely using Rust-native crates.
///
/// The `tar` crate rejects path traversal (absolute paths, `..` components)
/// by default in `unpack()`. We add an explicit pre-check as defense-in-depth.
fn extract_archive(archive_path: &Path, dest_dir: &Path) -> Result<(), String> {
    use bzip2::read::BzDecoder;
    use std::fs::File;
    use tar::Archive;

    let file = File::open(archive_path).map_err(|e| format!("open archive: {e}"))?;
    let decoder = BzDecoder::new(file);
    let mut archive = Archive::new(decoder);

    // Pre-validate: check all entries for path safety before extracting anything.
    // This is defense-in-depth — the tar crate also rejects traversal in unpack().
    {
        let file2 =
            File::open(archive_path).map_err(|e| format!("open archive for validation: {e}"))?;
        let decoder2 = BzDecoder::new(file2);
        let mut check_archive = Archive::new(decoder2);
        for entry in check_archive
            .entries()
            .map_err(|e| format!("read archive entries: {e}"))?
        {
            let entry = entry.map_err(|e| format!("archive entry: {e}"))?;
            let path = entry.path().map_err(|e| format!("entry path: {e}"))?;
            let path_str = path.to_string_lossy();

            // Reject absolute paths.
            if path.is_absolute() {
                return Err(format!("archive contains absolute path: {path_str}"));
            }
            // Reject path traversal.
            for component in path.components() {
                if matches!(component, std::path::Component::ParentDir) {
                    return Err(format!("archive contains path traversal: {path_str}"));
                }
            }
            // Reject symlinks.
            if entry.header().entry_type().is_symlink()
                || entry.header().entry_type().is_hard_link()
            {
                return Err(format!("archive contains symlink/hardlink: {path_str}"));
            }
        }
    }

    // Safe to extract — all entries validated.
    archive
        .unpack(dest_dir)
        .map_err(|e| format!("extract archive: {e}"))?;

    Ok(())
}

// ── Hash verification ─────────────────────────────────────────────────────────

/// Compute SHA-256 hash of a file. Returns lowercase hex string.
async fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| format!("read file for hash: {e}"))?;
    let hash = Sha256::digest(&bytes);
    Ok(hex::encode(hash))
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
    /// Set to `true` when Moonshine download completes during an active huddle.
    /// Polled by the huddle system to auto-start STT.
    moonshine_just_ready: Arc<AtomicBool>,
    /// Set to `true` when Supertonic download completes during an active huddle.
    supertonic_just_ready: Arc<AtomicBool>,
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
            moonshine_just_ready: Arc::new(AtomicBool::new(false)),
            supertonic_just_ready: Arc::new(AtomicBool::new(false)),
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

    /// Returns `true` if all expected Moonshine model files are present on disk
    /// and the version manifest matches the compiled-in version.
    pub fn is_moonshine_ready(&self) -> bool {
        let dir = self.models_dir.join(MOONSHINE_MODEL_DIR_NAME);
        let manifest_ok = std::fs::read_to_string(dir.join(MANIFEST_FILENAME))
            .map(|v| v.trim() == MOONSHINE_MODEL_VERSION)
            .unwrap_or(false);
        manifest_ok
            && MOONSHINE_EXPECTED_FILES
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

    /// Returns true (once) if Moonshine just became ready. Resets the flag.
    pub fn take_moonshine_ready(&self) -> bool {
        self.moonshine_just_ready.swap(false, Ordering::AcqRel)
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

    /// Returns `true` if all expected Supertonic model files are present on disk
    /// and the version manifest matches the compiled-in version.
    pub fn is_supertonic_ready(&self) -> bool {
        let dir = self.models_dir.join(SUPERTONIC_MODEL_DIR_NAME);
        let manifest_ok = std::fs::read_to_string(dir.join(MANIFEST_FILENAME))
            .map(|v| v.trim() == SUPERTONIC_MODEL_VERSION)
            .unwrap_or(false);
        manifest_ok
            && SUPERTONIC_EXPECTED_FILES
                .iter()
                .all(|f| dir.join(f).is_file())
    }

    /// Current Supertonic download status.
    pub fn supertonic_status(&self) -> ModelStatus {
        self.supertonic_status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Returns true (once) if Supertonic just became ready. Resets the flag.
    pub fn take_supertonic_ready(&self) -> bool {
        self.supertonic_just_ready.swap(false, Ordering::AcqRel)
    }

    /// Trigger a background download of the Supertonic TTS model (~253 MB total).
    ///
    /// Returns immediately. Progress is tracked via `supertonic_status()`.
    /// No-op if the model is already ready or a download is already running.
    pub fn start_supertonic_download(&self, http_client: reqwest::Client) {
        if self.is_supertonic_ready() {
            *self
                .supertonic_status
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = ModelStatus::Ready;
            return;
        }

        {
            let mut status = self
                .supertonic_status
                .lock()
                .unwrap_or_else(|e| e.into_inner());
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
            *status = ModelStatus::Downloading {
                progress_percent: 0,
            };
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
        *self
            .supertonic_status
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = status;
    }

    /// Download, extract, and verify the Moonshine model archive.
    async fn download_moonshine_model(&self, http_client: reqwest::Client) -> Result<(), String> {
        tokio::fs::create_dir_all(&self.models_dir)
            .await
            .map_err(|e| format!("create models dir: {e}"))?;

        self.set_moonshine_status(ModelStatus::Downloading {
            progress_percent: 0,
        });

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

        // Reject unexpectedly large downloads before we start.
        if let Some(total) = content_length {
            if total > MAX_MOONSHINE_DOWNLOAD_BYTES {
                return Err(format!(
                    "download too large: {total} bytes (max {MAX_MOONSHINE_DOWNLOAD_BYTES})"
                ));
            }
        }

        // Stream to disk instead of buffering the entire archive in memory.
        {
            use tokio::io::AsyncWriteExt;

            let mut file = tokio::fs::File::create(&archive_path)
                .await
                .map_err(|e| format!("create archive file: {e}"))?;

            let mut downloaded: u64 = 0;
            let mut response = response;

            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|e| format!("download stream error: {e}"))?
            {
                downloaded += chunk.len() as u64;

                // Guard against servers that lie about content-length.
                if downloaded > MAX_MOONSHINE_DOWNLOAD_BYTES {
                    let _ = tokio::fs::remove_file(&archive_path).await;
                    return Err(format!(
                        "download exceeded max size during streaming: \
                         {downloaded} bytes (max {MAX_MOONSHINE_DOWNLOAD_BYTES})"
                    ));
                }

                file.write_all(&chunk)
                    .await
                    .map_err(|e| format!("write archive: {e}"))?;

                if let Some(total) = content_length {
                    if total > 0 {
                        let pct = ((downloaded * 89) / total).min(89) as u8;
                        self.set_moonshine_status(ModelStatus::Downloading {
                            progress_percent: pct,
                        });
                    }
                }
            }

            file.flush()
                .await
                .map_err(|e| format!("flush archive: {e}"))?;

            eprintln!("sprout-desktop: downloaded {downloaded} bytes, wrote to disk");
        }

        // Verify archive integrity before extraction.
        let hash = sha256_file(&archive_path).await?;
        if hash != MOONSHINE_ARCHIVE_SHA256 {
            let _ = tokio::fs::remove_file(&archive_path).await;
            return Err(format!(
                "Moonshine archive integrity check failed: expected {}, got {}",
                MOONSHINE_ARCHIVE_SHA256, hash
            ));
        }

        self.set_moonshine_status(ModelStatus::Downloading {
            progress_percent: 90,
        });

        let temp_dir = self.models_dir.join("moonshine-tiny.tmp");
        let final_dir = self.models_dir.join(MOONSHINE_MODEL_DIR_NAME);

        if temp_dir.exists() {
            tokio::fs::remove_dir_all(&temp_dir)
                .await
                .map_err(|e| format!("remove stale temp dir: {e}"))?;
        }
        tokio::fs::create_dir_all(&temp_dir)
            .await
            .map_err(|e| format!("create temp dir: {e}"))?;

        eprintln!("sprout-desktop: extracting Moonshine archive…");
        let archive_path_clone = archive_path.clone();
        let temp_dir_clone = temp_dir.clone();
        tokio::task::spawn_blocking(move || extract_archive(&archive_path_clone, &temp_dir_clone))
            .await
            .map_err(|e| format!("tar task panicked: {e}"))??;

        let extracted_subdir = temp_dir.join(MOONSHINE_ARCHIVE_SUBDIR);
        if !extracted_subdir.is_dir() {
            let _ = tokio::fs::remove_dir_all(&temp_dir).await;
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
            let _ = tokio::fs::remove_dir_all(&temp_dir).await;
            return Err(format!(
                "model verification failed — missing: {}",
                missing.join(", "),
            ));
        }

        let backup_dir = final_dir.with_extension("old");
        if final_dir.exists() {
            if backup_dir.exists() {
                let _ = tokio::fs::remove_dir_all(&backup_dir).await;
            }
            tokio::fs::rename(&final_dir, &backup_dir)
                .await
                .map_err(|e| format!("backup old model: {e}"))?;
        }

        if let Err(e) = tokio::fs::rename(&extracted_subdir, &final_dir).await {
            if backup_dir.exists() {
                let _ = tokio::fs::rename(&backup_dir, &final_dir).await;
            }
            return Err(format!("install new model: {e}"));
        }

        // Write version manifest for cache invalidation on future upgrades.
        std::fs::write(final_dir.join(MANIFEST_FILENAME), MOONSHINE_MODEL_VERSION)
            .map_err(|e| format!("write model manifest: {e}"))?;

        let _ = tokio::fs::remove_dir_all(&backup_dir).await;
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
        let _ = tokio::fs::remove_file(&archive_path).await;

        eprintln!(
            "sprout-desktop: Moonshine model ready at {}",
            final_dir.display()
        );

        self.set_moonshine_status(ModelStatus::Ready);
        self.moonshine_just_ready.store(true, Ordering::Release);
        Ok(())
    }

    /// Download and verify the Supertonic TTS model files from HuggingFace.
    ///
    /// Downloads 7 files into `~/.sprout/models/supertonic/`.
    /// Files are written to a temp directory first, then moved atomically.
    async fn download_supertonic_model(&self, http_client: reqwest::Client) -> Result<(), String> {
        tokio::fs::create_dir_all(&self.models_dir)
            .await
            .map_err(|e| format!("create models dir: {e}"))?;

        self.set_supertonic_status(ModelStatus::Downloading {
            progress_percent: 0,
        });

        let final_dir = self.models_dir.join(SUPERTONIC_MODEL_DIR_NAME);
        let temp_dir = self.models_dir.join("supertonic.tmp");

        if temp_dir.exists() {
            tokio::fs::remove_dir_all(&temp_dir)
                .await
                .map_err(|e| format!("remove stale temp dir: {e}"))?;
        }
        tokio::fs::create_dir_all(&temp_dir)
            .await
            .map_err(|e| format!("create temp dir: {e}"))?;

        // (url_suffix, local_filename)
        let downloads: &[(&str, &str)] = &[
            ("onnx/duration_predictor.onnx", "duration_predictor.onnx"),
            ("onnx/text_encoder.onnx", "text_encoder.onnx"),
            ("onnx/vector_estimator.onnx", "vector_estimator.onnx"),
            ("onnx/vocoder.onnx", "vocoder.onnx"),
            ("onnx/tts.json", "tts.json"),
            ("onnx/unicode_indexer.json", "unicode_indexer.json"),
            ("voice_styles/F1.json", "F1.json"),
        ];

        let total_files = downloads.len() as u32;

        for (i, (url_suffix, filename)) in downloads.iter().enumerate() {
            let url = format!("{SUPERTONIC_HF_BASE}/{url_suffix}");
            eprintln!("sprout-desktop: downloading Supertonic {filename} from {url}");

            let response = http_client
                .get(&url)
                .send()
                .await
                .map_err(|e| format!("download {filename} request failed: {e}"))?;

            if !response.status().is_success() {
                let _ = tokio::fs::remove_dir_all(&temp_dir).await;
                return Err(format!(
                    "download {filename} HTTP {}: {}",
                    response.status().as_u16(),
                    response.status().canonical_reason().unwrap_or("unknown"),
                ));
            }

            let file_content_length = response.content_length();

            // Reject unexpectedly large files before we start.
            if let Some(total) = file_content_length {
                if total > MAX_SUPERTONIC_FILE_BYTES {
                    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
                    return Err(format!(
                        "download {filename} too large: {total} bytes \
                         (max {MAX_SUPERTONIC_FILE_BYTES})"
                    ));
                }
            }

            // Stream to disk instead of buffering the entire file in memory.
            use tokio::io::AsyncWriteExt;
            let dest = temp_dir.join(filename);
            let mut file = tokio::fs::File::create(&dest)
                .await
                .map_err(|e| format!("create {filename}: {e}"))?;

            let mut downloaded: u64 = 0;
            let mut response = response;

            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|e| format!("download {filename} stream error: {e}"))?
            {
                downloaded += chunk.len() as u64;

                // Guard against servers that lie about content-length.
                if downloaded > MAX_SUPERTONIC_FILE_BYTES {
                    let _ = tokio::fs::remove_file(&dest).await;
                    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
                    return Err(format!(
                        "download {filename} exceeded max size during streaming: \
                         {downloaded} bytes (max {MAX_SUPERTONIC_FILE_BYTES})"
                    ));
                }

                file.write_all(&chunk)
                    .await
                    .map_err(|e| format!("write {filename}: {e}"))?;

                // Progress: spread 0–89% across all files, with intra-file granularity.
                if let Some(total) = file_content_length {
                    if total > 0 {
                        let file_frac = downloaded as f64 / total as f64;
                        let base = (i as f64 / total_files as f64) * 89.0;
                        let span = 89.0 / total_files as f64;
                        let pct = (base + span * file_frac).min(89.0) as u8;
                        self.set_supertonic_status(ModelStatus::Downloading {
                            progress_percent: pct,
                        });
                    }
                }
            }

            file.flush()
                .await
                .map_err(|e| format!("flush {filename}: {e}"))?;

            eprintln!("sprout-desktop: downloaded {downloaded} bytes ({filename}), wrote to disk");

            // Verify file integrity against pinned hash.
            let expected_hash = SUPERTONIC_FILE_HASHES
                .iter()
                .find(|(name, _)| *name == *filename)
                .map(|(_, hash)| *hash);

            if let Some(expected) = expected_hash {
                let actual = sha256_file(&dest).await?;
                if actual != expected {
                    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
                    return Err(format!(
                        "Supertonic {filename} integrity check failed: \
                         expected {expected}, got {actual}"
                    ));
                }
            }

            // Ensure progress reflects file completion even without content-length.
            let pct = (((i as u32 + 1) * 89) / total_files).min(89) as u8;
            self.set_supertonic_status(ModelStatus::Downloading {
                progress_percent: pct,
            });
        }

        self.set_supertonic_status(ModelStatus::Downloading {
            progress_percent: 90,
        });

        // Verify all expected files landed in the temp dir.
        let missing: Vec<&str> = SUPERTONIC_EXPECTED_FILES
            .iter()
            .filter(|&&f| !temp_dir.join(f).is_file())
            .copied()
            .collect();

        if !missing.is_empty() {
            let _ = tokio::fs::remove_dir_all(&temp_dir).await;
            return Err(format!(
                "supertonic model verification failed — missing: {}",
                missing.join(", "),
            ));
        }

        // Atomic swap.
        let backup_dir = final_dir.with_extension("old");
        if final_dir.exists() {
            if backup_dir.exists() {
                let _ = tokio::fs::remove_dir_all(&backup_dir).await;
            }
            tokio::fs::rename(&final_dir, &backup_dir)
                .await
                .map_err(|e| format!("backup old supertonic model: {e}"))?;
        }

        if let Err(e) = tokio::fs::rename(&temp_dir, &final_dir).await {
            if backup_dir.exists() {
                let _ = tokio::fs::rename(&backup_dir, &final_dir).await;
            }
            return Err(format!("install new supertonic model: {e}"));
        }

        // Write version manifest for cache invalidation on future upgrades.
        std::fs::write(final_dir.join(MANIFEST_FILENAME), SUPERTONIC_MODEL_VERSION)
            .map_err(|e| format!("write model manifest: {e}"))?;

        let _ = tokio::fs::remove_dir_all(&backup_dir).await;

        eprintln!(
            "sprout-desktop: Supertonic model ready at {}",
            final_dir.display()
        );

        self.set_supertonic_status(ModelStatus::Ready);
        self.supertonic_just_ready.store(true, Ordering::Release);
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
    if path.is_file() {
        Some(path)
    } else {
        None
    }
}
