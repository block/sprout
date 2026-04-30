use anyhow::Result;
use nostr::{Keys, Tag};
use rmcp::{transport::stdio, ServiceExt};
use tracing_subscriber::EnvFilter;

use sprout_mcp::relay_client::RelayClient;
use sprout_mcp::server::SproutMcpServer;
use sprout_mcp::toolsets::ToolsetConfig;

/// Parse and validate the NIP-OA auth tag from the environment.
///
/// Returns `Ok(Some(tag))` if a valid auth tag is configured,
/// `Ok(None)` if no auth tag is set (or empty),
/// `Err` if the tag is present but malformed, invalid, or non-Unicode.
fn resolve_auth_tag(
    env_value: Result<String, std::env::VarError>,
    agent_pubkey: &nostr::PublicKey,
) -> anyhow::Result<Option<Tag>> {
    let tag_json = match env_value {
        Ok(s) if !s.is_empty() => s,
        Ok(_) => return Ok(None), // empty string — treat as not set
        Err(std::env::VarError::NotPresent) => return Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            anyhow::bail!("SPROUT_AUTH_TAG contains non-Unicode data");
        }
    };

    let tag = sprout_sdk::nip_oa::parse_auth_tag(&tag_json)
        .map_err(|e| anyhow::anyhow!("SPROUT_AUTH_TAG is malformed: {e}"))?;

    sprout_sdk::nip_oa::verify_auth_tag(&tag_json, agent_pubkey).map_err(|e| {
        anyhow::anyhow!(
            "SPROUT_AUTH_TAG signature verification failed for agent pubkey {}: {e}",
            agent_pubkey.to_hex()
        )
    })?;

    eprintln!(
        "sprout-mcp: NIP-OA auth tag configured (owner: {})",
        tag.as_slice().get(1).map(|s| &s[..8]).unwrap_or("?")
    );

    Ok(Some(tag))
}

#[tokio::main]
async fn main() -> Result<()> {
    // Install the ring crypto provider for rustls (required for wss:// connections).
    let _ = rustls::crypto::ring::default_provider().install_default();
    // Log to stderr — stdout is the MCP JSON-RPC channel.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("sprout_mcp=info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let relay_url =
        std::env::var("SPROUT_RELAY_URL").unwrap_or_else(|_| "ws://localhost:3000".to_string());

    let api_token = std::env::var("SPROUT_API_TOKEN").ok();

    let keys = match std::env::var("SPROUT_PRIVATE_KEY") {
        Ok(nsec) => Keys::parse(&nsec)?,
        Err(_) => {
            let keys = Keys::generate();
            eprintln!(
                "sprout-mcp: generated ephemeral keypair: {}",
                keys.public_key().to_hex()
            );
            keys
        }
    };

    let auth_tag = resolve_auth_tag(std::env::var("SPROUT_AUTH_TAG"), &keys.public_key())?;

    if auth_tag.is_some() {
        eprintln!("sprout-mcp: NIP-OA auth tag verified ✓");
    }

    let toolset_config = ToolsetConfig::from_env();
    eprintln!("sprout-mcp: toolsets: {:?}", toolset_config);

    eprintln!("sprout-mcp: connecting to relay at {relay_url}...");
    let client = RelayClient::connect(&relay_url, &keys, api_token.as_deref(), auth_tag).await?;
    eprintln!("sprout-mcp: connected and authenticated.");

    let tools_to_remove = toolset_config.tools_to_remove();
    let server = SproutMcpServer::new(client, Some(tools_to_remove));
    let service = server.serve(stdio()).await?;
    service.waiting().await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_valid_auth_tag(agent_keys: &nostr::Keys) -> String {
        let owner_keys = nostr::Keys::generate();
        sprout_sdk::nip_oa::compute_auth_tag(&owner_keys, &agent_keys.public_key(), "").unwrap()
    }

    #[test]
    fn resolve_auth_tag_valid() {
        let agent_keys = nostr::Keys::generate();
        let json = make_valid_auth_tag(&agent_keys);
        let result = resolve_auth_tag(Ok(json), &agent_keys.public_key());
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn resolve_auth_tag_not_present() {
        let keys = nostr::Keys::generate();
        let result = resolve_auth_tag(Err(std::env::VarError::NotPresent), &keys.public_key());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn resolve_auth_tag_empty_string() {
        let keys = nostr::Keys::generate();
        let result = resolve_auth_tag(Ok(String::new()), &keys.public_key());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn resolve_auth_tag_malformed_json() {
        let keys = nostr::Keys::generate();
        let result = resolve_auth_tag(Ok("not json".into()), &keys.public_key());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("malformed"));
    }

    #[test]
    fn resolve_auth_tag_bad_signature() {
        let agent_keys = nostr::Keys::generate();
        // Structurally valid (4 elements, correct hex lengths) but cryptographically wrong.
        let fake_json = format!(r#"["auth","{}","","{}"]"#, "a".repeat(64), "b".repeat(128));
        let result = resolve_auth_tag(Ok(fake_json), &agent_keys.public_key());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("verification failed"));
    }

    #[test]
    fn resolve_auth_tag_non_unicode() {
        let keys = nostr::Keys::generate();
        let result = resolve_auth_tag(
            Err(std::env::VarError::NotUnicode(std::ffi::OsString::from(
                "bad",
            ))),
            &keys.public_key(),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("non-Unicode"));
    }
}
