use crate::client::SproutClient;
use crate::error::CliError;
use crate::validate::{percent_encode, validate_hex64, validate_uuid};

pub async fn cmd_list_dms(
    client: &SproutClient,
    cursor: Option<&str>,
    limit: Option<u32>,
) -> Result<(), CliError> {
    let mut path = "/api/dms".to_string();
    let mut sep = '?';
    if let Some(c) = cursor {
        path.push_str(&format!("{}cursor={}", sep, percent_encode(c)));
        sep = '&';
    }
    if let Some(l) = limit {
        path.push_str(&format!("{}limit={}", sep, l));
    }
    client.run_get(&path).await
}

pub async fn cmd_open_dm(client: &SproutClient, pubkeys: &[String]) -> Result<(), CliError> {
    if pubkeys.is_empty() || pubkeys.len() > 8 {
        return Err(CliError::Usage("--pubkey: must provide 1–8 pubkeys".into()));
    }
    for pk in pubkeys {
        validate_hex64(pk)?;
    }
    client
        .run_post("/api/dms", &serde_json::json!({ "pubkeys": pubkeys }))
        .await
}

pub async fn cmd_add_dm_member(
    client: &SproutClient,
    channel_id: &str,
    pubkey: &str,
) -> Result<(), CliError> {
    validate_uuid(channel_id)?;
    validate_hex64(pubkey)?;
    client
        .run_post(
            &format!("/api/dms/{}/members", channel_id),
            &serde_json::json!({ "pubkeys": [pubkey] }),
        )
        .await
}
