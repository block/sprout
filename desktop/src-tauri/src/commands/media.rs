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
}

/// Extract the server authority from a URL for BUD-11 server tag scoping.
///
/// Returns `host` for default ports (80/443), `host:port` for non-default ports.
/// Uses `url::Url` for correct handling of IPv6 literals and edge cases.
/// The relay verifier does exact string equality against `SPROUT_MEDIA_SERVER_DOMAIN`,
/// so both sides must agree on the format.
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
/// This is the fd-based equivalent of `canonicalize()` — it returns the path
/// the kernel associates with the inode, not the pathname used to open it.
/// Immune to post-open renames/symlink swaps.
#[cfg(target_os = "macos")]
fn fd_real_path(file: &std::fs::File) -> Result<std::path::PathBuf, String> {
    use std::os::unix::io::AsRawFd;
    let fd = file.as_raw_fd();
    let mut buf = vec![0u8; libc::PATH_MAX as usize];
    let ret = unsafe { libc::fcntl(fd, libc::F_GETPATH, buf.as_mut_ptr()) };
    if ret == -1 {
        return Err(format!("fcntl F_GETPATH failed: {}", std::io::Error::last_os_error()));
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

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn fd_real_path(_file: &std::fs::File) -> Result<std::path::PathBuf, String> {
    // Fallback: no fd-based path resolution available. The trust boundary
    // relies on server-side content validation as defense-in-depth.
    Err("fd_real_path not supported on this platform".to_string())
}

fn sign_blossom_upload_auth(keys: &Keys, sha256: &str) -> Result<nostr::Event, String> {
    let now = Timestamp::now().as_u64();
    let mut tags = vec![
        Tag::parse(vec!["t", "upload"]).map_err(|e| e.to_string())?,
        Tag::parse(vec!["x", sha256]).map_err(|e| e.to_string())?,
        Tag::parse(vec!["expiration", &(now + 300).to_string()]).map_err(|e| e.to_string())?,
    ];
    // BUD-11: scope token to this server's domain
    let base_url = relay_api_base_url();
    if let Some(domain) = extract_server_authority(&base_url) {
        tags.push(Tag::parse(vec!["server".to_string(), domain]).map_err(|e| e.to_string())?);
    }
    EventBuilder::new(Kind::from(24242), "Upload sprout-media")
        .tags(tags)
        .sign_with_keys(keys)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn upload_media(
    file_path: String,
    is_temp: bool,
    filename: Option<String>,
    state: State<'_, AppState>,
) -> Result<BlobDescriptor, String> {
    // Trust boundary: ALL file reads must originate from the OS temp directory.
    // The webview writes drag-drop/paste files to temp, and the file dialog handler
    // copies selected files to temp before invoking this command. This prevents a
    // compromised renderer from exfiltrating arbitrary local files.
    //
    // Security: open the file FIRST, then resolve the fd's real filesystem path
    // (not the original pathname) to verify it's inside temp_dir. This closes
    // the TOCTOU gap — the fd is pinned to an inode, so path swaps after open()
    // cannot redirect the read. On macOS we use F_GETPATH; on Linux, /proc/self/fd.
    let path = std::path::Path::new(&file_path);

    // Open the file — acquires an fd pinned to the inode.
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;

    // Resolve the real path of the OPENED fd (not the original pathname).
    let fd_path = fd_real_path(&file)?;
    let canonical_temp = std::env::temp_dir()
        .canonicalize()
        .unwrap_or_else(|_| std::env::temp_dir());
    if !fd_path.starts_with(&canonical_temp) {
        return Err("upload source must be in system temp directory".to_string());
    }

    // Read from the already-opened fd — no second open(), no TOCTOU gap.
    use std::io::Read;
    let mut body = Vec::new();
    file.read_to_end(&mut body)
        .map_err(|e| format!("failed to read file: {e}"))?;
    drop(file); // release fd before cleanup

    // Clean up temp file using the fd-verified path.
    if is_temp {
        let _ = std::fs::remove_file(&fd_path);
    }

    // Detect MIME via magic bytes — enforce the same allowlist as the server.
    // Reject early to avoid wasting bandwidth on files the server will refuse.
    const ALLOWED_MIME: &[&str] = &["image/jpeg", "image/png", "image/gif", "image/webp"];
    let mime = infer::get(&body)
        .map(|t| t.mime_type().to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string());
    if !ALLOWED_MIME.contains(&mime.as_str()) {
        return Err(format!("unsupported file type: {mime}"));
    }

    // Compute SHA-256
    let sha256 = format!("{:x}", Sha256::digest(&body));

    // Sign kind:24242 auth event — clone keys out of the Mutex before any await
    let auth_event = {
        let keys = state.keys.lock().map_err(|e| e.to_string())?;
        sign_blossom_upload_auth(&keys, &sha256)?
    };

    let auth_header = format!(
        "Nostr {}",
        URL_SAFE_NO_PAD.encode(auth_event.as_json().as_bytes())
    );

    // PUT /media/upload
    let base_url = relay_api_base_url();
    let mut req = state
        .http_client
        .put(format!("{base_url}/media/upload"))
        .header("Authorization", &auth_header)
        .header("Content-Type", &mime)
        .header("X-SHA-256", &sha256);

    // Attach API token if available
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
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("upload failed ({status}): {body}"));
    }

    // Suppress unused variable warning for filename (reserved for future use)
    let _ = filename;

    resp.json::<BlobDescriptor>()
        .await
        .map_err(|e| format!("parse failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// url::Url normalizes default ports (80/443) to None, so our function
    /// correctly returns host-only for default ports and host:port for others.
    #[test]
    fn test_extract_server_authority_default_ports() {
        // Default ports → host only
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
        assert_eq!(
            extract_server_authority("http://relay.example.com:8080"),
            Some("relay.example.com:8080".to_string())
        );
    }

    #[test]
    fn test_extract_server_authority_ipv6() {
        // url::Url brackets IPv6 in host_str()
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
}
