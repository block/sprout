// ABOUTME: REST client for /api/feed endpoint.
// ABOUTME: Fetch personalized home feed (mentions, needs_action, activity).

use crate::converters::json_to_feed_item;
use crate::error::SproutError;
use crate::types::{FeedCategory, HomeFeed};

use super::HttpClient;

impl HttpClient {
    /// GET /api/feed — fetch the personalized home feed.
    pub async fn get_feed(&self, token: &str) -> Result<HomeFeed, SproutError> {
        let json = self.get_with_token("/api/feed", token).await?;

        let feed = json.get("feed");

        let mut items = Vec::new();

        if let Some(mentions) = feed
            .and_then(|f| f.get("mentions"))
            .and_then(|m| m.as_array())
        {
            for v in mentions {
                if let Some(item) = json_to_feed_item(v, FeedCategory::Mention) {
                    items.push(item);
                }
            }
        }

        if let Some(needs_action) = feed
            .and_then(|f| f.get("needs_action"))
            .and_then(|m| m.as_array())
        {
            for v in needs_action {
                if let Some(item) = json_to_feed_item(v, FeedCategory::NeedsAction) {
                    items.push(item);
                }
            }
        }

        if let Some(activity) = feed
            .and_then(|f| f.get("activity"))
            .and_then(|m| m.as_array())
        {
            for v in activity {
                if let Some(item) = json_to_feed_item(v, FeedCategory::Activity) {
                    items.push(item);
                }
            }
        }

        if let Some(agent) = feed
            .and_then(|f| f.get("agent_activity"))
            .and_then(|m| m.as_array())
        {
            for v in agent {
                if let Some(item) = json_to_feed_item(v, FeedCategory::AgentActivity) {
                    items.push(item);
                }
            }
        }

        let total = items.len() as u32;

        Ok(HomeFeed { items, total })
    }
}
