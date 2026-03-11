//! NIP-05 identity verification endpoint.

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::HeaderValue,
    response::{IntoResponse, Json, Response},
};
use nostr::util::hex as nostr_hex;
use serde::Deserialize;

use crate::state::AppState;

/// Query parameters for the NIP-05 identity verification endpoint.
#[derive(Deserialize)]
pub struct Nip05Query {
    /// The local part of the NIP-05 identifier to look up (e.g. `alice` from `alice@relay.example`).
    pub name: Option<String>,
}

/// `GET /.well-known/nostr.json` — NIP-05 identity verification.
/// No authentication required — public discovery endpoint.
pub async fn nostr_nip05(
    State(state): State<Arc<AppState>>,
    Query(params): Query<Nip05Query>,
) -> Response {
    let json = match params.name {
        None => serde_json::json!({ "names": {}, "relays": {} }),
        Some(n) => {
            let name = n.to_lowercase();
            // Extract domain from relay_url (e.g. "ws://sprout.block.xyz" → "sprout.block.xyz")
            let domain = extract_domain(&state.config.relay_url);
            match state.db.get_user_by_nip05(&name, &domain).await {
                Ok(Some(user)) => {
                    let hex_pubkey = nostr_hex::encode(&user.pubkey);
                    let relay_url = state.config.relay_url.clone();
                    serde_json::json!({
                        "names": { (name): hex_pubkey.clone() },
                        "relays": { (hex_pubkey): [relay_url] }
                    })
                }
                _ => serde_json::json!({ "names": {}, "relays": {} }),
            }
        }
    };

    let mut response = Json(json).into_response();
    response.headers_mut().insert(
        axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    response
}

/// Extract the domain (host) from a URL string.
/// e.g. "ws://localhost:3000" → "localhost", "wss://sprout.block.xyz" → "sprout.block.xyz"
pub(crate) fn extract_domain(url: &str) -> String {
    url.trim_start_matches("wss://")
        .trim_start_matches("ws://")
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split(':')
        .next()
        .unwrap_or("localhost")
        .split('/')
        .next()
        .unwrap_or("localhost")
        .to_lowercase()
}
