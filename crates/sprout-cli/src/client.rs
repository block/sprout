use std::time::Duration;

use nostr::Keys;

use crate::error::CliError;

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
