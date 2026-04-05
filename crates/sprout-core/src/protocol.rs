// ABOUTME: NIP-01 relay-to-client message parsing — shared by all Sprout client crates.
// ABOUTME: Pure serde_json parsing with no I/O dependencies.

use nostr::Event;
use serde_json::Value;
use thiserror::Error;

/// Errors that can occur when parsing a relay message.
#[derive(Debug, Error)]
pub enum ProtocolError {
    /// Failed to deserialize JSON.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// The relay sent a message that could not be parsed.
    #[error("unexpected relay message: {0}")]
    UnexpectedMessage(String),
}

/// A message received from a Nostr relay (NIP-01 relay-to-client).
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

/// The relay's response to a published event (NIP-01 `OK` message).
#[derive(Debug, Clone)]
pub struct OkResponse {
    /// Hex-encoded ID of the event that was acknowledged.
    pub event_id: String,
    /// Whether the relay accepted the event.
    pub accepted: bool,
    /// Human-readable reason string (empty when accepted without comment).
    pub message: String,
}

/// Parse a raw JSON WebSocket frame into a [`RelayMessage`].
///
/// Handles all NIP-01 relay-to-client message types: EVENT, OK, EOSE,
/// CLOSED, NOTICE, and AUTH.
pub fn parse_relay_message(text: &str) -> Result<RelayMessage, ProtocolError> {
    let arr: Vec<Value> = serde_json::from_str(text)?;

    let msg_type = arr
        .first()
        .and_then(|v| v.as_str())
        .ok_or_else(|| ProtocolError::UnexpectedMessage(text.to_string()))?;

    match msg_type {
        "EVENT" => {
            let sub_id = arr
                .get(1)
                .and_then(|v| v.as_str())
                .ok_or_else(|| ProtocolError::UnexpectedMessage(text.to_string()))?
                .to_string();
            let event: Event = serde_json::from_value(
                arr.get(2)
                    .cloned()
                    .ok_or_else(|| ProtocolError::UnexpectedMessage(text.to_string()))?,
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
                .ok_or_else(|| ProtocolError::UnexpectedMessage(text.to_string()))?
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
                .ok_or_else(|| ProtocolError::UnexpectedMessage(text.to_string()))?
                .to_string();
            Ok(RelayMessage::Eose {
                subscription_id: sub_id,
            })
        }
        "CLOSED" => {
            let sub_id = arr
                .get(1)
                .and_then(|v| v.as_str())
                .ok_or_else(|| ProtocolError::UnexpectedMessage(text.to_string()))?
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
                .ok_or_else(|| ProtocolError::UnexpectedMessage(text.to_string()))?
                .to_string();
            Ok(RelayMessage::Auth { challenge })
        }
        other => Err(ProtocolError::UnexpectedMessage(format!(
            "unknown message type: {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ok_accepted() {
        let msg = parse_relay_message(r#"["OK","abc123",true,""]"#).unwrap();
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
        let msg =
            parse_relay_message(r#"["OK","def456",false,"blocked: not authorized"]"#).unwrap();
        match msg {
            RelayMessage::Ok(ok) => {
                assert_eq!(ok.event_id, "def456");
                assert!(!ok.accepted);
                assert_eq!(ok.message, "blocked: not authorized");
            }
            _ => panic!("expected Ok"),
        }
    }

    #[test]
    fn parse_eose() {
        let msg = parse_relay_message(r#"["EOSE","sub1"]"#).unwrap();
        match msg {
            RelayMessage::Eose { subscription_id } => assert_eq!(subscription_id, "sub1"),
            _ => panic!("expected Eose"),
        }
    }

    #[test]
    fn parse_notice() {
        let msg = parse_relay_message(r#"["NOTICE","hello from relay"]"#).unwrap();
        match msg {
            RelayMessage::Notice { message } => assert_eq!(message, "hello from relay"),
            _ => panic!("expected Notice"),
        }
    }

    #[test]
    fn parse_auth() {
        let msg = parse_relay_message(r#"["AUTH","deadbeef1234"]"#).unwrap();
        match msg {
            RelayMessage::Auth { challenge } => assert_eq!(challenge, "deadbeef1234"),
            _ => panic!("expected Auth"),
        }
    }

    #[test]
    fn parse_closed() {
        let msg =
            parse_relay_message(r#"["CLOSED","sub2","auth-required: must authenticate"]"#)
                .unwrap();
        match msg {
            RelayMessage::Closed {
                subscription_id,
                message,
            } => {
                assert_eq!(subscription_id, "sub2");
                assert_eq!(message, "auth-required: must authenticate");
            }
            _ => panic!("expected Closed"),
        }
    }

    #[test]
    fn parse_unknown_type_errors() {
        let result = parse_relay_message(r#"["UNKNOWN","data"]"#);
        assert!(result.is_err());
    }

    #[test]
    fn parse_invalid_json_errors() {
        let result = parse_relay_message("not json");
        assert!(result.is_err());
    }
}
