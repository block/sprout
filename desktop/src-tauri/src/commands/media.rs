use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use nostr::{EventBuilder, JsonUtil, Keys, Kind, Tag, Timestamp};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::State;

use crate::app_state::AppState;
use crate::relay::relay_api_base_url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobDescriptor {
    pub url: String,
    pub sha256: String,
    pub size: u64,
    #[serde(rename = "type")]
    pub mime_type: String,
    pub uploaded: i64,
    pub dim: Option<String>,
    pub blurhash: Option<String>,
    pub thumb: Option<String>,
    /// Video duration in seconds. `None` for non-video blobs.
    pub duration: Option<f64>,
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Extract the server authority from a URL for BUD-11 server tag scoping.
///
/// Returns `host` for default ports (80/443), `host:port` for non-default ports.
fn extract_server_authority(url_str: &str) -> Option<String> {
    let parsed = url::Url::parse(url_str).ok()?;
    let host = parsed.host_str()?;
    match parsed.port() {
        Some(port) => Some(format!("{host}:{port}")),
        None => Some(host.to_string()),
    }
}

/// Resolve the real filesystem path of an already-opened file descriptor.
///
/// Returns the path the kernel associates with the inode, not the pathname
/// used to open it. Immune to post-open renames/symlink swaps.
#[cfg(target_os = "macos")]
fn fd_real_path(file: &std::fs::File) -> Result<std::path::PathBuf, String> {
    use std::os::unix::io::AsRawFd;
    let fd = file.as_raw_fd();
    let mut buf = vec![0u8; libc::PATH_MAX as usize];
    let ret = unsafe { libc::fcntl(fd, libc::F_GETPATH, buf.as_mut_ptr()) };
    if ret == -1 {
        return Err(format!(
            "fcntl F_GETPATH failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let nul = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    let s = std::str::from_utf8(&buf[..nul]).map_err(|e| e.to_string())?;
    Ok(std::path::PathBuf::from(s))
}

#[cfg(target_os = "linux")]
fn fd_real_path(file: &std::fs::File) -> Result<std::path::PathBuf, String> {
    use std::os::unix::io::AsRawFd;
    let fd = file.as_raw_fd();
    std::fs::read_link(format!("/proc/self/fd/{fd}")).map_err(|e| e.to_string())
}

#[cfg(target_os = "windows")]
fn fd_real_path(file: &std::fs::File) -> Result<std::path::PathBuf, String> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFinalPathNameByHandleW, FILE_NAME_NORMALIZED,
    };
    let handle = file.as_raw_handle() as isize;
    let mut buf = vec![0u16; 1024];
    let len = unsafe {
        GetFinalPathNameByHandleW(
            handle,
            buf.as_mut_ptr(),
            buf.len() as u32,
            FILE_NAME_NORMALIZED,
        )
    };
    if len == 0 {
        return Err(format!(
            "GetFinalPathNameByHandleW failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let path_str = String::from_utf16_lossy(&buf[..len as usize]);
    // Strip \\?\ prefix that Windows adds
    let cleaned = path_str.strip_prefix(r"\\?\").unwrap_or(&path_str);
    Ok(std::path::PathBuf::from(cleaned))
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn fd_real_path(_file: &std::fs::File) -> Result<std::path::PathBuf, String> {
    Err("fd_real_path not supported on this platform".to_string())
}

/// MIME allowlist — must match the server's allowed types.
const ALLOWED_MIME: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/webp",
    "video/mp4",
];

fn detect_and_validate_mime(body: &[u8]) -> Result<String, String> {
    let mime = infer::get(body)
        .map(|t| t.mime_type().to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string());
    if !ALLOWED_MIME.contains(&mime.as_str()) {
        return Err(format!("unsupported file type: {mime}"));
    }
    Ok(mime)
}

fn sign_blossom_upload_auth(keys: &Keys, sha256: &str) -> Result<nostr::Event, String> {
    let now = Timestamp::now().as_u64();
    let mut tags = vec![
        Tag::parse(vec!["t", "upload"]).map_err(|e| e.to_string())?,
        Tag::parse(vec!["x", sha256]).map_err(|e| e.to_string())?,
        Tag::parse(vec!["expiration", &(now + 300).to_string()]).map_err(|e| e.to_string())?,
    ];
    let base_url = relay_api_base_url();
    if let Some(domain) = extract_server_authority(&base_url) {
        tags.push(Tag::parse(vec!["server".to_string(), domain]).map_err(|e| e.to_string())?);
    }
    EventBuilder::new(Kind::from(24242), "Upload sprout-media")
        .tags(tags)
        .sign_with_keys(keys)
        .map_err(|e| e.to_string())
}

/// Execute the upload HTTP request. Shared by all upload entry points.
// TODO(v2): Stream large video files to the relay instead of buffering in RAM.
// Current approach works for small/medium videos but will OOM on 500MB files.
async fn do_upload(
    body: Vec<u8>,
    mime: &str,
    state: &State<'_, AppState>,
) -> Result<BlobDescriptor, String> {
    let sha256 = hex::encode(Sha256::digest(&body));

    let auth_event = {
        let keys = state.keys.lock().map_err(|e| e.to_string())?;
        sign_blossom_upload_auth(&keys, &sha256)?
    };

    let auth_header = format!(
        "Nostr {}",
        URL_SAFE_NO_PAD.encode(auth_event.as_json().as_bytes())
    );

    let base_url = relay_api_base_url();
    let mut req = state
        .http_client
        .put(format!("{base_url}/media/upload"))
        .header("Authorization", &auth_header)
        .header("Content-Type", mime)
        .header("X-SHA-256", &sha256);

    if let Some(ref token) = state.configured_api_token {
        req = req.header("X-Auth-Token", token.as_str());
    } else if let Ok(guard) = state.session_token.lock() {
        if let Some(ref token) = *guard {
            req = req.header("X-Auth-Token", token.as_str());
        }
    }

    let resp = req
        .body(body)
        .send()
        .await
        .map_err(|e| format!("upload failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("upload failed ({status}): {text}"));
    }

    resp.json::<BlobDescriptor>()
        .await
        .map_err(|e| format!("parse failed: {e}"))
}

// ── Commands ─────────────────────────────────────────────────────────────────

/// Upload a file that is already in the OS temp directory.
///
/// Trust boundary: only reads files inside `temp_dir()`. Opens the fd first,
/// then resolves the fd's real path to verify containment (TOCTOU-safe).
#[tauri::command]
pub async fn upload_media(
    file_path: String,
    is_temp: bool,
    state: State<'_, AppState>,
) -> Result<BlobDescriptor, String> {
    let path = std::path::Path::new(&file_path);
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;

    let fd_path = fd_real_path(&file)?;
    let canonical_temp = std::env::temp_dir()
        .canonicalize()
        .unwrap_or_else(|_| std::env::temp_dir());
    if !fd_path.starts_with(&canonical_temp) {
        return Err("upload source must be in system temp directory".to_string());
    }

    use std::io::Read;
    let mut body = Vec::new();
    file.read_to_end(&mut body)
        .map_err(|e| format!("failed to read file: {e}"))?;
    drop(file);

    if is_temp {
        let _ = std::fs::remove_file(&fd_path);
    }

    let mime = detect_and_validate_mime(&body)?;
    do_upload(body, &mime, &state).await
}

// ── Video transcode helpers ──────────────────────────────────────────────────

/// Check if ffmpeg is available on PATH.
fn find_ffmpeg() -> Result<(), String> {
    match std::process::Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
    {
        Ok(s) if s.success() => Ok(()),
        Ok(_) => Err(
            "ffmpeg was found but returned an error — it may be broken or misconfigured"
                .to_string(),
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(
            "ffmpeg is required for video uploads but was not found.\n\n\
             Install it:\n  \
             macOS:   brew install ffmpeg\n  \
             Linux:   sudo apt install ffmpeg\n  \
             Windows: winget install ffmpeg"
                .to_string(),
        ),
        Err(e) => Err(format!("failed to check for ffmpeg: {e}")),
    }
}

/// Detect if a file is a video based on magic bytes.
fn is_video_file(buf: &[u8]) -> bool {
    infer::get(buf).map_or(false, |t| t.mime_type().starts_with("video/"))
}

/// Transcode any video file to H.264/AAC/MP4/fast-start via ffmpeg.
///
/// Always re-encodes — handles HEVC, VP9, ProRes, non-faststart MP4, 10-bit,
/// wrong pixel format, MOV containers, etc. Output is guaranteed to pass the
/// relay's `validate_video_file()`.
///
/// Returns the path to a temp file. Caller must clean up.
fn transcode_to_mp4(source: &std::path::Path) -> Result<std::path::PathBuf, String> {
    find_ffmpeg()?;

    // UUID-based temp path — unique across concurrent uploads.
    let output =
        std::env::temp_dir().join(format!("sprout-transcode-{}.mp4", uuid::Uuid::new_v4()));

    let result = std::process::Command::new("ffmpeg")
        .arg("-y")
        .arg("-i")
        .arg(source) // OsStr — handles non-UTF-8 paths on Unix
        .args([
            "-c:v",
            "libx264",
            "-preset",
            "fast",
            "-crf",
            "23",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-b:a",
            "128k",
            "-movflags",
            "+faststart",
        ])
        .arg(&output)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("failed to run ffmpeg: {e}"))?;

    if !result.status.success() {
        let _ = std::fs::remove_file(&output);
        let stderr = String::from_utf8_lossy(&result.stderr);
        let detail = stderr
            .lines()
            .rev()
            .find(|l| !l.is_empty() && !l.starts_with("  "))
            .unwrap_or("unknown error");
        return Err(format!("Video conversion failed: {detail}"));
    }

    Ok(output)
}

/// Open a native file dialog, read the selected file, and upload it.
///
/// All file I/O happens in trusted Rust — the renderer never touches the
/// filesystem. This is the secure path for the 📎 paperclip button.
///
/// **Residual TOCTOU note:** The Tauri dialog plugin returns a pathname, not
/// a file handle, so there is a small race window between dialog return and
/// `File::open()`. This is an inherent limitation of the OS file-picker API
/// (no platform exposes a handle/bookmark from the open-file dialog in a way
/// the Tauri plugin surfaces). The risk is bounded: the attacker must be local
/// and must win a race against an immediate open. Server-side content validation
/// (MIME, image decode, size caps) provides defense-in-depth.
#[tauri::command]
pub async fn pick_and_upload_media(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<BlobDescriptor>, String> {
    use tauri_plugin_dialog::DialogExt;

    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .add_filter(
            "Media",
            &[
                "jpg", "jpeg", "png", "gif", "webp", "mp4", "mov", "mkv", "webm", "avi",
            ],
        )
        .pick_file(move |path| {
            let _ = tx.send(path);
        });

    let selected = rx.await.map_err(|_| "dialog cancelled".to_string())?;
    let file_path = match selected {
        Some(p) => p,
        None => return Ok(None),
    };

    let path = file_path.as_path().ok_or("invalid path")?.to_path_buf();

    // All sync I/O (sniff, transcode, read) runs off the async runtime to
    // avoid blocking Tokio worker threads during long ffmpeg transcodes.
    let body = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
        // Sniff magic bytes to decide: video → transcode, image → direct upload.
        let header = {
            use std::io::Read;
            let mut file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
            let mut buf = [0u8; 4096];
            let n = file.read(&mut buf).map_err(|e| e.to_string())?;
            buf[..n].to_vec()
        };

        if is_video_file(&header) {
            let transcoded = transcode_to_mp4(&path)?;
            let bytes = std::fs::read(&transcoded)
                .map_err(|e| format!("failed to read transcoded file: {e}"));
            let _ = std::fs::remove_file(&transcoded);
            bytes
        } else {
            std::fs::read(&path).map_err(|e| format!("failed to read file: {e}"))
        }
    })
    .await
    .map_err(|e| format!("transcode task failed: {e}"))??;

    let mime = detect_and_validate_mime(&body)?;
    do_upload(body, &mime, &state).await.map(Some)
}

/// Upload raw bytes directly (for paste and drag-drop).
///
/// The renderer already has the bytes in memory from the clipboard/drag event.
/// If the bytes are a video, they're written to a temp file, transcoded via
/// ffmpeg, and the transcoded output is uploaded instead.
#[tauri::command]
pub async fn upload_media_bytes(
    data: Vec<u8>,
    state: State<'_, AppState>,
) -> Result<BlobDescriptor, String> {
    if data.is_empty() {
        return Err("empty upload".to_string());
    }

    let body = if is_video_file(&data) {
        // Video: write to temp → transcode → read result. All blocking I/O
        // runs off the async runtime via spawn_blocking.
        tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
            let tmp_input =
                std::env::temp_dir().join(format!("sprout-drop-{}", uuid::Uuid::new_v4()));
            std::fs::write(&tmp_input, &data)
                .map_err(|e| format!("failed to write temp file: {e}"))?;
            let result = transcode_to_mp4(&tmp_input);
            let _ = std::fs::remove_file(&tmp_input);
            let transcoded = result?;
            let bytes = std::fs::read(&transcoded)
                .map_err(|e| format!("failed to read transcoded file: {e}"));
            let _ = std::fs::remove_file(&transcoded);
            bytes
        })
        .await
        .map_err(|e| format!("transcode task failed: {e}"))??
    } else {
        data
    };

    let mime = detect_and_validate_mime(&body)?;
    do_upload(body, &mime, &state).await
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_server_authority_default_ports() {
        assert_eq!(
            extract_server_authority("https://relay.example.com"),
            Some("relay.example.com".to_string())
        );
        assert_eq!(
            extract_server_authority("https://relay.example.com:443"),
            Some("relay.example.com".to_string())
        );
        assert_eq!(
            extract_server_authority("http://relay.example.com:80"),
            Some("relay.example.com".to_string())
        );
    }

    #[test]
    fn test_extract_server_authority_non_default_ports() {
        assert_eq!(
            extract_server_authority("http://localhost:3000"),
            Some("localhost:3000".to_string())
        );
        assert_eq!(
            extract_server_authority("https://relay.example.com:8443"),
            Some("relay.example.com:8443".to_string())
        );
    }

    #[test]
    fn test_extract_server_authority_ipv6() {
        assert_eq!(
            extract_server_authority("http://[::1]:3000"),
            Some("[::1]:3000".to_string())
        );
    }

    #[test]
    fn test_extract_server_authority_invalid() {
        assert_eq!(extract_server_authority("not-a-url"), None);
        assert_eq!(extract_server_authority(""), None);
    }

    #[test]
    fn test_detect_and_validate_mime_jpeg() {
        // Minimal JPEG: SOI + EOI
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0];
        assert_eq!(detect_and_validate_mime(&jpeg).unwrap(), "image/jpeg");
    }

    #[test]
    fn test_detect_and_validate_mime_rejects_text() {
        let text = b"hello world";
        assert!(detect_and_validate_mime(text).is_err());
    }

    #[test]
    fn test_is_video_file_mp4() {
        // Minimal ftyp box (MP4 magic bytes)
        let ftyp: &[u8] = &[
            0x00, 0x00, 0x00, 0x14, b'f', b't', b'y', b'p', b'i', b's', b'o', b'm', 0x00, 0x00,
            0x00, 0x00, b'i', b's', b'o', b'm',
        ];
        assert!(is_video_file(ftyp));
    }

    #[test]
    fn test_is_video_file_jpeg_is_not_video() {
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0];
        assert!(!is_video_file(&jpeg));
    }

    #[test]
    fn test_is_video_file_empty() {
        assert!(!is_video_file(&[]));
    }

    #[test]
    fn test_find_ffmpeg_runs() {
        // This test verifies the function doesn't panic.
        // It may pass or fail depending on whether ffmpeg is installed.
        let _ = find_ffmpeg();
    }
}
