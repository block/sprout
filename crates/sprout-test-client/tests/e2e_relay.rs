//! End-to-end integration tests for the Sprout relay.
//!
//! These tests require a running relay instance.  By default they are marked
//! `#[ignore]` so that `cargo test` does not fail in CI when the relay is not
//! available.
//!
//! # Running
//!
//! Start the relay, then run:
//!
//! ```text
//! cargo test --test e2e_relay -- --ignored
//! ```
//!
//! Override the relay URL with the `RELAY_URL` environment variable:
//!
//! ```text
//! RELAY_URL=ws://relay.example.com cargo test --test e2e_relay -- --ignored
//! ```

use std::time::Duration;

use nostr::{Alphabet, Filter, Keys, Kind, SingleLetterTag};
use sprout_test_client::{RelayMessage, SproutTestClient, TestClientError};

fn relay_url() -> String {
    std::env::var("RELAY_URL").unwrap_or_else(|_| "ws://localhost:3000".to_string())
}

fn sub_id(name: &str) -> String {
    format!("e2e-{name}-{}", uuid::Uuid::new_v4())
}

fn relay_http_url() -> String {
    relay_url()
        .replace("wss://", "https://")
        .replace("ws://", "http://")
        .trim_end_matches('/')
        .to_string()
}

/// Create a real channel in the DB via REST so the relay accepts events for it.
async fn create_test_channel(keys: &Keys) -> String {
    let client = reqwest::Client::new();
    let url = format!("{}/api/channels", relay_http_url());
    let pubkey_hex = keys.public_key().to_hex();
    let resp = client
        .post(&url)
        .header("X-Pubkey", &pubkey_hex)
        .json(&serde_json::json!({
            "name": format!("relay-e2e-{}", uuid::Uuid::new_v4()),
            "channel_type": "stream",
            "visibility": "open",
        }))
        .send()
        .await
        .expect("create channel request");
    assert_eq!(resp.status(), 201, "channel creation failed");
    let body: serde_json::Value = resp.json().await.expect("parse channel response");
    body["id"].as_str().expect("channel id").to_string()
}

#[tokio::test]
#[ignore]
async fn test_connect_and_authenticate() {
    let url = relay_url();
    let keys = Keys::generate();

    let client = SproutTestClient::connect(&url, &keys)
        .await
        .expect("should connect and authenticate");

    client.disconnect().await.expect("clean disconnect");
}

#[tokio::test]
#[ignore]
async fn test_send_event_and_receive_via_subscription() {
    let url = relay_url();
    let kind: u16 = 40001;

    let keys_a = Keys::generate();
    let keys_b = Keys::generate();
    let channel = create_test_channel(&keys_a).await;

    let mut client_a = SproutTestClient::connect(&url, &keys_a)
        .await
        .expect("client A connect");

    let sid = sub_id("send-recv");
    let filter = Filter::new()
        .kind(Kind::Custom(kind))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::H), [channel.as_str()]);

    client_a
        .subscribe(&sid, vec![filter])
        .await
        .expect("client A subscribe");

    // Drain EOSE so we're ready for live events.
    client_a
        .collect_until_eose(&sid, Duration::from_secs(5))
        .await
        .expect("client A EOSE");

    let mut client_b = SproutTestClient::connect(&url, &keys_b)
        .await
        .expect("client B connect");

    let content = format!("hello from B at {}", uuid::Uuid::new_v4());
    let ok = client_b
        .send_text_message(&keys_b, &channel, &content, kind)
        .await
        .expect("client B send");

    assert!(ok.accepted, "relay rejected event: {}", ok.message);

    let msg = client_a
        .recv_event(Duration::from_secs(5))
        .await
        .expect("client A recv");

    match msg {
        RelayMessage::Event { event, .. } => {
            assert_eq!(event.content, content);
            assert_eq!(event.pubkey, keys_b.public_key());
        }
        other => panic!("Expected Event, got {other:?}"),
    }

    client_a.disconnect().await.expect("disconnect A");
    client_b.disconnect().await.expect("disconnect B");
}

#[tokio::test]
#[ignore]
async fn test_subscription_filters_by_kind() {
    let url = relay_url();
    let target_kind: u16 = 40001;
    let other_kind: u16 = 40002;

    let keys = Keys::generate();
    let channel = create_test_channel(&keys).await;

    let mut client = SproutTestClient::connect(&url, &keys)
        .await
        .expect("connect");

    let sid = sub_id("filter-kind");
    let filter = Filter::new()
        .kind(Kind::Custom(target_kind))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::H), [channel.as_str()]);

    client
        .subscribe(&sid, vec![filter])
        .await
        .expect("subscribe");
    client
        .collect_until_eose(&sid, Duration::from_secs(5))
        .await
        .expect("EOSE");

    // Send one matching event and one non-matching event.
    let ok_match = client
        .send_text_message(&keys, &channel, "should arrive", target_kind)
        .await
        .expect("send matching");
    assert!(ok_match.accepted, "matching event rejected");

    let ok_other = client
        .send_text_message(&keys, &channel, "should not arrive", other_kind)
        .await
        .expect("send non-matching");
    assert!(ok_other.accepted, "non-matching event rejected");

    // We should receive exactly the matching event.
    let msg = client
        .recv_event(Duration::from_secs(5))
        .await
        .expect("recv event");

    match msg {
        RelayMessage::Event { event, .. } => {
            assert_eq!(event.content, "should arrive");
            assert_eq!(event.kind, Kind::Custom(target_kind));
        }
        other => panic!("Expected Event, got {other:?}"),
    }

    // No second event should arrive within a short timeout.
    let result = client.recv_event(Duration::from_millis(500)).await;
    match result {
        Err(TestClientError::Timeout) => { /* expected */ }
        Ok(RelayMessage::Event { event, .. }) => {
            panic!("Received unexpected event: kind={}", event.kind.as_u16());
        }
        Ok(other) => {
            // EOSE or NOTICE are fine to receive here.
            let _ = other;
        }
        Err(e) => panic!("Unexpected error: {e}"),
    }

    client.disconnect().await.expect("disconnect");
}

#[tokio::test]
#[ignore]
async fn test_close_subscription_stops_delivery() {
    let url = relay_url();
    let kind: u16 = 40001;

    let keys = Keys::generate();
    let channel = create_test_channel(&keys).await;
    let mut client = SproutTestClient::connect(&url, &keys)
        .await
        .expect("connect");

    let sid = sub_id("close-sub");
    let filter = Filter::new()
        .kind(Kind::Custom(kind))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::H), [channel.as_str()]);

    client
        .subscribe(&sid, vec![filter])
        .await
        .expect("subscribe");
    client
        .collect_until_eose(&sid, Duration::from_secs(5))
        .await
        .expect("EOSE");

    client
        .close_subscription(&sid)
        .await
        .expect("close subscription");

    tokio::time::sleep(Duration::from_millis(100)).await;

    let ok = client
        .send_text_message(&keys, &channel, "after close", kind)
        .await
        .expect("send");
    assert!(ok.accepted, "event rejected: {}", ok.message);

    let result = client.recv_event(Duration::from_millis(500)).await;
    match result {
        Err(TestClientError::Timeout) => { /* expected — no delivery */ }
        Ok(RelayMessage::Event { event, .. }) => {
            panic!(
                "Received event after subscription closed: {}",
                event.content
            );
        }
        Ok(_) => { /* NOTICE etc. are fine */ }
        Err(e) => panic!("Unexpected error: {e}"),
    }

    client.disconnect().await.expect("disconnect");
}

#[tokio::test]
#[ignore]
async fn test_unauthenticated_rejected() {
    let url = relay_url();
    let keys = Keys::generate();

    let mut client = SproutTestClient::connect_unauthenticated(&url)
        .await
        .expect("connect unauthenticated");

    tokio::time::sleep(Duration::from_millis(200)).await;

    let result = client
        .send_text_message(&keys, "some-channel", "unauthenticated message", 40001)
        .await;

    match result {
        Ok(ok) => {
            // Relay may accept the send but reject with OK false.
            assert!(
                !ok.accepted,
                "Relay accepted unauthenticated event — expected rejection"
            );
        }
        Err(TestClientError::ConnectionClosed) => {
            // Relay closed the connection — also acceptable.
        }
        Err(TestClientError::Timeout) => {
            // Relay may not respond at all to unauthenticated clients.
            // This is acceptable behaviour.
        }
        Err(e) => panic!("Unexpected error: {e}"),
    }

    let _ = client.disconnect().await;
}

#[tokio::test]
#[ignore]
async fn test_multiple_concurrent_clients() {
    let url = relay_url();
    let kind: u16 = 40001;

    let keys: Vec<Keys> = (0..3).map(|_| Keys::generate()).collect();
    let channel = create_test_channel(&keys[0]).await;

    let mut clients: Vec<SproutTestClient> =
        futures_util::future::try_join_all(keys.iter().map(|k| SproutTestClient::connect(&url, k)))
            .await
            .expect("all clients connect");

    let filter = Filter::new()
        .kind(Kind::Custom(kind))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::H), [channel.as_str()]);

    for (i, client) in clients.iter_mut().enumerate() {
        let sid = format!("multi-{i}");
        client
            .subscribe(&sid, vec![filter.clone()])
            .await
            .expect("subscribe");
        client
            .collect_until_eose(&sid, Duration::from_secs(5))
            .await
            .expect("EOSE");
    }

    let content = format!("broadcast-{}", uuid::Uuid::new_v4());
    let ok = clients[0]
        .send_text_message(&keys[0], &channel, &content, kind)
        .await
        .expect("send");
    assert!(ok.accepted, "event rejected: {}", ok.message);

    for (i, client) in clients.iter_mut().enumerate() {
        let msg = client
            .recv_event(Duration::from_secs(5))
            .await
            .unwrap_or_else(|e| panic!("client {i} recv failed: {e}"));

        match msg {
            RelayMessage::Event { event, .. } => {
                assert_eq!(event.content, content, "client {i} received wrong content");
            }
            other => panic!("client {i}: expected Event, got {other:?}"),
        }
    }

    for client in clients {
        client.disconnect().await.expect("disconnect");
    }
}

/// Historical events must be returned before EOSE.
#[tokio::test]
#[ignore]
async fn test_stored_events_returned_before_eose() {
    let url = relay_url();
    let kind: u16 = 40001;

    let keys = Keys::generate();
    let channel = create_test_channel(&keys).await;
    let mut client = SproutTestClient::connect(&url, &keys)
        .await
        .expect("connect");

    let content = format!("stored-{}", uuid::Uuid::new_v4());
    let ok = client
        .send_text_message(&keys, &channel, &content, kind)
        .await
        .expect("send");
    assert!(ok.accepted, "event rejected: {}", ok.message);

    let sid = sub_id("stored");
    let filter = Filter::new()
        .kind(Kind::Custom(kind))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::H), [channel.as_str()]);

    client
        .subscribe(&sid, vec![filter])
        .await
        .expect("subscribe");

    let events = client
        .collect_until_eose(&sid, Duration::from_secs(5))
        .await
        .expect("collect until EOSE");

    let found = events.iter().any(|e| e.content == content);
    assert!(
        found,
        "Stored event not returned before EOSE. Got: {events:?}"
    );

    client.disconnect().await.expect("disconnect");
}

/// Ephemeral events (kind 20000–29999) must be accepted but not persisted.
#[tokio::test]
#[ignore]
async fn test_ephemeral_event_not_stored() {
    let url = relay_url();
    let ephemeral_kind: u16 = 20001;

    let keys = Keys::generate();
    let channel = create_test_channel(&keys).await;
    let mut client = SproutTestClient::connect(&url, &keys)
        .await
        .expect("connect");

    let ok = client
        .send_text_message(&keys, &channel, "ephemeral content", ephemeral_kind)
        .await
        .expect("send ephemeral");
    assert!(
        ok.accepted,
        "relay rejected ephemeral event: {}",
        ok.message
    );

    let sid = sub_id("ephemeral");
    let filter = Filter::new()
        .kind(Kind::Custom(ephemeral_kind))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::H), [channel.as_str()]);

    client
        .subscribe(&sid, vec![filter])
        .await
        .expect("subscribe");

    let events = client
        .collect_until_eose(&sid, Duration::from_secs(5))
        .await
        .expect("collect until EOSE");

    assert!(
        events.is_empty(),
        "Ephemeral event must not be stored. Got: {events:?}"
    );

    client.disconnect().await.expect("disconnect");
}

/// Kind-22242 AUTH events submitted via EVENT must be rejected.
#[tokio::test]
#[ignore]
async fn test_auth_event_kind_rejected() {
    let url = relay_url();
    let keys = Keys::generate();
    let mut client = SproutTestClient::connect(&url, &keys)
        .await
        .expect("connect");

    let relay_url_parsed: nostr::Url = url.replace("ws://", "http://").parse().unwrap();
    let auth_event = nostr::EventBuilder::auth("fake-challenge", relay_url_parsed)
        .sign_with_keys(&keys)
        .expect("sign");

    let ok = client.send_event(auth_event).await.expect("send");

    assert!(
        !ok.accepted,
        "Relay must reject kind-22242 submitted as EVENT"
    );
    let msg_lower = ok.message.to_lowercase();
    assert!(
        msg_lower.contains("invalid") || msg_lower.contains("auth"),
        "Rejection message should mention 'invalid' or 'auth', got: {}",
        ok.message
    );

    client.disconnect().await.expect("disconnect");
}

/// NIP-11 max_subscriptions (100) must be enforced; 101st REQ gets CLOSED.
#[tokio::test]
#[ignore]
async fn test_subscription_limit_enforced() {
    let url = relay_url();
    let keys = Keys::generate();
    let mut client = SproutTestClient::connect(&url, &keys)
        .await
        .expect("connect");

    for i in 0..100 {
        let sid = format!("limit-sub-{i}");
        let filter = Filter::new().kind(Kind::Custom(40001));
        client
            .subscribe(&sid, vec![filter])
            .await
            .expect("subscribe");
        // Drain EOSE to avoid buffer buildup.
        client
            .collect_until_eose(&sid, Duration::from_secs(5))
            .await
            .expect("EOSE");
    }

    let overflow_sid = sub_id("overflow");
    // Use a kind that no other test writes, so we don't receive stale events.
    let filter = Filter::new().kind(Kind::Custom(49999));
    client
        .subscribe(&overflow_sid, vec![filter])
        .await
        .expect("send REQ");

    // Drain EOSE and stale events from the 100 earlier subscriptions
    // until we receive the CLOSED for the overflow subscription.
    let msg = loop {
        let m = client
            .recv_event(Duration::from_secs(5))
            .await
            .expect("recv CLOSED (or timeout)");
        match &m {
            RelayMessage::Eose { .. } => continue,
            RelayMessage::Event { .. } => continue, // stale event from earlier subs
            _ => break m,
        }
    };

    match msg {
        RelayMessage::Closed {
            subscription_id,
            message,
        } => {
            assert_eq!(subscription_id, overflow_sid);
            assert!(
                message.to_lowercase().contains("too many"),
                "Expected 'too many' in CLOSED message, got: {message}"
            );
        }
        other => panic!("Expected CLOSED for overflow subscription, got {other:?}"),
    }

    client.disconnect().await.expect("disconnect");
}

#[tokio::test]
#[ignore]
async fn test_nip11_relay_info() {
    let ws_url = relay_url();
    let http_url = ws_url
        .replace("ws://", "http://")
        .replace("wss://", "https://");
    let info_url = format!("{http_url}/info");

    let client = reqwest::Client::new();
    let resp = client
        .get(&info_url)
        .send()
        .await
        .expect("HTTP GET /info failed");

    assert!(
        resp.status().is_success(),
        "GET /info returned {}",
        resp.status()
    );

    let body: serde_json::Value = resp.json().await.expect("response is not valid JSON");

    assert!(body.get("name").is_some(), "Missing 'name' field");
    assert!(
        body.get("description").is_some(),
        "Missing 'description' field"
    );
    assert!(
        body.get("supported_nips").is_some(),
        "Missing 'supported_nips' field"
    );
    assert!(body.get("version").is_some(), "Missing 'version' field");

    let limitation = body.get("limitation").expect("Missing 'limitation' field");
    assert_eq!(
        limitation.get("max_subscriptions").and_then(|v| v.as_u64()),
        Some(100),
        "limitation.max_subscriptions must be 100"
    );
    assert!(
        limitation
            .get("auth_required")
            .and_then(|v| v.as_bool())
            .is_some(),
        "limitation.auth_required must be a boolean"
    );
}

/// Events signed by a key other than the authenticated pubkey must be rejected.
#[tokio::test]
#[ignore]
async fn test_pubkey_mismatch_rejected() {
    let url = relay_url();

    let keys_a = Keys::generate();
    let keys_b = Keys::generate();
    let channel = create_test_channel(&keys_a).await;

    let mut client = SproutTestClient::connect(&url, &keys_a)
        .await
        .expect("connect as keys_a");

    let ok = client
        .send_text_message(&keys_b, &channel, "impersonation attempt", 40001)
        .await
        .expect("send");

    assert!(
        !ok.accepted,
        "Relay must reject event signed by a different key than the authenticated pubkey"
    );

    client.disconnect().await.expect("disconnect");
}

#[tokio::test]
#[ignore]
async fn test_eose_sent_for_empty_subscription() {
    let url = relay_url();
    let kind: u16 = 40001;

    let keys = Keys::generate();
    let channel = create_test_channel(&keys).await;
    let mut client = SproutTestClient::connect(&url, &keys)
        .await
        .expect("connect");

    let sid = sub_id("empty-eose");
    let filter = Filter::new()
        .kind(Kind::Custom(kind))
        .custom_tag(SingleLetterTag::lowercase(Alphabet::H), [channel.as_str()])
        .since(nostr::Timestamp::now());

    client
        .subscribe(&sid, vec![filter])
        .await
        .expect("subscribe");

    let events = client
        .collect_until_eose(&sid, Duration::from_secs(5))
        .await
        .expect("collect until EOSE");

    // There should be no stored events (we just created this channel).
    assert!(
        events.is_empty(),
        "Expected no stored events, got: {events:?}"
    );

    client.disconnect().await.expect("disconnect");
}
