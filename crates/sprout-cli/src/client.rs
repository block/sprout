use std::time::Duration;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use nostr::{EventBuilder, JsonUtil, Keys, Kind, Tag};
use sha2::{Digest, Sha256};

use crate::error::CliError;

// Re-export from shared SDK for use by commands/messages.rs
pub use sprout_sdk::upload::{build_imeta_tag, BlobDescriptor};

// ---------------------------------------------------------------------------
// NIP-98 HTTP Auth
// ---------------------------------------------------------------------------

/// Sign a NIP-98 HTTP auth event (kind:27235) and return the Authorization header value.
fn sign_nip98(
    keys: &Keys,
    method: &str,
    url: &str,
    body: Option<&[u8]>,
) -> Result<String, CliError> {
    let mut tags = vec![
        Tag::parse(["u", url]).map_err(|e| CliError::Other(format!("tag error: {e}")))?,
        Tag::parse(["method", method]).map_err(|e| CliError::Other(format!("tag error: {e}")))?,
        Tag::parse(["nonce", &uuid::Uuid::new_v4().to_string()])
            .map_err(|e| CliError::Other(format!("tag error: {e}")))?,
    ];
    if let Some(b) = body {
        let hash = hex::encode(Sha256::digest(b));
        tags.push(
            Tag::parse(["payload", &hash])
                .map_err(|e| CliError::Other(format!("tag error: {e}")))?,
        );
    }
    let event = EventBuilder::new(Kind::Custom(27235), "")
        .tags(tags)
        .sign_with_keys(keys)
        .map_err(|e| CliError::Other(format!("NIP-98 signing failed: {e}")))?;
    let json = event.as_json();
    Ok(format!("Nostr {}", B64.encode(json.as_bytes())))
}

// ---------------------------------------------------------------------------
// SproutClient
// ---------------------------------------------------------------------------

pub struct SproutClient {
    http: reqwest::Client,
    relay_url: String,
    keys: Keys,
    auth_tag: Option<Tag>,
    auth_tag_json: Option<String>,
}

impl SproutClient {
    pub fn new(
        relay_url: String,
        keys: Keys,
        auth_tag: Option<Tag>,
        auth_tag_json: Option<String>,
    ) -> Result<Self, CliError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| CliError::Other(e.to_string()))?;
        Ok(Self {
            http,
            relay_url,
            keys,
            auth_tag,
            auth_tag_json,
        })
    }

    /// Get the keypair.
    pub fn keys(&self) -> &Keys {
        &self.keys
    }

    /// Get the relay base URL.
    #[allow(dead_code)]
    pub fn relay_url(&self) -> &str {
        &self.relay_url
    }

    /// Sign an event builder, injecting the NIP-OA auth tag if configured.
    pub fn sign_event(&self, builder: EventBuilder) -> Result<nostr::Event, CliError> {
        let builder = if let Some(ref tag) = self.auth_tag {
            builder.tags([tag.clone()])
        } else {
            builder
        };
        let event = builder
            .sign_with_keys(&self.keys)
            .map_err(|e| CliError::Other(format!("signing failed: {e}")))?;

        let auth_count = event
            .tags
            .iter()
            .filter(|t| t.as_slice().first().map(|s| s.as_str()) == Some("auth"))
            .count();
        let expected = if self.auth_tag.is_some() { 1 } else { 0 };
        if auth_count != expected {
            return Err(CliError::Other(format!(
                "event has {auth_count} auth tags — expected {expected}; \
                 callers must not add auth tags manually"
            )));
        }

        Ok(event)
    }

    fn with_auth_tag(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self.auth_tag_json {
            Some(ref json) => req.header("x-auth-tag", json),
            None => req,
        }
    }

    // -----------------------------------------------------------------------
    // HTTP Bridge: POST /query
    // -----------------------------------------------------------------------

    pub async fn query(&self, filter: &serde_json::Value) -> Result<String, CliError> {
        let url = format!("{}/query", self.relay_url);
        let body_bytes = serde_json::to_vec(&[filter])
            .map_err(|e| CliError::Other(format!("filter serialization failed: {e}")))?;
        let auth = sign_nip98(&self.keys, "POST", &url, Some(&body_bytes))?;

        let req = self
            .http
            .post(&url)
            .header("Authorization", &auth)
            .header("Content-Type", "application/json")
            .body(body_bytes);
        let resp = self.with_auth_tag(req).send().await?;

        self.handle_response(resp).await
    }

    #[allow(dead_code)]
    pub async fn count(&self, filter: &serde_json::Value) -> Result<String, CliError> {
        let url = format!("{}/count", self.relay_url);
        let body_bytes = serde_json::to_vec(&[filter])
            .map_err(|e| CliError::Other(format!("filter serialization failed: {e}")))?;
        let auth = sign_nip98(&self.keys, "POST", &url, Some(&body_bytes))?;

        let req = self
            .http
            .post(&url)
            .header("Authorization", &auth)
            .header("Content-Type", "application/json")
            .body(body_bytes);
        let resp = self.with_auth_tag(req).send().await?;

        self.handle_response(resp).await
    }

    // -----------------------------------------------------------------------
    // HTTP Bridge: POST /events
    // -----------------------------------------------------------------------

    pub async fn submit_event(&self, event: nostr::Event) -> Result<String, CliError> {
        let url = format!("{}/events", self.relay_url);
        let body_bytes = serde_json::to_vec(&event)
            .map_err(|e| CliError::Other(format!("event serialization failed: {e}")))?;
        let auth = sign_nip98(&self.keys, "POST", &url, Some(&body_bytes))?;

        let req = self
            .http
            .post(&url)
            .header("Authorization", &auth)
            .header("Content-Type", "application/json")
            .body(body_bytes);
        let resp = self.with_auth_tag(req).send().await?;

        self.handle_response(resp).await
    }

    // -----------------------------------------------------------------------
    // File upload (delegates to shared sprout-sdk upload pipeline)
    // -----------------------------------------------------------------------

    /// Upload a file to the relay's Blossom endpoint.
    pub async fn upload_file(&self, file_path: &str) -> Result<BlobDescriptor, CliError> {
        let opts = sprout_sdk::upload::UploadOptions {
            auth_tag_json: self.auth_tag_json.as_deref(),
            ..Default::default()
        };
        sprout_sdk::upload::upload_file(&self.http, &self.keys, &self.relay_url, file_path, &opts)
            .await
            .map_err(|e| CliError::Other(format!("upload failed: {e}")))
    }

    // -----------------------------------------------------------------------
    // Response handling
    // -----------------------------------------------------------------------

    async fn handle_response(&self, resp: reqwest::Response) -> Result<String, CliError> {
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
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
}

// ---------------------------------------------------------------------------
// URL normalization
// ---------------------------------------------------------------------------

/// Normalize a relay URL: ws:// → http://, wss:// → https://, strip trailing slash.
pub fn normalize_relay_url(url: &str) -> String {
    url.replace("wss://", "https://")
        .replace("ws://", "http://")
        .trim_end_matches('/')
        .to_string()
}
