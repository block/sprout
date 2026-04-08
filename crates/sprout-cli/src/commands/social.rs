use nostr::EventId;
use serde::Deserialize;

use crate::client::SproutClient;
use crate::error::CliError;
use crate::validate::validate_hex64;

/// Each command module defines its own `require_keys!` macro.
macro_rules! require_keys {
    ($client:expr) => {
        $client.keys().ok_or_else(|| {
            CliError::Key(
                "private key required for write operations (set SPROUT_PRIVATE_KEY)".into(),
            )
        })?
    };
}

/// Per-module helper (same pattern as messages.rs).
fn parse_event_id(hex: &str) -> Result<EventId, CliError> {
    EventId::parse(hex).map_err(|e| CliError::Usage(format!("invalid event ID: {e}")))
}

/// A single contact entry (CLI-local, not from sprout-sdk).
#[derive(Debug, Deserialize)]
pub struct ContactEntry {
    pub pubkey: String,
    #[serde(default)]
    pub relay_url: Option<String>,
    #[serde(default)]
    pub petname: Option<String>,
}

pub async fn cmd_publish_note(
    client: &SproutClient,
    content: &str,
    reply_to: Option<&str>,
) -> Result<(), CliError> {
    if let Some(r) = reply_to {
        validate_hex64(r)?;
    }

    let keys = require_keys!(client);
    let reply_id = reply_to.map(parse_event_id).transpose()?;

    let builder = sprout_sdk::build_note(content, reply_id)
        .map_err(|e| CliError::Other(format!("build error: {e}")))?;

    let event = builder
        .sign_with_keys(keys)
        .map_err(|e| CliError::Other(format!("signing failed: {e}")))?;

    let resp = client.submit_event(event).await?;
    println!("{resp}");
    Ok(())
}

pub async fn cmd_set_contact_list(
    client: &SproutClient,
    contacts_json: &str,
) -> Result<(), CliError> {
    let keys = require_keys!(client);
    let entries: Vec<ContactEntry> = serde_json::from_str(contacts_json)
        .map_err(|e| CliError::Usage(format!("invalid contacts JSON: {e}")))?;

    let contacts: Vec<(&str, Option<&str>, Option<&str>)> = entries
        .iter()
        .map(|c| {
            (
                c.pubkey.as_str(),
                c.relay_url.as_deref(),
                c.petname.as_deref(),
            )
        })
        .collect();

    let builder = sprout_sdk::build_contact_list(&contacts)
        .map_err(|e| CliError::Other(format!("build error: {e}")))?;

    let event = builder
        .sign_with_keys(keys)
        .map_err(|e| CliError::Other(format!("signing failed: {e}")))?;

    let resp = client.submit_event(event).await?;
    println!("{resp}");
    Ok(())
}

pub async fn cmd_get_event(client: &SproutClient, event_id: &str) -> Result<(), CliError> {
    validate_hex64(event_id)?;
    let path = format!("/api/events/{event_id}");
    client.run_get(&path).await
}

pub async fn cmd_get_user_notes(
    client: &SproutClient,
    pubkey: &str,
    limit: Option<u32>,
    before: Option<i64>,
    before_id: Option<&str>,
) -> Result<(), CliError> {
    validate_hex64(pubkey)?;
    if let Some(bid) = before_id {
        validate_hex64(bid)?;
    }
    let mut path = format!("/api/users/{pubkey}/notes");
    let mut params = vec![];
    if let Some(l) = limit {
        params.push(format!("limit={l}"));
    }
    if let Some(b) = before {
        params.push(format!("before={b}"));
    }
    if let Some(bid) = before_id {
        params.push(format!("before_id={bid}"));
    }
    if !params.is_empty() {
        path.push('?');
        path.push_str(&params.join("&"));
    }
    client.run_get(&path).await
}

pub async fn cmd_get_contact_list(client: &SproutClient, pubkey: &str) -> Result<(), CliError> {
    validate_hex64(pubkey)?;
    let path = format!("/api/users/{pubkey}/contact-list");
    client.run_get(&path).await
}
