//! Shared relay protocol types — NIP-01 message parsing, URL conversion,
//! percent-encoding, and deduplication.
//!
//! These utilities were previously duplicated between `sprout-acp` and
//! `sprout-mcp`. Centralised here so both crates can share one canonical
//! implementation.

use std::collections::HashSet;

use nostr::Event;
use serde_json::Value;

// ── Error type ────────────────────────────────────────────────────────────────

/// Errors that can occur while parsing a NIP-01 relay message.
#[derive(Debug, thiserror::Error)]
pub enum RelayProtocolError {
    /// The raw text was not valid JSON.
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    /// The message structure was not a recognised NIP-01 relay message.
    #[error("unexpected relay message: {0}")]
    UnexpectedMessage(String),
}

// ── RelayMessage ──────────────────────────────────────────────────────────────

/// The relay's response to a published event (NIP-01 `OK` message).
#[derive(Debug, Clone)]
pub struct OkResponse {
    /// Hex-encoded ID of the acknowledged event.
    pub event_id: String,
    /// Whether the relay accepted the event.
    pub accepted: bool,
    /// Human-readable reason (empty when accepted without comment).
    pub message: String,
}

/// A parsed NIP-01 relay-to-client message.
#[derive(Debug, Clone)]
pub enum RelayMessage {
    /// An event matching an active subscription.
    Event {
        /// The subscription ID this event belongs to.
        subscription_id: String,
        /// The Nostr event payload.
        event: Box<Event>,
    },
    /// Acknowledgement of a published event.
    Ok(OkResponse),
    /// End-of-stored-events marker for a subscription.
    Eose {
        /// The subscription ID that has reached end-of-stored-events.
        subscription_id: String,
    },
    /// The relay closed a subscription, usually with an error.
    Closed {
        /// The subscription ID that was closed.
        subscription_id: String,
        /// Human-readable reason for the closure.
        message: String,
    },
    /// A human-readable notice from the relay.
    Notice {
        /// The notice text.
        message: String,
    },
    /// A NIP-42 authentication challenge from the relay.
    Auth {
        /// The challenge string to sign.
        challenge: String,
    },
}

// ── parse_relay_message ───────────────────────────────────────────────────────

/// Parse a NIP-01 relay message from raw JSON text.
///
/// Handles `EVENT`, `OK`, `EOSE`, `CLOSED`, `NOTICE`, and `AUTH`.
/// Returns [`RelayProtocolError::UnexpectedMessage`] for any other type.
#[allow(clippy::result_large_err)]
pub fn parse_relay_message(text: &str) -> Result<RelayMessage, RelayProtocolError> {
    let arr: Vec<Value> = serde_json::from_str(text)?;

    let msg_type = arr
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| RelayProtocolError::UnexpectedMessage(text.to_string()))?;

    match msg_type {
        "EVENT" => {
            let sub_id = arr
                .get(1)
                .and_then(|v| v.as_str())
                .ok_or_else(|| RelayProtocolError::UnexpectedMessage(text.to_string()))?
                .to_string();
            let event: Event = serde_json::from_value(
                arr.get(2)
                    .cloned()
                    .ok_or_else(|| RelayProtocolError::UnexpectedMessage(text.to_string()))?,
            )?;
            Ok(RelayMessage::Event {
                subscription_id: sub_id,
                event: Box::new(event),
            })
        }
        "OK" => {
            let event_id = arr
                .get(1)
                .and_then(|v| v.as_str())
                .ok_or_else(|| RelayProtocolError::UnexpectedMessage(text.to_string()))?
                .to_string();
            let accepted = arr.get(2).and_then(|v| v.as_bool()).unwrap_or(false);
            let message = arr
                .get(3)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Ok(RelayMessage::Ok(OkResponse {
                event_id,
                accepted,
                message,
            }))
        }
        "EOSE" => {
            let sub_id = arr
                .get(1)
                .and_then(|v| v.as_str())
                .ok_or_else(|| RelayProtocolError::UnexpectedMessage(text.to_string()))?
                .to_string();
            Ok(RelayMessage::Eose {
                subscription_id: sub_id,
            })
        }
        "CLOSED" => {
            let sub_id = arr
                .get(1)
                .and_then(|v| v.as_str())
                .ok_or_else(|| RelayProtocolError::UnexpectedMessage(text.to_string()))?
                .to_string();
            let message = arr
                .get(2)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Ok(RelayMessage::Closed {
                subscription_id: sub_id,
                message,
            })
        }
        "NOTICE" => {
            let message = arr
                .get(1)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Ok(RelayMessage::Notice { message })
        }
        "AUTH" => {
            let challenge = arr
                .get(1)
                .and_then(|v| v.as_str())
                .ok_or_else(|| RelayProtocolError::UnexpectedMessage(text.to_string()))?
                .to_string();
            Ok(RelayMessage::Auth { challenge })
        }
        other => Err(RelayProtocolError::UnexpectedMessage(format!(
            "unknown message type: {other}"
        ))),
    }
}

// ── relay_ws_to_http ──────────────────────────────────────────────────────────

/// Convert a WebSocket relay URL to its HTTP equivalent.
///
/// `ws://` → `http://`, `wss://` → `https://`. Trailing slashes are stripped.
///
/// ```
/// # use sprout_core::relay_protocol::relay_ws_to_http;
/// assert_eq!(relay_ws_to_http("wss://relay.example.com/"), "https://relay.example.com");
/// assert_eq!(relay_ws_to_http("ws://localhost:8080"),       "http://localhost:8080");
/// ```
pub fn relay_ws_to_http(url: &str) -> String {
    url.replace("wss://", "https://")
        .replace("ws://", "http://")
        .trim_end_matches('/')
        .to_string()
}

// ── pct_encode ────────────────────────────────────────────────────────────────

/// Percent-encode a string per RFC 3986.
///
/// Unreserved characters (`A–Z a–z 0–9 - _ . ~`) pass through unchanged.
/// All other bytes are encoded as `%XX`.
///
/// ```
/// # use sprout_core::relay_protocol::pct_encode;
/// assert_eq!(pct_encode("AZaz09-_.~"), "AZaz09-_.~");
/// assert_eq!(pct_encode(" "),           "%20");
/// assert_eq!(pct_encode("👀"),          "%F0%9F%91%80");
/// ```
pub fn pct_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                use std::fmt::Write;
                let _ = write!(out, "%{byte:02X}");
            }
        }
    }
    out
}

// ── TwoGenDedup ───────────────────────────────────────────────────────────────

/// Bounded-memory deduplication set using two generations.
///
/// At any point between `limit/2` and `limit` recent IDs are remembered.
/// When the current generation fills to `limit/2`, it becomes the previous
/// generation and a fresh current generation starts. The oldest `limit/2`
/// IDs are then eligible to be forgotten — an acceptable tradeoff for
/// bounded-memory dedup where the Nostr `since` filter provides primary
/// replay protection.
pub struct TwoGenDedup {
    current: HashSet<String>,
    previous: HashSet<String>,
    limit: usize,
}

impl TwoGenDedup {
    /// Create a new dedup set with the given capacity limit.
    ///
    /// Rotation occurs when `current` reaches `limit / 2`.
    pub fn new(limit: usize) -> Self {
        Self {
            current: HashSet::new(),
            previous: HashSet::new(),
            limit,
        }
    }

    /// Returns `true` if `id` is in either generation.
    pub fn contains(&self, id: &str) -> bool {
        self.current.contains(id) || self.previous.contains(id)
    }

    /// Insert `id`. Returns `true` if it was new (not a duplicate).
    ///
    /// Triggers a generation rotation when `current` reaches `limit / 2`.
    pub fn insert(&mut self, id: String) -> bool {
        if self.contains(&id) {
            return false;
        }
        self.current.insert(id);
        if self.current.len() >= self.limit / 2 {
            self.previous = std::mem::take(&mut self.current);
        }
        true
    }

    /// Remove `id` from both generations.
    ///
    /// Used to un-deduplicate a dropped event so it can be replayed after
    /// reconnect.
    pub fn remove(&mut self, id: &str) {
        self.current.remove(id);
        self.previous.remove(id);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── relay_ws_to_http ──────────────────────────────────────────────────────

    #[test]
    fn relay_ws_to_http_plain() {
        assert_eq!(
            relay_ws_to_http("ws://relay.example.com"),
            "http://relay.example.com"
        );
    }

    #[test]
    fn relay_ws_to_http_secure() {
        assert_eq!(
            relay_ws_to_http("wss://relay.example.com"),
            "https://relay.example.com"
        );
    }

    #[test]
    fn relay_ws_to_http_strips_trailing_slash() {
        assert_eq!(
            relay_ws_to_http("wss://relay.example.com/"),
            "https://relay.example.com"
        );
    }

    #[test]
    fn relay_ws_to_http_with_path() {
        assert_eq!(
            relay_ws_to_http("wss://relay.example.com/nostr"),
            "https://relay.example.com/nostr"
        );
    }

    // ── pct_encode ────────────────────────────────────────────────────────────

    #[test]
    fn pct_encode_empty() {
        assert_eq!(pct_encode(""), "");
    }

    #[test]
    fn pct_encode_unreserved_passthrough() {
        assert_eq!(pct_encode("AZaz09-_.~"), "AZaz09-_.~");
    }

    #[test]
    fn pct_encode_space() {
        assert_eq!(pct_encode(" "), "%20");
    }

    #[test]
    fn pct_encode_reserved_chars() {
        assert_eq!(pct_encode("/"), "%2F");
        assert_eq!(pct_encode("+"), "%2B");
        assert_eq!(pct_encode("@"), "%40");
    }

    #[test]
    fn pct_encode_emoji() {
        assert_eq!(pct_encode("👀"), "%F0%9F%91%80");
        assert_eq!(pct_encode("💬"), "%F0%9F%92%AC");
    }

    // ── parse_relay_message ───────────────────────────────────────────────────

    #[test]
    fn parse_ok_accepted() {
        let text = r#"["OK","abc123",true,""]"#;
        let msg = parse_relay_message(text).unwrap();
        match msg {
            RelayMessage::Ok(ok) => {
                assert_eq!(ok.event_id, "abc123");
                assert!(ok.accepted);
                assert_eq!(ok.message, "");
            }
            _ => panic!("expected Ok"),
        }
    }

    #[test]
    fn parse_ok_rejected() {
        let text = r#"["OK","abc123",false,"blocked: spam"]"#;
        let msg = parse_relay_message(text).unwrap();
        match msg {
            RelayMessage::Ok(ok) => {
                assert_eq!(ok.event_id, "abc123");
                assert!(!ok.accepted);
                assert_eq!(ok.message, "blocked: spam");
            }
            _ => panic!("expected Ok"),
        }
    }

    #[test]
    fn parse_eose() {
        let text = r#"["EOSE","sub1"]"#;
        let msg = parse_relay_message(text).unwrap();
        match msg {
            RelayMessage::Eose { subscription_id } => assert_eq!(subscription_id, "sub1"),
            _ => panic!("expected Eose"),
        }
    }

    #[test]
    fn parse_notice() {
        let text = r#"["NOTICE","hello world"]"#;
        let msg = parse_relay_message(text).unwrap();
        match msg {
            RelayMessage::Notice { message } => assert_eq!(message, "hello world"),
            _ => panic!("expected Notice"),
        }
    }

    #[test]
    fn parse_notice_empty() {
        // NOTICE with no message field — graceful default
        let text = r#"["NOTICE"]"#;
        let msg = parse_relay_message(text).unwrap();
        match msg {
            RelayMessage::Notice { message } => assert_eq!(message, ""),
            _ => panic!("expected Notice"),
        }
    }

    #[test]
    fn parse_auth() {
        let text = r#"["AUTH","challenge-xyz"]"#;
        let msg = parse_relay_message(text).unwrap();
        match msg {
            RelayMessage::Auth { challenge } => assert_eq!(challenge, "challenge-xyz"),
            _ => panic!("expected Auth"),
        }
    }

    #[test]
    fn parse_closed() {
        let text = r#"["CLOSED","sub1","auth-required: please authenticate"]"#;
        let msg = parse_relay_message(text).unwrap();
        match msg {
            RelayMessage::Closed {
                subscription_id,
                message,
            } => {
                assert_eq!(subscription_id, "sub1");
                assert_eq!(message, "auth-required: please authenticate");
            }
            _ => panic!("expected Closed"),
        }
    }

    #[test]
    fn parse_invalid_json() {
        let err = parse_relay_message("not json").unwrap_err();
        assert!(matches!(err, RelayProtocolError::Json(_)));
    }

    #[test]
    fn parse_unknown_type() {
        let err = parse_relay_message(r#"["UNKNOWN","x"]"#).unwrap_err();
        assert!(matches!(err, RelayProtocolError::UnexpectedMessage(_)));
    }

    #[test]
    fn parse_empty_array() {
        let err = parse_relay_message("[]").unwrap_err();
        assert!(matches!(err, RelayProtocolError::UnexpectedMessage(_)));
    }

    // ── TwoGenDedup ───────────────────────────────────────────────────────────

    #[test]
    fn dedup_insert_and_contains() {
        let mut d = TwoGenDedup::new(100);
        assert!(d.insert("a".to_string()));
        assert!(d.contains("a"));
        assert!(!d.insert("a".to_string())); // duplicate
    }

    #[test]
    fn dedup_remove() {
        let mut d = TwoGenDedup::new(100);
        d.insert("a".to_string());
        d.remove("a");
        assert!(!d.contains("a"));
        // re-insert after remove should succeed
        assert!(d.insert("a".to_string()));
    }

    #[test]
    fn dedup_rotation() {
        // limit=12 → rotate at 6 entries
        let mut d = TwoGenDedup::new(12);
        for i in 0..6u32 {
            assert!(d.insert(i.to_string()));
        }
        // After 6 inserts the 6th triggers rotation:
        // previous = {0..5}, current = {}
        // IDs 0-5 should still be found (in previous).
        for i in 0..6u32 {
            assert!(
                d.contains(&i.to_string()),
                "id {i} should still be present after rotation"
            );
        }

        // Fill current again to trigger a second rotation.
        for i in 6..12u32 {
            d.insert(i.to_string());
        }
        // Now previous = {6..11}, current = {}
        // IDs 0-5 are gone (they were in old previous, now discarded).
        for i in 0..6u32 {
            assert!(
                !d.contains(&i.to_string()),
                "id {i} should be gone after second rotation"
            );
        }
        for i in 6..12u32 {
            assert!(d.contains(&i.to_string()), "id {i} should still be present");
        }
    }

    #[test]
    fn dedup_remove_from_previous() {
        let mut d = TwoGenDedup::new(4); // rotate at 2
        d.insert("a".to_string());
        d.insert("b".to_string()); // triggers rotation: previous={a,b}, current={}
        assert!(d.contains("a")); // in previous
        d.remove("a");
        assert!(!d.contains("a"));
    }
}
