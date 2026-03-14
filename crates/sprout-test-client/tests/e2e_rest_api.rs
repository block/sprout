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

/// Make an authenticated PUT request using the `X-Pubkey` dev-mode header.
async fn authed_put(
    client: &Client,
    url: &str,
    pubkey_hex: &str,
    body: serde_json::Value,
) -> reqwest::Response {
    client
        .put(url)
        .header("X-Pubkey", pubkey_hex)
        .json(&body)
        .send()
        .await
        .unwrap_or_else(|e| panic!("HTTP PUT {url} failed: {e}"))
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
    let filter = Filter::new().kind(Kind::Custom(9)).custom_tag(
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
    let event = nostr::EventBuilder::new(Kind::Custom(9), &content, [h_tag])
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

/// PUT /api/presence sets the user's presence and can be read back.
#[tokio::test]
#[ignore]
async fn test_set_presence_online() {
    let client = http_client();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();

    // Set presence to "online" via REST.
    let url = format!("{}/api/presence", relay_http_url());
    let resp = authed_put(
        &client,
        &url,
        &pubkey_hex,
        serde_json::json!({"status": "online"}),
    )
    .await;
    assert_eq!(resp.status(), 200, "PUT /api/presence should return 200");
    let body: serde_json::Value = resp.json().await.expect("JSON");
    assert_eq!(body["status"].as_str(), Some("online"));
    assert_eq!(
        body["ttl_seconds"].as_u64(),
        Some(90),
        "online presence should have 90s TTL"
    );

    // Verify via GET.
    let get_url = format!("{}/api/presence?pubkeys={pubkey_hex}", relay_http_url());
    let resp = authed_get(&client, &get_url, &pubkey_hex).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("JSON");
    assert_eq!(
        body[&pubkey_hex].as_str(),
        Some("online"),
        "presence should be 'online' after PUT"
    );
}

/// PUT /api/presence with "away" then "offline" updates and clears presence.
#[tokio::test]
#[ignore]
async fn test_set_presence_away_and_offline() {
    let client = http_client();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();
    let url = format!("{}/api/presence", relay_http_url());

    // Set to "away".
    let resp = authed_put(
        &client,
        &url,
        &pubkey_hex,
        serde_json::json!({"status": "away"}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let away_body: serde_json::Value = resp.json().await.expect("JSON");
    assert_eq!(
        away_body["status"].as_str(),
        Some("away"),
        "PUT response should echo 'away'"
    );
    assert_eq!(
        away_body["ttl_seconds"].as_u64(),
        Some(90),
        "away should have 90s TTL"
    );

    // Verify "away".
    let get_url = format!("{}/api/presence?pubkeys={pubkey_hex}", relay_http_url());
    let resp = authed_get(&client, &get_url, &pubkey_hex).await;
    let body: serde_json::Value = resp.json().await.expect("JSON");
    assert_eq!(body[&pubkey_hex].as_str(), Some("away"));

    // Set to "offline" — should clear presence.
    let resp = authed_put(
        &client,
        &url,
        &pubkey_hex,
        serde_json::json!({"status": "offline"}),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let offline_body: serde_json::Value = resp.json().await.expect("JSON");
    assert_eq!(
        offline_body["status"].as_str(),
        Some("offline"),
        "PUT response should echo 'offline'"
    );
    assert_eq!(
        offline_body["ttl_seconds"].as_u64(),
        Some(0),
        "offline should have 0 TTL"
    );

    // Verify "offline" (key deleted from Redis, defaults to "offline").
    let resp = authed_get(&client, &get_url, &pubkey_hex).await;
    let body: serde_json::Value = resp.json().await.expect("JSON");
    assert_eq!(body[&pubkey_hex].as_str(), Some("offline"));
}

/// PUT /api/presence with an invalid status returns 422 with standard error envelope.
#[tokio::test]
#[ignore]
async fn test_set_presence_invalid_status() {
    let client = http_client();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();

    let url = format!("{}/api/presence", relay_http_url());
    let resp = authed_put(
        &client,
        &url,
        &pubkey_hex,
        serde_json::json!({"status": "invisible"}),
    )
    .await;
    assert_eq!(
        resp.status(),
        422,
        "invalid enum variant should return 422 Unprocessable Entity"
    );
    let body: serde_json::Value = resp.json().await.expect("JSON");
    assert!(
        body["error"].as_str().is_some(),
        "422 response should contain standard error envelope, got: {body}"
    );
}

/// PUT /api/presence without auth returns 401.
#[tokio::test]
#[ignore]
async fn test_set_presence_requires_auth() {
    let client = http_client();
    let url = format!("{}/api/presence", relay_http_url());
    let resp = client
        .put(&url)
        .json(&serde_json::json!({"status": "online"}))
        .send()
        .await
        .expect("request failed");
    assert_eq!(
        resp.status(),
        401,
        "PUT /api/presence without auth should return 401"
    );
    let body: serde_json::Value = resp.json().await.expect("JSON");
    assert_eq!(
        body["error"].as_str(),
        Some("authentication required"),
        "401 response should contain standard error envelope"
    );
}

/// PUT /api/presence with missing status field returns a structured error.
#[tokio::test]
#[ignore]
async fn test_set_presence_missing_field() {
    let client = http_client();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();

    let url = format!("{}/api/presence", relay_http_url());
    let resp = authed_put(&client, &url, &pubkey_hex, serde_json::json!({})).await;
    assert_eq!(
        resp.status(),
        422,
        "missing required field should return 422"
    );
    let body: serde_json::Value = resp.json().await.expect("JSON");
    assert!(
        body["error"].as_str().is_some(),
        "422 response should contain standard error envelope, got: {body}"
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
    let event = nostr::EventBuilder::new(Kind::Custom(9), &content, [h_tag])
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
    assert_eq!(
        body["about"].as_str(),
        Some("Testing public profile endpoint")
    );
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
        .get(format!(
            "{}/api/users/{}/profile",
            relay_http_url(),
            some_pubkey
        ))
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
        &format!(
            "{}/api/users/{}/profile",
            relay_http_url(),
            "not-a-valid-hex"
        ),
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
        .send()
        .await
        .expect("PUT alice");

    client
        .put(format!("{}/api/users/me/profile", relay_http_url()))
        .header("X-Pubkey", &hex_b)
        .json(&serde_json::json!({"display_name": "Bob Batch"}))
        .send()
        .await
        .expect("PUT bob");

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
        missing
            .iter()
            .any(|v| v.as_str() == Some(&unknown_hex.to_lowercase())),
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
/// Also verifies that 64-char non-hex strings (e.g. "g" * 64) go to missing.
#[tokio::test]
#[ignore]
async fn test_batch_profiles_invalid_length_in_missing() {
    let client = http_client();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();

    let non_hex_64 = "g".repeat(64);
    let resp = authed_post_json(
        &client,
        &format!("{}/api/users/batch", relay_http_url()),
        &pubkey_hex,
        serde_json::json!({"pubkeys": ["tooshort", "x".repeat(100), non_hex_64]}),
    )
    .await;

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("json");
    let missing = body["missing"].as_array().expect("missing");
    assert_eq!(
        missing.len(),
        3,
        "wrong-length and 64-char non-hex inputs should all be in missing"
    );

    let missing_strs: Vec<&str> = missing.iter().filter_map(|v| v.as_str()).collect();
    assert!(
        missing_strs.contains(&"tooshort"),
        "short input should be in missing"
    );
    assert!(
        missing_strs.iter().any(|s| s.len() == 100),
        "too-long input should be in missing"
    );
    assert!(
        missing_strs.contains(&"g".repeat(64).as_str()),
        "64-char non-hex should be in missing"
    );
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
        .send()
        .await
        .expect("PUT");

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
        .get(format!(
            "{}/.well-known/nostr.json?name=nonexistent",
            relay_http_url()
        ))
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
    assert!(
        cors.is_some(),
        "NIP-05 must have Access-Control-Allow-Origin header"
    );
    assert_eq!(cors.unwrap().to_str().unwrap(), "*");
}

/// Full round-trip: set nip05_handle via PUT, then verify via /.well-known/nostr.json.
#[tokio::test]
#[ignore]
async fn test_nip05_round_trip_set_and_lookup() {
    let client = http_client();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();

    // Set NIP-05 handle — use "testuser@localhost" since relay_url is ws://localhost:3000
    let unique_name = format!("nip05test{}", &pubkey_hex[..8]);
    let handle = format!("{}@localhost", unique_name);
    let resp = authed_put(
        &client,
        &format!("{}/api/users/me/profile", relay_http_url()),
        &pubkey_hex,
        serde_json::json!({"nip05_handle": handle}),
    )
    .await;
    assert_eq!(resp.status(), 200, "set nip05_handle should succeed");

    // Query NIP-05 endpoint
    let resp = client
        .get(format!(
            "{}/.well-known/nostr.json?name={}",
            relay_http_url(),
            unique_name
        ))
        .send()
        .await
        .expect("nip05 request");
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.expect("json");
    let names = body["names"].as_object().expect("names map");
    assert!(
        names.contains_key(&unique_name),
        "NIP-05 should resolve the name. Got: {:?}",
        names
    );
    let resolved_pubkey = names[&unique_name].as_str().expect("pubkey string");
    assert_eq!(
        resolved_pubkey, pubkey_hex,
        "NIP-05 resolved pubkey should match"
    );
}

/// Setting nip05_handle to empty string clears it.
#[tokio::test]
#[ignore]
async fn test_nip05_clear_handle() {
    let client = http_client();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();

    let unique_name = format!("cleartest{}", &pubkey_hex[..8]);
    let handle = format!("{}@localhost", unique_name);

    // Set handle
    let resp = authed_put(
        &client,
        &format!("{}/api/users/me/profile", relay_http_url()),
        &pubkey_hex,
        serde_json::json!({"nip05_handle": handle}),
    )
    .await;
    assert_eq!(resp.status(), 200);

    // Clear handle
    let resp = authed_put(
        &client,
        &format!("{}/api/users/me/profile", relay_http_url()),
        &pubkey_hex,
        serde_json::json!({"nip05_handle": ""}),
    )
    .await;
    assert_eq!(resp.status(), 200);

    // Verify cleared — NIP-05 should no longer resolve
    let resp = client
        .get(format!(
            "{}/.well-known/nostr.json?name={}",
            relay_http_url(),
            unique_name
        ))
        .send()
        .await
        .expect("nip05 request");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("json");
    let names = body["names"].as_object().expect("names map");
    assert!(
        !names.contains_key(&unique_name),
        "NIP-05 should NOT resolve after clearing. Got: {:?}",
        names
    );
}

/// Duplicate nip05_handle returns 409 Conflict.
#[tokio::test]
#[ignore]
async fn test_nip05_duplicate_handle_conflict() {
    let client = http_client();
    let keys_a = Keys::generate();
    let pubkey_a = keys_a.public_key().to_hex();
    let keys_b = Keys::generate();
    let pubkey_b = keys_b.public_key().to_hex();

    let unique_name = format!("duptest{}", &pubkey_a[..8]);
    let handle = format!("{}@localhost", unique_name);

    // User A sets handle
    let resp = authed_put(
        &client,
        &format!("{}/api/users/me/profile", relay_http_url()),
        &pubkey_a,
        serde_json::json!({"nip05_handle": handle}),
    )
    .await;
    assert_eq!(resp.status(), 200);

    // User B tries same handle → 409
    let resp = authed_put(
        &client,
        &format!("{}/api/users/me/profile", relay_http_url()),
        &pubkey_b,
        serde_json::json!({"nip05_handle": handle}),
    )
    .await;
    assert_eq!(
        resp.status(),
        409,
        "duplicate nip05_handle should return 409 Conflict"
    );
}

// ── Agent Channel Protection tests ───────────────────────────────────────────

/// PUT /api/users/me/channel-add-policy updates the policy and returns the new value.
/// Cycles through owner_only → nobody → anyone to verify each round-trip.
#[tokio::test]
#[ignore]
async fn test_set_channel_add_policy_returns_updated_policy() {
    let client = http_client();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();
    let url = format!("{}/api/users/me/channel-add-policy", relay_http_url());

    // Set to owner_only
    let resp = authed_put(
        &client,
        &url,
        &pubkey_hex,
        serde_json::json!({ "channel_add_policy": "owner_only" }),
    )
    .await;
    assert_eq!(resp.status(), 200, "expected 200 for owner_only");
    let body: serde_json::Value = resp.json().await.expect("json");
    assert_eq!(
        body["channel_add_policy"].as_str(),
        Some("owner_only"),
        "body should reflect owner_only"
    );

    // Set to nobody
    let resp = authed_put(
        &client,
        &url,
        &pubkey_hex,
        serde_json::json!({ "channel_add_policy": "nobody" }),
    )
    .await;
    assert_eq!(resp.status(), 200, "expected 200 for nobody");
    let body: serde_json::Value = resp.json().await.expect("json");
    assert_eq!(
        body["channel_add_policy"].as_str(),
        Some("nobody"),
        "body should reflect nobody"
    );

    // Set to anyone
    let resp = authed_put(
        &client,
        &url,
        &pubkey_hex,
        serde_json::json!({ "channel_add_policy": "anyone" }),
    )
    .await;
    assert_eq!(resp.status(), 200, "expected 200 for anyone");
    let body: serde_json::Value = resp.json().await.expect("json");
    assert_eq!(
        body["channel_add_policy"].as_str(),
        Some("anyone"),
        "body should reflect anyone"
    );
}

/// PUT /api/users/me/channel-add-policy rejects unknown policy values with 400.
#[tokio::test]
#[ignore]
async fn test_set_channel_add_policy_rejects_invalid() {
    let client = http_client();
    let keys = Keys::generate();
    let pubkey_hex = keys.public_key().to_hex();

    let resp = authed_put(
        &client,
        &format!("{}/api/users/me/channel-add-policy", relay_http_url()),
        &pubkey_hex,
        serde_json::json!({ "channel_add_policy": "invalid_value" }),
    )
    .await;
    assert_eq!(resp.status(), 400, "invalid policy value should return 400");
}

/// Default policy (no policy set) allows anyone to add the agent to a channel.
#[tokio::test]
#[ignore]
async fn test_add_member_default_policy_allows_anyone() {
    let client = http_client();
    let owner_keys = Keys::generate();
    let owner_hex = owner_keys.public_key().to_hex();
    let agent_keys = Keys::generate();
    let agent_hex = agent_keys.public_key().to_hex();

    // Owner creates a channel (agent has no policy set — default "anyone")
    let channel_name = format!("e2e-policy-default-{}", uuid::Uuid::new_v4().simple());
    let create_resp = authed_post_json(
        &client,
        &format!("{}/api/channels", relay_http_url()),
        &owner_hex,
        serde_json::json!({
            "name": channel_name,
            "channel_type": "stream",
            "visibility": "open"
        }),
    )
    .await;
    assert_eq!(create_resp.status(), 201, "channel creation should succeed");
    let channel: serde_json::Value = create_resp.json().await.expect("json");
    let channel_id = channel["id"].as_str().expect("channel id");

    // Owner adds agent — default policy should allow it
    let resp = authed_post_json(
        &client,
        &format!("{}/api/channels/{channel_id}/members", relay_http_url()),
        &owner_hex,
        serde_json::json!({ "pubkeys": [agent_hex], "role": "member" }),
    )
    .await;
    assert_eq!(resp.status(), 200, "add member should return 200");
    let body: serde_json::Value = resp.json().await.expect("json");
    let added = body["added"].as_array().expect("added array");
    assert!(
        added.iter().any(|v| v.as_str() == Some(&agent_hex)),
        "agent should be in added list; got body: {body}"
    );
}

/// owner_only policy blocks a non-owner stranger from adding the agent.
#[tokio::test]
#[ignore]
async fn test_add_member_owner_only_blocks_non_owner() {
    let client = http_client();
    let agent_keys = Keys::generate();
    let agent_hex = agent_keys.public_key().to_hex();
    let stranger_keys = Keys::generate();
    let stranger_hex = stranger_keys.public_key().to_hex();

    // Agent sets policy to owner_only
    let resp = authed_put(
        &client,
        &format!("{}/api/users/me/channel-add-policy", relay_http_url()),
        &agent_hex,
        serde_json::json!({ "channel_add_policy": "owner_only" }),
    )
    .await;
    assert_eq!(resp.status(), 200, "policy set should succeed");

    // Stranger creates a channel
    let channel_name = format!("e2e-owner-only-block-{}", uuid::Uuid::new_v4().simple());
    let create_resp = authed_post_json(
        &client,
        &format!("{}/api/channels", relay_http_url()),
        &stranger_hex,
        serde_json::json!({
            "name": channel_name,
            "channel_type": "stream",
            "visibility": "open"
        }),
    )
    .await;
    assert_eq!(create_resp.status(), 201, "channel creation should succeed");
    let channel: serde_json::Value = create_resp.json().await.expect("json");
    let channel_id = channel["id"].as_str().expect("channel id");

    // Stranger tries to add agent — should be blocked by owner_only policy
    let resp = authed_post_json(
        &client,
        &format!("{}/api/channels/{channel_id}/members", relay_http_url()),
        &stranger_hex,
        serde_json::json!({ "pubkeys": [agent_hex], "role": "member" }),
    )
    .await;
    assert_eq!(
        resp.status(),
        200,
        "add member returns 200 even on policy block"
    );
    let body: serde_json::Value = resp.json().await.expect("json");
    let errors = body["errors"].as_array().expect("errors array");
    let agent_error = errors
        .iter()
        .find(|e| e["pubkey"].as_str() == Some(&agent_hex))
        .unwrap_or_else(|| panic!("agent should be in errors; got body: {body}"));
    let error_msg = agent_error["error"].as_str().unwrap_or("");
    assert!(
        error_msg.contains("policy:owner_only"),
        "error should mention policy:owner_only; got: {error_msg}"
    );
}

/// owner_only policy: when no owner is set, even the channel creator is blocked.
///
/// Full AC-2 coverage (owner IS set and can add) requires setting `agent_owner_pubkey`
/// via `sprout-admin set-agent-owner`, which is not available in e2e REST tests.
/// The owner bypass path is covered by DB-level unit tests in sprout-db.
///
/// What we CAN verify here: with `owner_only` set and no owner configured, the policy
/// blocks everyone — including the channel creator — because NULL owner matches nobody.
#[tokio::test]
#[ignore]
async fn test_add_member_owner_only_null_owner_blocks_all() {
    let client = http_client();
    let agent_keys = Keys::generate();
    let agent_hex = agent_keys.public_key().to_hex();
    let channel_creator_keys = Keys::generate();
    let channel_creator_hex = channel_creator_keys.public_key().to_hex();

    // Agent sets policy to owner_only (no agent_owner_pubkey is set — NULL by default).
    let resp = authed_put(
        &client,
        &format!("{}/api/users/me/channel-add-policy", relay_http_url()),
        &agent_hex,
        serde_json::json!({ "channel_add_policy": "owner_only" }),
    )
    .await;
    assert_eq!(resp.status(), 200, "policy set should succeed");

    // Channel creator creates a channel.
    let channel_name = format!("e2e-owner-only-noowner-{}", uuid::Uuid::new_v4().simple());
    let create_resp = authed_post_json(
        &client,
        &format!("{}/api/channels", relay_http_url()),
        &channel_creator_hex,
        serde_json::json!({
            "name": channel_name,
            "channel_type": "stream",
            "visibility": "open"
        }),
    )
    .await;
    assert_eq!(create_resp.status(), 201, "channel creation should succeed");
    let channel: serde_json::Value = create_resp.json().await.expect("json");
    let channel_id = channel["id"].as_str().expect("channel id");

    // Channel creator tries to add agent — owner_only with NULL owner blocks everyone.
    let resp = authed_post_json(
        &client,
        &format!("{}/api/channels/{channel_id}/members", relay_http_url()),
        &channel_creator_hex,
        serde_json::json!({ "pubkeys": [agent_hex], "role": "member" }),
    )
    .await;
    assert_eq!(
        resp.status(),
        200,
        "add member returns 200 even on policy block"
    );
    let body: serde_json::Value = resp.json().await.expect("json");
    let errors = body["errors"].as_array().expect("errors array");
    let agent_error = errors
        .iter()
        .find(|e| e["pubkey"].as_str() == Some(&agent_hex))
        .unwrap_or_else(|| panic!("agent should be in errors; got body: {body}"));
    let error_msg = agent_error["error"].as_str().unwrap_or("");
    assert!(
        error_msg.contains("policy:owner_only"),
        "error should mention policy:owner_only when no owner is set; got: {error_msg}"
    );
}

/// nobody policy blocks all external callers from adding the agent.
#[tokio::test]
#[ignore]
async fn test_add_member_nobody_blocks_all() {
    let client = http_client();
    let agent_keys = Keys::generate();
    let agent_hex = agent_keys.public_key().to_hex();
    let stranger_keys = Keys::generate();
    let stranger_hex = stranger_keys.public_key().to_hex();

    // Agent sets policy to nobody
    let resp = authed_put(
        &client,
        &format!("{}/api/users/me/channel-add-policy", relay_http_url()),
        &agent_hex,
        serde_json::json!({ "channel_add_policy": "nobody" }),
    )
    .await;
    assert_eq!(resp.status(), 200, "policy set should succeed");

    // Stranger creates a channel
    let channel_name = format!("e2e-nobody-block-{}", uuid::Uuid::new_v4().simple());
    let create_resp = authed_post_json(
        &client,
        &format!("{}/api/channels", relay_http_url()),
        &stranger_hex,
        serde_json::json!({
            "name": channel_name,
            "channel_type": "stream",
            "visibility": "open"
        }),
    )
    .await;
    assert_eq!(create_resp.status(), 201, "channel creation should succeed");
    let channel: serde_json::Value = create_resp.json().await.expect("json");
    let channel_id = channel["id"].as_str().expect("channel id");

    // Stranger tries to add agent — nobody policy blocks all
    let resp = authed_post_json(
        &client,
        &format!("{}/api/channels/{channel_id}/members", relay_http_url()),
        &stranger_hex,
        serde_json::json!({ "pubkeys": [agent_hex], "role": "member" }),
    )
    .await;
    assert_eq!(
        resp.status(),
        200,
        "add member returns 200 even on policy block"
    );
    let body: serde_json::Value = resp.json().await.expect("json");
    let errors = body["errors"].as_array().expect("errors array");
    let agent_error = errors
        .iter()
        .find(|e| e["pubkey"].as_str() == Some(&agent_hex))
        .unwrap_or_else(|| panic!("agent should be in errors; got body: {body}"));
    let error_msg = agent_error["error"].as_str().unwrap_or("");
    assert!(
        error_msg.contains("policy:nobody"),
        "error should mention policy:nobody; got: {error_msg}"
    );
}

/// Self-add bypasses the nobody policy — an agent can always add itself.
#[tokio::test]
#[ignore]
async fn test_self_add_bypasses_policy() {
    let client = http_client();
    let agent_keys = Keys::generate();
    let agent_hex = agent_keys.public_key().to_hex();

    // Agent sets policy to nobody
    let resp = authed_put(
        &client,
        &format!("{}/api/users/me/channel-add-policy", relay_http_url()),
        &agent_hex,
        serde_json::json!({ "channel_add_policy": "nobody" }),
    )
    .await;
    assert_eq!(resp.status(), 200, "policy set should succeed");

    // Agent creates a channel (making agent the owner)
    let channel_name = format!("e2e-self-add-{}", uuid::Uuid::new_v4().simple());
    let create_resp = authed_post_json(
        &client,
        &format!("{}/api/channels", relay_http_url()),
        &agent_hex,
        serde_json::json!({
            "name": channel_name,
            "channel_type": "stream",
            "visibility": "open"
        }),
    )
    .await;
    assert_eq!(create_resp.status(), 201, "channel creation should succeed");
    let channel: serde_json::Value = create_resp.json().await.expect("json");
    let channel_id = channel["id"].as_str().expect("channel id");

    // Agent adds itself — self-add should bypass nobody policy
    let resp = authed_post_json(
        &client,
        &format!("{}/api/channels/{channel_id}/members", relay_http_url()),
        &agent_hex,
        serde_json::json!({ "pubkeys": [agent_hex], "role": "member" }),
    )
    .await;
    assert_eq!(resp.status(), 200, "self-add should return 200");
    let body: serde_json::Value = resp.json().await.expect("json");
    let added = body["added"].as_array().expect("added array");
    assert!(
        added.iter().any(|v| v.as_str() == Some(&agent_hex)),
        "agent should be in added list (self-add bypasses nobody); got body: {body}"
    );
}

/// Batch add with mixed policies: allowed agent goes to added, blocked agent goes to errors.
#[tokio::test]
#[ignore]
async fn test_batch_add_mixed_policies() {
    let client = http_client();
    let owner_keys = Keys::generate();
    let owner_hex = owner_keys.public_key().to_hex();
    let allowed_agent_keys = Keys::generate();
    let allowed_agent_hex = allowed_agent_keys.public_key().to_hex();
    let blocked_agent_keys = Keys::generate();
    let blocked_agent_hex = blocked_agent_keys.public_key().to_hex();

    // Blocked agent sets policy to nobody
    let resp = authed_put(
        &client,
        &format!("{}/api/users/me/channel-add-policy", relay_http_url()),
        &blocked_agent_hex,
        serde_json::json!({ "channel_add_policy": "nobody" }),
    )
    .await;
    assert_eq!(resp.status(), 200, "policy set should succeed");

    // Owner creates a channel (allowed_agent has no policy — default "anyone")
    let channel_name = format!("e2e-batch-mixed-{}", uuid::Uuid::new_v4().simple());
    let create_resp = authed_post_json(
        &client,
        &format!("{}/api/channels", relay_http_url()),
        &owner_hex,
        serde_json::json!({
            "name": channel_name,
            "channel_type": "stream",
            "visibility": "open"
        }),
    )
    .await;
    assert_eq!(create_resp.status(), 201, "channel creation should succeed");
    let channel: serde_json::Value = create_resp.json().await.expect("json");
    let channel_id = channel["id"].as_str().expect("channel id");

    // Owner adds both agents in one batch request
    let resp = authed_post_json(
        &client,
        &format!("{}/api/channels/{channel_id}/members", relay_http_url()),
        &owner_hex,
        serde_json::json!({
            "pubkeys": [allowed_agent_hex, blocked_agent_hex],
            "role": "member"
        }),
    )
    .await;
    assert_eq!(resp.status(), 200, "batch add should return 200");
    let body: serde_json::Value = resp.json().await.expect("json");

    // Allowed agent should be in added
    let added = body["added"].as_array().expect("added array");
    assert!(
        added.iter().any(|v| v.as_str() == Some(&allowed_agent_hex)),
        "allowed_agent should be in added; got body: {body}"
    );

    // Blocked agent should be in errors
    let errors = body["errors"].as_array().expect("errors array");
    let blocked_error = errors
        .iter()
        .find(|e| e["pubkey"].as_str() == Some(&blocked_agent_hex))
        .unwrap_or_else(|| panic!("blocked_agent should be in errors; got body: {body}"));
    let error_msg = blocked_error["error"].as_str().unwrap_or("");
    assert!(
        error_msg.contains("policy:nobody"),
        "error should mention policy:nobody; got: {error_msg}"
    );
}
