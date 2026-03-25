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

    let keys = require_keys!(client);

    // Read-merge-write: fetch current profile, merge in the new fields, then sign.
    // This preserves fields the caller didn't specify (e.g. existing avatar stays
    // intact when only --name is passed).
    let current = fetch_current_profile(client).await?;

    // Merge: caller-supplied fields win; fall back to current profile values.
    let merged_name = display_name
        .map(|s| s.to_string())
        .or_else(|| {
            current
                .get("display_name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .or_else(|| {
            current
                .get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        });
    let merged_picture = avatar_url.map(|s| s.to_string()).or_else(|| {
        current
            .get("picture")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    });
    let merged_about = about.map(|s| s.to_string()).or_else(|| {
        current
            .get("about")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    });
    let merged_nip05 = nip05_handle.map(|s| s.to_string()).or_else(|| {
        current
            .get("nip05")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    });

    let builder = sprout_sdk::build_profile(
        merged_name.as_deref(),
        None, // `name` field (username) — not exposed by CLI; preserve via display_name
        merged_picture.as_deref(),
        merged_about.as_deref(),
        merged_nip05.as_deref(),
        None, // agent_type — not exposed by CLI
    )
    .map_err(|e| CliError::Other(format!("build_profile failed: {e}")))?;

    let event = builder
        .sign_with_keys(keys)
        .map_err(|e| CliError::Other(format!("signing failed: {e}")))?;

    let resp = client.submit_event(event).await?;
    println!("{resp}");
    Ok(())
}

/// Fetch the current user's profile metadata from GET /api/users/me/profile.
/// Returns the parsed JSON object (kind:0 content fields), or an empty object
/// if the profile hasn't been set yet.
async fn fetch_current_profile(
    client: &SproutClient,
) -> Result<serde_json::Map<String, serde_json::Value>, CliError> {
    let raw = client.get_raw("/api/users/me/profile").await;
    match raw {
        Ok(body) => {
            let v: serde_json::Value = serde_json::from_str(&body)
                .map_err(|e| CliError::Other(format!("failed to parse profile response: {e}")))?;
            // The relay may return the profile fields at top level or nested under "profile"
            let obj = if let Some(profile) = v.get("profile").and_then(|p| p.as_object()) {
                profile.clone()
            } else if let Some(obj) = v.as_object() {
                obj.clone()
            } else {
                serde_json::Map::new()
            };
            Ok(obj)
        }
        // 404 = no profile yet — start fresh
        Err(CliError::Relay { status: 404, .. }) => Ok(serde_json::Map::new()),
        Err(e) => Err(e),
    }
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
