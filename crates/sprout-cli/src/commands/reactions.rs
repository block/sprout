use crate::client::SproutClient;
use crate::error::CliError;
use crate::validate::{percent_encode, validate_hex64};

pub async fn cmd_add_reaction(
    client: &SproutClient,
    event_id: &str,
    emoji: &str,
) -> Result<(), CliError> {
    validate_hex64(event_id)?;
    client
        .run_post(
            &format!("/api/messages/{}/reactions", percent_encode(event_id)),
            &serde_json::json!({ "emoji": emoji }),
        )
        .await
}

pub async fn cmd_remove_reaction(
    client: &SproutClient,
    event_id: &str,
    emoji: &str,
) -> Result<(), CliError> {
    validate_hex64(event_id)?;
    client
        .run_delete(&format!(
            "/api/messages/{}/reactions/{}",
            percent_encode(event_id),
            percent_encode(emoji),
        ))
        .await
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
