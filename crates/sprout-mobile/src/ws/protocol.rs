//! Client-to-relay message formatting for outgoing WebSocket frames.
//! Formats EVENT, REQ, CLOSE, and AUTH messages as NIP-01 JSON arrays.
#![allow(dead_code)]

use nostr::{Event, Filter};
use serde_json::{json, Value};

/// Format an EVENT message for submission to the relay.
pub(crate) fn format_event(event: &Event) -> String {
    let event_json = serde_json::to_value(event)
        .expect("nostr::Event serialization is infallible for well-formed events");
    json!(["EVENT", event_json]).to_string()
}

/// Format a REQ message to open a subscription.
pub(crate) fn format_req(sub_id: &str, filters: &[Filter]) -> String {
    let mut msg: Vec<Value> = vec![json!("REQ"), json!(sub_id)];
    for f in filters {
        msg.push(serde_json::to_value(f).unwrap_or(json!({})));
    }
    Value::Array(msg).to_string()
}

/// Format a CLOSE message to cancel a subscription.
pub(crate) fn format_close(sub_id: &str) -> String {
    json!(["CLOSE", sub_id]).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind};

    #[test]
    fn format_event_is_valid_json_array() {
        let keys = Keys::generate();
        let event = EventBuilder::new(Kind::TextNote, "hello", [])
            .sign_with_keys(&keys)
            .unwrap();
        let text = format_event(&event);
        let arr: Vec<Value> = serde_json::from_str(&text).unwrap();
        assert_eq!(arr[0], "EVENT");
        assert!(arr[1].is_object());
    }

    #[test]
    fn format_req_has_correct_structure() {
        let filter = Filter::new().kind(Kind::TextNote);
        let text = format_req("sub-1", &[filter]);
        let arr: Vec<Value> = serde_json::from_str(&text).unwrap();
        assert_eq!(arr[0], "REQ");
        assert_eq!(arr[1], "sub-1");
        assert!(arr[2].is_object());
    }

    #[test]
    fn format_close_is_valid() {
        let text = format_close("sub-1");
        let arr: Vec<Value> = serde_json::from_str(&text).unwrap();
        assert_eq!(arr[0], "CLOSE");
        assert_eq!(arr[1], "sub-1");
    }
}
