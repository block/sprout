//! E2E tests for the Sprout REST API.
//!
//! These tests require a running relay instance with `require_auth_token=false`
//! (dev mode). By default they are marked `#[ignore]` so that `cargo test`
//! does not fail in CI when the relay is not available.
//!
//! # Running
//!
//! Start the relay, then run:
//!
//! ```text
//! RELAY_URL=ws://localhost:3001 cargo test -p sprout-test-client --test e2e_rest_api -- --ignored
//! ```
//!
//! # Auth
//!
//! In dev mode (`require_auth_token=false`) the relay accepts an
//! `X-Pubkey: <hex>` header as authentication. Tests generate fresh
//! [`nostr::Keys`] per test and pass the hex-encoded public key.
//!
//! # Channel setup
//!
//! Each test creates its own channels dynamically via `POST /api/channels`.
//! No pre-seeded data is required — tests are fully self-contained and work
//! against a fresh database. Some tests also send messages via WebSocket to
//! set up search / feed data.

use std::time::Duration;

use nostr::{Alphabet, Filter, Keys, Kind, SingleLetterTag, Tag};
use reqwest::Client;
use sprout_test_client::{RelayMessage, SproutTestClient};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// WebSocket relay URL (e.g. `ws://localhost:3001`).
fn relay_ws_url() -> String {
    std::env::var("RELAY_URL").unwrap_or_else(|_| "ws://localhost:3001".to_string())
}

/// HTTP base URL derived from the WebSocket URL.
fn relay_http_url() -> String {
    relay_ws_url()
        .replace("wss://", "https://")
        .replace("ws://", "http://")
}

/// Build a `reqwest::Client` with a short timeout.
fn http_client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("failed to build HTTP client")
}

/// Make an authenticated GET request using the `X-Pubkey` dev-mode header.
async fn authed_get(client: &Client, url: &str, pubkey_hex: &str) -> reqwest::Response {
    client
        .get(url)
        .header("X-Pubkey", pubkey_hex)
        .send()
        .await
        .unwrap_or_else(|e| panic!("HTTP GET {url} failed: {e}"))
}

/// Make an authenticated POST request using the `X-Pubkey` dev-mode header.
async fn authed_post_json(
    client: &Client,
    url: &str,
    pubkey_hex: &str,
    body: serde_json::Value,
) -> reqwest::Response {
    client
        .post(url)
        .header("X-Pubkey", pubkey_hex)
        .json(&body)
        .send()
        .await
        .unwrap_or_else(|e| panic!("HTTP POST {url} failed: {e}"))
}

// ── Channel tests ─────────────────────────────────────────────────────────────

/// GET /api/channels returns a non-empty list with the expected fields.
#[tokio::test]
#[ignore]
async fn test_list_channels_returns_expected_fields() {
    let client = http_client();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();

    let url = format!("{}/api/channels", relay_http_url());

    // Ensure at least one channel exists (fresh DB may be empty).
    let seed_resp = authed_post_json(
        &client,
        &url,
        &pubkey_hex,
        serde_json::json!({
            "name": format!("list-test-{}", uuid::Uuid::new_v4()),
            "channel_type": "stream",
            "visibility": "open",
            "description": "Seed channel for list test"
        }),
    )
    .await;
    assert_eq!(
        seed_resp.status(),
        201,
        "bootstrap channel creation must succeed"
    );

    let resp = authed_get(&client, &url, &pubkey_hex).await;

    assert_eq!(resp.status(), 200, "expected 200 OK from /api/channels");

    let body: serde_json::Value = resp.json().await.expect("response must be JSON");
    let channels = body
        .as_array()
        .expect("/api/channels must return a JSON array");

    assert!(
        !channels.is_empty(),
        "expected at least one channel in the list"
    );

    for ch in channels {
        assert!(ch.get("id").is_some(), "channel missing 'id' field");
        assert!(ch.get("name").is_some(), "channel missing 'name' field");
        assert!(
            ch.get("channel_type").is_some(),
            "channel missing 'channel_type' field"
        );
        assert!(
            ch.get("description").is_some(),
            "channel missing 'description' field"
        );
    }
}

/// POST /api/channels creates a new channel owned by the requester.
#[tokio::test]
#[ignore]
async fn test_create_channel_returns_channel_record() {
    let client = http_client();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();
    let url = format!("{}/api/channels", relay_http_url());
    let channel_name = format!("desktop-create-{}", uuid::Uuid::new_v4());

    let resp = authed_post_json(
        &client,
        &url,
        &pubkey_hex,
        serde_json::json!({
            "name": channel_name,
            "channel_type": "stream",
            "visibility": "private",
            "description": "Created by the REST API test"
        }),
    )
    .await;

    assert_eq!(
        resp.status(),
        201,
        "expected 201 Created from POST /api/channels"
    );

    let created: serde_json::Value = resp.json().await.expect("response must be JSON");
    assert!(created.get("id").is_some(), "channel missing 'id' field");
    assert_eq!(created["name"].as_str(), Some(channel_name.as_str()));
    assert_eq!(created["channel_type"].as_str(), Some("stream"));
    assert_eq!(
        created["description"].as_str(),
        Some("Created by the REST API test")
    );

    let list_resp = authed_get(&client, &url, &pubkey_hex).await;
    assert_eq!(
        list_resp.status(),
        200,
        "expected 200 OK from /api/channels"
    );
    let channels: Vec<serde_json::Value> = list_resp.json().await.expect("response must be JSON");
    assert!(
        channels
            .iter()
            .any(|channel| channel["id"] == created["id"] && channel["name"] == created["name"]),
        "newly-created private channel should be visible to its creator"
    );
}

/// Open channels are visible to any authenticated user (no prior membership required).
#[tokio::test]
#[ignore]
async fn test_channel_visibility_open_channels_visible_to_all() {
    let client = http_client();

    // Use two completely independent keypairs — neither has any prior membership.
    let keys_a = Keys::generate();
    let keys_b = Keys::generate();

    let url = format!("{}/api/channels", relay_http_url());

    // Create an open channel as keys_a so there is at least one open channel to verify.
    let open_channel_name = format!("e2e-open-{}", uuid::Uuid::new_v4().simple());
    let create_resp = authed_post_json(
        &client,
        &url,
        &keys_a.public_key().to_hex(),
        serde_json::json!({
            "name": open_channel_name,
            "channel_type": "stream",
            "visibility": "open"
        }),
    )
    .await;
    assert_eq!(
        create_resp.status(),
        201,
        "failed to create open channel for visibility test"
    );
    let created_channel: serde_json::Value = create_resp.json().await.expect("JSON");
    let open_channel_id = created_channel["id"]
        .as_str()
        .expect("created channel must have id")
        .to_string();

    let resp_a = authed_get(&client, &url, &keys_a.public_key().to_hex()).await;
    let resp_b = authed_get(&client, &url, &keys_b.public_key().to_hex()).await;

    assert_eq!(resp_a.status(), 200);
    assert_eq!(resp_b.status(), 200);

    let channels_a: Vec<serde_json::Value> = resp_a.json().await.expect("JSON");
    let channels_b: Vec<serde_json::Value> = resp_b.json().await.expect("JSON");

    // Both users should see the open channel we just created.
    let ids_a: std::collections::HashSet<String> = channels_a
        .iter()
        .filter_map(|c| c["id"].as_str().map(|s| s.to_string()))
        .collect();
    let ids_b: std::collections::HashSet<String> = channels_b
        .iter()
        .filter_map(|c| c["id"].as_str().map(|s| s.to_string()))
        .collect();

    assert!(
        ids_a.contains(&open_channel_id),
        "keys_a should see the open channel we created (id={open_channel_id})"
    );
    assert!(
        ids_b.contains(&open_channel_id),
        "keys_b (unrelated user) should also see the open channel (id={open_channel_id})"
    );
}

/// REST-created channel messages must fan out to WebSocket subscribers and
/// carry the canonical channel `h` tag.
#[tokio::test]
#[ignore]
async fn test_rest_send_message_reaches_websocket_channel_subscriptions() {
    let client = http_client();
    let subscriber_keys = Keys::generate();
    let poster_keys = Keys::generate();
    let ws_url = relay_ws_url();

    // Create a fresh open channel for this test.
    let channels_url = format!("{}/api/channels", relay_http_url());
    let channel_name = format!("e2e-rest-live-{}", uuid::Uuid::new_v4().simple());
    let create_resp = authed_post_json(
        &client,
        &channels_url,
        &poster_keys.public_key().to_hex(),
        serde_json::json!({
            "name": channel_name,
            "channel_type": "stream",
            "visibility": "open"
        }),
    )
    .await;
    assert_eq!(create_resp.status(), 201, "failed to create test channel");
    let created: serde_json::Value = create_resp.json().await.expect("JSON");
    let channel_id = created["id"]
        .as_str()
        .expect("channel must have id")
        .to_string();

    let mut subscriber = SproutTestClient::connect(&ws_url, &subscriber_keys)
        .await
        .expect("WebSocket connect failed");

    let sid = format!("rest-live-{}", uuid::Uuid::new_v4().simple());
    let filter = Filter::new().kind(Kind::Custom(40001)).custom_tag(
        SingleLetterTag::lowercase(Alphabet::H),
        [channel_id.as_str()],
    );

    subscriber
        .subscribe(&sid, vec![filter])
        .await
        .expect("subscribe failed");
    subscriber
        .collect_until_eose(&sid, Duration::from_secs(5))
        .await
        .expect("EOSE failed");

    let content = format!("E2E REST live message: {}", uuid::Uuid::new_v4().simple());
    let url = format!("{}/api/channels/{}/messages", relay_http_url(), channel_id);
    let resp = authed_post_json(
        &client,
        &url,
        &poster_keys.public_key().to_hex(),
        serde_json::json!({ "content": content }),
    )
    .await;

    assert_eq!(resp.status(), 200, "expected 200 OK from REST send_message");

    let message = subscriber
        .recv_event(Duration::from_secs(5))
        .await
        .expect("subscriber did not receive live event");

    match message {
        RelayMessage::Event { event, .. } => {
            assert_eq!(event.content, content);

            let tags: Vec<Vec<String>> = event
                .tags
                .iter()
                .map(|tag| tag.as_slice().iter().map(|part| part.to_string()).collect())
                .collect();

            assert!(
                tags.iter()
                    .any(|tag| tag.len() >= 2 && tag[0] == "h" && tag[1] == channel_id),
                "REST-created message is missing the channel h tag: {tags:?}"
            );
            assert!(
                tags.iter().any(|tag| {
                    tag.len() >= 2 && tag[0] == "p" && tag[1] == poster_keys.public_key().to_hex()
                }),
                "REST-created message is missing the sender attribution p tag: {tags:?}"
            );
        }
        other => panic!("expected live EVENT after REST send_message, got {other:?}"),
    }

    subscriber.disconnect().await.expect("disconnect failed");
}

/// GET /api/channels requires authentication — unauthenticated requests are rejected.
#[tokio::test]
#[ignore]
async fn test_channels_requires_auth() {
    let client = http_client();
    let url = format!("{}/api/channels", relay_http_url());

    // No X-Pubkey header.
    let resp = client.get(&url).send().await.expect("request failed");

    assert_eq!(
        resp.status(),
        401,
        "expected 401 Unauthorized when no auth header is provided"
    );
}

// ── Search tests ──────────────────────────────────────────────────────────────

/// GET /api/search returns results scoped to the authenticated user's accessible channels.
#[tokio::test]
#[ignore]
async fn test_search_returns_results_for_open_channels() {
    let client = http_client();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();

    // The seeded data contains messages with "Hello" — use a wildcard search
    // to get all indexed events in accessible channels.
    let url = format!("{}/api/search?q=*", relay_http_url());
    let resp = authed_get(&client, &url, &pubkey_hex).await;

    assert_eq!(resp.status(), 200, "expected 200 OK from /api/search");

    let body: serde_json::Value = resp.json().await.expect("response must be JSON");
    assert!(body.get("hits").is_some(), "response missing 'hits' field");
    assert!(
        body.get("found").is_some(),
        "response missing 'found' field"
    );

    let hits = body["hits"].as_array().expect("'hits' must be an array");

    for hit in hits {
        assert!(hit.get("event_id").is_some(), "hit missing 'event_id'");
        assert!(hit.get("content").is_some(), "hit missing 'content'");
        assert!(hit.get("kind").is_some(), "hit missing 'kind'");
        assert!(hit.get("pubkey").is_some(), "hit missing 'pubkey'");
        assert!(hit.get("channel_id").is_some(), "hit missing 'channel_id'");
    }
}

/// GET /api/search with a specific query returns only matching events.
#[tokio::test]
#[ignore]
async fn test_search_returns_indexed_event() {
    let client = http_client();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();
    let ws_url = relay_ws_url();

    // Create a channel for this test so the event is accepted by the relay.
    let channels_url = format!("{}/api/channels", relay_http_url());
    let channel_name = format!("e2e-search-{}", uuid::Uuid::new_v4().simple());
    let create_resp = authed_post_json(
        &client,
        &channels_url,
        &pubkey_hex,
        serde_json::json!({
            "name": channel_name,
            "channel_type": "stream",
            "visibility": "open"
        }),
    )
    .await;
    assert_eq!(create_resp.status(), 201, "failed to create test channel");
    let created: serde_json::Value = create_resp.json().await.expect("JSON");
    let channel_id = created["id"]
        .as_str()
        .expect("channel must have id")
        .to_string();

    let unique_token = format!("e2e-search-{}", uuid::Uuid::new_v4().simple());
    let content = format!("E2E REST search test marker: {unique_token}");

    let mut ws_client = SproutTestClient::connect(&ws_url, &keys)
        .await
        .expect("WebSocket connect failed");

    let h_tag = Tag::parse(&["h", &channel_id]).expect("tag parse failed");
    let event = nostr::EventBuilder::new(Kind::Custom(40001), &content, [h_tag])
        .sign_with_keys(&keys)
        .expect("event sign failed");

    let ok = ws_client
        .send_event(event)
        .await
        .expect("send_event failed");
    assert!(ok.accepted, "relay rejected event: {}", ok.message);

    ws_client.disconnect().await.ok();

    // Wait briefly for the search index to catch up.
    tokio::time::sleep(Duration::from_millis(500)).await;

    // The unique_token is UUID simple format (hex only) — safe to use directly in the URL.
    let url = format!("{}/api/search?q={unique_token}", relay_http_url());
    let resp = authed_get(&client, &url, &pubkey_hex).await;

    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.expect("JSON");
    let hits = body["hits"].as_array().expect("hits array");

    assert!(
        !hits.is_empty(),
        "expected at least one search hit for unique token '{unique_token}'"
    );

    let first_content = hits[0]["content"].as_str().unwrap_or("");
    assert!(
        first_content.contains(&unique_token),
        "expected hit content to contain '{unique_token}', got: '{first_content}'"
    );
}

/// GET /api/search with empty query returns all accessible events.
#[tokio::test]
#[ignore]
async fn test_search_empty_query_returns_all() {
    let client = http_client();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();

    let url = format!("{}/api/search", relay_http_url());
    let resp = authed_get(&client, &url, &pubkey_hex).await;

    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.expect("JSON");
    assert!(body["hits"].is_array(), "'hits' must be an array");
    assert!(body["found"].is_number(), "'found' must be a number");
}

// ── Presence tests ────────────────────────────────────────────────────────────

/// GET /api/presence returns "offline" for a pubkey with no presence event.
#[tokio::test]
#[ignore]
async fn test_presence_offline_by_default() {
    let client = http_client();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();

    let url = format!("{}/api/presence?pubkeys={pubkey_hex}", relay_http_url());
    let resp = authed_get(&client, &url, &pubkey_hex).await;

    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.expect("JSON");
    let status = body[&pubkey_hex].as_str().expect("expected string status");
    assert_eq!(status, "offline", "fresh key should be 'offline'");
}

/// Sending a presence event (kind:20001) via WebSocket updates the presence store.
#[tokio::test]
#[ignore]
async fn test_presence_set_and_query() {
    let client = http_client();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();
    let ws_url = relay_ws_url();

    let mut ws_client = SproutTestClient::connect(&ws_url, &keys)
        .await
        .expect("WebSocket connect failed");

    let presence_event = nostr::EventBuilder::new(Kind::Custom(20001), "online", [])
        .sign_with_keys(&keys)
        .expect("event sign failed");

    let ok = ws_client
        .send_event(presence_event)
        .await
        .expect("send_event failed");
    assert!(ok.accepted, "relay rejected presence event: {}", ok.message);

    // Keep the WebSocket connection alive briefly so presence is registered.
    tokio::time::sleep(Duration::from_millis(200)).await;

    let url = format!("{}/api/presence?pubkeys={pubkey_hex}", relay_http_url());
    let resp = authed_get(&client, &url, &pubkey_hex).await;

    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.expect("JSON");
    let status = body[&pubkey_hex].as_str().expect("expected string status");
    assert_eq!(
        status, "online",
        "expected 'online' after sending presence event"
    );

    let offline_event = nostr::EventBuilder::new(Kind::Custom(20001), "offline", [])
        .sign_with_keys(&keys)
        .expect("event sign failed");
    ws_client.send_event(offline_event).await.ok();
    ws_client.disconnect().await.ok();
}

/// GET /api/presence with multiple pubkeys returns a status for each.
#[tokio::test]
#[ignore]
async fn test_presence_bulk_query() {
    let client = http_client();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();

    // Generate two fresh keys — both should be offline.
    let keys_a = Keys::generate();
    let keys_b = Keys::generate();
    let pk_a = keys_a.public_key().to_hex();
    let pk_b = keys_b.public_key().to_hex();

    let url = format!("{}/api/presence?pubkeys={pk_a},{pk_b}", relay_http_url());
    let resp = authed_get(&client, &url, &pubkey_hex).await;

    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.expect("JSON");
    assert!(body.is_object(), "presence response must be an object");

    assert!(
        body.get(&pk_a).is_some(),
        "pk_a missing from presence response"
    );
    assert!(
        body.get(&pk_b).is_some(),
        "pk_b missing from presence response"
    );

    assert_eq!(body[&pk_a].as_str(), Some("offline"));
    assert_eq!(body[&pk_b].as_str(), Some("offline"));
}

/// GET /api/presence with no pubkeys returns an empty object.
#[tokio::test]
#[ignore]
async fn test_presence_empty_pubkeys() {
    let client = http_client();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();

    let url = format!("{}/api/presence", relay_http_url());
    let resp = authed_get(&client, &url, &pubkey_hex).await;

    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.expect("JSON");
    assert!(
        body.as_object().map(|o| o.is_empty()).unwrap_or(false),
        "expected empty object for no pubkeys"
    );
}

// ── Agents tests ──────────────────────────────────────────────────────────────

/// GET /api/agents returns a JSON array with the expected fields.
#[tokio::test]
#[ignore]
async fn test_agents_list() {
    let client = http_client();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();

    let url = format!("{}/api/agents", relay_http_url());
    let resp = authed_get(&client, &url, &pubkey_hex).await;

    assert_eq!(resp.status(), 200, "expected 200 OK from /api/agents");

    let body: serde_json::Value = resp.json().await.expect("response must be JSON");
    let agents = body
        .as_array()
        .expect("/api/agents must return a JSON array");

    for agent in agents {
        assert!(agent.get("pubkey").is_some(), "agent missing 'pubkey'");
        assert!(agent.get("name").is_some(), "agent missing 'name'");
        assert!(agent.get("status").is_some(), "agent missing 'status'");
        assert!(agent.get("channels").is_some(), "agent missing 'channels'");
        assert!(
            agent.get("capabilities").is_some(),
            "agent missing 'capabilities'"
        );

        // 'channels' must be an array.
        assert!(
            agent["channels"].is_array(),
            "agent 'channels' must be an array"
        );
        // 'capabilities' must be an array.
        assert!(
            agent["capabilities"].is_array(),
            "agent 'capabilities' must be an array"
        );
        // 'status' must be a string.
        assert!(
            agent["status"].is_string(),
            "agent 'status' must be a string"
        );
    }
}

/// GET /api/agents requires authentication.
#[tokio::test]
#[ignore]
async fn test_agents_requires_auth() {
    let client = http_client();
    let url = format!("{}/api/agents", relay_http_url());

    let resp = client.get(&url).send().await.expect("request failed");

    assert_eq!(
        resp.status(),
        401,
        "expected 401 Unauthorized when no auth header is provided"
    );
}

/// GET /api/agents only returns agents in channels accessible to the requester.
#[tokio::test]
#[ignore]
async fn test_agents_scoped_to_accessible_channels() {
    let client = http_client();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();

    let url = format!("{}/api/agents", relay_http_url());
    let resp = authed_get(&client, &url, &pubkey_hex).await;

    assert_eq!(resp.status(), 200);

    let agents: Vec<serde_json::Value> = resp.json().await.expect("JSON");

    let channels_url = format!("{}/api/channels", relay_http_url());
    let channels_resp = authed_get(&client, &channels_url, &pubkey_hex).await;
    let channels: Vec<serde_json::Value> = channels_resp.json().await.expect("JSON");
    let accessible_names: std::collections::HashSet<String> = channels
        .iter()
        .filter_map(|c| c["name"].as_str().map(|s| s.to_string()))
        .collect();

    // Every channel listed for each agent must be accessible to this user.
    for agent in &agents {
        let agent_channels = agent["channels"].as_array().expect("channels array");
        for ch in agent_channels {
            let ch_name = ch.as_str().expect("channel name must be a string");
            assert!(
                accessible_names.contains(ch_name),
                "agent channel '{ch_name}' is not in the user's accessible channels"
            );
        }
    }
}

// ── Feed tests ────────────────────────────────────────────────────────────────

/// GET /api/feed returns a structured feed with the expected shape.
///
/// This test is skipped if the relay does not expose `/api/feed` (older builds).
#[tokio::test]
#[ignore]
async fn test_feed_returns_activity() {
    let client = http_client();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();
    let ws_url = relay_ws_url();

    let url = format!("{}/api/feed", relay_http_url());

    // Probe the endpoint — skip gracefully if the relay doesn't have it yet.
    let probe = client
        .get(&url)
        .header("X-Pubkey", &pubkey_hex)
        .send()
        .await
        .expect("probe request failed");

    if probe.status() == 404 {
        eprintln!("SKIP test_feed_returns_activity: /api/feed not available on this relay build");
        return;
    }

    // Create a channel for this test so the event is accepted by the relay.
    let channels_url = format!("{}/api/channels", relay_http_url());
    let channel_name = format!("e2e-feed-{}", uuid::Uuid::new_v4().simple());
    let create_resp = authed_post_json(
        &client,
        &channels_url,
        &pubkey_hex,
        serde_json::json!({
            "name": channel_name,
            "channel_type": "stream",
            "visibility": "open"
        }),
    )
    .await;
    assert_eq!(create_resp.status(), 201, "failed to create test channel");
    let created_channel: serde_json::Value = create_resp.json().await.expect("JSON");
    let channel_id = created_channel["id"]
        .as_str()
        .expect("channel must have id")
        .to_string();

    // Send a message to the open channel so there is activity to return.
    let unique_token = format!("e2e-feed-{}", uuid::Uuid::new_v4().simple());
    let content = format!("E2E feed test: {unique_token}");

    let mut ws_client = SproutTestClient::connect(&ws_url, &keys)
        .await
        .expect("WebSocket connect failed");

    let h_tag = Tag::parse(&["h", &channel_id]).expect("tag parse failed");
    let event = nostr::EventBuilder::new(Kind::Custom(40001), &content, [h_tag])
        .sign_with_keys(&keys)
        .expect("event sign failed");

    let ok = ws_client
        .send_event(event)
        .await
        .expect("send_event failed");
    assert!(ok.accepted, "relay rejected event: {}", ok.message);
    ws_client.disconnect().await.ok();

    // Small delay to let the event propagate.
    tokio::time::sleep(Duration::from_millis(200)).await;

    let resp = authed_get(&client, &url, &pubkey_hex).await;
    assert_eq!(resp.status(), 200, "expected 200 OK from /api/feed");

    let body: serde_json::Value = resp.json().await.expect("response must be JSON");

    let feed = body.get("feed").expect("response missing 'feed' key");
    let meta = body.get("meta").expect("response missing 'meta' key");

    assert!(feed.get("mentions").is_some(), "feed missing 'mentions'");
    assert!(
        feed.get("needs_action").is_some(),
        "feed missing 'needs_action'"
    );
    assert!(feed.get("activity").is_some(), "feed missing 'activity'");
    assert!(
        feed.get("agent_activity").is_some(),
        "feed missing 'agent_activity'"
    );

    assert!(meta.get("since").is_some(), "meta missing 'since'");
    assert!(meta.get("total").is_some(), "meta missing 'total'");
    assert!(
        meta.get("generated_at").is_some(),
        "meta missing 'generated_at'"
    );

    assert!(
        feed["activity"].is_array(),
        "feed 'activity' must be an array"
    );

    // The activity array should contain our message (it's in an open channel).
    let activity = feed["activity"].as_array().expect("activity array");
    let found = activity.iter().any(|item| {
        item["content"]
            .as_str()
            .unwrap_or("")
            .contains(&unique_token)
    });

    assert!(
        found,
        "expected to find our message '{unique_token}' in feed activity"
    );
}

/// GET /api/feed with `types=activity` returns only the activity section.
#[tokio::test]
#[ignore]
async fn test_feed_type_filter() {
    let client = http_client();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();

    let url = format!("{}/api/feed?types=activity", relay_http_url());

    let probe = client
        .get(&url)
        .header("X-Pubkey", &pubkey_hex)
        .send()
        .await
        .expect("probe request failed");

    if probe.status() == 404 {
        eprintln!("SKIP test_feed_type_filter: /api/feed not available on this relay build");
        return;
    }

    assert_eq!(probe.status(), 200);

    let body: serde_json::Value = probe.json().await.expect("JSON");
    let feed = &body["feed"];

    // When filtering to 'activity', the other sections should be empty arrays.
    assert_eq!(
        feed["mentions"].as_array().map(|a| a.len()),
        Some(0),
        "mentions should be empty when types=activity"
    );
    assert_eq!(
        feed["needs_action"].as_array().map(|a| a.len()),
        Some(0),
        "needs_action should be empty when types=activity"
    );
}

/// GET /api/feed requires authentication.
#[tokio::test]
#[ignore]
async fn test_feed_requires_auth() {
    let client = http_client();
    let url = format!("{}/api/feed", relay_http_url());

    let resp = client.get(&url).send().await.expect("request failed");

    // Either 401 (auth required) or 404 (older build without feed route).
    let status = resp.status().as_u16();
    assert!(
        status == 401 || status == 404,
        "expected 401 or 404, got {status}"
    );
}

// ── Auth edge cases ───────────────────────────────────────────────────────────

/// An invalid X-Pubkey header is rejected with 401.
#[tokio::test]
#[ignore]
async fn test_invalid_pubkey_header_rejected() {
    let client = http_client();
    let url = format!("{}/api/channels", relay_http_url());

    let resp = client
        .get(&url)
        .header("X-Pubkey", "not-a-valid-hex-pubkey")
        .send()
        .await
        .expect("request failed");

    assert_eq!(
        resp.status(),
        401,
        "expected 401 for invalid X-Pubkey header"
    );
}

/// A valid X-Pubkey header is accepted and returns 200.
#[tokio::test]
#[ignore]
async fn test_valid_pubkey_header_accepted() {
    let client = http_client();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();

    let url = format!("{}/api/channels", relay_http_url());
    let resp = authed_get(&client, &url, &pubkey_hex).await;

    assert_eq!(resp.status(), 200, "expected 200 for valid X-Pubkey header");
}

// ── Public profile tests ──────────────────────────────────────────────────────

/// GET /api/users/:pubkey/profile returns the profile for a known user.
#[tokio::test]
#[ignore]
async fn test_get_user_profile_returns_known_user() {
    let client = http_client();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();

    // Set a profile first
    let put_resp = client
        .put(format!("{}/api/users/me/profile", relay_http_url()))
        .header("X-Pubkey", &pubkey_hex)
        .json(&serde_json::json!({
            "display_name": "Profile Test User",
            "about": "Testing public profile endpoint"
        }))
        .send()
        .await
        .expect("PUT profile");
    assert_eq!(put_resp.status(), 200);

    // Read it back via the new public endpoint (using a different reader)
    let reader_keys = Keys::generate();
    let reader_hex = reader_keys.public_key().to_hex();

    let resp = authed_get(
        &client,
        &format!("{}/api/users/{}/profile", relay_http_url(), pubkey_hex),
        &reader_hex,
    )
    .await;

    assert_eq!(resp.status(), 200, "expected 200 for known user profile");
    let body: serde_json::Value = resp.json().await.expect("json");
    assert_eq!(body["pubkey"].as_str(), Some(pubkey_hex.as_str()));
    assert_eq!(body["display_name"].as_str(), Some("Profile Test User"));
    assert_eq!(body["about"].as_str(), Some("Testing public profile endpoint"));
}

/// GET /api/users/:pubkey/profile returns 404 for an unknown user.
#[tokio::test]
#[ignore]
async fn test_get_user_profile_returns_404_for_unknown() {
    let client = http_client();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();
    // Use a pubkey that has never been registered
    let unknown_hex = Keys::generate().public_key().to_hex();

    let resp = authed_get(
        &client,
        &format!("{}/api/users/{}/profile", relay_http_url(), unknown_hex),
        &pubkey_hex,
    )
    .await;

    assert_eq!(resp.status(), 404, "expected 404 for unknown user");
}

/// GET /api/users/:pubkey/profile returns 401 without authentication.
#[tokio::test]
#[ignore]
async fn test_get_user_profile_requires_auth() {
    let client = http_client();
    let some_pubkey = Keys::generate().public_key().to_hex();

    let resp = client
        .get(format!("{}/api/users/{}/profile", relay_http_url(), some_pubkey))
        .send()
        .await
        .expect("GET profile");

    assert_eq!(resp.status(), 401, "expected 401 without auth");
}

/// GET /api/users/:pubkey/profile returns 400 for an invalid pubkey.
#[tokio::test]
#[ignore]
async fn test_get_user_profile_rejects_invalid_pubkey() {
    let client = http_client();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();

    let resp = authed_get(
        &client,
        &format!("{}/api/users/{}/profile", relay_http_url(), "not-a-valid-hex"),
        &pubkey_hex,
    )
    .await;

    assert_eq!(resp.status(), 400, "expected 400 for invalid pubkey hex");
}

/// POST /api/users/batch returns found profiles and a missing list.
#[tokio::test]
#[ignore]
async fn test_batch_profiles_known_and_unknown() {
    let client = http_client();

    // Create two users with profiles
    let keys_a = Keys::generate();
    let hex_a = keys_a.public_key().to_hex();
    let keys_b = Keys::generate();
    let hex_b = keys_b.public_key().to_hex();
    let unknown_hex = Keys::generate().public_key().to_hex();

    client
        .put(format!("{}/api/users/me/profile", relay_http_url()))
        .header("X-Pubkey", &hex_a)
        .json(&serde_json::json!({"display_name": "Alice Batch"}))
        .send().await.expect("PUT alice");

    client
        .put(format!("{}/api/users/me/profile", relay_http_url()))
        .header("X-Pubkey", &hex_b)
        .json(&serde_json::json!({"display_name": "Bob Batch"}))
        .send().await.expect("PUT bob");

    // Batch lookup
    let resp = authed_post_json(
        &client,
        &format!("{}/api/users/batch", relay_http_url()),
        &hex_a,
        serde_json::json!({
            "pubkeys": [hex_a, hex_b, unknown_hex]
        }),
    )
    .await;

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("json");

    let profiles = body["profiles"].as_object().expect("profiles map");
    assert_eq!(profiles.len(), 2, "expected 2 found profiles");
    assert_eq!(
        profiles[&hex_a.to_lowercase()]["display_name"].as_str(),
        Some("Alice Batch")
    );
    assert_eq!(
        profiles[&hex_b.to_lowercase()]["display_name"].as_str(),
        Some("Bob Batch")
    );

    let missing = body["missing"].as_array().expect("missing array");
    assert!(
        missing.iter().any(|v| v.as_str() == Some(&unknown_hex.to_lowercase())),
        "unknown pubkey should be in missing"
    );
}

/// POST /api/users/batch returns 400 when more than 200 pubkeys are submitted.
#[tokio::test]
#[ignore]
async fn test_batch_profiles_rejects_over_200() {
    let client = http_client();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();

    let pubkeys: Vec<String> = (0..201).map(|i| format!("{:064x}", i)).collect();

    let resp = authed_post_json(
        &client,
        &format!("{}/api/users/batch", relay_http_url()),
        &pubkey_hex,
        serde_json::json!({"pubkeys": pubkeys}),
    )
    .await;

    assert_eq!(resp.status(), 400, "expected 400 for >200 pubkeys");
}

/// POST /api/users/batch returns 401 without authentication.
#[tokio::test]
#[ignore]
async fn test_batch_profiles_requires_auth() {
    let client = http_client();

    let resp = client
        .post(format!("{}/api/users/batch", relay_http_url()))
        .json(&serde_json::json!({"pubkeys": ["abc"]}))
        .send()
        .await
        .expect("POST batch");

    assert_eq!(resp.status(), 401, "expected 401 without auth");
}

/// POST /api/users/batch places invalid-length inputs in the missing list.
#[tokio::test]
#[ignore]
async fn test_batch_profiles_invalid_length_in_missing() {
    let client = http_client();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();

    let resp = authed_post_json(
        &client,
        &format!("{}/api/users/batch", relay_http_url()),
        &pubkey_hex,
        serde_json::json!({"pubkeys": ["tooshort", "x".repeat(100)]}),
    )
    .await;

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("json");
    let missing = body["missing"].as_array().expect("missing");
    assert_eq!(missing.len(), 2, "both invalid-length inputs should be in missing");
}

/// POST /api/users/batch normalizes pubkey case before lookup.
#[tokio::test]
#[ignore]
async fn test_batch_profiles_case_normalized() {
    let client = http_client();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();

    // Set profile
    client
        .put(format!("{}/api/users/me/profile", relay_http_url()))
        .header("X-Pubkey", &pubkey_hex)
        .json(&serde_json::json!({"display_name": "Case Test"}))
        .send().await.expect("PUT");

    // Query with uppercase version
    let upper_hex = pubkey_hex.to_uppercase();
    let resp = authed_post_json(
        &client,
        &format!("{}/api/users/batch", relay_http_url()),
        &pubkey_hex,
        serde_json::json!({"pubkeys": [upper_hex]}),
    )
    .await;

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("json");
    let profiles = body["profiles"].as_object().expect("profiles");
    assert_eq!(profiles.len(), 1, "uppercase pubkey should match");
}

// ── NIP-05 tests ──────────────────────────────────────────────────────────────

/// GET /.well-known/nostr.json?name=nonexistent returns empty names and relays.
#[tokio::test]
#[ignore]
async fn test_nip05_returns_empty_for_unknown_name() {
    let client = http_client();

    let resp = client
        .get(format!("{}/.well-known/nostr.json?name=nonexistent", relay_http_url()))
        .send()
        .await
        .expect("GET nip05");

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("json");
    assert_eq!(body["names"].as_object().unwrap().len(), 0);
    assert_eq!(body["relays"].as_object().unwrap().len(), 0);
}

/// GET /.well-known/nostr.json with no name param returns empty names.
#[tokio::test]
#[ignore]
async fn test_nip05_no_name_returns_empty() {
    let client = http_client();

    let resp = client
        .get(format!("{}/.well-known/nostr.json", relay_http_url()))
        .send()
        .await
        .expect("GET nip05");

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("json");
    assert_eq!(body["names"].as_object().unwrap().len(), 0);
}

/// GET /.well-known/nostr.json includes the required CORS header.
#[tokio::test]
#[ignore]
async fn test_nip05_has_cors_header() {
    let client = http_client();

    let resp = client
        .get(format!("{}/.well-known/nostr.json", relay_http_url()))
        .send()
        .await
        .expect("GET nip05");

    assert_eq!(resp.status(), 200);
    let cors = resp.headers().get("access-control-allow-origin");
    assert!(cors.is_some(), "NIP-05 must have Access-Control-Allow-Origin header");
    assert_eq!(cors.unwrap().to_str().unwrap(), "*");
}
