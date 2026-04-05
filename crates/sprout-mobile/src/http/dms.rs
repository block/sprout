// ABOUTME: REST client for /api/dms endpoints.
// ABOUTME: List, create, and manage direct message conversations.

use crate::converters::json_to_dm;
use crate::error::SproutError;
use crate::types::DmConversation;

use super::HttpClient;

impl HttpClient {
    /// GET /api/dms — list DM conversations.
    pub async fn list_dms(&self, token: &str) -> Result<Vec<DmConversation>, SproutError> {
        let json = self.get_with_token("/api/dms", token).await?;
        let arr = json.as_array().ok_or_else(|| SproutError::InternalError {
            message: "expected array from /api/dms".to_string(),
        })?;
        Ok(arr.iter().filter_map(json_to_dm).collect())
    }

    /// POST /api/dms — open or create a DM conversation.
    pub async fn open_dm(
        &self,
        token: &str,
        pubkeys: &[String],
    ) -> Result<DmConversation, SproutError> {
        let body = serde_json::json!({ "pubkeys": pubkeys });
        let json = self.post_with_token("/api/dms", token, &body).await?;
        json_to_dm(&json).ok_or_else(|| SproutError::InternalError {
            message: "failed to parse DM response".to_string(),
        })
    }
}
