use crate::client::SproutClient;
use crate::error::CliError;
use crate::validate::percent_encode;

pub async fn cmd_get_feed(
    client: &SproutClient,
    since: Option<i64>,
    limit: Option<u32>,
    types: Option<&str>,
) -> Result<(), CliError> {
    let limit = limit.unwrap_or(20).min(50);
    let mut path = format!("/api/feed?limit={}", limit);
    if let Some(s) = since {
        path.push_str(&format!("&since={s}"));
    }
    if let Some(t) = types {
        path.push_str(&format!("&types={}", percent_encode(t)));
    }
    client.run_get(&path).await
}
