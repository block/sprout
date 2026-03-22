use crate::client::SproutClient;
use crate::error::CliError;
use crate::validate::{percent_encode, read_or_stdin, validate_hex64, validate_uuid};

pub async fn cmd_list_channels(
    client: &SproutClient,
    visibility: Option<&str>,
    member: Option<bool>,
) -> Result<(), CliError> {
    let mut path = "/api/channels".to_string();
    let mut sep = '?';
    if let Some(v) = visibility {
        path.push_str(&format!("{}visibility={}", sep, percent_encode(v)));
        sep = '&';
    }
    if let Some(m) = member {
        path.push_str(&format!("{}member={}", sep, m));
    }
    client.run_get(&path).await
}

pub async fn cmd_get_channel(client: &SproutClient, channel_id: &str) -> Result<(), CliError> {
    validate_uuid(channel_id)?;
    client
        .run_get(&format!("/api/channels/{}", channel_id))
        .await
}

pub async fn cmd_create_channel(
    client: &SproutClient,
    name: &str,
    channel_type: &str,
    visibility: &str,
    description: Option<&str>,
) -> Result<(), CliError> {
    match channel_type {
        "stream" | "forum" => {}
        _ => {
            return Err(CliError::Usage(format!(
                "--type must be 'stream' or 'forum' (got: {channel_type})"
            )))
        }
    }
    match visibility {
        "open" | "private" => {}
        _ => {
            return Err(CliError::Usage(format!(
                "--visibility must be 'open' or 'private' (got: {visibility})"
            )))
        }
    }
    let mut body = serde_json::json!({
        "name": name,
        "channel_type": channel_type,
        "visibility": visibility,
    });
    if let Some(d) = description {
        body["description"] = d.into();
    }
    client.run_post("/api/channels", &body).await
}

pub async fn cmd_update_channel(
    client: &SproutClient,
    channel_id: &str,
    name: Option<&str>,
    description: Option<&str>,
) -> Result<(), CliError> {
    if name.is_none() && description.is_none() {
        return Err(CliError::Usage(
            "at least one field required (--name, --description)".into(),
        ));
    }
    validate_uuid(channel_id)?;
    let mut body = serde_json::json!({});
    if let Some(n) = name {
        body["name"] = n.into();
    }
    if let Some(d) = description {
        body["description"] = d.into();
    }
    client
        .run_put(&format!("/api/channels/{}", channel_id), &body)
        .await
}

pub async fn cmd_set_channel_topic(
    client: &SproutClient,
    channel_id: &str,
    topic: &str,
) -> Result<(), CliError> {
    validate_uuid(channel_id)?;
    client
        .run_put(
            &format!("/api/channels/{}/topic", channel_id),
            &serde_json::json!({ "topic": topic }),
        )
        .await
}

pub async fn cmd_set_channel_purpose(
    client: &SproutClient,
    channel_id: &str,
    purpose: &str,
) -> Result<(), CliError> {
    validate_uuid(channel_id)?;
    client
        .run_put(
            &format!("/api/channels/{}/purpose", channel_id),
            &serde_json::json!({ "purpose": purpose }),
        )
        .await
}

pub async fn cmd_join_channel(client: &SproutClient, channel_id: &str) -> Result<(), CliError> {
    validate_uuid(channel_id)?;
    client
        .run_post(
            &format!("/api/channels/{}/join", channel_id),
            &serde_json::json!({}),
        )
        .await
}

pub async fn cmd_leave_channel(client: &SproutClient, channel_id: &str) -> Result<(), CliError> {
    validate_uuid(channel_id)?;
    client
        .run_post(
            &format!("/api/channels/{}/leave", channel_id),
            &serde_json::json!({}),
        )
        .await
}

pub async fn cmd_archive_channel(client: &SproutClient, channel_id: &str) -> Result<(), CliError> {
    validate_uuid(channel_id)?;
    client
        .run_post(
            &format!("/api/channels/{}/archive", channel_id),
            &serde_json::json!({}),
        )
        .await
}

pub async fn cmd_unarchive_channel(
    client: &SproutClient,
    channel_id: &str,
) -> Result<(), CliError> {
    validate_uuid(channel_id)?;
    client
        .run_post(
            &format!("/api/channels/{}/unarchive", channel_id),
            &serde_json::json!({}),
        )
        .await
}

pub async fn cmd_delete_channel(client: &SproutClient, channel_id: &str) -> Result<(), CliError> {
    validate_uuid(channel_id)?;
    client
        .run_delete(&format!("/api/channels/{}", channel_id))
        .await
}

pub async fn cmd_list_channel_members(
    client: &SproutClient,
    channel_id: &str,
) -> Result<(), CliError> {
    validate_uuid(channel_id)?;
    client
        .run_get(&format!("/api/channels/{}/members", channel_id))
        .await
}

pub async fn cmd_add_channel_member(
    client: &SproutClient,
    channel_id: &str,
    pubkey: &str,
    role: Option<&str>,
) -> Result<(), CliError> {
    validate_uuid(channel_id)?;
    validate_hex64(pubkey)?;
    let mut body = serde_json::json!({ "pubkeys": [pubkey] });
    if let Some(r) = role {
        body["role"] = r.into();
    }
    client
        .run_post(&format!("/api/channels/{}/members", channel_id), &body)
        .await
}

pub async fn cmd_remove_channel_member(
    client: &SproutClient,
    channel_id: &str,
    pubkey: &str,
) -> Result<(), CliError> {
    validate_uuid(channel_id)?;
    validate_hex64(pubkey)?;
    client
        .run_delete(&format!(
            "/api/channels/{}/members/{}",
            channel_id,
            percent_encode(pubkey),
        ))
        .await
}

pub async fn cmd_get_canvas(client: &SproutClient, channel_id: &str) -> Result<(), CliError> {
    validate_uuid(channel_id)?;
    client
        .run_get(&format!("/api/channels/{}/canvas", channel_id))
        .await
}

pub async fn cmd_set_canvas(
    client: &SproutClient,
    channel_id: &str,
    content: &str,
) -> Result<(), CliError> {
    validate_uuid(channel_id)?;
    let content = read_or_stdin(content)?;
    client
        .run_put(
            &format!("/api/channels/{}/canvas", channel_id),
            &serde_json::json!({ "content": content }),
        )
        .await
}
