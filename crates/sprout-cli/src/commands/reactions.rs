use nostr::EventId;

use crate::client::SproutClient;
use crate::error::CliError;
use crate::validate::{percent_encode, validate_hex64};

/// Require keys on the client — fail fast with a clear error if absent.
macro_rules! require_keys {
    ($client:expr) => {
        $client.keys().ok_or_else(|| {
            CliError::Key(
                "private key required for write operations (set SPROUT_PRIVATE_KEY)".into(),
            )
        })?
    };
}

pub async fn cmd_add_reaction(
    client: &SproutClient,
    event_id: &str,
    emoji: &str,
) -> Result<(), CliError> {
    validate_hex64(event_id)?;
    let keys = require_keys!(client);

    let target_eid =
        EventId::parse(event_id).map_err(|e| CliError::Usage(format!("invalid event ID: {e}")))?;

    let builder = sprout_sdk::build_reaction(target_eid, emoji)
        .map_err(|e| CliError::Other(format!("build_reaction failed: {e}")))?;

    let event = builder
        .sign_with_keys(keys)
        .map_err(|e| CliError::Other(format!("signing failed: {e}")))?;

    let resp = client.submit_event(event).await?;
    println!("{resp}");
    Ok(())
}

pub async fn cmd_remove_reaction(
    client: &SproutClient,
    event_id: &str,
    emoji: &str,
) -> Result<(), CliError> {
    validate_hex64(event_id)?;
    let keys = require_keys!(client);

    // To build a NIP-05 deletion event we need the original reaction event ID.
    // Fetch the reactions list and find the caller's reaction for this emoji.
    let reaction_event_id = find_my_reaction_event_id(client, event_id, emoji).await?;

    let builder = sprout_sdk::build_remove_reaction(reaction_event_id)
        .map_err(|e| CliError::Other(format!("build_remove_reaction failed: {e}")))?;

    let event = builder
        .sign_with_keys(keys)
        .map_err(|e| CliError::Other(format!("signing failed: {e}")))?;

    let resp = client.submit_event(event).await?;
    println!("{resp}");
    Ok(())
}

/// Fetch GET /api/messages/{id}/reactions and find the caller's reaction event ID
/// for the given emoji. The caller's pubkey is inferred from the retained keys.
///
/// The reactions list groups by emoji and includes per-user `reaction_event_id` if
/// the relay exposes it. If the field is absent, falls back to REST DELETE.
async fn find_my_reaction_event_id(
    client: &SproutClient,
    message_event_id: &str,
    emoji: &str,
) -> Result<EventId, CliError> {
    let keys = require_keys!(client);
    let my_pubkey = keys.public_key().to_hex();

    let raw = client
        .get_raw(&format!(
            "/api/messages/{}/reactions",
            percent_encode(message_event_id)
        ))
        .await?;

    let v: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| CliError::Other(format!("failed to parse reactions response: {e}")))?;

    // Response shape: { "reactions": [ { "emoji": "👍", "users": [ { "pubkey": "...", "reaction_event_id": "..." } ] } ] }
    let reactions = v
        .get("reactions")
        .and_then(|r| r.as_array())
        .ok_or_else(|| CliError::Other("reactions response missing 'reactions' array".into()))?;

    let empty_vec = vec![];
    for group in reactions {
        let group_emoji = group.get("emoji").and_then(|e| e.as_str()).unwrap_or("");
        if group_emoji != emoji {
            continue;
        }
        let users = group
            .get("users")
            .and_then(|u| u.as_array())
            .unwrap_or(&empty_vec);
        for user in users {
            let pubkey = user.get("pubkey").and_then(|p| p.as_str()).unwrap_or("");
            if !pubkey.eq_ignore_ascii_case(&my_pubkey) {
                continue;
            }
            // Found our reaction — extract the reaction_event_id if present
            if let Some(reid) = user.get("reaction_event_id").and_then(|r| r.as_str()) {
                return EventId::parse(reid)
                    .map_err(|e| CliError::Other(format!("invalid reaction_event_id: {e}")));
            }
            // TODO: relay does not yet expose reaction_event_id in list response.
            // When it does, this branch will be unreachable.
            return Err(CliError::Other(
                "relay does not expose reaction_event_id in reactions list; \
                 cannot build signed deletion event"
                    .into(),
            ));
        }
    }

    Err(CliError::Other(format!(
        "no reaction with emoji '{emoji}' found for your pubkey on event {message_event_id}"
    )))
}

pub async fn cmd_get_reactions(client: &SproutClient, event_id: &str) -> Result<(), CliError> {
    validate_hex64(event_id)?;
    client
        .run_get(&format!(
            "/api/messages/{}/reactions",
            percent_encode(event_id),
        ))
        .await
}
