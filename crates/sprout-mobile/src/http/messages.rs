// ABOUTME: REST client for message and thread endpoints.
// ABOUTME: Fetch channel messages, thread replies, and reactions.

use crate::converters::json_to_message;
use crate::error::SproutError;
use crate::types::{Message, MessagePage, ReactionSummary};

use super::HttpClient;

impl HttpClient {
    /// GET /api/channels/{id}/messages — list messages (paginated).
    pub async fn list_messages(
        &self,
        token: &str,
        channel_id: &str,
        before: Option<&str>,
        limit: u32,
    ) -> Result<MessagePage, SproutError> {
        let mut path = format!("/api/channels/{channel_id}/messages?limit={limit}");
        if let Some(cursor) = before {
            path.push_str(&format!("&cursor={cursor}"));
        }
        let json = self.get_with_token(&path, token).await?;

        let messages = json
            .as_array()
            .map(|arr| arr.iter().filter_map(json_to_message).collect::<Vec<_>>())
            .unwrap_or_default();

        let has_more = messages.len() == limit as usize;

        Ok(MessagePage { messages, has_more })
    }

    /// GET /api/channels/{id}/threads/{event_id} — fetch a full thread.
    pub async fn get_thread(
        &self,
        token: &str,
        channel_id: &str,
        root_event_id: &str,
    ) -> Result<Vec<Message>, SproutError> {
        let json = self
            .get_with_token(
                &format!("/api/channels/{channel_id}/threads/{root_event_id}"),
                token,
            )
            .await?;

        // The response may have a "replies" array.
        let replies = json
            .get("replies")
            .and_then(|r| r.as_array())
            .map(|arr| arr.iter().filter_map(json_to_message).collect())
            .unwrap_or_default();

        Ok(replies)
    }

    /// GET /api/messages/{event_id}/reactions — list reactions on a message.
    #[allow(dead_code)]
    pub async fn get_reactions(
        &self,
        token: &str,
        event_id: &str,
    ) -> Result<Vec<ReactionSummary>, SproutError> {
        let json = self
            .get_with_token(&format!("/api/messages/{event_id}/reactions"), token)
            .await?;

        let reactions = json
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| {
                        Some(ReactionSummary {
                            emoji: v.get("emoji")?.as_str()?.to_string(),
                            count: v.get("count")?.as_u64()? as u32,
                            reacted_by_me: false, // populated client-side
                            my_reaction_event_id: None,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(reactions)
    }
}
