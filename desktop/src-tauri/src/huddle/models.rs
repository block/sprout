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
#[rustfmt::skip]
const SUPERTONIC_FILE_HASHES: &[(&str, &str)] = &[
    ("duration_predictor.onnx", "6d556b3691165c364be91dc0bd894656b5949f5acd2750d8ec2f954010845011"),
    ("text_encoder.onnx",        "dd5f535ed629f7df86071043e15f541ce1b2ab7f1bdbce4c7892b307bca79fa3"),
    ("vector_estimator.onnx",    "105e9d66fd8756876b210a6b4aa03fc393b1eaca3a8dadcc8d9a3bc785c86a35"),
    ("vocoder.onnx",             "19bd51f47a186069c752403518a40f7ea4c647455056d2511f7249691ecddf7c"),
    ("tts.json",                 "ee531d9af9b80438a2ed703e22155ee6c83b12595ab22fd3bb6de94c7502fe96"),
    ("unicode_indexer.json",     "b7662a73a0703f43b97c0f2e089f8e8325e26f5d841aca393b5a54c509c92df1"),
    ("F1.json",                  "6106950ebeb8a5da29ea22075f605db659cd07dbc288a68292543d9129aa250f"),
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

// ── Shared HTTP helpers ───────────────────────────────────────────────────────

/// Send a GET request and return the response, or a descriptive error.
async fn fetch_url(
    client: &reqwest::Client,
    url: &str,
    label: &str,
) -> Result<reqwest::Response, String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("download {label} request failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "download {label} HTTP {}: {}",
            response.status().as_u16(),
            response.status().canonical_reason().unwrap_or("unknown"),
        ));
    }
    Ok(response)
}

/// Create (or recreate) a temp directory, removing any stale one first.
async fn fresh_temp_dir(path: &Path) -> Result<(), String> {
    if path.exists() {
        tokio::fs::remove_dir_all(path)
            .await
            .map_err(|e| format!("remove stale temp dir: {e}"))?;
    }
    tokio::fs::create_dir_all(path)
        .await
        .map_err(|e| format!("create temp dir: {e}"))
}

/// Stream an HTTP response to a file with progress reporting and size limits.
///
/// Calls `progress_fn(bytes_downloaded, content_length)` after each chunk.
/// Returns the total number of bytes written.
async fn download_file<F>(
    response: reqwest::Response,
    dest: &Path,
    max_bytes: u64,
    label: &str,
    progress_fn: F,
) -> Result<u64, String>
where
    F: Fn(u64, Option<u64>),
{
    use tokio::io::AsyncWriteExt;

    let content_length = response.content_length();
    if let Some(total) = content_length {
        if total > max_bytes {
            return Err(format!(
                "download {label} too large: {total} bytes (max {max_bytes})"
            ));
        }
    }

    let mut file = tokio::fs::File::create(dest)
        .await
        .map_err(|e| format!("create {label}: {e}"))?;
    let mut downloaded: u64 = 0;
    let mut response = response;

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("download {label} stream error: {e}"))?
    {
        downloaded += chunk.len() as u64;
        if downloaded > max_bytes {
            let _ = tokio::fs::remove_file(dest).await;
            return Err(format!(
                "download {label} exceeded max size: {downloaded} bytes (max {max_bytes})"
            ));
        }
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("write {label}: {e}"))?;
        progress_fn(downloaded, content_length);
    }

    file.flush()
        .await
        .map_err(|e| format!("flush {label}: {e}"))?;
    Ok(downloaded)
}

// ── ModelSlot ─────────────────────────────────────────────────────────────────

/// Per-model state + config. `ModelManager` owns two of these (moonshine, supertonic).
#[derive(Clone)]
struct ModelSlot {
    dir_name: &'static str,                  // subdir under ~/.sprout/models/
    expected_files: &'static [&'static str], // files required for "ready"
    version: &'static str,                   // manifest version; increment to force re-download
    status: Arc<Mutex<ModelStatus>>,
    just_ready: Arc<AtomicBool>, // fires once when download completes
}

impl ModelSlot {
    fn new(
        dir_name: &'static str,
        expected_files: &'static [&'static str],
        version: &'static str,
    ) -> Self {
        Self {
            dir_name,
            expected_files,
            version,
            status: Arc::new(Mutex::new(ModelStatus::NotDownloaded)),
            just_ready: Arc::new(AtomicBool::new(false)),
        }
    }

    fn model_dir(&self, models_dir: &Path) -> PathBuf {
        models_dir.join(self.dir_name)
    }

    fn is_ready(&self, models_dir: &Path) -> bool {
        let dir = self.model_dir(models_dir);
        std::fs::read_to_string(dir.join(MANIFEST_FILENAME))
            .map(|v| v.trim() == self.version)
            .unwrap_or(false)
            && self.expected_files.iter().all(|f| dir.join(f).is_file())
    }

    fn dir_if_ready(&self, models_dir: &Path) -> Option<PathBuf> {
        self.is_ready(models_dir)
            .then(|| self.model_dir(models_dir))
    }

    fn status(&self) -> ModelStatus {
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    fn set_status(&self, s: ModelStatus) {
        *self.status.lock().unwrap_or_else(|e| e.into_inner()) = s;
    }
    fn take_ready(&self) -> bool {
        self.just_ready.swap(false, Ordering::AcqRel)
    }

    /// Spawn a background download task if not already ready or downloading.
    fn start_download<F, Fut>(
        &self,
        models_dir: &Path,
        http_client: reqwest::Client,
        name: &'static str,
        download_fn: F,
    ) where
        F: FnOnce(reqwest::Client) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<(), String>> + Send,
    {
        if self.is_ready(models_dir) {
            self.set_status(ModelStatus::Ready);
            return;
        }
        {
            let mut st = self.status.lock().unwrap_or_else(|e| e.into_inner());
            match *st {
                ModelStatus::Downloading { .. } | ModelStatus::Ready => return,
                _ => {}
            }
            *st = ModelStatus::Downloading {
                progress_percent: 0,
            };
        }
        let slot = self.clone();
        tokio::spawn(async move {
            if let Err(e) = download_fn(http_client).await {
                eprintln!("sprout-desktop: {name} download failed: {e}");
                slot.set_status(ModelStatus::Error(e));
            }
        });
    }

    /// Verify files in `source_dir`, atomic-swap into final location, write manifest, signal ready.
    /// `temp_cleanup`: optional extra dir to remove (e.g. outer extraction dir for Moonshine).
    async fn verify_and_install(
        &self,
        models_dir: &Path,
        source_dir: &Path,
        temp_cleanup: Option<&Path>,
    ) -> Result<(), String> {
        let missing: Vec<&str> = self
            .expected_files
            .iter()
            .filter(|&&f| !source_dir.join(f).is_file())
            .copied()
            .collect();
        if !missing.is_empty() {
            return Err(format!(
                "model verification failed — missing: {}",
                missing.join(", ")
            ));
        }

        let final_dir = self.model_dir(models_dir);
        let backup_dir = final_dir.with_extension("old");

        if final_dir.exists() {
            if backup_dir.exists() {
                let _ = tokio::fs::remove_dir_all(&backup_dir).await;
            }
            tokio::fs::rename(&final_dir, &backup_dir)
                .await
                .map_err(|e| format!("backup old model: {e}"))?;
        }
        if let Err(e) = tokio::fs::rename(source_dir, &final_dir).await {
            if backup_dir.exists() {
                let _ = tokio::fs::rename(&backup_dir, &final_dir).await;
            }
            return Err(format!("install new model: {e}"));
        }

        std::fs::write(final_dir.join(MANIFEST_FILENAME), self.version)
            .map_err(|e| format!("write model manifest: {e}"))?;
        let _ = tokio::fs::remove_dir_all(&backup_dir).await;
        if let Some(extra) = temp_cleanup {
            let _ = tokio::fs::remove_dir_all(extra).await;
        }

        self.set_status(ModelStatus::Ready);
        self.just_ready.store(true, Ordering::Release);
        Ok(())
    }
}

// ── ModelManager ──────────────────────────────────────────────────────────────

/// Manages download and location of STT/TTS model files.
///
/// Cheap to clone — all inner state is behind `Arc`.
#[derive(Clone)]
pub struct ModelManager {
    /// `~/.sprout/models/`
    models_dir: PathBuf,
    moonshine: ModelSlot,
    supertonic: ModelSlot,
}

impl ModelManager {
    /// Create a new `ModelManager` rooted at `~/.sprout/models/`.
    ///
    /// Returns `None` if the home directory cannot be resolved.
    pub fn new() -> Option<Self> {
        let models_dir = dirs::home_dir()?.join(".sprout").join("models");
        Some(Self {
            models_dir,
            moonshine: ModelSlot::new(
                MOONSHINE_MODEL_DIR_NAME,
                MOONSHINE_EXPECTED_FILES,
                MOONSHINE_MODEL_VERSION,
            ),
            supertonic: ModelSlot::new(
                SUPERTONIC_MODEL_DIR_NAME,
                SUPERTONIC_EXPECTED_FILES,
                SUPERTONIC_MODEL_VERSION,
            ),
        })
    }

    // ── Moonshine accessors ───────────────────────────────────────────────────

    /// Path to the Moonshine model directory, or `None` if not ready.
    pub fn moonshine_model_dir(&self) -> Option<PathBuf> {
        self.moonshine.dir_if_ready(&self.models_dir)
    }
    /// `true` if all Moonshine files are present and the manifest version matches.
    pub fn is_moonshine_ready(&self) -> bool {
        self.moonshine.is_ready(&self.models_dir)
    }
    /// Current Moonshine download status.
    pub fn moonshine_status(&self) -> ModelStatus {
        self.moonshine.status()
    }
    /// Returns `true` once when Moonshine just became ready. Resets the flag.
    pub fn take_moonshine_ready(&self) -> bool {
        self.moonshine.take_ready()
    }

    // ── Supertonic accessors ──────────────────────────────────────────────────

    /// Path to the Supertonic model directory, or `None` if not ready.
    pub fn supertonic_model_dir(&self) -> Option<PathBuf> {
        self.supertonic.dir_if_ready(&self.models_dir)
    }
    /// `true` if all Supertonic files are present and the manifest version matches.
    pub fn is_supertonic_ready(&self) -> bool {
        self.supertonic.is_ready(&self.models_dir)
    }
    /// Current Supertonic download status.
    pub fn supertonic_status(&self) -> ModelStatus {
        self.supertonic.status()
    }
    /// Returns `true` once when Supertonic just became ready. Resets the flag.
    pub fn take_supertonic_ready(&self) -> bool {
        self.supertonic.take_ready()
    }

    // ── Download triggers ─────────────────────────────────────────────────────

    /// Start a background Moonshine download. No-op if already ready or downloading.
    pub fn start_moonshine_download(&self, http_client: reqwest::Client) {
        let manager = self.clone();
        self.moonshine.start_download(
            &self.models_dir,
            http_client,
            "moonshine",
            move |client| async move { manager.download_moonshine_model(client).await },
        );
    }

    /// Start a background Supertonic download (~253 MB). No-op if already ready or downloading.
    pub fn start_supertonic_download(&self, http_client: reqwest::Client) {
        let manager = self.clone();
        self.supertonic.start_download(
            &self.models_dir,
            http_client,
            "supertonic",
            move |client| async move { manager.download_supertonic_model(client).await },
        );
    }

    // ── Private download implementations ─────────────────────────────────────

    /// Download, extract, and verify the Moonshine model archive.
    async fn download_moonshine_model(&self, http_client: reqwest::Client) -> Result<(), String> {
        tokio::fs::create_dir_all(&self.models_dir)
            .await
            .map_err(|e| format!("create models dir: {e}"))?;

        let archive_path = self.models_dir.join("moonshine-tiny.tar.bz2");
        let temp_dir = self.models_dir.join("moonshine-tiny.tmp");

        eprintln!("sprout-desktop: downloading Moonshine model from {MOONSHINE_DOWNLOAD_URL}");
        let response = fetch_url(&http_client, MOONSHINE_DOWNLOAD_URL, "moonshine archive").await?;

        let slot = self.moonshine.clone();
        let bytes = download_file(
            response,
            &archive_path,
            MAX_MOONSHINE_DOWNLOAD_BYTES,
            "moonshine archive",
            |downloaded, content_length| {
                if let Some(total) = content_length {
                    if total > 0 {
                        let pct = ((downloaded * 89) / total).min(89) as u8;
                        slot.set_status(ModelStatus::Downloading {
                            progress_percent: pct,
                        });
                    }
                }
            },
        )
        .await?;
        eprintln!("sprout-desktop: downloaded {bytes} bytes, wrote to disk");

        // Verify archive integrity before extraction.
        let hash = sha256_file(&archive_path).await?;
        if hash != MOONSHINE_ARCHIVE_SHA256 {
            let _ = tokio::fs::remove_file(&archive_path).await;
            return Err(format!(
                "Moonshine archive integrity check failed: expected {MOONSHINE_ARCHIVE_SHA256}, got {hash}"
            ));
        }

        self.moonshine.set_status(ModelStatus::Downloading {
            progress_percent: 90,
        });
        fresh_temp_dir(&temp_dir).await?;

        eprintln!("sprout-desktop: extracting Moonshine archive…");
        let (ap, td) = (archive_path.clone(), temp_dir.clone());
        tokio::task::spawn_blocking(move || extract_archive(&ap, &td))
            .await
            .map_err(|e| format!("tar task panicked: {e}"))??;

        let extracted_subdir = temp_dir.join(MOONSHINE_ARCHIVE_SUBDIR);
        if !extracted_subdir.is_dir() {
            let _ = tokio::fs::remove_dir_all(&temp_dir).await;
            return Err(format!(
                "expected subdir '{MOONSHINE_ARCHIVE_SUBDIR}' not found after extraction"
            ));
        }

        // verify_and_install takes the subdir (actual model files); temp_cleanup removes outer dir.
        if let Err(e) = self
            .moonshine
            .verify_and_install(&self.models_dir, &extracted_subdir, Some(&temp_dir))
            .await
        {
            let _ = tokio::fs::remove_dir_all(&temp_dir).await;
            let _ = tokio::fs::remove_file(&archive_path).await;
            return Err(e);
        }
        let _ = tokio::fs::remove_file(&archive_path).await;

        eprintln!(
            "sprout-desktop: Moonshine model ready at {}",
            self.moonshine.model_dir(&self.models_dir).display()
        );
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

        let temp_dir = self.models_dir.join("supertonic.tmp");
        fresh_temp_dir(&temp_dir).await?;

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

            let response = fetch_url(&http_client, &url, filename).await.map_err(|e| {
                let _ = std::fs::remove_dir_all(&temp_dir);
                e
            })?;

            let dest = temp_dir.join(filename);
            let slot = self.supertonic.clone();
            let file_index = i as u32;
            let bytes = download_file(
                response,
                &dest,
                MAX_SUPERTONIC_FILE_BYTES,
                filename,
                |downloaded, content_length| {
                    if let Some(total) = content_length {
                        if total > 0 {
                            let file_frac = downloaded as f64 / total as f64;
                            let base = (file_index as f64 / total_files as f64) * 89.0;
                            let span = 89.0 / total_files as f64;
                            let pct = (base + span * file_frac).min(89.0) as u8;
                            slot.set_status(ModelStatus::Downloading {
                                progress_percent: pct,
                            });
                        }
                    }
                },
            )
            .await
            .map_err(|e| {
                let _ = std::fs::remove_dir_all(&temp_dir);
                e
            })?;
            eprintln!("sprout-desktop: downloaded {bytes} bytes ({filename}), wrote to disk");

            // Verify file integrity against pinned hash.
            if let Some(&(_, expected)) =
                SUPERTONIC_FILE_HASHES.iter().find(|(n, _)| *n == *filename)
            {
                let actual = sha256_file(&dest).await?;
                if actual != expected {
                    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
                    return Err(format!(
                        "Supertonic {filename} integrity check failed: expected {expected}, got {actual}"
                    ));
                }
            }

            // Ensure progress reflects file completion even without content-length.
            let pct = (((i as u32 + 1) * 89) / total_files).min(89) as u8;
            self.supertonic.set_status(ModelStatus::Downloading {
                progress_percent: pct,
            });
        }

        self.supertonic.set_status(ModelStatus::Downloading {
            progress_percent: 90,
        });

        if let Err(e) = self
            .supertonic
            .verify_and_install(&self.models_dir, &temp_dir, None)
            .await
        {
            let _ = tokio::fs::remove_dir_all(&temp_dir).await;
            return Err(e);
        }

        eprintln!(
            "sprout-desktop: Supertonic model ready at {}",
            self.supertonic.model_dir(&self.models_dir).display()
        );
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
