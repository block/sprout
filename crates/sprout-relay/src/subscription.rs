//! Subscription registry with (channel, kind) index for O(1) fan-out.

use std::collections::HashMap;

use dashmap::DashMap;
use nostr::{Filter, Kind};
use uuid::Uuid;

use sprout_core::{filter::filters_match, StoredEvent};

/// Connection identifier — a UUID assigned to each WebSocket connection.
pub type ConnId = Uuid;
/// Subscription identifier — the client-supplied string from a REQ message.
pub type SubId = String;
/// Stored subscription entry: filters paired with an optional channel scope.
pub type SubEntry = (Vec<Filter>, Option<Uuid>);

/// Index key combining a channel and event kind for O(1) fan-out lookups.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IndexKey {
    /// The channel this key is scoped to.
    pub channel_id: Uuid,
    /// The Nostr event kind this key is scoped to.
    pub kind: Kind,
}

/// Thread-safe registry of active subscriptions with a (channel, kind) index for O(1) fan-out.
#[derive(Debug, Default)]
pub struct SubscriptionRegistry {
    /// Maps conn_id → sub_id → (filters, channel_id).
    /// Storing channel_id alongside filters enables O(1) targeted index removal.
    subs: DashMap<ConnId, HashMap<SubId, SubEntry>>,
    channel_kind_index: DashMap<IndexKey, Vec<(ConnId, SubId)>>,
    /// Subscriptions with a channel_id but no kind filter — need to receive ALL kinds.
    channel_wildcard_index: DashMap<Uuid, Vec<(ConnId, SubId)>>,
}

impl SubscriptionRegistry {
    /// Creates a new empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces any existing subscription with the same sub_id (NIP-01).
    pub fn register(
        &self,
        conn_id: ConnId,
        sub_id: SubId,
        filters: Vec<Filter>,
        channel_id: Option<Uuid>,
    ) {
        self.remove_subscription(conn_id, &sub_id);

        self.subs
            .entry(conn_id)
            .or_default()
            .insert(sub_id.clone(), (filters.clone(), channel_id));

        if let Some(ch_id) = channel_id {
            match extract_kinds_from_filters(&filters) {
                None => {
                    // At least one filter has no `kinds` constraint — wildcard,
                    // this sub wants all kinds in this channel.
                    self.channel_wildcard_index
                        .entry(ch_id)
                        .or_default()
                        .push((conn_id, sub_id.clone()));
                }
                Some(kinds) if kinds.is_empty() => {
                    // All filters had explicit empty kinds lists (`kinds: []`).
                    // Per NIP-01, `kinds: []` means "match no kinds" — this
                    // subscription will never receive any events. Do not index it
                    // anywhere; `filters_match` will reject all events at fan-out.
                }
                Some(kinds) => {
                    for kind in kinds {
                        let key = IndexKey {
                            channel_id: ch_id,
                            kind,
                        };
                        self.channel_kind_index
                            .entry(key)
                            .or_default()
                            .push((conn_id, sub_id.clone()));
                    }
                }
            }
        }
    }

    /// Remove a single subscription and clean up its index entries.
    pub fn remove_subscription(&self, conn_id: ConnId, sub_id: &str) {
        if let Some(mut conn_subs) = self.subs.get_mut(&conn_id) {
            if let Some((filters, channel_id)) = conn_subs.remove(sub_id) {
                self.remove_from_index(conn_id, sub_id, &filters, channel_id);
            }
        }
    }

    /// Remove all subscriptions for a connection and clean up index entries.
    pub fn remove_connection(&self, conn_id: ConnId) {
        if let Some((_, conn_subs)) = self.subs.remove(&conn_id) {
            for (sub_id, (filters, channel_id)) in &conn_subs {
                self.remove_from_index(conn_id, sub_id, filters, *channel_id);
            }
        }
    }

    /// Return all (conn_id, sub_id) pairs whose filters match the given event.
    pub fn fan_out(&self, event: &StoredEvent) -> Vec<(ConnId, SubId)> {
        let mut results = Vec::new();

        if let Some(channel_id) = event.channel_id {
            let key = IndexKey {
                channel_id,
                kind: event.event.kind,
            };
            if let Some(candidates) = self.channel_kind_index.get(&key) {
                for (conn_id, sub_id) in candidates.iter() {
                    if let Some(conn_subs) = self.subs.get(conn_id) {
                        if let Some((filters, _)) = conn_subs.get(sub_id.as_str()) {
                            if filters_match(filters, event) {
                                results.push((*conn_id, sub_id.clone()));
                            }
                        }
                    }
                }
            }
            // Also check wildcard (channel-only, kindless) index
            if let Some(wildcards) = self.channel_wildcard_index.get(&channel_id) {
                for (conn_id, sub_id) in wildcards.iter() {
                    if let Some(conn_subs) = self.subs.get(conn_id) {
                        if let Some((filters, _)) = conn_subs.get(sub_id.as_str()) {
                            if filters_match(filters, event) {
                                results.push((*conn_id, sub_id.clone()));
                            }
                        }
                    }
                }
            }
        } else {
            for conn_entry in self.subs.iter() {
                let conn_id = *conn_entry.key();
                for (sub_id, (filters, _)) in conn_entry.value().iter() {
                    if filters_match(filters, event) {
                        results.push((conn_id, sub_id.clone()));
                    }
                }
            }
        }

        // NOTE: Global subscriptions (channel_id = None) intentionally do NOT
        // receive channel-scoped events. Delivering channel events to global subs
        // would bypass the channel membership check performed in req.rs, leaking
        // private channel content to unauthorized subscribers. Clients must
        // subscribe to a specific channel to receive its events — that path goes
        // through the access-control check that verifies membership.

        results
    }

    /// Return the filters for a specific subscription, or `None` if not found.
    pub fn get_filters(&self, conn_id: ConnId, sub_id: &str) -> Option<Vec<Filter>> {
        self.subs
            .get(&conn_id)
            .and_then(|conn_subs| conn_subs.get(sub_id).map(|(filters, _)| filters.clone()))
    }

    /// Return the total number of active subscriptions across all connections.
    pub fn total_subscriptions(&self) -> usize {
        self.subs.iter().map(|e| e.value().len()).sum()
    }

    /// Return the total number of connections with at least one active subscription.
    pub fn total_connections(&self) -> usize {
        self.subs.len()
    }

    /// Removes a subscription from the channel_kind_index (or channel_wildcard_index) using
    /// targeted O(k) lookup where k = number of kinds in the filters, instead of O(n) full-scan.
    ///
    /// If `channel_id` is None the subscription was never indexed (slow-path), so there
    /// is nothing to remove.
    fn remove_from_index(
        &self,
        conn_id: ConnId,
        sub_id: &str,
        filters: &[Filter],
        channel_id: Option<Uuid>,
    ) {
        if let Some(ch_id) = channel_id {
            match extract_kinds_from_filters(filters) {
                // None = wildcard (at least one filter had no kinds constraint)
                None => {
                    // Was in wildcard index
                    if let Some(mut entries) = self.channel_wildcard_index.get_mut(&ch_id) {
                        entries.retain(|(cid, sid)| !(*cid == conn_id && sid == sub_id));
                        if entries.is_empty() {
                            drop(entries);
                            self.channel_wildcard_index.remove(&ch_id);
                        }
                    }
                }
                Some(kinds) if kinds.is_empty() => {
                    // `kinds: []` subscriptions are never indexed (they match nothing),
                    // so there is nothing to remove here.
                }
                Some(kinds) => {
                    // Was in kind-specific index
                    for kind in kinds {
                        let key = IndexKey {
                            channel_id: ch_id,
                            kind,
                        };
                        if let Some(mut entries) = self.channel_kind_index.get_mut(&key) {
                            entries.retain(|(cid, sid)| !(*cid == conn_id && sid == sub_id));
                            if entries.is_empty() {
                                drop(entries);
                                self.channel_kind_index.remove(&key);
                            }
                        }
                    }
                }
            }
        }
        // If no channel_id, there's nothing in the index to remove (slow-path subs aren't indexed)
    }
}

/// Returns the union of all `kinds` across filters, or `None` if any filter
/// lacks a `kinds` array (meaning that filter matches all kinds — wildcard).
///
/// NIP-01 OR semantics: a subscription with multiple filters is satisfied when
/// *any* filter matches. If one filter has no `kinds` constraint it matches
/// every kind, making the whole subscription a wildcard regardless of the other
/// filters.
fn extract_kinds_from_filters(filters: &[Filter]) -> Option<Vec<Kind>> {
    let mut seen = std::collections::HashSet::new();
    let mut kinds = Vec::new();
    for f in filters {
        match &f.kinds {
            Some(filter_kinds) => {
                for k in filter_kinds {
                    if seen.insert(*k) {
                        kinds.push(*k);
                    }
                }
            }
            None => {
                // At least one filter has no kind constraint — the whole
                // subscription is a wildcard.
                return None;
            }
        }
    }
    Some(kinds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use nostr::{EventBuilder, Keys, Kind};
    use sprout_core::StoredEvent;

    fn make_stored_event(kind: Kind, channel_id: Option<Uuid>) -> StoredEvent {
        let keys = Keys::generate();
        let event = EventBuilder::new(kind, "test", [])
            .sign_with_keys(&keys)
            .expect("sign");
        StoredEvent::with_received_at(event, Utc::now(), channel_id, true)
    }

    #[test]
    fn test_subscription_registry_register_and_fan_out() {
        let registry = SubscriptionRegistry::new();
        let conn_id = Uuid::new_v4();
        let channel_id = Uuid::new_v4();
        let sub_id = "sub1".to_string();

        let filters = vec![Filter::new().kind(Kind::TextNote)];
        registry.register(conn_id, sub_id.clone(), filters, Some(channel_id));

        let event = make_stored_event(Kind::TextNote, Some(channel_id));
        let matches = registry.fan_out(&event);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].0, conn_id);
        assert_eq!(matches[0].1, sub_id);
    }

    #[test]
    fn test_subscription_registry_remove() {
        let registry = SubscriptionRegistry::new();
        let conn_id = Uuid::new_v4();
        let channel_id = Uuid::new_v4();
        let sub_id = "sub1".to_string();

        let filters = vec![Filter::new().kind(Kind::TextNote)];
        registry.register(conn_id, sub_id.clone(), filters, Some(channel_id));

        registry.remove_subscription(conn_id, &sub_id);

        let event = make_stored_event(Kind::TextNote, Some(channel_id));
        let matches = registry.fan_out(&event);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_subscription_registry_remove_connection() {
        let registry = SubscriptionRegistry::new();
        let conn_id = Uuid::new_v4();
        let channel_id = Uuid::new_v4();

        registry.register(
            conn_id,
            "sub1".to_string(),
            vec![Filter::new().kind(Kind::TextNote)],
            Some(channel_id),
        );
        registry.register(
            conn_id,
            "sub2".to_string(),
            vec![Filter::new().kind(Kind::Metadata)],
            Some(channel_id),
        );

        assert_eq!(registry.total_subscriptions(), 2);

        registry.remove_connection(conn_id);

        assert_eq!(registry.total_subscriptions(), 0);
        assert_eq!(registry.total_connections(), 0);
    }

    #[test]
    fn test_subscription_registry_channel_kind_index() {
        let registry = SubscriptionRegistry::new();
        let channel_id = Uuid::new_v4();

        let mut conn_ids = Vec::new();
        for i in 0..3 {
            let conn_id = Uuid::new_v4();
            conn_ids.push(conn_id);
            registry.register(
                conn_id,
                format!("sub{i}"),
                vec![Filter::new().kind(Kind::TextNote)],
                Some(channel_id),
            );
        }

        let event = make_stored_event(Kind::TextNote, Some(channel_id));
        let matches = registry.fan_out(&event);
        assert_eq!(matches.len(), 3);

        let event_meta = make_stored_event(Kind::Metadata, Some(channel_id));
        let matches_meta = registry.fan_out(&event_meta);
        assert!(matches_meta.is_empty());
    }

    #[test]
    fn test_subscription_registry_replace_existing() {
        let registry = SubscriptionRegistry::new();
        let conn_id = Uuid::new_v4();
        let channel_id = Uuid::new_v4();

        registry.register(
            conn_id,
            "sub1".to_string(),
            vec![Filter::new().kind(Kind::TextNote)],
            Some(channel_id),
        );

        registry.register(
            conn_id,
            "sub1".to_string(),
            vec![Filter::new().kind(Kind::Metadata)],
            Some(channel_id),
        );

        let event1 = make_stored_event(Kind::TextNote, Some(channel_id));
        let matches1 = registry.fan_out(&event1);
        assert!(matches1.is_empty());

        let event0 = make_stored_event(Kind::Metadata, Some(channel_id));
        let matches0 = registry.fan_out(&event0);
        assert_eq!(matches0.len(), 1);
    }

    #[test]
    fn test_subscription_registry_no_channel_slow_path() {
        let registry = SubscriptionRegistry::new();
        let conn_id = Uuid::new_v4();

        registry.register(
            conn_id,
            "sub1".to_string(),
            vec![Filter::new().kind(Kind::TextNote)],
            None, // no channel
        );

        let event = make_stored_event(Kind::TextNote, None);
        let matches = registry.fan_out(&event);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_subscription_registry_get_filters() {
        let registry = SubscriptionRegistry::new();
        let conn_id = Uuid::new_v4();
        let filters = vec![Filter::new().kind(Kind::TextNote)];

        registry.register(conn_id, "sub1".to_string(), filters.clone(), None);

        let retrieved = registry.get_filters(conn_id, "sub1");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().len(), 1);

        let missing = registry.get_filters(conn_id, "nonexistent");
        assert!(missing.is_none());
    }

    #[test]
    fn test_remove_from_index_targeted_no_full_scan() {
        // Verify that removing a subscription only touches the relevant index keys.
        // We register subs for two different channels and two different kinds,
        // then remove one and confirm the other channel's index is untouched.
        let registry = SubscriptionRegistry::new();
        let conn_a = Uuid::new_v4();
        let conn_b = Uuid::new_v4();
        let channel_x = Uuid::new_v4();
        let channel_y = Uuid::new_v4();

        registry.register(
            conn_a,
            "sub_a".to_string(),
            vec![Filter::new().kind(Kind::TextNote)],
            Some(channel_x),
        );
        registry.register(
            conn_b,
            "sub_b".to_string(),
            vec![Filter::new().kind(Kind::TextNote)],
            Some(channel_y),
        );

        registry.remove_subscription(conn_a, "sub_a");

        let key_x = IndexKey {
            channel_id: channel_x,
            kind: Kind::TextNote,
        };
        assert!(registry.channel_kind_index.get(&key_x).is_none());

        let key_y = IndexKey {
            channel_id: channel_y,
            kind: Kind::TextNote,
        };
        let entries = registry
            .channel_kind_index
            .get(&key_y)
            .expect("channel_y index intact");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, conn_b);
    }

    #[test]
    fn test_kindless_channel_subscription_receives_all_kinds() {
        // A subscription with channel_id but NO kind filter should receive events
        // of any kind posted to that channel.
        let registry = SubscriptionRegistry::new();
        let conn_id = Uuid::new_v4();
        let channel_id = Uuid::new_v4();
        let sub_id = "wildcard_sub".to_string();

        let filters = vec![Filter::new()]; // kindless — no .kind() constraint
        registry.register(conn_id, sub_id.clone(), filters, Some(channel_id));

        let event_text = make_stored_event(Kind::TextNote, Some(channel_id));
        let matches = registry.fan_out(&event_text);
        assert_eq!(matches.len(), 1, "kindless sub should receive TextNote");
        assert_eq!(matches[0].0, conn_id);
        assert_eq!(matches[0].1, sub_id);

        let event_meta = make_stored_event(Kind::Metadata, Some(channel_id));
        let matches = registry.fan_out(&event_meta);
        assert_eq!(matches.len(), 1, "kindless sub should receive Metadata");

        let event_custom = make_stored_event(Kind::Custom(9999), Some(channel_id));
        let matches = registry.fan_out(&event_custom);
        assert_eq!(matches.len(), 1, "kindless sub should receive custom kind");

        let other_channel = Uuid::new_v4();
        let event_other = make_stored_event(Kind::TextNote, Some(other_channel));
        let matches = registry.fan_out(&event_other);
        assert!(
            matches.is_empty(),
            "kindless sub should not receive events from other channels"
        );
    }

    #[test]
    fn test_kindless_subscription_remove_cleans_wildcard_index() {
        // Verify that removing a kindless subscription cleans up the wildcard index.
        let registry = SubscriptionRegistry::new();
        let conn_id = Uuid::new_v4();
        let channel_id = Uuid::new_v4();

        let filters = vec![Filter::new()]; // kindless
        registry.register(conn_id, "sub1".to_string(), filters, Some(channel_id));

        assert!(registry.channel_wildcard_index.get(&channel_id).is_some());

        registry.remove_subscription(conn_id, "sub1");

        assert!(registry.channel_wildcard_index.get(&channel_id).is_none());

        let event = make_stored_event(Kind::TextNote, Some(channel_id));
        let matches = registry.fan_out(&event);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_kindless_and_kinded_subs_coexist() {
        // Both a kindless sub and a kind-specific sub in the same channel should
        // both receive events of the matching kind.
        let registry = SubscriptionRegistry::new();
        let conn_wildcard = Uuid::new_v4();
        let conn_kinded = Uuid::new_v4();
        let channel_id = Uuid::new_v4();

        registry.register(
            conn_wildcard,
            "sub_wildcard".to_string(),
            vec![Filter::new()],
            Some(channel_id),
        );

        registry.register(
            conn_kinded,
            "sub_kinded".to_string(),
            vec![Filter::new().kind(Kind::TextNote)],
            Some(channel_id),
        );

        let event_text = make_stored_event(Kind::TextNote, Some(channel_id));
        let matches = registry.fan_out(&event_text);
        assert_eq!(
            matches.len(),
            2,
            "both wildcard and kinded sub should match TextNote"
        );

        let event_meta = make_stored_event(Kind::Metadata, Some(channel_id));
        let matches = registry.fan_out(&event_meta);
        assert_eq!(matches.len(), 1, "only wildcard sub should match Metadata");
        assert_eq!(matches[0].0, conn_wildcard);
    }

    #[test]
    fn test_kindless_subscription_replace() {
        // Replacing a kindless sub with a kinded sub should move it from wildcard
        // index to kind-specific index, and vice versa.
        let registry = SubscriptionRegistry::new();
        let conn_id = Uuid::new_v4();
        let channel_id = Uuid::new_v4();

        registry.register(
            conn_id,
            "sub1".to_string(),
            vec![Filter::new()],
            Some(channel_id),
        );
        assert!(registry.channel_wildcard_index.get(&channel_id).is_some());

        registry.register(
            conn_id,
            "sub1".to_string(),
            vec![Filter::new().kind(Kind::TextNote)],
            Some(channel_id),
        );

        assert!(registry.channel_wildcard_index.get(&channel_id).is_none());

        let key = IndexKey {
            channel_id,
            kind: Kind::TextNote,
        };
        assert!(registry.channel_kind_index.get(&key).is_some());

        let event_meta = make_stored_event(Kind::Metadata, Some(channel_id));
        let matches = registry.fan_out(&event_meta);
        assert!(matches.is_empty());

        let event_text = make_stored_event(Kind::TextNote, Some(channel_id));
        let matches = registry.fan_out(&event_text);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_empty_kinds_array_matches_nothing() {
        // NIP-01: `kinds: []` means "match no kinds". A subscription with an
        // explicit empty kinds list should never receive any events — it should
        // NOT be treated as a wildcard (match-all).
        let registry = SubscriptionRegistry::new();
        let conn_id = Uuid::new_v4();
        let channel_id = Uuid::new_v4();

        let filter_empty_kinds = Filter::new().kinds(vec![] as Vec<Kind>);
        registry.register(
            conn_id,
            "sub_empty_kinds".to_string(),
            vec![filter_empty_kinds],
            Some(channel_id),
        );

        assert!(
            registry.channel_wildcard_index.get(&channel_id).is_none(),
            "kinds:[] sub must NOT be in the wildcard index"
        );

        let key = IndexKey {
            channel_id,
            kind: Kind::TextNote,
        };
        assert!(
            registry.channel_kind_index.get(&key).is_none(),
            "kinds:[] sub must NOT be in the kind-specific index"
        );

        let event = make_stored_event(Kind::TextNote, Some(channel_id));
        let matches = registry.fan_out(&event);
        assert!(
            matches.is_empty(),
            "kinds:[] sub must not receive any events (got {:?})",
            matches
        );

        let event_meta = make_stored_event(Kind::Metadata, Some(channel_id));
        let matches = registry.fan_out(&event_meta);
        assert!(
            matches.is_empty(),
            "kinds:[] sub must not receive Metadata events"
        );
    }

    #[test]
    fn test_global_sub_does_not_receive_channel_events() {
        // Security regression test: a global subscription (channel_id = None) must
        // NOT receive events that are scoped to a channel. Doing so would bypass the
        // channel membership check and leak private channel content.
        let registry = SubscriptionRegistry::new();
        let conn_id = Uuid::new_v4();
        let channel_id = Uuid::new_v4();

        registry.register(
            conn_id,
            "global_sub".to_string(),
            vec![Filter::new().kind(Kind::TextNote)],
            None, // global — no channel scope
        );

        let channel_event = make_stored_event(Kind::TextNote, Some(channel_id));
        let matches = registry.fan_out(&channel_event);
        assert!(
            matches.is_empty(),
            "global sub must not receive channel-scoped events (got {:?})",
            matches
        );

        let global_event = make_stored_event(Kind::TextNote, None);
        let matches = registry.fan_out(&global_event);
        assert_eq!(
            matches.len(),
            1,
            "global sub should still receive non-channel events"
        );
        assert_eq!(matches[0].0, conn_id);
    }

    #[test]
    fn test_empty_kinds_array_remove_is_noop() {
        // Removing a kinds:[] subscription should not panic or corrupt the index.
        let registry = SubscriptionRegistry::new();
        let conn_id = Uuid::new_v4();
        let channel_id = Uuid::new_v4();

        let filter_empty_kinds = Filter::new().kinds(vec![] as Vec<Kind>);
        registry.register(
            conn_id,
            "sub_empty".to_string(),
            vec![filter_empty_kinds],
            Some(channel_id),
        );

        registry.remove_subscription(conn_id, "sub_empty");

        assert!(registry.channel_wildcard_index.get(&channel_id).is_none());
        let key = IndexKey {
            channel_id,
            kind: Kind::TextNote,
        };
        assert!(registry.channel_kind_index.get(&key).is_none());
    }
}
