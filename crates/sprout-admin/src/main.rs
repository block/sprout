#![deny(unsafe_code)]

use anyhow::Result;
use clap::{Parser, Subcommand};
use nostr::nips::nip19::ToBech32;
use nostr::{Keys, PublicKey};
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
    },
    /// List all active API tokens.
    ListTokens,
    /// Remove an identity binding (for key rotation or offboarding).
    UnbindIdentity {
        /// Corporate user identifier (UID from identity JWT).
        #[arg(long)]
        uid: String,

        /// Device common name. If omitted, removes all bindings for the UID.
        #[arg(long)]
        device_cn: Option<String>,

        /// Also clear verified_name from the user record(s).
        #[arg(long, default_value_t = false)]
        clear_name: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://sprout:sprout_dev@localhost:5432/sprout".to_string());

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
        } => mint_token(&db, &name, &scopes, pubkey.as_deref(), owner_pubkey).await?,
        Command::ListTokens => list_tokens(&db).await?,
        Command::UnbindIdentity {
            uid,
            device_cn,
            clear_name,
        } => unbind_identity(&db, &uid, device_cn.as_deref(), clear_name).await?,
    }

    Ok(())
}

async fn mint_token(
    db: &Db,
    name: &str,
    scopes_str: &str,
    pubkey_hex: Option<&str>,
    owner_pubkey: Option<String>,
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

    let pubkey_bytes = pubkey.serialize().to_vec();

    // ── Enforce shutdown-required scopes (before any DB writes) ─────────────
    // Two triggers, same as the relay path:
    // 1. Explicit --owner-pubkey (bootstrap mint)
    // 2. Agent already has an owner in the DB (re-mint must preserve controllability)
    // Fail closed: DB lookup error → assume owned → enforce scopes.
    let has_existing_owner = match db.get_agent_channel_policy(&pubkey_bytes).await {
        Ok(Some((_, Some(_)))) => true,
        Ok(_) => false,
        Err(e) => {
            eprintln!("warning: owner lookup failed (assuming owned): {e}");
            true // fail closed
        }
    };
    if owner_pubkey.is_some() || has_existing_owner {
        let required = [
            "users:read",
            "messages:read",
            "messages:write",
            "channels:read",
        ];
        for r in &required {
            if !scopes.iter().any(|s| s == r) {
                anyhow::bail!("owned agents require the '{r}' scope for agent controllability");
            }
        }
    }

    // ── Validate owner_pubkey (before any DB writes) ─────────────────────────
    let validated_owner = if let Some(ref owner_hex) = owner_pubkey {
        let owner_bytes =
            hex::decode(owner_hex).map_err(|e| anyhow::anyhow!("invalid owner pubkey hex: {e}"))?;
        if owner_bytes.len() != 32 {
            anyhow::bail!("owner pubkey must be 32 bytes (64 hex chars)");
        }
        Some(owner_bytes)
    } else {
        None
    };

    // ── DB writes (all validation passed) ────────────────────────────────────
    db.ensure_user(&pubkey_bytes).await?;

    if let Some(owner_bytes) = validated_owner {
        db.ensure_user(&owner_bytes).await?;
        let was_set = db.set_agent_owner(&pubkey_bytes, &owner_bytes).await?;
        if !was_set {
            let existing = db
                .get_agent_channel_policy(&pubkey_bytes)
                .await?
                .and_then(|(_, owner)| owner);
            if existing.as_deref() != Some(owner_bytes.as_slice()) {
                anyhow::bail!(
                    "agent already has a different owner — refusing to mint token for non-owner"
                );
            }
            eprintln!("note: agent already owned by the requested pubkey — proceeding");
        }
    }

    let raw_token = generate_token();
    let token_hash = hash_token(&raw_token);

    let token_id = db
        .create_api_token(&token_hash, &pubkey_bytes, name, &scopes, None, None)
        .await?;

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

async fn unbind_identity(
    db: &Db,
    uid: &str,
    device_cn: Option<&str>,
    clear_name: bool,
) -> Result<()> {
    if let Some(device_cn) = device_cn {
        // Single binding removal
        let binding = db.get_identity_binding(uid, device_cn).await?;
        let deleted = db.delete_identity_binding(uid, device_cn).await?;
        if deleted {
            println!("Removed identity binding for uid={uid}, device_cn={device_cn}");
            if clear_name {
                if let Some(binding) = binding {
                    let cleared = db.clear_verified_name(&binding.pubkey).await?;
                    if cleared {
                        println!("Cleared verified_name for the bound pubkey");
                    }
                }
            }
        } else {
            println!("No binding found for uid={uid}, device_cn={device_cn}");
        }
    } else {
        // Remove all bindings for the UID
        let bindings = db.get_bindings_for_uid(uid).await?;
        let count = db.delete_bindings_for_uid(uid).await?;
        println!("Removed {count} identity binding(s) for uid={uid}");
        if clear_name {
            for binding in &bindings {
                let cleared = db.clear_verified_name(&binding.pubkey).await?;
                if cleared {
                    println!(
                        "Cleared verified_name for pubkey bound to device_cn={}",
                        binding.device_cn
                    );
                }
            }
        }
    }
    Ok(())
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
