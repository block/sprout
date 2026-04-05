// ABOUTME: REST client for /api/search endpoint.
// ABOUTME: Full-text search across accessible messages via Typesense.

use crate::converters::json_to_search_result;
use crate::error::SproutError;
use crate::types::SearchResult;

use super::HttpClient;

impl HttpClient {
    /// GET /api/search — full-text search.
    pub async fn search(&self, token: &str, query: &str) -> Result<Vec<SearchResult>, SproutError> {
        let path = format!("/api/search?q={}&limit=50", urlencoding::encode(query));
        let json = self.get_with_token(&path, token).await?;
        let hits = json
            .get("hits")
            .and_then(|h| h.as_array())
            .map(|arr| arr.iter().filter_map(json_to_search_result).collect())
            .unwrap_or_default();
        Ok(hits)
    }
}
