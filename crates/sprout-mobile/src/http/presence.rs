// ABOUTME: REST client for /api/presence endpoint.
// ABOUTME: Get and set user presence status (online/away/offline).

use crate::error::SproutError;
use crate::types::PresenceStatus;

use super::HttpClient;

impl HttpClient {
    /// PUT /api/presence — set own presence status.
    pub async fn set_presence(
        &self,
        token: &str,
        status: &PresenceStatus,
    ) -> Result<(), SproutError> {
        let status_str = match status {
            PresenceStatus::Online => "online",
            PresenceStatus::Away => "away",
            PresenceStatus::Offline => "offline",
        };
        let body = serde_json::json!({ "status": status_str });
        self.put_with_token("/api/presence", token, &body).await?;
        Ok(())
    }
}
