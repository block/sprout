// ABOUTME: NIP-42 WebSocket authentication — challenge/response signing.
// ABOUTME: Builds kind:22242 auth events in response to relay challenges.

use nostr::{EventBuilder, Keys, Kind, Tag, Url};

use crate::error::SproutError;

/// Build a signed NIP-42 AUTH event in response to a relay challenge.
///
/// Returns the signed event ready for sending as `["AUTH", <event>]`
/// over the WebSocket.
pub(crate) fn build_auth_event(
    keys: &Keys,
    challenge: &str,
    relay_url: &str,
    api_token: Option<&str>,
) -> Result<nostr::Event, SproutError> {
    let relay: Url =
        relay_url
            .parse()
            .map_err(|e: url::ParseError| SproutError::InternalError {
                message: format!("invalid relay URL: {e}"),
            })?;

    let mut tags = vec![
        Tag::parse(&["relay", relay.as_str()]).map_err(tag_err)?,
        Tag::parse(&["challenge", challenge]).map_err(tag_err)?,
    ];

    // If we have an API token, add it as an auth_token tag.
    if let Some(token) = api_token {
        tags.push(Tag::parse(&["auth_token", token]).map_err(tag_err)?);
    }

    EventBuilder::new(Kind::Authentication, "", tags)
        .sign_with_keys(keys)
        .map_err(|e| SproutError::InvalidKey {
            message: format!("NIP-42 signing failed: {e}"),
        })
}

fn tag_err(e: nostr::event::tag::Error) -> SproutError {
    SproutError::InternalError {
        message: format!("tag error: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_valid_auth_event() {
        let keys = Keys::generate();
        let event =
            build_auth_event(&keys, "test-challenge", "wss://relay.example.com", None).unwrap();
        assert_eq!(event.kind, nostr::Kind::Authentication);

        let tags: Vec<Vec<String>> = event
            .tags
            .iter()
            .map(|t| t.as_slice().iter().map(|s| s.to_string()).collect())
            .collect();

        assert!(tags
            .iter()
            .any(|t| t[0] == "challenge" && t[1] == "test-challenge"));
        assert!(tags.iter().any(|t| t[0] == "relay"));
    }

    #[test]
    fn includes_auth_token_tag() {
        let keys = Keys::generate();
        let event = build_auth_event(
            &keys,
            "challenge",
            "wss://relay.example.com",
            Some("sprout_abc123"),
        )
        .unwrap();

        let tags: Vec<Vec<String>> = event
            .tags
            .iter()
            .map(|t| t.as_slice().iter().map(|s| s.to_string()).collect())
            .collect();

        assert!(tags
            .iter()
            .any(|t| t[0] == "auth_token" && t[1] == "sprout_abc123"));
    }
}
