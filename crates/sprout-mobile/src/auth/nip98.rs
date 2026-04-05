// ABOUTME: NIP-98 HTTP authentication — stateless event-based auth for REST endpoints.
// ABOUTME: Builds kind:27235 events with URL, method, and payload hash tags.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use nostr::{EventBuilder, JsonUtil, Keys, Kind, Tag};
use sha2::{Digest, Sha256};

use crate::error::SproutError;

/// Build a NIP-98 `Authorization: Nostr <base64>` header value.
///
/// The event is kind 27235 (HttpAuth) signed by `keys`, with tags:
/// - `["u", url]` — the canonical request URL
/// - `["method", method]` — HTTP method (GET, POST, PUT, DELETE)
/// - `["payload", sha256_hex]` — SHA-256 of the request body (omitted if body is empty)
pub(crate) fn build_nip98_auth_header(
    keys: &Keys,
    url: &str,
    method: &str,
    body: Option<&[u8]>,
) -> Result<String, SproutError> {
    let mut tags = vec![
        Tag::parse(&["u", url]).map_err(tag_err)?,
        Tag::parse(&["method", method]).map_err(tag_err)?,
    ];

    if let Some(bytes) = body {
        if !bytes.is_empty() {
            let hash = Sha256::digest(bytes);
            let sha256_hex = hex::encode(hash);
            tags.push(Tag::parse(&["payload", &sha256_hex]).map_err(tag_err)?);
        }
    }

    let event = EventBuilder::new(Kind::HttpAuth, "", tags)
        .sign_with_keys(keys)
        .map_err(|e| SproutError::InvalidKey {
            message: format!("NIP-98 signing failed: {e}"),
        })?;

    Ok(format!("Nostr {}", B64.encode(event.as_json())))
}

fn tag_err(e: nostr::event::tag::Error) -> SproutError {
    SproutError::InternalError {
        message: format!("tag error: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_valid_nip98_header() {
        let keys = Keys::generate();
        let header =
            build_nip98_auth_header(&keys, "https://example.com/api/tokens", "POST", Some(b"{}"))
                .unwrap();
        assert!(header.starts_with("Nostr "));

        // Decode and verify it's valid JSON with expected fields.
        let b64_part = &header["Nostr ".len()..];
        let json_bytes = B64.decode(b64_part).unwrap();
        let event: serde_json::Value = serde_json::from_slice(&json_bytes).unwrap();
        assert_eq!(event["kind"], 27235);

        let tags = event["tags"].as_array().unwrap();
        let tag_names: Vec<&str> = tags.iter().map(|t| t[0].as_str().unwrap()).collect();
        assert!(tag_names.contains(&"u"));
        assert!(tag_names.contains(&"method"));
        assert!(tag_names.contains(&"payload"));
    }

    #[test]
    fn no_payload_tag_when_body_empty() {
        let keys = Keys::generate();
        let header =
            build_nip98_auth_header(&keys, "https://example.com/api/channels", "GET", None)
                .unwrap();

        let b64_part = &header["Nostr ".len()..];
        let json_bytes = B64.decode(b64_part).unwrap();
        let event: serde_json::Value = serde_json::from_slice(&json_bytes).unwrap();
        let tags = event["tags"].as_array().unwrap();
        let tag_names: Vec<&str> = tags.iter().map(|t| t[0].as_str().unwrap()).collect();
        assert!(!tag_names.contains(&"payload"));
    }
}
