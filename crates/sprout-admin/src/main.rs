#![deny(unsafe_code)]

//! Sprout instance administration CLI.
//!
//! In the pure Nostr architecture, API tokens no longer exist.
//! Admin operations are performed via signed Nostr events (NIP-43 relay admin commands).
//! This binary is retained as a placeholder for future admin tooling.

use anyhow::Result;
use clap::{Parser, Subcommand};
use nostr::Keys;
use sprout_db::{Db, DbConfig};

#[derive(Parser)]
#[command(name = "sprout-admin", about = "Sprout instance administration")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Add a pubkey to the relay membership list.
    AddMember {
        /// Nostr public key (hex) to add.
        #[arg(long)]
        pubkey: String,

        /// Role: "admin" or "member" (default: member).
        #[arg(long, default_value = "member")]
        role: String,
    },
    /// List all relay members.
    ListMembers,
    /// Generate a new Nostr keypair (for bootstrapping).
    GenerateKey,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::GenerateKey => {
            let keys = Keys::generate();
            println!("Public key:  {}", keys.public_key().to_hex());
            println!("Secret key:  {}", keys.secret_key().display_secret());
            println!("\nSet SPROUT_PRIVATE_KEY to the secret key to use this identity.");
        }
        Command::AddMember { pubkey, role } => {
            let db = connect_db().await?;
            let pk_bytes = hex::decode(&pubkey)?;
            if pk_bytes.len() != 32 {
                anyhow::bail!("pubkey must be 32 bytes (64 hex chars)");
            }
            db.ensure_user(&pk_bytes).await?;
            // Add to relay members via DB (admin bootstrap — normally done via kind:9030)
            db.add_relay_member(&pubkey, &role, None).await?;
            println!("Added {} as {} to relay membership list.", pubkey, role);
        }
        Command::ListMembers => {
            let db = connect_db().await?;
            let members = db.list_relay_members().await?;
            if members.is_empty() {
                println!("No relay members found.");
            } else {
                println!("{:<66}  {:<10}", "Pubkey", "Role");
                println!("{}", "-".repeat(78));
                for m in &members {
                    println!("{:<66}  {:<10}", hex::encode(&m.pubkey), m.role);
                }
            }
        }
    }

    Ok(())
}

async fn connect_db() -> Result<Db> {
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://sprout:sprout_dev@localhost:5432/sprout".to_string());
    let db = Db::new(&DbConfig {
        database_url: db_url,
        ..DbConfig::default()
    })
    .await?;
    Ok(db)
}
