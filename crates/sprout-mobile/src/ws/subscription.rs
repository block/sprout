//! Subscription lifecycle helpers — REQ, EOSE, event routing.
//! Routes incoming events to the appropriate SproutEventListener callbacks.

use nostr::{Filter, Kind};
use uuid::Uuid;

/// Generate a unique subscription ID.
pub(crate) fn generate_sub_id(prefix: &str) -> String {
    format!("{}-{}", prefix, Uuid::new_v4().as_simple())
}

/// Build a filter for subscribing to a channel's messages.
pub(crate) fn channel_message_filter(channel_id: &str) -> Filter {
    Filter::new()
        .kinds(vec![
            Kind::Custom(9),     // Stream message
            Kind::Custom(40003), // Edit
            Kind::Custom(5),     // Deletion
            Kind::Custom(7),     // Reaction
            Kind::Custom(40099), // System message
        ])
        .custom_tag(
            nostr::SingleLetterTag::lowercase(nostr::Alphabet::H),
            [channel_id],
        )
}

/// Build a filter for typing indicators in a channel.
#[allow(dead_code)]
pub(crate) fn typing_indicator_filter(channel_id: &str) -> Filter {
    Filter::new().kind(Kind::Custom(20002)).custom_tag(
        nostr::SingleLetterTag::lowercase(nostr::Alphabet::H),
        [channel_id],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sub_id_has_prefix() {
        let id = generate_sub_id("msgs");
        assert!(id.starts_with("msgs-"));
        assert!(id.len() > 10);
    }

    #[test]
    fn sub_ids_are_unique() {
        let id1 = generate_sub_id("test");
        let id2 = generate_sub_id("test");
        assert_ne!(id1, id2);
    }
}
