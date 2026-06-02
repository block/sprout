use crate::client::{normalize_write_response, SproutClient};
use crate::error::CliError;

/// Custom emoji entry in CLI output.
#[derive(Debug, serde::Serialize)]
struct EmojiEntry {
    shortcode: String,
    url: String,
}

fn parse_custom_emoji_set(events: &[serde_json::Value]) -> Vec<EmojiEntry> {
    let Some(event) = events.first() else {
        return vec![];
    };
    let Some(tags) = event.get("tags").and_then(|v| v.as_array()) else {
        return vec![];
    };
    let mut out = Vec::new();
    for tag in tags {
        let Some(parts) = tag.as_array() else {
            continue;
        };
        if parts.first().and_then(|v| v.as_str()) != Some("emoji") {
            continue;
        }
        let Some(shortcode) = parts.get(1).and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(url) = parts.get(2).and_then(|v| v.as_str()) else {
            continue;
        };
        out.push(EmojiEntry {
            shortcode: shortcode.to_string(),
            url: url.to_string(),
        });
    }
    out.sort_by(|a, b| a.shortcode.cmp(&b.shortcode));
    out
}

async fn cmd_list(client: &SproutClient) -> Result<(), CliError> {
    let filter = serde_json::json!({
        "kinds": [sprout_sdk::kind::KIND_EMOJI_SET],
        "#d": [sprout_sdk::kind::KIND_EMOJI_SET_D_TAG],
        "limit": 1
    });
    let raw = client.query(&filter).await?;
    let events: Vec<serde_json::Value> = serde_json::from_str(&raw)
        .map_err(|e| CliError::Other(format!("failed to parse emoji set query: {e}")))?;
    let emojis = parse_custom_emoji_set(&events);
    let output = serde_json::json!({ "emojis": emojis });
    println!("{}", serde_json::to_string(&output).unwrap_or_default());
    Ok(())
}

async fn cmd_set(client: &SproutClient, shortcode: &str, url: &str) -> Result<(), CliError> {
    let builder = sprout_sdk::build_set_custom_emoji(shortcode, url)
        .map_err(|e| CliError::Other(format!("build_set_custom_emoji failed: {e}")))?;
    let event = client.sign_event(builder)?;
    let resp = client.submit_event(event).await?;
    println!("{}", normalize_write_response(&resp));
    Ok(())
}

async fn cmd_rm(client: &SproutClient, shortcode: &str) -> Result<(), CliError> {
    let builder = sprout_sdk::build_remove_custom_emoji(shortcode)
        .map_err(|e| CliError::Other(format!("build_remove_custom_emoji failed: {e}")))?;
    let event = client.sign_event(builder)?;
    let resp = client.submit_event(event).await?;
    println!("{}", normalize_write_response(&resp));
    Ok(())
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

pub async fn dispatch(cmd: crate::EmojiCmd, client: &SproutClient) -> Result<(), CliError> {
    use crate::EmojiCmd;
    match cmd {
        EmojiCmd::List => cmd_list(client).await,
        EmojiCmd::Set { shortcode, url } => cmd_set(client, &shortcode, &url).await,
        EmojiCmd::Rm { shortcode } => cmd_rm(client, &shortcode).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_custom_emoji_set_extracts_emoji_tags() {
        let events = vec![serde_json::json!({
            "tags": [
                ["d", "sprout:relay-emoji"],
                ["emoji", "zort", "https://example.com/zort.png"],
                ["emoji", "narf", "https://example.com/narf.png"]
            ]
        })];
        let emojis = parse_custom_emoji_set(&events);
        assert_eq!(emojis.len(), 2);
        assert_eq!(emojis[0].shortcode, "narf");
        assert_eq!(emojis[1].shortcode, "zort");
    }
}
