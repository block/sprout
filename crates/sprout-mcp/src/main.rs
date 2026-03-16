use anyhow::Result;
use nostr::Keys;
use rmcp::{transport::stdio, ServiceExt};
use tracing_subscriber::EnvFilter;

use sprout_mcp::relay_client::RelayClient;
use sprout_mcp::server::SproutMcpServer;
use sprout_mcp::toolsets::ToolsetConfig;

#[tokio::main]
async fn main() -> Result<()> {
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

    let toolset_config = ToolsetConfig::from_env();
    eprintln!("sprout-mcp: toolsets: {:?}", toolset_config);

    eprintln!("sprout-mcp: connecting to relay at {relay_url}...");
    let client = RelayClient::connect(&relay_url, &keys, api_token.as_deref()).await?;
    eprintln!("sprout-mcp: connected and authenticated.");

    let tools_to_remove = toolset_config.tools_to_remove();
    let server = SproutMcpServer::new(client, Some(tools_to_remove));
    let service = server.serve(stdio()).await?;
    service.waiting().await?;

    Ok(())
}
