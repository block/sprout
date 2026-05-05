use std::time::Duration;

use nostr::Keys;

use crate::error::CliError;

// ---------------------------------------------------------------------------
// Blob / Media types
// ---------------------------------------------------------------------------

/// Descriptor returned by the relay after a successful upload.
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

/// Build an `imeta` tag array from a BlobDescriptor (NIP-92 media metadata).
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

/// MIME types accepted for upload.
const ALLOWED_MIMES: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/webp",
    "video/mp4",
];

/// Maximum file size for image uploads (50 MB).
const MAX_IMAGE_BYTES: u64 = 50 * 1024 * 1024;

/// Maximum file size for video uploads (500 MB).
const MAX_VIDEO_BYTES: u64 = 500 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

pub enum Auth {
    Bearer(String),  // SPROUT_API_TOKEN or auto-minted via SPROUT_PRIVATE_KEY
    DevMode(String), // SPROUT_PUBKEY — X-Pubkey header, relay must have require_auth_token=false
}

// ---------------------------------------------------------------------------
// SproutClient
// ---------------------------------------------------------------------------

pub struct SproutClient {
    http: reqwest::Client,
    relay_url: String, // base URL, no trailing slash, e.g. "https://relay.sprout.place"
    auth: Auth,
    keys: Option<Keys>, // retained for event signing (write operations)
}

impl SproutClient {
    pub fn new(relay_url: String, auth: Auth) -> Result<Self, CliError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| CliError::Other(e.to_string()))?;
        Ok(Self {
            http,
            relay_url,
            auth,
            keys: None,
        })
    }

    /// Attach a keypair for signing write operations.
    pub fn with_keys(mut self, keys: Keys) -> Self {
        self.keys = Some(keys);
        self
    }

    /// Get the retained keypair, if available.
    pub fn keys(&self) -> Option<&Keys> {
        self.keys.as_ref()
    }

    // -----------------------------------------------------------------------
    // Core request method
    // -----------------------------------------------------------------------

    async fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<String, CliError> {
        let url = format!("{}{}", self.relay_url, path);

        let builder = self.http.request(method, &url);
        let builder = self.apply_auth(builder);
        let builder = match body {
            Some(b) => builder.json(b),
            None => builder,
        };

        let resp = builder.send().await?; // reqwest::Error → CliError::Network via From

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            // Try to extract relay's error message from JSON body
            let message = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| {
                    v.get("error")
                        .or_else(|| v.get("message"))
                        .and_then(|m| m.as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or(body);
            return Err(CliError::Relay {
                status,
                body: message,
            });
        }

        Ok(resp.text().await?)
    }

    pub(crate) fn apply_auth(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.auth {
            Auth::Bearer(token) => builder.header("Authorization", format!("Bearer {}", token)),
            Auth::DevMode(pk) => builder.header("X-Pubkey", pk),
        }
    }

    // -----------------------------------------------------------------------
    // Signed event submission
    // -----------------------------------------------------------------------

    /// Submit a signed Nostr event via POST /api/events.
    pub async fn submit_event(&self, event: nostr::Event) -> Result<String, CliError> {
        let body = serde_json::to_vec(&event)
            .map_err(|e| CliError::Other(format!("event serialization failed: {e}")))?;
        let url = format!("{}/api/events", self.relay_url);
        let builder = self.http.post(&url);
        let builder = self.apply_auth(builder);
        let resp = builder
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            let message = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| {
                    v.get("error")
                        .and_then(|m| m.as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or(body);
            return Err(CliError::Relay {
                status,
                body: message,
            });
        }
        Ok(resp.text().await?)
    }

    // -----------------------------------------------------------------------
    // File upload (Blossom protocol)
    // -----------------------------------------------------------------------

    /// Upload a file to the relay's Blossom endpoint.
    /// Returns a BlobDescriptor on success.
    pub async fn upload_file(&self, file_path: &str) -> Result<BlobDescriptor, CliError> {
        let keys = self.keys().ok_or_else(|| {
            CliError::Key("private key required for uploads (set SPROUT_PRIVATE_KEY)".into())
        })?;

        // 1. Read file — validate it exists and is a regular file
        let metadata = std::fs::metadata(file_path)
            .map_err(|e| CliError::Other(format!("cannot access {file_path}: {e}")))?;
        if !metadata.is_file() {
            return Err(CliError::Usage(format!("{file_path} is not a file")));
        }

        let bytes = std::fs::read(file_path)
            .map_err(|e| CliError::Other(format!("failed to read {file_path}: {e}")))?;

        // 2. Detect MIME from magic bytes
        let mime = infer::get(&bytes)
            .map(|t| t.mime_type().to_string())
            .unwrap_or_else(|| "application/octet-stream".to_string());

        if !ALLOWED_MIMES.contains(&mime.as_str()) {
            return Err(CliError::Usage(format!("unsupported file type: {mime}")));
        }

        // 3. Size check
        let max = if mime.starts_with("video/") {
            MAX_VIDEO_BYTES
        } else {
            MAX_IMAGE_BYTES
        };
        if bytes.len() as u64 > max {
            return Err(CliError::Usage(format!(
                "file too large: {} bytes (max {})",
                bytes.len(),
                max
            )));
        }

        // 4. SHA-256
        use sha2::{Digest, Sha256};
        let sha256 = hex::encode(Sha256::digest(&bytes));

        // 5. Sign Blossom auth event (kind:24242)
        use nostr::{EventBuilder, Kind, Tag, Timestamp};
        let now = Timestamp::now().as_u64();
        let expiry = if mime.starts_with("video/") {
            3600
        } else {
            600
        };
        let exp_str = (now + expiry).to_string();

        let mut tags = vec![
            Tag::parse(&["t", "upload"]).map_err(|e| CliError::Other(e.to_string()))?,
            Tag::parse(&["x", &sha256]).map_err(|e| CliError::Other(e.to_string()))?,
            Tag::parse(&["expiration", &exp_str]).map_err(|e| CliError::Other(e.to_string()))?,
        ];
        // Extract server domain from relay URL for BUD-11 server tag
        if let Ok(parsed) = url::Url::parse(&self.relay_url) {
            if let Some(host) = parsed.host_str() {
                let domain = match parsed.port() {
                    Some(port) => format!("{host}:{port}"),
                    None => host.to_string(),
                };
                tags.push(
                    Tag::parse(&["server", &domain]).map_err(|e| CliError::Other(e.to_string()))?,
                );
            }
        }

        let auth_event = EventBuilder::new(Kind::from(24242), "Upload file", tags)
            .sign_with_keys(keys)
            .map_err(|e| CliError::Other(format!("signing failed: {e}")))?;

        // 6. Base64url encode the auth event for the header
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;
        use nostr::JsonUtil;
        let auth_header = format!(
            "Nostr {}",
            URL_SAFE_NO_PAD.encode(auth_event.as_json().as_bytes())
        );

        // 7. PUT request to /media/upload — with generous per-request timeout.
        // The shared client has a 10s timeout for REST calls; uploads need more.
        let upload_timeout = if mime.starts_with("video/") {
            std::time::Duration::from_secs(600)
        } else {
            std::time::Duration::from_secs(120)
        };
        let url = format!("{}/media/upload", self.relay_url);
        let mut req = self
            .http
            .put(&url)
            .timeout(upload_timeout)
            .header("Authorization", &auth_header)
            .header("Content-Type", &mime)
            .header("X-SHA-256", &sha256);
        // Add bearer token as X-Auth-Token for relay auth
        if let Auth::Bearer(ref token) = self.auth {
            req = req.header("X-Auth-Token", token.as_str());
        }

        let resp = req.body(bytes).send().await?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(CliError::Relay { status, body });
        }

        resp.json::<BlobDescriptor>()
            .await
            .map_err(|e| CliError::Other(format!("invalid upload response: {e}")))
    }

    // -----------------------------------------------------------------------
    // Convenience wrappers — print response to stdout
    // -----------------------------------------------------------------------

    pub async fn run_get(&self, path: &str) -> Result<(), CliError> {
        let resp = self.request(reqwest::Method::GET, path, None).await?;
        println!("{resp}");
        Ok(())
    }

    pub async fn run_post(&self, path: &str, body: &serde_json::Value) -> Result<(), CliError> {
        let resp = self
            .request(reqwest::Method::POST, path, Some(body))
            .await?;
        println!("{resp}");
        Ok(())
    }

    pub async fn run_put(&self, path: &str, body: &serde_json::Value) -> Result<(), CliError> {
        let resp = self.request(reqwest::Method::PUT, path, Some(body)).await?;
        println!("{resp}");
        Ok(())
    }

    pub async fn run_delete(&self, path: &str) -> Result<(), CliError> {
        let resp = self.request(reqwest::Method::DELETE, path, None).await?;
        println!("{resp}");
        Ok(())
    }

    // For commands that need the raw response string (e.g. get_users multi-dispatch)
    pub async fn get_raw(&self, path: &str) -> Result<String, CliError> {
        self.request(reqwest::Method::GET, path, None).await
    }

    pub async fn post_raw(&self, path: &str, body: &serde_json::Value) -> Result<String, CliError> {
        self.request(reqwest::Method::POST, path, Some(body)).await
    }
}

// ---------------------------------------------------------------------------
// URL normalization
// ---------------------------------------------------------------------------

/// Normalize a relay URL: ws:// → http://, wss:// → https://, strip trailing slash.
/// SPROUT_RELAY_URL may be ws/wss (copied from MCP config).
pub fn normalize_relay_url(url: &str) -> String {
    url.replace("wss://", "https://")
        .replace("ws://", "http://")
        .trim_end_matches('/')
        .to_string()
}

// ---------------------------------------------------------------------------
// Auto-mint token (NIP-98)
// ---------------------------------------------------------------------------

/// Mint a short-lived Bearer token using NIP-98 HTTP auth.
/// Called at startup when SPROUT_PRIVATE_KEY is set.
/// Returns `(token, keys)` so the caller can retain the keypair for signed writes.
pub async fn auto_mint_token(
    relay_url: &str,
    private_key_str: &str,
) -> Result<(String, Keys), CliError> {
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine;
    use nostr::{EventBuilder, JsonUtil, Kind, Tag};
    use sha2::{Digest, Sha256};

    let keys = Keys::parse(private_key_str)
        .map_err(|e| CliError::Key(format!("invalid SPROUT_PRIVATE_KEY: {e}")))?;

    let token_url = format!("{}/api/tokens", relay_url);

    // Body bytes for payload hash
    let body = serde_json::json!({
        "name": "sprout-cli-auto",
        "scopes": [
            "messages:read", "messages:write",
            "channels:read", "channels:write",
            "users:read", "users:write",
            "files:read", "files:write"
        ],
        "expires_in_days": 1
    });
    let body_bytes = serde_json::to_vec(&body).map_err(|e| CliError::Other(e.to_string()))?;

    // SHA-256 hex of body bytes (NIP-98 payload tag)
    let hash = Sha256::digest(&body_bytes);
    let sha256_hex = hash
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();

    // Build NIP-98 event
    let event = EventBuilder::new(
        Kind::HttpAuth,
        "",
        [
            Tag::parse(&["u", &token_url]).map_err(|e| CliError::Key(format!("tag error: {e}")))?,
            Tag::parse(&["method", "POST"])
                .map_err(|e| CliError::Key(format!("tag error: {e}")))?,
            Tag::parse(&["payload", &sha256_hex])
                .map_err(|e| CliError::Key(format!("tag error: {e}")))?,
        ],
    )
    .sign_with_keys(&keys)
    .map_err(|e| CliError::Key(format!("signing failed: {e}")))?;

    let auth_header = format!("Nostr {}", B64.encode(event.as_json()));

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| CliError::Other(e.to_string()))?;

    // Tell the relay which scheme we signed so the canonical URL matches.
    // The relay defaults x-forwarded-proto to "https"; without this header,
    // http:// URLs fail NIP-98 verification on localhost.
    let proto = if token_url.starts_with("https://") {
        "https"
    } else {
        "http"
    };

    let resp = http
        .post(&token_url)
        .header("Authorization", auth_header)
        .header("x-forwarded-proto", proto)
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(CliError::Auth(format!(
            "auto-mint failed ({status}): {body}"
        )));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| CliError::Other(format!("invalid auto-mint response: {e}")))?;

    let token = json
        .get("token")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| CliError::Other("auto-mint response missing 'token' field".into()))?;

    Ok((token, keys))
}
