// ABOUTME: REST client for Blossom media upload (PUT /media/upload).
// ABOUTME: Handles SHA-256 hashing, kind:24242 auth event signing, and blob descriptor parsing.

use nostr::{EventBuilder, JsonUtil, Keys, Kind, Tag, Timestamp};
use sha2::{Digest, Sha256};

use crate::error::SproutError;
use crate::types::MediaUploadResult;

use super::HttpClient;

impl HttpClient {
    /// Upload media bytes to the relay's Blossom media server.
    pub async fn upload_media_bytes(
        &self,
        token: &str,
        keys: &Keys,
        bytes: Vec<u8>,
    ) -> Result<MediaUploadResult, SproutError> {
        let sha256_hex = hex::encode(Sha256::digest(&bytes));
        let blossom_header = build_blossom_auth(keys, &sha256_hex)?;

        let json = self
            .upload_media(token, &blossom_header, &sha256_hex, bytes)
            .await?;

        Ok(MediaUploadResult {
            url: json
                .get("url")
                .and_then(|u| u.as_str())
                .unwrap_or("")
                .to_string(),
            sha256: json
                .get("sha256")
                .and_then(|s| s.as_str())
                .unwrap_or(&sha256_hex)
                .to_string(),
            size: json.get("size").and_then(|s| s.as_u64()).unwrap_or(0),
            mime_type: json
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("application/octet-stream")
                .to_string(),
            dimensions: json
                .get("dim")
                .and_then(|d| d.as_str())
                .map(|s| s.to_string()),
            blurhash: json
                .get("blurhash")
                .and_then(|b| b.as_str())
                .map(|s| s.to_string()),
            thumbnail_url: json
                .get("thumb")
                .and_then(|t| t.as_str())
                .map(|s| s.to_string()),
        })
    }
}

/// Build a Blossom BUD-11 auth header (kind 24242 event).
fn build_blossom_auth(keys: &Keys, sha256_hex: &str) -> Result<String, SproutError> {
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine;

    let now = Timestamp::now().as_u64();
    let expiration = (now + 300).to_string(); // 5 minutes

    let tags = vec![
        Tag::parse(&["t", "upload"]).map_err(tag_err)?,
        Tag::parse(&["x", sha256_hex]).map_err(tag_err)?,
        Tag::parse(&["expiration", &expiration]).map_err(tag_err)?,
    ];

    let event = EventBuilder::new(Kind::Custom(24242), "Upload sprout-mobile", tags)
        .sign_with_keys(keys)
        .map_err(|e| SproutError::InvalidKey {
            message: format!("Blossom auth signing failed: {e}"),
        })?;

    Ok(format!("Nostr {}", B64.encode(event.as_json())))
}

fn tag_err(e: nostr::event::tag::Error) -> SproutError {
    SproutError::InternalError {
        message: format!("tag error: {e}"),
    }
}
