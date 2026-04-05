// ABOUTME: REST client for /api/channels endpoints.
// ABOUTME: List, create, update, archive, join, and leave channels.

use crate::converters::{json_to_channel, json_to_member};
use crate::error::SproutError;
use crate::types::{Channel, ChannelMember};

use super::HttpClient;

impl HttpClient {
    /// GET /api/channels — list channels accessible to the current user.
    pub async fn list_channels(&self, token: &str) -> Result<Vec<Channel>, SproutError> {
        let json = self.get_with_token("/api/channels", token).await?;
        let arr = json.as_array().ok_or_else(|| SproutError::InternalError {
            message: "expected array from /api/channels".to_string(),
        })?;
        Ok(arr.iter().filter_map(json_to_channel).collect())
    }

    /// GET /api/channels/{id} — get channel details.
    pub async fn get_channel_detail(
        &self,
        token: &str,
        channel_id: &str,
    ) -> Result<Channel, SproutError> {
        let json = self
            .get_with_token(&format!("/api/channels/{channel_id}"), token)
            .await?;
        json_to_channel(&json).ok_or_else(|| SproutError::InternalError {
            message: "failed to parse channel response".to_string(),
        })
    }

    /// GET /api/channels/{id}/members — list channel members.
    pub async fn list_channel_members(
        &self,
        token: &str,
        channel_id: &str,
    ) -> Result<Vec<ChannelMember>, SproutError> {
        let json = self
            .get_with_token(&format!("/api/channels/{channel_id}/members"), token)
            .await?;
        let arr = json.as_array().ok_or_else(|| SproutError::InternalError {
            message: "expected array from members endpoint".to_string(),
        })?;
        Ok(arr.iter().filter_map(json_to_member).collect())
    }
}
