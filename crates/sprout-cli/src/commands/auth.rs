use crate::error::CliError;
use crate::validate::validate_uuid;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use nostr::{EventBuilder, JsonUtil, Keys, Kind, Tag};
use sha2::{Digest, Sha256};

/// Mint a long-lived (7-day) API token via NIP-98 and print it to stdout.
///
/// `private_key` is the --private-key CLI flag value; falls back to SPROUT_PRIVATE_KEY env var.
/// Caller stores the printed token (e.g. `export SPROUT_API_TOKEN=$(sprout auth)`).
pub async fn cmd_auth(relay_url: &str, private_key: Option<&str>) -> Result<(), CliError> {
    let env_key;
    let private_key_str = match private_key {
        Some(k) => k,
        None => {
            env_key = std::env::var("SPROUT_PRIVATE_KEY").map_err(|_| {
                CliError::Auth(
                    "SPROUT_PRIVATE_KEY not set (use --private-key or set env var)".into(),
                )
            })?;
            &env_key
        }
    };

    let keys = Keys::parse(private_key_str)
        .map_err(|e| CliError::Key(format!("invalid private key: {e}")))?;

    let token_url = format!("{}/api/tokens", relay_url);

    // Request body — long-lived token, 7 days
    let body = serde_json::json!({
        "name": "sprout-cli",
        "scopes": [
            "messages:read", "messages:write",
            "channels:read", "channels:write",
            "users:read",    "users:write",
            "files:read",    "files:write"
        ],
        "expires_in_days": 7
    });
    let body_bytes = serde_json::to_vec(&body).map_err(|e| CliError::Other(e.to_string()))?;

    // SHA-256 hex of body bytes (NIP-98 payload tag)
    let hash = Sha256::digest(&body_bytes);
    let sha256_hex: String = hash.iter().map(|b| format!("{:02x}", b)).collect();

    // Build NIP-98 event
    let event = EventBuilder::new(
        Kind::HttpAuth,
        "",
        [
            Tag::parse(&["u", &token_url])
                .map_err(|e| CliError::Key(format!("tag build failed: {e}")))?,
            Tag::parse(&["method", "POST"])
                .map_err(|e| CliError::Key(format!("tag build failed: {e}")))?,
            Tag::parse(&["payload", &sha256_hex])
                .map_err(|e| CliError::Key(format!("tag build failed: {e}")))?,
        ],
    )
    .sign_with_keys(&keys)
    .map_err(|e| CliError::Key(format!("signing failed: {e}")))?;

    let auth_header = format!("Nostr {}", B64.encode(event.as_json()));

    // POST /api/tokens
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| CliError::Other(e.to_string()))?;

    let resp = http
        .post(&token_url)
        .header("Authorization", auth_header)
        .json(&body)
        .send()
        .await
        .map_err(CliError::Network)?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body_text = resp.text().await.unwrap_or_default();
        return Err(CliError::Auth(format!(
            "token mint failed ({status}): {body_text}"
        )));
    }

    let resp_body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| CliError::Other(format!("invalid response: {e}")))?;

    let token = resp_body
        .get("token")
        .and_then(|t| t.as_str())
        .ok_or_else(|| CliError::Other("response missing 'token' field".into()))?;

    println!("{token}");
    Ok(())
}

pub async fn cmd_list_tokens(client: &crate::client::SproutClient) -> Result<(), CliError> {
    client.run_get("/api/tokens").await
}

pub async fn cmd_delete_token(
    client: &crate::client::SproutClient,
    id: &str,
) -> Result<(), CliError> {
    validate_uuid(id)?;
    client.run_delete(&format!("/api/tokens/{}", id)).await
}

pub async fn cmd_delete_all_tokens(client: &crate::client::SproutClient) -> Result<(), CliError> {
    client.run_delete("/api/tokens").await
}
