#![deny(unsafe_code)]

use anyhow::Result;
use clap::{Parser, Subcommand};
use nostr::nips::nip19::ToBech32;
use nostr::{Keys, PublicKey};
use serde::Serialize;
use sprout_auth::token::{generate_token, hash_token};
use sprout_db::{Db, DbConfig};

#[derive(Parser)]
#[command(name = "sprout-admin", about = "Sprout instance administration")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a new API token for an agent.
    MintToken {
        /// Token name
        #[arg(long)]
        name: String,

        /// Comma-separated scopes (messages:read, messages:write, channels:read,
        /// channels:write, admin:channels, files:read, files:write)
        #[arg(long)]
        scopes: String,

        /// Nostr public key (hex). If omitted, generates a new keypair.
        #[arg(long)]
        pubkey: Option<String>,

        /// Hex pubkey of the human operator who owns this agent.
        /// If provided, sets agent_owner_pubkey in the users table.
        #[arg(long)]
        owner_pubkey: Option<String>,

        /// Emit a machine-readable JSON payload instead of the boxed summary.
        #[arg(long)]
        json: bool,
    },
    /// List all active API tokens.
    ListTokens,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "mysql://sprout:sprout_dev@localhost:3306/sprout".to_string());

    let db = Db::new(&DbConfig {
        database_url: db_url,
        ..DbConfig::default()
    })
    .await?;

    match cli.command {
        Command::MintToken {
            name,
            scopes,
            pubkey,
            owner_pubkey,
            json,
        } => mint_token(&db, &name, &scopes, pubkey.as_deref(), owner_pubkey, json).await?,
        Command::ListTokens => list_tokens(&db).await?,
    }

    Ok(())
}

async fn mint_token(
    db: &Db,
    name: &str,
    scopes_str: &str,
    pubkey_hex: Option<&str>,
    owner_pubkey: Option<String>,
    json: bool,
) -> Result<()> {
    let scopes: Vec<String> = scopes_str
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    let (pubkey, generated_keys) = match pubkey_hex {
        Some(hex) => (PublicKey::from_hex(hex)?, None),
        None => {
            let keys = Keys::generate();
            (keys.public_key(), Some(keys))
        }
    };

    let pubkey_bytes = pubkey.to_bytes().to_vec();

    db.ensure_user(&pubkey_bytes).await?;

    // Set agent owner if --owner-pubkey was provided
    if let Some(ref owner_hex) = owner_pubkey {
        let owner_bytes =
            hex::decode(owner_hex).map_err(|e| anyhow::anyhow!("invalid owner pubkey hex: {e}"))?;
        if owner_bytes.len() != 32 {
            anyhow::bail!("owner pubkey must be 32 bytes (64 hex chars)");
        }
        // Ensure owner's user row exists (FK constraint requires it)
        db.ensure_user(&owner_bytes).await?;
        db.set_agent_owner(&pubkey_bytes, &owner_bytes).await?;
    }

    let raw_token = generate_token();
    let token_hash = hash_token(&raw_token);

    let token_id = db
        .create_api_token(&token_hash, &pubkey_bytes, name, &scopes, None, None)
        .await?;

    let private_key_nsec = generated_keys.as_ref().map(|keys| {
        keys.secret_key()
            .to_bech32()
            .unwrap_or_else(|_| "error encoding".into())
    });

    if json {
        let payload = MintTokenJsonOutput {
            token_id: token_id.to_string(),
            name: name.to_string(),
            scopes,
            pubkey: pubkey.to_hex(),
            private_key_nsec,
            api_token: raw_token,
        };
        println!(
            "{}",
            serde_json::to_string(&payload).expect("mint token JSON should serialize")
        );
        return Ok(());
    }

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║  Token minted successfully!                                 ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  Token ID:    {:<46} ║", token_id);
    println!("║  Name:        {:<46} ║", name);
    println!("║  Scopes:      {:<46} ║", scopes_str);
    println!("║  Pubkey:      {}...║", &pubkey.to_hex()[..48]);
    println!("╠══════════════════════════════════════════════════════════════╣");

    if let Some(keys) = generated_keys {
        println!("║  ⚠️  SAVE THESE — shown only once!                          ║");
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║  Private key (nsec):                                        ║");
        println!(
            "║  {}  ║",
            keys.secret_key()
                .to_bech32()
                .unwrap_or_else(|_| "error encoding".into())
        );
        println!("║                                                              ║");
    }

    println!("║  API Token:                                                  ║");
    println!("║  {}  ║", raw_token);
    println!("╚══════════════════════════════════════════════════════════════╝");

    Ok(())
}

#[derive(Serialize)]
struct MintTokenJsonOutput {
    token_id: String,
    name: String,
    scopes: Vec<String>,
    pubkey: String,
    private_key_nsec: Option<String>,
    api_token: String,
}

async fn list_tokens(db: &Db) -> Result<()> {
    let tokens = db.list_active_tokens().await?;

    if tokens.is_empty() {
        println!("No active tokens found.");
        return Ok(());
    }

    println!(
        "{:<36}  {:<20}  {:<40}  {:<20}",
        "ID", "Name", "Scopes", "Created"
    );
    println!("{}", "-".repeat(120));

    for t in &tokens {
        let scopes_str = t.scopes.join(",");
        let id_str = t.id.to_string();
        println!(
            "{:<36}  {:<20}  {:<40}  {:<20}",
            &id_str[..36.min(id_str.len())],
            &t.name[..20.min(t.name.len())],
            &scopes_str[..40.min(scopes_str.len())],
            t.created_at.format("%Y-%m-%d %H:%M"),
        );
    }

    Ok(())
}
