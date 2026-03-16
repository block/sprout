//! REQ handler — subscribe, deliver historical events, then EOSE.

use std::collections::HashSet;
use std::sync::Arc;

use tracing::{debug, warn};

use hex;
use nostr::Filter;
use sprout_core::filter::filters_match;
use sprout_core::kind::{
    KIND_GIFT_WRAP, KIND_MEMBER_ADDED_NOTIFICATION, KIND_MEMBER_REMOVED_NOTIFICATION,
};
use sprout_db::EventQuery;

use sprout_auth::Scope;

use crate::connection::{AuthState, ConnectionState};
use crate::protocol::RelayMessage;
use crate::state::AppState;

const MAX_HISTORICAL_LIMIT: i64 = 500;
const MAX_SUBSCRIPTIONS: usize = 1024;

/// Handle a REQ message: register the subscription, deliver historical events, then send EOSE.
pub async fn handle_req(
    sub_id: String,
    filters: Vec<Filter>,
    conn: Arc<ConnectionState>,
    state: Arc<AppState>,
) {
    let (conn_id, pubkey_bytes) = {
        let auth = conn.auth_state.read().await;
        match &*auth {
            AuthState::Authenticated(ctx) => {
                if !ctx.scopes.is_empty() && !ctx.scopes.contains(&Scope::MessagesRead) {
                    conn.send(RelayMessage::notice("restricted: insufficient scope"));
                    conn.send(RelayMessage::closed(
                        &sub_id,
                        "restricted: insufficient scope",
                    ));
                    return;
                }

                let pk_bytes = ctx.pubkey.serialize().to_vec();

                let subs = conn.subscriptions.lock().await;
                if !subs.contains_key(&sub_id) && subs.len() >= MAX_SUBSCRIPTIONS {
                    conn.send(RelayMessage::closed(
                        &sub_id,
                        "error: too many subscriptions",
                    ));
                    return;
                }

                (conn.conn_id, pk_bytes)
            }
            _ => {
                conn.send(RelayMessage::notice(
                    "auth-required: authenticate before subscribing",
                ));
                conn.send(RelayMessage::closed(
                    &sub_id,
                    "auth-required: not authenticated",
                ));
                return;
            }
        }
    };

    let accessible_channels = match state.db.get_accessible_channel_ids(&pubkey_bytes).await {
        Ok(ids) => ids,
        Err(e) => {
            warn!(conn_id = %conn_id, "Failed to get accessible channels: {e}");
            conn.send(RelayMessage::closed(&sub_id, "error: database error"));
            return;
        }
    };

    let channel_id = extract_channel_id_from_filters(&filters);

    // ── NIP-50 search: intercept BEFORE #p gating ────────────────────────────
    // Search filters are one-shot (not registered as persistent subscriptions).
    // They never deliver gift wraps (not indexed) or membership notifications
    // (global, no channel_id), so the #p gate below is irrelevant for them.
    // Intercepting here lets clients send `{"search":"foo"}` without `kinds`.
    let has_search = filters.iter().any(|f| f.search.is_some());
    if has_search {
        if filters.iter().any(|f| f.search.is_none()) {
            conn.send(RelayMessage::closed(
                &sub_id,
                "error: mixed search and non-search filters not supported",
            ));
            return;
        }
        handle_search_req(&sub_id, &filters, &accessible_channels, &conn, &state).await;
        return;
    }

    // ── #p gating for globally-stored sensitive kinds ─────────────────────────
    // Only applies to GLOBAL subscriptions (channel_id = None). Channel-scoped
    // subscriptions can never receive globally-stored events — the fan_out()
    // invariant in subscription.rs prevents it.
    const P_GATED_KINDS: [u32; 3] = [
        KIND_MEMBER_ADDED_NOTIFICATION,
        KIND_MEMBER_REMOVED_NOTIFICATION,
        KIND_GIFT_WRAP,
    ];

    if channel_id.is_none() {
        let authed_pubkey_hex = hex::encode(&pubkey_bytes);
        let p_tag = nostr::SingleLetterTag::lowercase(nostr::Alphabet::P);

        for filter in &filters {
            let can_match_p_gated = filter.kinds.as_ref().is_none_or(|ks| {
                ks.iter()
                    .any(|k| P_GATED_KINDS.contains(&(k.as_u16() as u32)))
            });
            if can_match_p_gated {
                let has_matching_p = filter.generic_tags.get(&p_tag).is_some_and(|values| {
                    !values.is_empty() && values.iter().all(|v| *v == authed_pubkey_hex)
                });
                if !has_matching_p {
                    conn.send(RelayMessage::closed(
                        &sub_id,
                        "restricted: p-gated events require #p matching your pubkey",
                    ));
                    return;
                }
            }
        }
    }

    // Check channel access BEFORE registering the subscription.
    if let Some(ch_id) = channel_id {
        if !accessible_channels.contains(&ch_id) {
            conn.send(RelayMessage::closed(
                &sub_id,
                "restricted: not a channel member",
            ));
            return;
        }
    }

    {
        let mut subs = conn.subscriptions.lock().await;
        subs.insert(sub_id.clone(), filters.clone());
    }

    state
        .sub_registry
        .register(conn_id, sub_id.clone(), filters.clone(), channel_id);

    debug!(conn_id = %conn_id, sub_id = %sub_id, "Subscription registered");

    // NIP-01 OR semantics: execute one DB query per filter and deduplicate results
    // by event ID. Collapsing all filters into a single query would merge their
    // time windows and limits, causing under-fetching when filters have different
    // per-filter limits or non-overlapping time windows.
    let mut seen_ids: HashSet<nostr::EventId> = HashSet::new();
    let mut total_sent: usize = 0;

    for filter in &filters {
        let params = filter_to_query_params(filter, channel_id);

        let filter_events = state.db.query_events(&params).await;

        let events = match filter_events {
            Ok(evs) => evs,
            Err(e) => {
                warn!(conn_id = %conn_id, sub_id = %sub_id, "Historical query failed: {e}");
                conn.send(RelayMessage::eose(&sub_id));
                return;
            }
        };

        for stored in &events {
            if !seen_ids.insert(stored.event.id) {
                continue;
            }

            // Apply full NIP-01 filter matching (handles fields not in the DB query).
            if !filters_match(&filters, stored) {
                continue;
            }

            if let Some(ch_id) = stored.channel_id {
                if !accessible_channels.contains(&ch_id) {
                    continue;
                }
            }

            let msg = RelayMessage::event(&sub_id, &stored.event);
            if !conn.send(msg) {
                return;
            }
            total_sent += 1;
        }
    }

    conn.send(RelayMessage::eose(&sub_id));

    debug!(
        conn_id = %conn_id,
        sub_id = %sub_id,
        count = total_sent,
        "EOSE sent after historical delivery"
    );
}

/// Handle a NIP-50 search REQ: query Typesense, fetch full events, deliver results, EOSE.
/// Search subscriptions are one-shot — no persistent subscription is registered.
/// Maximum Typesense pages to fetch per filter (prevents unbounded loops).
const MAX_SEARCH_PAGES: u32 = 10;

async fn handle_search_req(
    sub_id: &str,
    filters: &[Filter],
    accessible_channels: &[uuid::Uuid],
    conn: &ConnectionState,
    state: &AppState,
) {
    if accessible_channels.is_empty() {
        conn.send(RelayMessage::eose(sub_id));
        return;
    }

    let all_channels_filter = {
        let ids: Vec<String> = accessible_channels
            .iter()
            .map(|id| id.to_string())
            .collect();
        format!("channel_id:=[{}]", ids.join(","))
    };

    let mut seen_ids: HashSet<nostr::EventId> = HashSet::new();

    for filter in filters {
        let search_text = match &filter.search {
            Some(s) if !s.is_empty() => s.clone(),
            _ => continue,
        };

        let limit = filter
            .limit
            .map(|l| (l as u32).min(MAX_HISTORICAL_LIMIT as u32))
            .unwrap_or(MAX_HISTORICAL_LIMIT as u32);

        if limit == 0 {
            continue; // NIP-01: limit 0 means "no results from this filter"
        }

        // Push as many NIP-01 constraints into Typesense as possible so
        // post-filtering is a correction step, not the primary filter.
        //
        // If the filter has a #h tag, push the specific channel(s) into Typesense
        // instead of the full accessible set. This prevents cross-channel hits from
        // consuming pagination budget and causing under-fetch.
        let h_tag = nostr::SingleLetterTag::lowercase(nostr::Alphabet::H);
        let channel_scope = filter
            .generic_tags
            .get(&h_tag)
            .filter(|vs| !vs.is_empty())
            .map(|vs| {
                // Intersect with accessible channels — prevents inaccessible or
                // malformed #h values from consuming pagination budget.
                let valid: Vec<String> = vs
                    .iter()
                    .filter_map(|v| v.parse::<uuid::Uuid>().ok())
                    .filter(|id| accessible_channels.contains(id))
                    .map(|id| id.to_string())
                    .collect();
                if valid.is_empty() {
                    return all_channels_filter.clone();
                }
                format!("channel_id:=[{}]", valid.join(","))
            })
            .unwrap_or_else(|| all_channels_filter.clone());
        let mut filter_parts = vec![channel_scope];
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

        // Paginate: keep fetching pages until we've emitted `limit` results
        // or exhausted the search result set. This ensures post-filtering
        // doesn't silently reduce the result count below the requested limit.
        let mut emitted: u32 = 0;
        // Always fetch full pages (100) regardless of limit — post-filtering
        // may discard many hits, so we need headroom to fill the requested limit.
        let per_page: u32 = 100;

        for page in 1..=MAX_SEARCH_PAGES {
            if emitted >= limit {
                break;
            }

            let search_query = sprout_search::SearchQuery {
                q: search_text.clone(),
                filter_by: Some(filter_by.clone()),
                sort_by: None, // Typesense default = relevance (text_match score)
                page,
                per_page,
            };

            let search_result = match state.search.search(&search_query).await {
                Ok(r) => r,
                Err(e) => {
                    warn!(sub_id = %sub_id, "NIP-50 search failed: {e}");
                    break;
                }
            };

            let page_empty = search_result.hits.is_empty();
            let exhausted = (page as u64) * (per_page as u64) >= search_result.found;

            let hit_ids: Vec<Vec<u8>> = search_result
                .hits
                .into_iter()
                .filter(|h| h.channel_id.is_some())
                .filter_map(|h| hex::decode(&h.event_id).ok())
                .filter(|bytes| bytes.len() == 32)
                .collect();

            if !hit_ids.is_empty() {
                let id_refs: Vec<&[u8]> = hit_ids.iter().map(|b| b.as_slice()).collect();
                let events = match state.db.get_events_by_ids(&id_refs).await {
                    Ok(evs) => evs,
                    Err(e) => {
                        warn!(sub_id = %sub_id, "NIP-50 batch fetch failed: {e}");
                        break;
                    }
                };

                let event_map: std::collections::HashMap<[u8; 32], &sprout_core::StoredEvent> =
                    events
                        .iter()
                        .map(|ev| (ev.event.id.to_bytes(), ev))
                        .collect();

                for hit_id in &hit_ids {
                    if emitted >= limit {
                        break;
                    }
                    let id_array: [u8; 32] = match hit_id.as_slice().try_into() {
                        Ok(a) => a,
                        Err(_) => continue,
                    };
                    let stored = match event_map.get(&id_array) {
                        Some(ev) => ev,
                        None => continue,
                    };
                    // NIP-01 post-filtering against THIS filter only (not OR of all filters).
                    if !filters_match(std::slice::from_ref(filter), stored) {
                        continue;
                    }
                    if let Some(ch_id) = stored.channel_id {
                        if !accessible_channels.contains(&ch_id) {
                            continue;
                        }
                    }
                    // Dedup AFTER acceptance — an event that fails filter A's constraints
                    // must remain eligible for filter B (NIP-01 OR semantics).
                    if !seen_ids.insert(stored.event.id) {
                        continue;
                    }
                    if !conn.send(RelayMessage::event(sub_id, &stored.event)) {
                        return;
                    }
                    emitted += 1;
                }
            }

            if page_empty || exhausted {
                break;
            }
        }
    }

    conn.send(RelayMessage::eose(sub_id));
}

/// Convert a single NIP-01 filter into an [`EventQuery`] for the database.
///
/// Each filter is queried independently so that per-filter `limit` and time
/// windows are respected. Results are deduplicated by event ID in the caller.
fn filter_to_query_params(filter: &Filter, channel_id: Option<uuid::Uuid>) -> EventQuery {
    let kinds: Option<Vec<i32>> = filter.kinds.as_ref().map(|ks| {
        if ks.is_empty() {
            // kinds:[] means "match no kinds" — skip this filter entirely by
            // returning a sentinel that the DB query will produce zero rows for.
            // We use Some(vec![]) which the DB layer treats as "no matching kinds".
            vec![]
        } else {
            // Cast to i32 for MySQL INT column; safe because all Sprout kinds fit in i32.
            ks.iter().map(|k| k.as_u16() as i32).collect()
        }
    });

    let since = filter
        .since
        .and_then(|s| chrono::DateTime::from_timestamp(s.as_u64() as i64, 0));
    let until = filter
        .until
        .and_then(|u| chrono::DateTime::from_timestamp(u.as_u64() as i64, 0));
    let limit = filter
        .limit
        .map(|l| (l as i64).min(MAX_HISTORICAL_LIMIT))
        .unwrap_or(MAX_HISTORICAL_LIMIT);

    // Push single-author filter into SQL (EventQuery.pubkey is Option<Vec<u8>>).
    // Multi-author filters fall through to in-memory filters_match post-filtering.
    let pubkey = filter.authors.as_ref().and_then(|authors| {
        if authors.len() == 1 {
            authors.iter().next().map(|pk| pk.serialize().to_vec())
        } else {
            None
        }
    });

    EventQuery {
        channel_id,
        kinds,
        pubkey,
        since,
        until,
        limit: Some(limit),
        ..Default::default()
    }
}

/// Extract a single channel UUID from filter generic tags, or `None` if the
/// subscription is logically global.
///
/// Checks the `"h"` tag key — channel-scoped subscriptions use `#h = <uuid>`.
///
/// Returns `None` when:
/// - Any filter has no channel tag (that filter matches all channels → global sub), or
/// - Multiple distinct channel UUIDs appear across filters (can't index under one channel).
///
/// Callers that receive `None` treat the subscription as global (slow-path fan-out).
fn extract_channel_id_from_filters(filters: &[Filter]) -> Option<uuid::Uuid> {
    let mut found_id: Option<uuid::Uuid> = None;
    for f in filters {
        let mut filter_has_channel = false;
        for (tag_key, tag_values) in f.generic_tags.iter() {
            let key = tag_key.to_string();
            if key == "h" {
                for val in tag_values {
                    if let Ok(id) = val.parse::<uuid::Uuid>() {
                        filter_has_channel = true;
                        match found_id {
                            Some(existing) if existing != id => {
                                // Multiple distinct channel IDs — fall back to global.
                                return None;
                            }
                            _ => found_id = Some(id),
                        }
                    }
                }
            }
        }
        if !filter_has_channel {
            // This filter has no channel constraint — the subscription is global.
            return None;
        }
    }
    found_id
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{Alphabet, Filter, SingleLetterTag};

    fn filter_with_channel(channel_id: uuid::Uuid) -> Filter {
        Filter::new().custom_tag(
            SingleLetterTag::lowercase(Alphabet::H),
            [channel_id.to_string()],
        )
    }

    #[test]
    fn test_extract_channel_id_single_channel() {
        let channel_id = uuid::Uuid::new_v4();
        let filters = vec![filter_with_channel(channel_id)];
        assert_eq!(extract_channel_id_from_filters(&filters), Some(channel_id));
    }

    #[test]
    fn test_extract_channel_id_mixed_channels_returns_none() {
        let channel_a = uuid::Uuid::new_v4();
        let channel_b = uuid::Uuid::new_v4();
        let filters = vec![
            filter_with_channel(channel_a),
            filter_with_channel(channel_b),
        ];
        assert_eq!(extract_channel_id_from_filters(&filters), None);
    }

    #[test]
    fn test_extract_channel_id_no_channel_tag_returns_none() {
        let filters = vec![Filter::new()];
        assert_eq!(extract_channel_id_from_filters(&filters), None);
    }

    #[test]
    fn test_extract_channel_id_one_filter_missing_channel_returns_none() {
        // Even if one filter has a channel, a second filter without one makes it global.
        let channel_id = uuid::Uuid::new_v4();
        let filters = vec![filter_with_channel(channel_id), Filter::new()];
        assert_eq!(extract_channel_id_from_filters(&filters), None);
    }

    #[test]
    fn test_extract_channel_id_same_channel_multiple_filters() {
        let channel_id = uuid::Uuid::new_v4();
        let filters = vec![
            filter_with_channel(channel_id),
            filter_with_channel(channel_id),
        ];
        assert_eq!(extract_channel_id_from_filters(&filters), Some(channel_id));
    }

    #[test]
    fn test_search_filter_detection() {
        let search_filter = Filter::new().search("hello world");
        let filters = vec![search_filter];
        assert!(filters.iter().any(|f| f.search.is_some()));
    }

    #[test]
    fn test_mixed_search_and_non_search_detection() {
        let search_filter = Filter::new().search("hello");
        let plain_filter = Filter::new();
        let filters = vec![search_filter, plain_filter];
        let has_search = filters.iter().any(|f| f.search.is_some());
        let has_non_search = filters.iter().any(|f| f.search.is_none());
        assert!(has_search && has_non_search, "should detect mixed filters");
    }

    #[test]
    fn test_all_search_filters_not_mixed() {
        let f1 = Filter::new().search("hello");
        let f2 = Filter::new().search("world");
        let filters = vec![f1, f2];
        let has_search = filters.iter().any(|f| f.search.is_some());
        let has_non_search = filters.iter().any(|f| f.search.is_none());
        assert!(has_search);
        assert!(!has_non_search, "all-search filters should not be mixed");
    }
}
