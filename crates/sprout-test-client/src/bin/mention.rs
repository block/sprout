//! Send an @mention event to a Sprout channel targeting a specific pubkey.
//! Usage: mention <channel_uuid> <target_pubkey_hex> <message>

use nostr::{EventBuilder, Keys, Kind, Tag};
use sprout_test_client::SproutTestClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("Usage: mention <channel_uuid> <target_pubkey_hex> <message>");
        std::process::exit(1);
    }
    let channel_id = &args[1];
    let target_pubkey = &args[2];
    let message = &args[3];

    let url = std::env::var("SPROUT_RELAY_URL").unwrap_or_else(|_| "ws://localhost:3000".into());
    let keys = Keys::generate();
    println!("Sender pubkey: {}", keys.public_key().to_hex());

    let mut client = SproutTestClient::connect(&url, &keys).await?;

    let e_tag = Tag::parse(&["e", channel_id])?;
    let p_tag = Tag::parse(&["p", target_pubkey])?;
    let event =
        EventBuilder::new(Kind::Custom(40001), message, [e_tag, p_tag]).sign_with_keys(&keys)?;

    let ok = client.send_event(event).await?;
    if ok.accepted {
        println!("✅ @mention sent: {}", ok.event_id);
    } else {
        eprintln!("❌ Rejected: {}", ok.message);
    }
    Ok(())
}
