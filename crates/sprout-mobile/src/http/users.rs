// ABOUTME: REST client for /api/users endpoints.
// ABOUTME: Fetch profiles, batch lookup, search users, and update own profile.

use crate::converters::json_to_profile;
use crate::error::SproutError;
use crate::types::UserProfile;

use super::HttpClient;

impl HttpClient {
    /// GET /api/users/{pubkey}/profile — get a user's profile.
    pub async fn get_user_profile(
        &self,
        token: &str,
        pubkey: &str,
    ) -> Result<UserProfile, SproutError> {
        let json = self
            .get_with_token(&format!("/api/users/{pubkey}/profile"), token)
            .await?;
        json_to_profile(&json).ok_or_else(|| SproutError::NotFound {
            entity: format!("profile for {pubkey}"),
        })
    }

    /// POST /api/users/batch — batch-fetch multiple profiles.
    pub async fn get_users_batch(
        &self,
        token: &str,
        pubkeys: &[String],
    ) -> Result<Vec<UserProfile>, SproutError> {
        let body = serde_json::json!({ "pubkeys": pubkeys });
        let json = self
            .post_with_token("/api/users/batch", token, &body)
            .await?;

        let profiles = json
            .get("profiles")
            .and_then(|p| p.as_object())
            .map(|map| {
                map.iter()
                    .filter_map(|(pubkey, v)| {
                        let mut profile = json_to_profile(v)?;
                        // The batch endpoint returns profiles keyed by pubkey;
                        // the inner object may not repeat the pubkey field.
                        if profile.pubkey.is_empty() {
                            profile.pubkey = pubkey.clone();
                        }
                        Some(profile)
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(profiles)
    }

    /// GET /api/users/search — search users by name or pubkey.
    pub async fn search_users(
        &self,
        token: &str,
        query: &str,
    ) -> Result<Vec<UserProfile>, SproutError> {
        let path = format!("/api/users/search?q={}", urlencoding::encode(query));
        let json = self.get_with_token(&path, token).await?;
        let arr = json
            .as_array()
            .map(|a| a.iter().filter_map(json_to_profile).collect())
            .unwrap_or_default();
        Ok(arr)
    }
}
