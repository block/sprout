//! Client-side Blossom upload pipeline for Sprout media.
//!
//! Reads bytes (or a local file), validates MIME type and size, computes SHA-256,
//! signs a kind:24242 Blossom auth event, PUTs to the relay's `/media/upload`
//! endpoint, and returns a [`BlobDescriptor`].
//!
//! # Memory Model
//!
//! This implementation buffers the entire file in RAM to compute the SHA-256
//! hash and reuse the bytes for the HTTP body in a single pass. A two-pass
//! streaming implementation could hash from disk first, then stream the body
//! separately — but that adds complexity for marginal benefit given typical
//! agent upload sizes (images ≤50 MB). For very large videos (approaching
//! 500 MB), callers should wrap in `spawn_blocking`.
//!
//! Requires the `upload` feature.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use nostr::{EventBuilder, JsonUtil, Keys, Kind, Tag, Timestamp};
use sha2::{Digest, Sha256};

// ── Constants ─────────────────────────────────────────────────────────────────

/// MIME types accepted for upload.
pub const ALLOWED_MIMES: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/webp",
    "video/mp4",
];

/// Default maximum file size for image uploads (50 MB).
///
/// This is a client-side preflight check. The relay may have different limits
/// configured via `max_image_bytes` in its environment.
pub const MAX_IMAGE_BYTES: u64 = 50 * 1024 * 1024;

/// Default maximum file size for GIF uploads (10 MB).
///
/// Matches the relay's default `max_gif_bytes` configuration.
pub const MAX_GIF_BYTES: u64 = 10 * 1024 * 1024;

/// Default maximum file size for video uploads (500 MB).
///
/// This is a client-side preflight check. The relay may have different limits
/// configured via `max_video_bytes` in its environment.
pub const MAX_VIDEO_BYTES: u64 = 500 * 1024 * 1024;

// ── Types ─────────────────────────────────────────────────────────────────────

/// Descriptor returned by the relay after a successful upload.
///
/// Mirrors the server's `sprout_media::BlobDescriptor` response.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BlobDescriptor {
    /// Public URL of the uploaded blob.
    pub url: String,
    /// Hex-encoded SHA-256 of the file content.
    pub sha256: String,
    /// File size in bytes.
    pub size: u64,
    /// MIME type (e.g. `image/jpeg`).
    #[serde(rename = "type")]
    pub mime_type: String,
    /// Unix timestamp when the file was uploaded.
    pub uploaded: i64,
    /// Image dimensions as `<width>x<height>` (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dim: Option<String>,
    /// Blurhash placeholder string (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blurhash: Option<String>,
    /// Thumbnail URL (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb: Option<String>,
    /// Duration in seconds for video/audio (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
}

/// Optional configuration for upload behavior.
///
/// Use [`UploadOptions::default()`] for standard behavior (auto-detect server
/// domain from the relay URL, skip server tag for localhost).
#[derive(Debug, Clone, Default)]
pub struct UploadOptions<'a> {
    /// NIP-OA auth tag JSON string for the `x-auth-tag` header.
    pub auth_tag_json: Option<&'a str>,
    /// API token (`sprout_*`) for the `X-Auth-Token` header.
    ///
    /// Required in production when the relay has `require_auth_token=true`.
    /// In dev mode (localhost) this can be omitted.
    pub auth_token: Option<&'a str>,
    /// Override the BUD-11 server domain tag.
    ///
    /// - `None` (default): auto-extracted from the relay URL. Localhost is suppressed.
    /// - `Some(domain)`: use this exact domain string in the server tag.
    pub server_domain: Option<&'a str>,
}

/// Errors from the upload pipeline.
#[derive(Debug, thiserror::Error)]
pub enum UploadError {
    /// File not found on disk.
    #[error("file not found: {0}")]
    FileNotFound(String),
    /// Path exists but is not a regular file.
    #[error("not a file: {0}")]
    NotAFile(String),
    /// File exceeds the size limit for its type.
    #[error("file too large: {size} bytes (max {max})")]
    FileTooLarge {
        /// Actual file size.
        size: u64,
        /// Maximum allowed size.
        max: u64,
    },
    /// MIME type is not in the allowlist.
    #[error("unsupported file type: {0}")]
    UnsupportedMime(String),
    /// Nostr event signing failed.
    #[error("signing failed: {0}")]
    SigningFailed(String),
    /// Server returned a non-success status.
    #[error("upload rejected ({status}): {body}")]
    ServerRejected {
        /// HTTP status code.
        status: u16,
        /// Response body text.
        body: String,
    },
    /// HTTP transport error.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    /// Could not parse the server's response.
    #[error("invalid response: {0}")]
    InvalidResponse(String),
    /// Filesystem I/O error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Upload raw bytes to a Sprout relay.
///
/// Detects MIME from magic bytes, validates type and size, computes SHA-256,
/// signs a Blossom auth event, and PUTs to the relay.
///
/// Takes ownership of `bytes` to avoid an extra copy when building the HTTP body.
pub async fn upload_bytes(
    http: &reqwest::Client,
    keys: &Keys,
    relay_http_url: &str,
    bytes: Vec<u8>,
    opts: &UploadOptions<'_>,
) -> Result<BlobDescriptor, UploadError> {
    // 1. Detect MIME from magic bytes — never trust caller-supplied types.
    let mime = infer::get(&bytes)
        .map(|t| t.mime_type())
        .unwrap_or("application/octet-stream");

    if !ALLOWED_MIMES.contains(&mime) {
        return Err(UploadError::UnsupportedMime(mime.to_string()));
    }

    // 2. Size check against type-specific limits.
    let max_size = match mime {
        "image/gif" => MAX_GIF_BYTES,
        m if m.starts_with("video/") => MAX_VIDEO_BYTES,
        _ => MAX_IMAGE_BYTES,
    };
    let size = bytes.len() as u64;
    if size > max_size {
        return Err(UploadError::FileTooLarge {
            size,
            max: max_size,
        });
    }

    // 3. SHA-256.
    let sha256 = hex::encode(Sha256::digest(&bytes));

    // 4. Resolve server domain: explicit override wins, then auto-extract.
    let auto_domain = extract_server_authority(relay_http_url);
    let server_domain = opts.server_domain.or(auto_domain.as_deref());

    // 5. Sign Blossom auth event (kind:24242).
    let expiry_secs: u64 = if mime.starts_with("video/") {
        3600
    } else {
        600
    };
    let auth_header = sign_blossom_auth(keys, &sha256, expiry_secs, server_domain)?;

    // 6. HTTP PUT with generous timeout for large files.
    let upload_timeout = if mime.starts_with("video/") {
        std::time::Duration::from_secs(600)
    } else {
        std::time::Duration::from_secs(120)
    };

    let url = format!("{}/media/upload", relay_http_url.trim_end_matches('/'));
    let mut req = http
        .put(&url)
        .timeout(upload_timeout)
        .header("Authorization", &auth_header)
        .header("Content-Type", mime)
        .header("X-SHA-256", &sha256);

    if let Some(tag) = opts.auth_tag_json {
        req = req.header("x-auth-tag", tag);
    }
    if let Some(token) = opts.auth_token {
        req = req.header("X-Auth-Token", token);
    }

    let resp = req.body(bytes).send().await?;

    // 7. Handle response.
    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(UploadError::ServerRejected { status, body });
    }

    let body = resp.text().await?;
    serde_json::from_str::<BlobDescriptor>(&body)
        .map_err(|e| UploadError::InvalidResponse(format!("{e}: {body}")))
}

/// Upload a local file to a Sprout relay.
///
/// Validates MIME type from a small prefix before reading the full file,
/// avoiding unnecessary RAM usage for unsupported types. For large files
/// on async runtimes, wrap in `tokio::task::spawn_blocking`.
pub async fn upload_file(
    http: &reqwest::Client,
    keys: &Keys,
    relay_http_url: &str,
    file_path: &str,
    opts: &UploadOptions<'_>,
) -> Result<BlobDescriptor, UploadError> {
    let metadata = std::fs::metadata(file_path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            UploadError::FileNotFound(file_path.to_string())
        } else {
            UploadError::Io(e)
        }
    })?;
    if !metadata.is_file() {
        return Err(UploadError::NotAFile(file_path.to_string()));
    }

    let file_size = metadata.len();

    // Early size rejection before buffering into RAM.
    if file_size > MAX_VIDEO_BYTES {
        return Err(UploadError::FileTooLarge {
            size: file_size,
            max: MAX_VIDEO_BYTES,
        });
    }

    // Read a small prefix to detect MIME and reject unsupported/oversize files
    // before committing to a full read. 4 KiB is enough for all magic-byte checks.
    use std::io::Read;
    let mut file = std::fs::File::open(file_path)?;
    let mut prefix = [0u8; 4096];
    let n = file.read(&mut prefix)?;

    let mime = infer::get(&prefix[..n])
        .map(|t| t.mime_type())
        .unwrap_or("application/octet-stream");

    if !ALLOWED_MIMES.contains(&mime) {
        return Err(UploadError::UnsupportedMime(mime.to_string()));
    }

    // Type-specific size check before full read.
    let max_size = match mime {
        "image/gif" => MAX_GIF_BYTES,
        m if m.starts_with("video/") => MAX_VIDEO_BYTES,
        _ => MAX_IMAGE_BYTES,
    };
    if file_size > max_size {
        return Err(UploadError::FileTooLarge {
            size: file_size,
            max: max_size,
        });
    }

    // Read the remainder with a hard cap — defends against file growth after metadata().
    let mut bytes = prefix[..n].to_vec();
    let remaining_limit = max_size.saturating_sub(n as u64) + 1;
    let mut limited = file.take(remaining_limit);
    limited.read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max_size {
        return Err(UploadError::FileTooLarge {
            size: bytes.len() as u64,
            max: max_size,
        });
    }

    upload_bytes(http, keys, relay_http_url, bytes, opts).await
}

/// Build a NIP-92 `imeta` tag from a [`BlobDescriptor`].
///
/// The returned `Vec<String>` is suitable for passing to `Tag::parse`.
pub fn build_imeta_tag(d: &BlobDescriptor) -> Vec<String> {
    let mut tag = vec![
        "imeta".to_string(),
        format!("url {}", d.url),
        format!("m {}", d.mime_type),
        format!("x {}", d.sha256),
        format!("size {}", d.size),
    ];
    if let Some(ref dim) = d.dim {
        tag.push(format!("dim {dim}"));
    }
    if let Some(ref bh) = d.blurhash {
        tag.push(format!("blurhash {bh}"));
    }
    if let Some(ref th) = d.thumb {
        tag.push(format!("thumb {th}"));
    }
    if let Some(dur) = d.duration {
        tag.push(format!("duration {dur}"));
    }
    tag
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Sign a Blossom kind:24242 auth event and return the `Authorization` header value.
fn sign_blossom_auth(
    keys: &Keys,
    sha256: &str,
    expiry_secs: u64,
    server_domain: Option<&str>,
) -> Result<String, UploadError> {
    let now = Timestamp::now().as_u64();
    let exp_str = (now + expiry_secs).to_string();

    let mut tags = vec![
        Tag::parse(["t", "upload"]).map_err(|e| UploadError::SigningFailed(e.to_string()))?,
        Tag::parse(["x", sha256]).map_err(|e| UploadError::SigningFailed(e.to_string()))?,
        Tag::parse(["expiration", &exp_str])
            .map_err(|e| UploadError::SigningFailed(e.to_string()))?,
    ];
    if let Some(domain) = server_domain {
        tags.push(
            Tag::parse(["server", domain])
                .map_err(|e| UploadError::SigningFailed(e.to_string()))?,
        );
    }

    let event = EventBuilder::new(Kind::from(24242), "Upload file")
        .tags(tags)
        .sign_with_keys(keys)
        .map_err(|e| UploadError::SigningFailed(e.to_string()))?;

    Ok(format!(
        "Nostr {}",
        URL_SAFE_NO_PAD.encode(event.as_json().as_bytes())
    ))
}

/// Extract the server authority from a URL for BUD-11 server tag scoping.
///
/// Returns `host` for default ports, `host:port` for non-default ports.
/// Returns `None` for localhost/127.0.0.1/::1 (no server tag in dev mode —
/// avoids rejection when the relay doesn't have `server_domain` configured).
fn extract_server_authority(url_str: &str) -> Option<String> {
    let parsed = url::Url::parse(url_str).ok()?;
    let host = parsed.host_str()?;
    if host == "localhost" || host == "127.0.0.1" || host == "::1" {
        return None;
    }
    match parsed.port() {
        Some(port) => Some(format!("{host}:{port}")),
        None => Some(host.to_string()),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_imeta_tag_full() {
        let d = BlobDescriptor {
            url: "https://r.example.com/abc.jpg".to_string(),
            sha256: "deadbeef".to_string(),
            size: 1234,
            mime_type: "image/jpeg".to_string(),
            uploaded: 1700000000,
            dim: Some("800x600".to_string()),
            blurhash: Some("LEHV6n".to_string()),
            thumb: Some("https://r.example.com/abc_t.jpg".to_string()),
            duration: None,
        };
        let tag = build_imeta_tag(&d);
        assert_eq!(tag[0], "imeta");
        assert_eq!(tag[1], "url https://r.example.com/abc.jpg");
        assert_eq!(tag[2], "m image/jpeg");
        assert_eq!(tag[3], "x deadbeef");
        assert_eq!(tag[4], "size 1234");
        assert_eq!(tag[5], "dim 800x600");
        assert_eq!(tag[6], "blurhash LEHV6n");
        assert_eq!(tag[7], "thumb https://r.example.com/abc_t.jpg");
        assert_eq!(tag.len(), 8);
    }

    #[test]
    fn test_build_imeta_tag_video_with_duration() {
        let d = BlobDescriptor {
            url: "https://r.example.com/vid.mp4".to_string(),
            sha256: "aabb".to_string(),
            size: 5_000_000,
            mime_type: "video/mp4".to_string(),
            uploaded: 1700000000,
            dim: Some("1280x720".to_string()),
            blurhash: None,
            thumb: None,
            duration: Some(42.5),
        };
        let tag = build_imeta_tag(&d);
        assert!(tag.contains(&"duration 42.5".to_string()));
        assert!(tag.contains(&"dim 1280x720".to_string()));
        assert_eq!(tag.len(), 7);
    }

    #[test]
    fn test_build_imeta_tag_minimal() {
        let d = BlobDescriptor {
            url: "https://r.example.com/min.png".to_string(),
            sha256: "0000".to_string(),
            size: 100,
            mime_type: "image/png".to_string(),
            uploaded: 1700000000,
            dim: None,
            blurhash: None,
            thumb: None,
            duration: None,
        };
        let tag = build_imeta_tag(&d);
        assert_eq!(tag.len(), 5);
    }

    #[test]
    fn test_extract_server_authority() {
        assert_eq!(
            extract_server_authority("https://relay.example.com"),
            Some("relay.example.com".to_string())
        );
        assert_eq!(
            extract_server_authority("https://relay.example.com:8443"),
            Some("relay.example.com:8443".to_string())
        );
        // Localhost suppressed — no server tag in dev mode.
        assert_eq!(extract_server_authority("http://localhost:3000"), None);
        assert_eq!(extract_server_authority("http://127.0.0.1:3000"), None);
        assert_eq!(extract_server_authority("not-a-url"), None);
    }

    #[test]
    fn test_size_limits_are_documented_defaults() {
        assert_eq!(MAX_IMAGE_BYTES, 50 * 1024 * 1024);
        assert_eq!(MAX_GIF_BYTES, 10 * 1024 * 1024);
        assert_eq!(MAX_VIDEO_BYTES, 500 * 1024 * 1024);
    }

    #[test]
    fn test_allowed_mimes() {
        assert!(ALLOWED_MIMES.contains(&"image/jpeg"));
        assert!(ALLOWED_MIMES.contains(&"video/mp4"));
        assert!(!ALLOWED_MIMES.contains(&"application/pdf"));
    }

    #[test]
    fn test_upload_options_default() {
        let opts = UploadOptions::default();
        assert!(opts.auth_tag_json.is_none());
        assert!(opts.server_domain.is_none());
    }
}
