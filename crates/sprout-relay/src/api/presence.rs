//! GET /api/presence — bulk presence lookup by pubkey.

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use serde::Deserialize;

use crate::state::AppState;

use super::extract_auth_pubkey;

/// Query parameters for the presence endpoint.
#[derive(Debug, Deserialize)]
pub struct PresenceParams {
    /// Comma-separated list of hex-encoded public keys to look up.
    pub pubkeys: Option<String>,
}

/// Bulk presence lookup for a comma-separated list of hex pubkeys.
///
/// Caps at 200 pubkeys to prevent DoS. Returns `"offline"` for any pubkey
/// not found in the presence store.
pub async fn presence_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<PresenceParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (_pubkey, _pubkey_bytes) = extract_auth_pubkey(&headers, &state).await?;

    let pubkeys_param = params.pubkeys.unwrap_or_default();

    // Parse comma-separated hex pubkeys; skip invalid ones. Cap at 200 to prevent DoS.
    let pubkeys: Vec<nostr::PublicKey> = pubkeys_param
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .take(200)
        .filter_map(|hex| nostr::PublicKey::from_hex(hex).ok())
        .collect();

    if pubkeys.is_empty() {
        return Ok(Json(serde_json::json!({})));
    }

    let presence_map = state
        .pubsub
        .get_presence_bulk(&pubkeys)
        .await
        .unwrap_or_default();

    // Build result: pubkey_hex → status. Include "offline" for any requested
    // pubkey not found in the presence map.
    let mut result = serde_json::Map::new();
    for pk in &pubkeys {
        let hex = pk.to_hex();
        let status = presence_map
            .get(&hex)
            .cloned()
            .unwrap_or_else(|| "offline".to_string());
        result.insert(hex, serde_json::Value::String(status));
    }

    Ok(Json(serde_json::Value::Object(result)))
}
