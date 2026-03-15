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
    // Residual TOCTOU note: we canonicalize and read from the canonical path, which
    // eliminates symlink-based attacks at the original path. A narrow race remains
    // between canonicalize() and read() where the canonical path itself could be
    // swapped, but exploiting this requires local filesystem access (not just renderer
    // compromise). True fd-based O_NOFOLLOW reading would close this fully but is
    // complex cross-platform; the server-side content validation (magic bytes, MIME
    // allowlist, size caps) provides defense-in-depth against exfiltrated non-image data.
    let path = std::path::Path::new(&file_path);
    // Canonicalize BOTH paths: on macOS, temp_dir() returns /var/... but
    // canonicalize() resolves the symlink to /private/var/..., so a naive
    // starts_with() would reject every legitimate temp file.
    let canonical_temp = std::env::temp_dir()
        .canonicalize()
        .unwrap_or_else(|_| std::env::temp_dir());
    let canonical = path.canonicalize().map_err(|e| e.to_string())?;
    if !canonical.starts_with(&canonical_temp) {
        return Err("upload source must be in system temp directory".to_string());
    }
    // Read from the validated canonical path, not the original string.
    let body_result = tokio::fs::read(&canonical).await;
    // Clean up temp file (from drag-drop/paste) regardless of read success.
    // Remove by canonical path to ensure we delete the validated file.
    if is_temp {
        let _ = tokio::fs::remove_file(&canonical).await;
    }
    let body = body_result.map_err(|e| format!("failed to read file: {e}"))?;

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
