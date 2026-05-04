//! Nostr HTTP bridge — POST /events, /query, /count with NIP-98 auth.
//!
//! These endpoints provide HTTP access to the relay's Nostr protocol,
//! authenticated via NIP-98 signed events.

use std::sync::Arc;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Json,
};
use base64::Engine;
use serde_json::Value;

use crate::handlers::ingest::{IngestAuth, IngestError};
use crate::state::AppState;

use super::{api_error, internal_error};

// ── NIP-98 verification ──────────────────────────────────────────────────────

/// Verify bridge auth: NIP-98 (production) or X-Pubkey (dev mode).
///
/// Returns the authenticated public key and an event ID for replay detection.
/// For X-Pubkey dev mode, the event ID is a zero hash (no replay concern).
fn verify_bridge_auth(
    headers: &HeaderMap,
    method: &str,
    url: &str,
    body: Option<&[u8]>,
    require_auth_token: bool,
) -> Result<(nostr::PublicKey, [u8; 32]), (StatusCode, Json<Value>)> {
    // Try NIP-98 first (Authorization: Nostr <base64>)
    if let Some(auth_str) = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Nostr "))
    {
        let event_json = {
            use base64::engine::general_purpose::STANDARD as BASE64;
            let bytes = BASE64
                .decode(auth_str)
                .map_err(|_| api_error(StatusCode::UNAUTHORIZED, "invalid base64 in Nostr auth"))?;
            String::from_utf8(bytes)
                .map_err(|_| api_error(StatusCode::UNAUTHORIZED, "invalid UTF-8 in Nostr auth"))?
        };

        let event: nostr::Event = serde_json::from_str(&event_json)
            .map_err(|_| api_error(StatusCode::UNAUTHORIZED, "invalid NIP-98 event JSON"))?;
        let event_id_bytes = event.id.to_bytes();

        let pubkey = sprout_auth::verify_nip98_event(&event_json, url, method, body)
            .map_err(|e| api_error(StatusCode::UNAUTHORIZED, &format!("NIP-98: {e}")))?;

        return Ok((pubkey, event_id_bytes));
    }

    // Dev-mode fallback: X-Pubkey header (only when require_auth_token is false)
    if !require_auth_token {
        if let Some(hex_val) = headers.get("x-pubkey").and_then(|v| v.to_str().ok()) {
            let pubkey = nostr::PublicKey::from_hex(hex_val)
                .map_err(|_| api_error(StatusCode::UNAUTHORIZED, "invalid X-Pubkey hex"))?;
            // Zero event ID — no replay detection needed for dev mode
            return Ok((pubkey, [0u8; 32]));
        }
    }

    Err(api_error(StatusCode::UNAUTHORIZED, "missing Nostr auth"))
}

/// Check NIP-98 replay and record the event ID.
fn check_nip98_replay(
    state: &AppState,
    event_id_bytes: [u8; 32],
) -> Result<(), (StatusCode, Json<Value>)> {
    // Skip replay detection for dev-mode X-Pubkey auth (zero hash).
    if event_id_bytes == [0u8; 32] {
        return Ok(());
    }
    if state.nip98_seen.get(&event_id_bytes).is_some() {
        return Err(api_error(
            StatusCode::UNAUTHORIZED,
            "NIP-98: replay detected",
        ));
    }
    state.nip98_seen.insert(event_id_bytes, ());
    Ok(())
}

/// Reconstruct the canonical URL for NIP-98 verification from the relay config.
fn canonical_url(relay_url: &str, path: &str) -> String {
    let base = relay_url
        .trim()
        .trim_end_matches('/')
        .replace("wss://", "https://")
        .replace("ws://", "http://");
    format!("{base}{path}")
}

// ── Channel access helpers ───────────────────────────────────────────────────

/// Extract a channel UUID from a single filter's `#h` tag.
fn extract_channel_from_filter(filter: &nostr::Filter) -> Option<uuid::Uuid> {
    let h_tag = nostr::SingleLetterTag::lowercase(nostr::Alphabet::H);
    filter.generic_tags.get(&h_tag).and_then(|vs| {
        if vs.len() == 1 {
            vs.iter().next()?.parse::<uuid::Uuid>().ok()
        } else {
            None
        }
    })
}

// ── POST /events ─────────────────────────────────────────────────────────────

/// Submit a signed Nostr event via HTTP bridge (NIP-98 auth).
pub async fn submit_event(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let url = canonical_url(&state.config.relay_url, "/events");
    let (pubkey, event_id_bytes) = verify_bridge_auth(
        &headers,
        "POST",
        &url,
        Some(&body),
        state.config.require_auth_token,
    )?;
    check_nip98_replay(&state, event_id_bytes)?;
    let pubkey_bytes = pubkey.serialize().to_vec();

    // Enforce relay membership
    super::relay_members::enforce_relay_membership(&state, &pubkey_bytes).await?;

    let event: nostr::Event = serde_json::from_slice(&body)
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, &format!("invalid event JSON: {e}")))?;

    let auth = IngestAuth::Http {
        pubkey,
        scopes: sprout_auth::Scope::all_known(), // Pure Nostr: full scopes, channel access via membership
        auth_method: crate::handlers::ingest::HttpAuthMethod::Nip98,
    };

    match crate::handlers::ingest::ingest_event(&state, event, auth).await {
        Ok(result) => Ok(Json(serde_json::json!({
            "event_id": result.event_id,
            "accepted": result.accepted,
            "message": result.message,
        }))),
        Err(e) => match e {
            IngestError::Rejected(msg) => Err(api_error(StatusCode::BAD_REQUEST, &msg)),
            IngestError::AuthFailed(msg) => Err(api_error(StatusCode::FORBIDDEN, &msg)),
            IngestError::Internal(msg) => Err(internal_error(&msg)),
        },
    }
}

// ── POST /query ──────────────────────────────────────────────────────────────

/// Query events via HTTP bridge (NIP-98 auth). Returns JSON array of events.
///
/// Enforces channel access: results are filtered to channels the user can access.
pub async fn query_events(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let url = canonical_url(&state.config.relay_url, "/query");
    let (pubkey, event_id_bytes) = verify_bridge_auth(
        &headers,
        "POST",
        &url,
        Some(&body),
        state.config.require_auth_token,
    )?;
    check_nip98_replay(&state, event_id_bytes)?;
    let pubkey_bytes = pubkey.serialize().to_vec();

    super::relay_members::enforce_relay_membership(&state, &pubkey_bytes).await?;

    let filters: Vec<nostr::Filter> = serde_json::from_slice(&body)
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, &format!("invalid filters: {e}")))?;

    // P-gated kinds (gift wraps, member notifications, observer frames) require
    // the caller's own pubkey in the #p tag — same enforcement as WS REQ handler.
    let authed_pubkey_hex = pubkey.to_hex();
    if !crate::handlers::req::p_gated_filters_authorized(&filters, &authed_pubkey_hex) {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            "restricted: p-gated kinds require #p tag matching your pubkey",
        ));
    }

    // Get channels this user can access — same enforcement as WS REQ handler.
    let accessible_channels = state
        .get_accessible_channel_ids_cached(&pubkey_bytes)
        .await
        .map_err(|e| internal_error(&format!("channel access lookup: {e}")))?;

    // ── NIP-50 search: route to Typesense if any filter has a `search` field ──
    if filters.iter().any(|f| f.search.is_some()) {
        return handle_bridge_search(&state, &filters, &accessible_channels).await;
    }

    // Execute each filter and collect results, enforcing channel access.
    let mut events: Vec<Value> = Vec::new();
    for filter in &filters {
        // If filter targets a specific channel, verify access.
        if let Some(ch_id) = extract_channel_from_filter(filter) {
            if !accessible_channels.contains(&ch_id) {
                continue; // Skip filters targeting inaccessible channels.
            }
        }

        let query =
            crate::handlers::req::build_event_query_from_filter(filter, &pubkey_bytes, &state)
                .await;
        match state.db.query_events(&query).await {
            Ok(stored_events) => {
                for se in stored_events {
                    // Post-filter: only return events from accessible channels.
                    if let Some(ch_id) = se.channel_id {
                        if !accessible_channels.contains(&ch_id) {
                            continue;
                        }
                    }
                    // Post-filter: verify event matches the full filter (generic tags, etc.).
                    // The DB query may not push down all constraints (e.g. #e, #a tags).
                    if !sprout_core::filter::filters_match(std::slice::from_ref(filter), &se) {
                        continue;
                    }
                    if let Ok(v) = serde_json::to_value(&se.event) {
                        events.push(v);
                    }
                }
            }
            Err(e) => {
                return Err(internal_error(&format!("query error: {e}")));
            }
        }
    }

    Ok(Json(Value::Array(events)))
}

// ── POST /count ──────────────────────────────────────────────────────────────

/// Count events via HTTP bridge (NIP-98 auth). Returns `{"count": N}`.
///
/// Enforces channel access: only counts events in channels the user can access.
/// For filters without a `#h` tag, falls back to per-event counting with access checks.
pub async fn count_events(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let url = canonical_url(&state.config.relay_url, "/count");
    let (pubkey, event_id_bytes) = verify_bridge_auth(
        &headers,
        "POST",
        &url,
        Some(&body),
        state.config.require_auth_token,
    )?;
    check_nip98_replay(&state, event_id_bytes)?;
    let pubkey_bytes = pubkey.serialize().to_vec();

    super::relay_members::enforce_relay_membership(&state, &pubkey_bytes).await?;

    let filters: Vec<nostr::Filter> = serde_json::from_slice(&body)
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, &format!("invalid filters: {e}")))?;

    // P-gated kinds enforcement — same as WS REQ and /query.
    let authed_pubkey_hex = pubkey.to_hex();
    if !crate::handlers::req::p_gated_filters_authorized(&filters, &authed_pubkey_hex) {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            "restricted: p-gated kinds require #p tag matching your pubkey",
        ));
    }

    // Get channels this user can access.
    let accessible_channels = state
        .get_accessible_channel_ids_cached(&pubkey_bytes)
        .await
        .map_err(|e| internal_error(&format!("channel access lookup: {e}")))?;

    let mut total: u64 = 0;
    for filter in &filters {
        // If filter targets a specific channel, verify access.
        if let Some(ch_id) = extract_channel_from_filter(filter) {
            if !accessible_channels.contains(&ch_id) {
                continue; // Skip filters targeting inaccessible channels.
            }
            // Channel is accessible — safe to count directly via DB.
            let query =
                crate::handlers::req::build_event_query_from_filter(filter, &pubkey_bytes, &state)
                    .await;
            match state.db.count_events(&query).await {
                Ok(n) => total += n as u64,
                Err(e) => {
                    return Err(internal_error(&format!("count error: {e}")));
                }
            }
        } else {
            // No channel filter — must count only accessible events.
            // Fall back to query + post-filter since count_events can't
            // restrict to a set of channels.
            let query =
                crate::handlers::req::build_event_query_from_filter(filter, &pubkey_bytes, &state)
                    .await;
            match state.db.query_events(&query).await {
                Ok(stored_events) => {
                    for se in stored_events {
                        match se.channel_id {
                            Some(ch_id) if !accessible_channels.contains(&ch_id) => continue,
                            _ => {}
                        }
                        // Post-filter: verify event matches the full filter (generic tags, etc.).
                        if !sprout_core::filter::filters_match(std::slice::from_ref(filter), &se) {
                            continue;
                        }
                        total += 1;
                    }
                }
                Err(e) => {
                    return Err(internal_error(&format!("count error: {e}")));
                }
            }
        }
    }

    Ok(Json(serde_json::json!({ "count": total })))
}

// ── NIP-50 search via HTTP bridge ────────────────────────────────────────────

/// Handle search filters by routing to Typesense, then fetching full events from DB.
/// Returns first page of results (no pagination for bridge MVP).
async fn handle_bridge_search(
    state: &AppState,
    filters: &[nostr::Filter],
    accessible_channels: &[uuid::Uuid],
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Bridge always includes global (non-channel) events — same as WS with full scopes.
    let channel_scope = match crate::handlers::req::build_search_channel_scope_filter(
        accessible_channels,
        true, // include_global
    ) {
        Some(f) => f,
        None => return Ok(Json(Value::Array(Vec::new()))),
    };

    let mut events: Vec<Value> = Vec::new();
    let mut seen_ids: std::collections::HashSet<[u8; 32]> = std::collections::HashSet::new();

    for filter in filters {
        let search_text = match &filter.search {
            Some(s) if !s.is_empty() => s.clone(),
            _ => continue,
        };

        let limit = filter.limit.unwrap_or(100).min(500) as u32;
        if limit == 0 {
            continue;
        }

        // Build Typesense filter — push channel scope + NIP-01 constraints.
        let h_tag = nostr::SingleLetterTag::lowercase(nostr::Alphabet::H);
        let filter_channel_scope =
            if let Some(vs) = filter.generic_tags.get(&h_tag).filter(|vs| !vs.is_empty()) {
                let valid: Vec<String> = vs
                    .iter()
                    .filter_map(|v| v.parse::<uuid::Uuid>().ok())
                    .filter(|id| accessible_channels.contains(id))
                    .map(|id| id.to_string())
                    .collect();
                if valid.is_empty() {
                    continue; // All #h values inaccessible — skip filter.
                }
                format!("channel_id:=[{}]", valid.join(","))
            } else {
                channel_scope.clone()
            };

        let mut filter_parts = vec![filter_channel_scope];
        if let Some(ref kinds) = filter.kinds {
            if !kinds.is_empty() {
                let kind_vals: Vec<String> = kinds.iter().map(|k| k.as_u16().to_string()).collect();
                filter_parts.push(format!("kind:=[{}]", kind_vals.join(",")));
            }
        }
        if let Some(ref authors) = filter.authors {
            if !authors.is_empty() {
                let author_vals: Vec<String> = authors.iter().map(|a| a.to_hex()).collect();
                filter_parts.push(format!("pubkey:=[{}]", author_vals.join(",")));
            }
        }
        if let Some(since) = filter.since {
            filter_parts.push(format!("created_at:>={}", since.as_u64()));
        }
        if let Some(until) = filter.until {
            filter_parts.push(format!("created_at:<={}", until.as_u64()));
        }

        let filter_by = filter_parts.join(" && ");

        let search_query = sprout_search::SearchQuery {
            q: search_text,
            filter_by: Some(filter_by),
            sort_by: None, // Typesense default = relevance
            page: 1,
            per_page: limit,
        };

        let search_result = state
            .search
            .search(&search_query)
            .await
            .map_err(|e| internal_error(&format!("search error: {e}")))?;

        // Fetch full events from DB by ID.
        let hit_ids: Vec<Vec<u8>> = search_result
            .hits
            .into_iter()
            .filter_map(|h| hex::decode(&h.event_id).ok())
            .filter(|bytes| bytes.len() == 32)
            .collect();

        if hit_ids.is_empty() {
            continue;
        }

        let id_refs: Vec<&[u8]> = hit_ids.iter().map(|b| b.as_slice()).collect();
        let stored_events = state
            .db
            .get_events_by_ids(&id_refs)
            .await
            .map_err(|e| internal_error(&format!("search fetch error: {e}")))?;

        // Build lookup map to preserve Typesense relevance ordering.
        let event_map: std::collections::HashMap<[u8; 32], &sprout_core::StoredEvent> =
            stored_events
                .iter()
                .map(|ev| (ev.event.id.to_bytes(), ev))
                .collect();

        for hit_id in &hit_ids {
            let id_array: [u8; 32] = match hit_id.as_slice().try_into() {
                Ok(a) => a,
                Err(_) => continue,
            };
            let stored = match event_map.get(&id_array) {
                Some(ev) => ev,
                None => continue,
            };
            // Channel access post-filter.
            if let Some(ch_id) = stored.channel_id {
                if !accessible_channels.contains(&ch_id) {
                    continue;
                }
            }
            // Dedup across filters.
            if !seen_ids.insert(id_array) {
                continue;
            }
            if let Ok(v) = serde_json::to_value(&stored.event) {
                events.push(v);
            }
        }
    }

    Ok(Json(Value::Array(events)))
}
