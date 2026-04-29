//! Presence API — GET /api/presence (bulk lookup) and PUT /api/presence (set status).

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use nostr::{EventBuilder, Kind};
use serde::Deserialize;
use sprout_core::event::StoredEvent;
use sprout_core::kind::KIND_PRESENCE_UPDATE;
use sprout_core::PresenceStatus;
use sprout_pubsub::presence::PRESENCE_TTL_SECS;

use crate::state::AppState;

use super::extract_auth_context;

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
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::ChannelsRead)
        .map_err(super::scope_error)?;

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
        .map_err(|e| {
            tracing::error!("presence bulk lookup failed: {e}");
            super::internal_error(&format!("presence store error: {e}"))
        })?;

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

/// Request body for `PUT /api/presence`.
#[derive(Debug, Deserialize)]
pub struct SetPresenceBody {
    /// Presence status to set.
    pub status: PresenceStatus,
}

/// Set the authenticated user's presence status.
///
/// Accepts `{"status": "online" | "away" | "offline"}` (case-sensitive).
/// Serde rejects unknown variants automatically, returning a 422.
/// - `"offline"` clears the presence entry (TTL 0).
/// - `"online"` / `"away"` upsert the entry with a 90-second TTL.
///
/// Returns `{"status": "...", "ttl_seconds": N}`.
///
/// **Note:** The WebSocket path (kind:20001) accepts arbitrary status strings
/// for forward-compatibility, but the REST/MCP surface intentionally restricts
/// to the curated enum above. Aligning the WebSocket path is tracked separately.
pub async fn set_presence_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    super::ApiJson(body): super::ApiJson<SetPresenceBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::ChannelsRead)
        .map_err(super::scope_error)?;
    let pubkey = ctx.pubkey;

    let status_str = body.status.as_str().to_string();

    match body.status {
        PresenceStatus::Online | PresenceStatus::Away => {
            state
                .pubsub
                .set_presence(&pubkey, &status_str)
                .await
                .map_err(|e| super::internal_error(&format!("presence error: {e}")))?;
        }
        PresenceStatus::Offline => {
            state
                .pubsub
                .clear_presence(&pubkey)
                .await
                .map_err(|e| super::internal_error(&format!("presence error: {e}")))?;
        }
    }

    // Fan out a synthetic kind:20001 event to WebSocket subscribers so clients
    // that subscribe to presence updates receive REST-originated changes too.
    //
    // We build the event with the authenticated user's pubkey as author (so
    // `authors` filters match correctly), then sign it with the relay keypair.
    // The signature is relay-issued rather than user-issued — this is an
    // internal synthetic event that is never persisted or forwarded externally,
    // and the relay does not re-verify signatures during fan-out.
    match EventBuilder::new(Kind::Custom(KIND_PRESENCE_UPDATE as u16), &status_str, [])
        .build(pubkey)
        .sign_with_keys(&state.relay_keypair)
    {
        Ok(event) => {
            let stored = StoredEvent::new(event.clone(), None);
            let matches = state.sub_registry.fan_out(&stored);
            metrics::histogram!("sprout_fanout_recipients").record(matches.len() as f64);
            let event_json = serde_json::to_string(&event)
                .expect("nostr::Event serialization is infallible for well-formed events");
            let mut drop_count = 0u32;
            for (target_conn_id, sub_id) in &matches {
                let msg = format!(r#"["EVENT","{}",{}]"#, sub_id, event_json);
                if !state.conn_manager.send_to(*target_conn_id, msg) {
                    drop_count += 1;
                }
            }
            if drop_count > 0 {
                tracing::warn!(
                    pubkey = %pubkey.to_hex(),
                    drop_count,
                    "presence fan-out: {drop_count} connection(s) cancelled due to full/closed buffers"
                );
            }
        }
        Err(e) => {
            // Non-fatal: presence was written to Redis; fan-out failure is logged but
            // does not roll back the write or fail the request.
            tracing::warn!(pubkey = %pubkey.to_hex(), "presence fan-out: failed to build synthetic event: {e}");
        }
    }

    Ok(Json(serde_json::json!({
        "status": body.status,
        "ttl_seconds": if body.status == PresenceStatus::Offline { 0 } else { PRESENCE_TTL_SECS },
    })))
}
