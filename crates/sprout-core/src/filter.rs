//! NIP-01 filter matching.
//!
//! Multiple filters are OR-ed; fields within one filter are AND-ed.

use nostr::Filter;

use crate::event::StoredEvent;

/// Returns `true` if the event matches any of the provided NIP-01 filters.
pub fn filters_match(filters: &[Filter], event: &StoredEvent) -> bool {
    filters.iter().any(|f| filter_match_one(f, event))
}

fn filter_match_one(f: &Filter, ev: &StoredEvent) -> bool {
    if let Some(kinds) = &f.kinds {
        if !kinds.contains(&ev.event.kind) {
            return false;
        }
    }

    if let Some(authors) = &f.authors {
        if !authors.contains(&ev.event.pubkey) {
            return false;
        }
    }

    if let Some(since) = f.since {
        if ev.event.created_at < since {
            return false;
        }
    }

    if let Some(until) = f.until {
        if ev.event.created_at > until {
            return false;
        }
    }

    // NIP-01 allows prefix matching on event IDs.
    if let Some(ids) = &f.ids {
        let event_id_hex = ev.event.id.to_hex();
        if !ids.iter().any(|id| event_id_hex.starts_with(&id.to_hex())) {
            return false;
        }
    }

    for (tag_key, tag_values) in f.generic_tags.iter() {
        let tag_key_str = tag_key.to_string();
        let has_match = tag_values.iter().any(|filter_val| {
            ev.event
                .tags
                .iter()
                .filter(|t| t.kind().to_string() == tag_key_str)
                .filter_map(|t| t.content())
                .any(|event_val| event_val == filter_val.as_str())
        });
        if !has_match {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{make_event_with_keys, make_stored_event};
    use chrono::Utc;
    use nostr::{EventBuilder, Keys, Kind, Tag, Timestamp};

    fn stored_with_tag(tag: Tag) -> StoredEvent {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::TextNote, "test", [tag])
            .sign_with_keys(&keys)
            .expect("sign");
        StoredEvent::with_received_at(event, Utc::now(), None, true)
    }

    #[test]
    fn kind_author_since_until_tag_matching() {
        let keys = Keys::generate();
        let ev = StoredEvent::with_received_at(
            make_event_with_keys(&keys, Kind::TextNote),
            Utc::now(),
            None,
            true,
        );
        let pubkey = keys.public_key();
        let now_ts = nostr::Timestamp::now();
        let past = Timestamp::from(now_ts.as_u64() - 3600);
        let future = Timestamp::from(now_ts.as_u64() + 3600);

        assert!(filters_match(&[Filter::new().kind(Kind::TextNote)], &ev));
        assert!(!filters_match(
            &[Filter::new().kind(Kind::ContactList)],
            &ev
        ));

        assert!(filters_match(&[Filter::new().author(pubkey)], &ev));
        assert!(!filters_match(
            &[Filter::new().author(Keys::generate().public_key())],
            &ev
        ));

        assert!(filters_match(
            &[Filter::new().kind(Kind::TextNote).author(pubkey)],
            &ev
        ));
        assert!(!filters_match(
            &[Filter::new().kind(Kind::ContactList).author(pubkey)],
            &ev
        ));

        assert!(filters_match(&[Filter::new().since(past)], &ev));
        assert!(!filters_match(&[Filter::new().since(future)], &ev));
        assert!(filters_match(&[Filter::new().until(future)], &ev));
        assert!(!filters_match(&[Filter::new().until(past)], &ev));
    }

    #[test]
    fn or_semantics() {
        let ev = make_stored_event(Kind::TextNote, None);
        let miss = Filter::new().kind(Kind::ContactList);
        let hit = Filter::new().kind(Kind::TextNote);
        assert!(filters_match(&[miss.clone(), hit], &ev));
        assert!(!filters_match(
            &[miss, Filter::new().kind(Kind::EventDeletion)],
            &ev
        ));
        assert!(!filters_match(&[], &ev));
    }

    #[test]
    fn tag_matching() {
        let target_id = nostr::EventId::all_zeros();
        let ev = stored_with_tag(Tag::event(target_id));
        assert!(filters_match(&[Filter::new().event(target_id)], &ev));
        assert!(!filters_match(
            &[Filter::new().event(nostr::EventId::from_byte_array([1u8; 32]))],
            &ev
        ));
    }
}
