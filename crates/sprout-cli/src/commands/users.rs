use crate::client::SproutClient;
use crate::error::CliError;
use crate::validate::{percent_encode, validate_hex64};

/// 3-way dispatch based on pubkey count:
///   0 pubkeys → GET /api/users/me/profile
///   1 pubkey  → GET /api/users/{pk}/profile
///   2+ pubkeys → POST /api/users/batch
pub async fn cmd_get_users(client: &SproutClient, pubkeys: &[String]) -> Result<(), CliError> {
    for pk in pubkeys {
        validate_hex64(pk)?;
    }
    if pubkeys.len() > 200 {
        return Err(CliError::Usage("--pubkey: maximum 200 pubkeys".into()));
    }

    let resp = match pubkeys.len() {
        0 => client.get_raw("/api/users/me/profile").await?,
        1 => {
            client
                .get_raw(&format!(
                    "/api/users/{}/profile",
                    percent_encode(&pubkeys[0])
                ))
                .await?
        }
        _ => {
            client
                .post_raw(
                    "/api/users/batch",
                    &serde_json::json!({ "pubkeys": pubkeys }),
                )
                .await?
        }
    };
    println!("{resp}");
    Ok(())
}

pub async fn cmd_set_profile(
    client: &SproutClient,
    display_name: Option<&str>,
    avatar_url: Option<&str>,
    about: Option<&str>,
    nip05_handle: Option<&str>,
) -> Result<(), CliError> {
    if display_name.is_none() && avatar_url.is_none() && about.is_none() && nip05_handle.is_none() {
        return Err(CliError::Usage(
            "at least one field required (--name, --avatar, --about, --nip05)".into(),
        ));
    }
    let mut body = serde_json::json!({});
    if let Some(n) = display_name {
        body["display_name"] = n.into();
    }
    if let Some(a) = avatar_url {
        body["avatar_url"] = a.into();
    }
    if let Some(a) = about {
        body["about"] = a.into();
    }
    if let Some(h) = nip05_handle {
        body["nip05_handle"] = h.into();
    }
    client.run_put("/api/users/me/profile", &body).await
}

pub async fn cmd_get_presence(client: &SproutClient, pubkeys_csv: &str) -> Result<(), CliError> {
    for pk in pubkeys_csv.split(',') {
        let pk = pk.trim();
        if !pk.is_empty() {
            validate_hex64(pk)?;
        }
    }
    let path = format!("/api/presence?pubkeys={}", percent_encode(pubkeys_csv));
    client.run_get(&path).await
}

pub async fn cmd_set_presence(client: &SproutClient, status: &str) -> Result<(), CliError> {
    match status {
        "online" | "away" | "offline" => {}
        _ => {
            return Err(CliError::Usage(format!(
                "--status must be one of: online, away, offline (got: {status})"
            )))
        }
    }
    client
        .run_put("/api/presence", &serde_json::json!({ "status": status }))
        .await
}

pub async fn cmd_set_channel_add_policy(
    client: &SproutClient,
    policy: &str,
) -> Result<(), CliError> {
    match policy {
        "anyone" | "owner_only" | "nobody" => {}
        _ => {
            return Err(CliError::Usage(format!(
                "--policy must be one of: anyone, owner_only, nobody (got: {policy})"
            )))
        }
    }
    client
        .run_put(
            "/api/users/me/channel-add-policy",
            &serde_json::json!({ "channel_add_policy": policy }),
        )
        .await
}
