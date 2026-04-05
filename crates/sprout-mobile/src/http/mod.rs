//! HTTP client modules for Sprout relay REST API calls.
//! Handles auth headers (Bearer token or NIP-98), error mapping, and request dispatch.

/// Channel endpoints client.
pub mod channels;
/// Direct-messages endpoints client.
pub mod dms;
/// Home feed endpoint client.
pub mod feed;
/// Media upload endpoint client.
pub mod media;
/// Message and thread endpoints client.
pub mod messages;
/// Presence endpoints client.
pub mod presence;
/// Search endpoints client.
pub mod search;
/// Users endpoints client.
pub mod users;

use std::time::Duration;

use nostr::Keys;
use reqwest::{Client, Response, StatusCode};
use serde_json::Value;

use crate::auth::nip98::build_nip98_auth_header;
use crate::error::SproutError;

/// HTTP client for the Sprout relay REST API.
pub(crate) struct HttpClient {
    client: Client,
    base_url: String,
}

impl HttpClient {
    /// Create a new HTTP client pointing at the relay.
    /// `relay_url` should be a ws:// or wss:// URL; it's normalized to http/https.
    pub fn new(relay_url: &str) -> Result<Self, SproutError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| SproutError::NetworkError {
                message: e.to_string(),
            })?;

        let base_url = normalize_relay_url(relay_url);

        Ok(Self { client, base_url })
    }

    /// Make a GET request with Bearer token auth.
    pub async fn get_with_token(&self, path: &str, token: &str) -> Result<Value, SproutError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
            .map_err(network_err)?;
        handle_response(resp).await
    }

    /// Make a POST request with Bearer token auth and JSON body.
    pub async fn post_with_token(
        &self,
        path: &str,
        token: &str,
        body: &Value,
    ) -> Result<Value, SproutError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .json(body)
            .send()
            .await
            .map_err(network_err)?;
        handle_response(resp).await
    }

    /// Make a PUT request with Bearer token auth and JSON body.
    pub async fn put_with_token(
        &self,
        path: &str,
        token: &str,
        body: &Value,
    ) -> Result<Value, SproutError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .put(&url)
            .header("Authorization", format!("Bearer {token}"))
            .json(body)
            .send()
            .await
            .map_err(network_err)?;
        handle_response(resp).await
    }

    /// Make a DELETE request with Bearer token auth.
    #[allow(dead_code)]
    pub async fn delete_with_token(&self, path: &str, token: &str) -> Result<(), SproutError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .delete(&url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .await
            .map_err(network_err)?;
        let status = resp.status();
        if status.is_success() {
            Ok(())
        } else {
            let body = resp.text().await.unwrap_or_default();
            Err(relay_error(status, &body))
        }
    }

    /// Make a POST request with NIP-98 auth (for token minting bootstrap).
    pub async fn post_with_nip98(
        &self,
        path: &str,
        keys: &Keys,
        body: &Value,
    ) -> Result<Value, SproutError> {
        let url = format!("{}{}", self.base_url, path);
        let body_bytes = serde_json::to_vec(body).map_err(|e| SproutError::InternalError {
            message: e.to_string(),
        })?;
        let auth_header = build_nip98_auth_header(keys, &url, "POST", Some(&body_bytes))?;

        let proto = if url.starts_with("https://") {
            "https"
        } else {
            "http"
        };

        let resp = self
            .client
            .post(&url)
            .header("Authorization", auth_header)
            .header("x-forwarded-proto", proto)
            .header("Content-Type", "application/json")
            .body(body_bytes)
            .send()
            .await
            .map_err(network_err)?;
        handle_response(resp).await
    }

    /// Submit a signed Nostr event via POST /api/events with Bearer auth.
    pub async fn submit_event(
        &self,
        token: &str,
        event: &nostr::Event,
    ) -> Result<Value, SproutError> {
        let url = format!("{}/api/events", self.base_url);
        let event_json = serde_json::to_value(event).map_err(|e| SproutError::InternalError {
            message: e.to_string(),
        })?;
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .json(&event_json)
            .send()
            .await
            .map_err(network_err)?;
        handle_response(resp).await
    }

    /// Upload raw bytes via PUT /media/upload with Blossom auth headers.
    pub async fn upload_media(
        &self,
        token: &str,
        blossom_auth_header: &str,
        sha256_hex: &str,
        bytes: Vec<u8>,
    ) -> Result<Value, SproutError> {
        let url = format!("{}/media/upload", self.base_url);
        let resp = self
            .client
            .put(&url)
            .header("Authorization", blossom_auth_header)
            .header("X-Auth-Token", token)
            .header("X-SHA-256", sha256_hex)
            .body(bytes)
            .send()
            .await
            .map_err(network_err)?;
        handle_response(resp).await
    }

    /// Get the base URL (for NIP-98 URL construction).
    #[allow(dead_code)]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

/// Convert ws/wss relay URL to http/https.
fn normalize_relay_url(url: &str) -> String {
    url.replace("wss://", "https://")
        .replace("ws://", "http://")
        .trim_end_matches('/')
        .to_string()
}

/// Map reqwest errors to SproutError.
fn network_err(e: reqwest::Error) -> SproutError {
    SproutError::NetworkError {
        message: e.to_string(),
    }
}

/// Parse a relay HTTP response, extracting error messages on failure.
async fn handle_response(resp: Response) -> Result<Value, SproutError> {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();

    if status.is_success() {
        if body.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&body).map_err(|e| SproutError::InternalError {
            message: format!("invalid JSON response: {e}"),
        })
    } else {
        Err(relay_error(status, &body))
    }
}

/// Build a SproutError from a non-2xx response.
fn relay_error(status: StatusCode, body: &str) -> SproutError {
    // Try to extract a message from the JSON error body.
    let message = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| {
            v.get("message")
                .or_else(|| v.get("error"))
                .and_then(|m| m.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| body.to_string());

    match status {
        StatusCode::UNAUTHORIZED => SproutError::AuthFailed { message },
        StatusCode::FORBIDDEN => SproutError::PermissionDenied { message },
        StatusCode::NOT_FOUND => SproutError::NotFound { entity: message },
        _ => SproutError::RelayError {
            status: status.as_u16(),
            message,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_ws_to_http() {
        assert_eq!(
            normalize_relay_url("ws://localhost:3000"),
            "http://localhost:3000"
        );
        assert_eq!(
            normalize_relay_url("wss://relay.example.com"),
            "https://relay.example.com"
        );
        assert_eq!(
            normalize_relay_url("wss://relay.example.com/"),
            "https://relay.example.com"
        );
    }

    #[test]
    fn http_urls_pass_through() {
        assert_eq!(
            normalize_relay_url("https://relay.example.com"),
            "https://relay.example.com"
        );
    }
}
