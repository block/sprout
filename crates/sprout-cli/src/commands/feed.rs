use std::cmp::Reverse;

use crate::client::{normalize_events, SproutClient};
use crate::error::CliError;

/// Get activity feed — query events mentioning our pubkey (via p-tag).
pub async fn cmd_get_feed(
    client: &SproutClient,
    since: Option<i64>,
    limit: Option<u32>,
    _types: Option<&str>,
    format: &crate::OutputFormat,
) -> Result<(), CliError> {
    let my_pk = client.keys().public_key().to_hex();
    let limit = limit.unwrap_or(20).min(50);

    let mut filter = serde_json::json!({
        "#p": [my_pk],
        "limit": limit
    });

    if let Some(s) = since {
        filter["since"] = serde_json::json!(s);
    }

    let resp = client.query(&filter).await?;
    let mut events: Vec<serde_json::Value> = serde_json::from_str(&resp).unwrap_or_default();
    events.sort_by_key(|e| Reverse(e.get("created_at").and_then(|v| v.as_u64()).unwrap_or(0)));
    let normalized = normalize_events(&events);
    let output = match format {
        crate::OutputFormat::Compact => {
            let evts: Vec<serde_json::Value> =
                serde_json::from_str(&normalized).unwrap_or_default();
            let compact: Vec<serde_json::Value> = evts
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "id": e.get("id").cloned().unwrap_or_default(),
                        "content": e.get("content").cloned().unwrap_or_default(),
                        "created_at": e.get("created_at").cloned().unwrap_or_default(),
                    })
                })
                .collect();
            serde_json::to_string(&compact).unwrap_or_default()
        }
        crate::OutputFormat::Json => normalized,
    };
    println!("{output}");
    Ok(())
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

pub async fn dispatch(
    cmd: crate::FeedCmd,
    client: &SproutClient,
    format: &crate::OutputFormat,
) -> Result<(), CliError> {
    use crate::FeedCmd;
    match cmd {
        FeedCmd::Get {
            since,
            limit,
            types,
        } => cmd_get_feed(client, since, limit, types.as_deref(), format).await,
    }
}
