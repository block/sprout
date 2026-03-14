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
    let kind: u16 = 9;

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
    let target_kind: u16 = 9;
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
    let kind: u16 = 9;

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
        .send_text_message(&keys, "some-channel", "unauthenticated message", 9)
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
    let kind: u16 = 9;

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
    let kind: u16 = 9;

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
        let filter = Filter::new().kind(Kind::Custom(9));
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
        .send_text_message(&keys_b, &channel, "impersonation attempt", 9)
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
    let kind: u16 = 9;

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

/// Kind:0 NIP-05 sync regression test.
///
/// Verifies:
/// 1. A valid `nip05` in kind:0 content is synced to the profile and resolvable via NIP-05 endpoint.
/// 2. An off-domain `nip05` in kind:0 content is NOT synced (handle is cleared).
#[tokio::test]
#[ignore]
async fn test_kind0_nip05_sync() {
    let url = relay_url();
    let http = relay_http_url();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();

    // Extract the relay domain from the relay URL for building a valid NIP-05 handle.
    // e.g. "ws://localhost:3000" → "localhost"
    let relay_domain = url
        .trim_start_matches("wss://")
        .trim_start_matches("ws://")
        .split(':')
        .next()
        .unwrap_or("localhost")
        .split('/')
        .next()
        .unwrap_or("localhost")
        .to_lowercase();

    let unique_name = format!("kind0test{}", &pubkey_hex[..8]);
    let valid_handle = format!("{}@{}", unique_name, relay_domain);

    // Step 1: Connect and publish kind:0 with a valid nip05 handle.
    let mut client = SproutTestClient::connect(&url, &keys)
        .await
        .expect("connect");

    let kind0_content = serde_json::json!({
        "display_name": "Kind0 Test User",
        "nip05": valid_handle,
    })
    .to_string();

    let event = nostr::EventBuilder::new(Kind::Custom(0), kind0_content, [])
        .sign_with_keys(&keys)
        .expect("sign kind:0");

    let ok = client.send_event(event).await.expect("send kind:0");
    assert!(
        ok.accepted,
        "kind:0 event should be accepted: {:?}",
        ok.message
    );

    // Give the relay a moment to process the side effect.
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Step 2: Verify the profile has the NIP-05 handle via REST GET.
    let http_client = reqwest::Client::new();
    let profile_resp = http_client
        .get(format!("{}/api/users/{}/profile", http, pubkey_hex))
        .header("X-Pubkey", &pubkey_hex)
        .send()
        .await
        .expect("GET profile");
    assert_eq!(
        profile_resp.status(),
        200,
        "profile should exist after kind:0"
    );
    let profile: serde_json::Value = profile_resp.json().await.expect("profile json");
    assert_eq!(
        profile["nip05_handle"].as_str(),
        Some(valid_handle.as_str()),
        "nip05_handle should be synced from kind:0"
    );

    // Step 3: Verify NIP-05 resolves via /.well-known/nostr.json.
    let nip05_resp = http_client
        .get(format!(
            "{}/.well-known/nostr.json?name={}",
            http, unique_name
        ))
        .send()
        .await
        .expect("GET nostr.json");
    assert_eq!(nip05_resp.status(), 200);
    let nip05_body: serde_json::Value = nip05_resp.json().await.expect("nip05 json");
    let resolved_pubkey = nip05_body["names"][&unique_name].as_str();
    assert_eq!(
        resolved_pubkey,
        Some(pubkey_hex.as_str()),
        "NIP-05 should resolve the pubkey after kind:0 sync"
    );

    // Step 4: Publish another kind:0 with an off-domain nip05 (should be cleared).
    let off_domain_content = serde_json::json!({
        "display_name": "Kind0 Test User",
        "nip05": format!("{}@evil.com", unique_name),
    })
    .to_string();

    let event2 = nostr::EventBuilder::new(Kind::Custom(0), off_domain_content, [])
        .sign_with_keys(&keys)
        .expect("sign kind:0 off-domain");

    let ok2 = client
        .send_event(event2)
        .await
        .expect("send kind:0 off-domain");
    assert!(
        ok2.accepted,
        "off-domain kind:0 should still be accepted (stored but handle cleared)"
    );

    tokio::time::sleep(Duration::from_millis(300)).await;

    // Step 5: Verify the handle was CLEARED (not set to the off-domain value).
    let profile_resp2 = http_client
        .get(format!("{}/api/users/{}/profile", http, pubkey_hex))
        .header("X-Pubkey", &pubkey_hex)
        .send()
        .await
        .expect("GET profile after off-domain kind:0");
    assert_eq!(profile_resp2.status(), 200);
    let profile2: serde_json::Value = profile_resp2.json().await.expect("profile json");
    let handle_after = profile2["nip05_handle"].as_str().unwrap_or("");
    assert!(
        handle_after.is_empty() || handle_after == "null",
        "nip05_handle should be cleared after off-domain kind:0, got: {:?}",
        profile2["nip05_handle"]
    );

    // Step 6: Confirm NIP-05 no longer resolves.
    let nip05_resp2 = http_client
        .get(format!(
            "{}/.well-known/nostr.json?name={}",
            http, unique_name
        ))
        .send()
        .await
        .expect("GET nostr.json after clear");
    let nip05_body2: serde_json::Value = nip05_resp2.json().await.expect("nip05 json");
    assert!(
        nip05_body2["names"][&unique_name].is_null(),
        "NIP-05 should not resolve after handle was cleared"
    );

    client.disconnect().await.expect("disconnect");
}

/// NIP-29 kind 9000 (PUT_USER): default policy ("anyone") allows a third party to add an agent.
#[tokio::test]
#[ignore]
async fn test_nip29_put_user_default_policy_allows() {
    let url = relay_url();

    let channel_owner_keys = Keys::generate();
    let agent_keys = Keys::generate();
    let agent_pubkey_hex = agent_keys.public_key().to_hex();

    // Create a channel owned by channel_owner.
    let channel_id = create_test_channel(&channel_owner_keys).await;

    // Connect as channel_owner.
    let mut ws = SproutTestClient::connect(&url, &channel_owner_keys)
        .await
        .expect("connect as channel_owner");

    // Build kind 9000 PUT_USER event: h = channel_id, p = agent pubkey.
    let h_tag = nostr::Tag::parse(&["h", &channel_id]).expect("h tag");
    let p_tag = nostr::Tag::parse(&["p", &agent_pubkey_hex]).expect("p tag");
    let event = nostr::EventBuilder::new(Kind::Custom(9000), "", [h_tag, p_tag])
        .sign_with_keys(&channel_owner_keys)
        .expect("sign kind 9000");

    let ok = ws.send_event(event).await.expect("send kind 9000");

    assert!(
        ok.accepted,
        "default policy should allow PUT_USER, got: {}",
        ok.message
    );

    ws.disconnect().await.expect("disconnect");
}

/// NIP-29 kind 9000 (PUT_USER): "nobody" policy blocks a third party from adding the agent.
#[tokio::test]
#[ignore]
async fn test_nip29_put_user_nobody_blocks() {
    let url = relay_url();

    let channel_owner_keys = Keys::generate();
    let agent_keys = Keys::generate();
    let agent_pubkey_hex = agent_keys.public_key().to_hex();

    // Set agent's channel_add_policy to "nobody" via REST.
    let http_client = reqwest::Client::new();
    let resp = http_client
        .put(format!(
            "{}/api/users/me/channel-add-policy",
            relay_http_url()
        ))
        .header("X-Pubkey", &agent_pubkey_hex)
        .json(&serde_json::json!({ "channel_add_policy": "nobody" }))
        .send()
        .await
        .expect("set policy request");
    assert_eq!(resp.status(), 200, "set policy failed");

    // Create a channel owned by channel_owner (not the agent).
    let channel_id = create_test_channel(&channel_owner_keys).await;

    // Connect as channel_owner.
    let mut ws = SproutTestClient::connect(&url, &channel_owner_keys)
        .await
        .expect("connect as channel_owner");

    // Build kind 9000 PUT_USER event targeting the agent.
    let h_tag = nostr::Tag::parse(&["h", &channel_id]).expect("h tag");
    let p_tag = nostr::Tag::parse(&["p", &agent_pubkey_hex]).expect("p tag");
    let event = nostr::EventBuilder::new(Kind::Custom(9000), "", [h_tag, p_tag])
        .sign_with_keys(&channel_owner_keys)
        .expect("sign kind 9000");

    let ok = ws.send_event(event).await.expect("send kind 9000");

    assert!(
        !ok.accepted,
        "nobody policy should block PUT_USER, but relay accepted it"
    );
    assert!(
        ok.message.contains("policy:nobody"),
        "rejection message should contain 'policy:nobody', got: {}",
        ok.message
    );

    ws.disconnect().await.expect("disconnect");
}

/// NIP-29 kind 9000 (PUT_USER): self-add bypasses "nobody" policy — an agent can always add itself.
#[tokio::test]
#[ignore]
async fn test_nip29_put_user_self_add_bypasses_policy() {
    let url = relay_url();

    let agent_keys = Keys::generate();
    let agent_pubkey_hex = agent_keys.public_key().to_hex();

    // Set agent's channel_add_policy to "nobody" via REST.
    let http_client = reqwest::Client::new();
    let resp = http_client
        .put(format!(
            "{}/api/users/me/channel-add-policy",
            relay_http_url()
        ))
        .header("X-Pubkey", &agent_pubkey_hex)
        .json(&serde_json::json!({ "channel_add_policy": "nobody" }))
        .send()
        .await
        .expect("set policy request");
    assert_eq!(resp.status(), 200, "set policy failed");

    // Create a channel where the agent is the owner.
    let channel_id = create_test_channel(&agent_keys).await;

    // Connect as agent.
    let mut ws = SproutTestClient::connect(&url, &agent_keys)
        .await
        .expect("connect as agent");

    // Build kind 9000 PUT_USER event where agent targets ITSELF.
    let h_tag = nostr::Tag::parse(&["h", &channel_id]).expect("h tag");
    let p_tag = nostr::Tag::parse(&["p", &agent_pubkey_hex]).expect("p tag");
    let event = nostr::EventBuilder::new(Kind::Custom(9000), "", [h_tag, p_tag])
        .sign_with_keys(&agent_keys)
        .expect("sign kind 9000");

    let ok = ws.send_event(event).await.expect("send kind 9000");

    assert!(
        ok.accepted,
        "self-add should bypass nobody policy, got: {}",
        ok.message
    );

    ws.disconnect().await.expect("disconnect");
}

/// NIP-29 kind 9000: `owner_only` policy blocks third-party PUT_USER.
#[tokio::test]
#[ignore]
async fn test_nip29_put_user_owner_only_blocks() {
    let url = relay_url();

    let channel_owner_keys = Keys::generate();
    let agent_keys = Keys::generate();
    let agent_pubkey_hex = agent_keys.public_key().to_hex();

    // Set agent's channel_add_policy to "owner_only" via REST.
    let http_client = reqwest::Client::new();
    let resp = http_client
        .put(format!(
            "{}/api/users/me/channel-add-policy",
            relay_http_url()
        ))
        .header("X-Pubkey", &agent_pubkey_hex)
        .json(&serde_json::json!({ "channel_add_policy": "owner_only" }))
        .send()
        .await
        .expect("set policy request");
    assert_eq!(resp.status(), 200, "set policy failed");

    // Create a channel owned by channel_owner (not the agent).
    let channel_id = create_test_channel(&channel_owner_keys).await;

    // Connect as channel_owner.
    let mut ws = SproutTestClient::connect(&url, &channel_owner_keys)
        .await
        .expect("connect as channel_owner");

    // Build kind 9000 PUT_USER event targeting the agent.
    let h_tag = nostr::Tag::parse(&["h", &channel_id]).expect("h tag");
    let p_tag = nostr::Tag::parse(&["p", &agent_pubkey_hex]).expect("p tag");
    let event = nostr::EventBuilder::new(Kind::Custom(9000), "", [h_tag, p_tag])
        .sign_with_keys(&channel_owner_keys)
        .expect("sign kind 9000");

    let ok = ws.send_event(event).await.expect("send kind 9000");

    assert!(
        !ok.accepted,
        "owner_only policy should block third-party PUT_USER, but relay accepted it"
    );
    assert!(
        ok.message.contains("policy:owner_only"),
        "rejection message should contain 'policy:owner_only', got: {}",
        ok.message
    );

    ws.disconnect().await.expect("disconnect");
}
