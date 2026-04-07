//! End-to-end tests for NIP-23 long-form content (kind:30023).
//!
//! These tests require a running relay instance. By default they are marked
//! `#[ignore]` so that `cargo test` does not fail in CI when the relay is not
//! available.
//!
//! # Running
//!
//! Start the relay, then run:
//!
//! ```text
//! cargo test --test e2e_long_form -- --ignored
//! ```
//!
//! Override the relay URL with the `RELAY_URL` environment variable:
//!
//! ```text
//! RELAY_URL=ws://relay.example.com cargo test --test e2e_long_form -- --ignored
//! ```

use std::time::Duration;

use nostr::{Alphabet, EventBuilder, Filter, Keys, Kind, SingleLetterTag, Tag, Timestamp};
use sprout_test_client::SproutTestClient;

const KIND_LONG_FORM: u16 = 30023;

fn relay_url() -> String {
    std::env::var("RELAY_URL").unwrap_or_else(|_| "ws://localhost:3000".to_string())
}

fn sub_id(name: &str) -> String {
    format!("e2e-{name}-{}", uuid::Uuid::new_v4())
}

/// Build a kind:30023 event with standard NIP-23 tags.
fn build_long_form_event(
    keys: &Keys,
    d_tag: &str,
    title: &str,
    content: &str,
    extra_tags: Vec<Tag>,
) -> nostr::Event {
    let mut tags = vec![
        Tag::parse(&["d", d_tag]).unwrap(),
        Tag::parse(&["title", title]).unwrap(),
    ];
    tags.extend(extra_tags);
    EventBuilder::new(Kind::Custom(KIND_LONG_FORM), content, tags)
        .sign_with_keys(keys)
        .unwrap()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// kind:30023 events are accepted by the relay.
#[tokio::test]
#[ignore]
async fn test_long_form_accepted() {
    let url = relay_url();
    let keys = Keys::generate();
    let mut client = SproutTestClient::connect(&url, &keys)
        .await
        .expect("connect");

    let event = build_long_form_event(
        &keys,
        "test-article-accept",
        "Test Article",
        "# Hello\n\nThis is a test article.",
        vec![],
    );

    let ok = client.send_event(event).await.expect("send event");
    assert!(
        ok.accepted,
        "relay should accept kind:30023: {}",
        ok.message
    );

    client.disconnect().await.expect("disconnect");
}

/// kind:30023 events are retrievable via REQ with kinds filter.
#[tokio::test]
#[ignore]
async fn test_long_form_retrievable() {
    let url = relay_url();
    let keys = Keys::generate();
    let mut client = SproutTestClient::connect(&url, &keys)
        .await
        .expect("connect");

    let d_tag = format!("retrieve-{}", uuid::Uuid::new_v4().simple());
    let event = build_long_form_event(
        &keys,
        &d_tag,
        "Retrievable Article",
        "# Retrievable\n\nBody text.",
        vec![],
    );
    let event_id = event.id;

    let ok = client.send_event(event).await.expect("send event");
    assert!(ok.accepted, "relay should accept: {}", ok.message);

    // Query back by kind + author
    let sid = sub_id("retrieve");
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_LONG_FORM))
        .author(keys.public_key());
    client
        .subscribe(&sid, vec![filter])
        .await
        .expect("subscribe");

    let events = client
        .collect_until_eose(&sid, Duration::from_secs(5))
        .await
        .expect("collect");

    assert!(
        events.iter().any(|e| e.id == event_id),
        "should find the published article in query results"
    );

    client.disconnect().await.expect("disconnect");
}

/// kind:30023 is stored globally (channel_id = NULL) — stray h-tags are ignored.
/// An event with a stray h-tag should still be retrievable via a global query
/// (no h-tag filter), proving it was stored as global.
#[tokio::test]
#[ignore]
async fn test_long_form_stray_h_tag_ignored() {
    let url = relay_url();
    let keys = Keys::generate();
    let mut client = SproutTestClient::connect(&url, &keys)
        .await
        .expect("connect");

    // Publish with a stray h-tag (a UUID that doesn't correspond to any channel).
    let fake_channel = uuid::Uuid::new_v4().to_string();
    let d_tag = format!("stray-h-{}", uuid::Uuid::new_v4().simple());
    let event = build_long_form_event(
        &keys,
        &d_tag,
        "Stray H-Tag Article",
        "Should be stored globally despite h-tag.",
        vec![Tag::parse(&["h", &fake_channel]).unwrap()],
    );
    let event_id = event.id;

    let ok = client.send_event(event).await.expect("send event");
    assert!(ok.accepted, "relay should accept: {}", ok.message);

    // Query globally (no h-tag filter) — should find the article.
    let sid = sub_id("stray-h");
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_LONG_FORM))
        .author(keys.public_key());
    client
        .subscribe(&sid, vec![filter])
        .await
        .expect("subscribe");

    let events = client
        .collect_until_eose(&sid, Duration::from_secs(5))
        .await
        .expect("collect");

    assert!(
        events.iter().any(|e| e.id == event_id),
        "article with stray h-tag should be retrievable via global query"
    );

    // NOTE: Ideally, querying with #h=<fake_channel> should NOT return the
    // article since it's global. However, the raw h-tag remains on the stored
    // event (Nostr events are signed — tags can't be stripped without breaking
    // the signature), and the read-path filter matching in filter.rs treats
    // explicit h-tags as authoritative. This is a pre-existing limitation
    // affecting all global-only kinds (0, 1, 3, 30023) and should be fixed
    // in the filter layer as a follow-up.

    client.disconnect().await.expect("disconnect");
}

/// NIP-33 replacement: publishing a newer kind:30023 with the same d-tag replaces the old one.
#[tokio::test]
#[ignore]
async fn test_long_form_nip33_replacement() {
    let url = relay_url();
    let keys = Keys::generate();
    let mut client = SproutTestClient::connect(&url, &keys)
        .await
        .expect("connect");

    let d_tag = format!("replace-{}", uuid::Uuid::new_v4().simple());

    // Publish v1
    let v1 = build_long_form_event(&keys, &d_tag, "Article v1", "Version 1 content.", vec![]);
    let ok1 = client.send_event(v1).await.expect("send v1");
    assert!(ok1.accepted, "v1 should be accepted: {}", ok1.message);

    // Small delay to ensure different created_at timestamps
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Publish v2 with the same d-tag
    let v2 = build_long_form_event(
        &keys,
        &d_tag,
        "Article v2",
        "Version 2 content — updated.",
        vec![],
    );
    let v2_id = v2.id;
    let ok2 = client.send_event(v2).await.expect("send v2");
    assert!(ok2.accepted, "v2 should be accepted: {}", ok2.message);

    // Query — should only get v2 (v1 replaced)
    let sid = sub_id("replace");
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_LONG_FORM))
        .author(keys.public_key())
        .custom_tag(SingleLetterTag::lowercase(Alphabet::D), [d_tag.as_str()]);
    client
        .subscribe(&sid, vec![filter])
        .await
        .expect("subscribe");

    let events = client
        .collect_until_eose(&sid, Duration::from_secs(5))
        .await
        .expect("collect");

    assert_eq!(
        events.len(),
        1,
        "should have exactly one event after replacement"
    );
    assert_eq!(events[0].id, v2_id, "surviving event should be v2");
    assert!(
        events[0].content.contains("Version 2"),
        "content should be v2"
    );

    client.disconnect().await.expect("disconnect");
}

/// NIP-33 stale-write protection: an older event cannot replace a newer one.
#[tokio::test]
#[ignore]
async fn test_long_form_stale_write_rejected() {
    let url = relay_url();
    let keys = Keys::generate();
    let mut client = SproutTestClient::connect(&url, &keys)
        .await
        .expect("connect");

    let d_tag = format!("stale-{}", uuid::Uuid::new_v4().simple());

    // Publish the "newer" event first (with a future-ish timestamp)
    let newer = {
        let tags = vec![
            Tag::parse(&["d", &d_tag]).unwrap(),
            Tag::parse(&["title", "Newer Article"]).unwrap(),
        ];
        EventBuilder::new(Kind::Custom(KIND_LONG_FORM), "Newer content.", tags)
            .custom_created_at(Timestamp::from(nostr::Timestamp::now().as_u64() + 100))
            .sign_with_keys(&keys)
            .unwrap()
    };
    let newer_id = newer.id;
    let ok1 = client.send_event(newer).await.expect("send newer");
    assert!(ok1.accepted, "newer should be accepted: {}", ok1.message);

    // Now try to publish an "older" event with the same d-tag but earlier timestamp
    let older = {
        let tags = vec![
            Tag::parse(&["d", &d_tag]).unwrap(),
            Tag::parse(&["title", "Older Article"]).unwrap(),
        ];
        EventBuilder::new(Kind::Custom(KIND_LONG_FORM), "Older content.", tags)
            .custom_created_at(Timestamp::from(nostr::Timestamp::now().as_u64() - 100))
            .sign_with_keys(&keys)
            .unwrap()
    };
    let _ok2 = client.send_event(older).await.expect("send older");
    // Stale write may be rejected or accepted-as-duplicate — either way,
    // the older event must NOT replace the newer one.

    // Query — should still have the newer event
    let sid = sub_id("stale");
    let filter = Filter::new()
        .kind(Kind::Custom(KIND_LONG_FORM))
        .author(keys.public_key())
        .custom_tag(SingleLetterTag::lowercase(Alphabet::D), [d_tag.as_str()]);
    client
        .subscribe(&sid, vec![filter])
        .await
        .expect("subscribe");

    let events = client
        .collect_until_eose(&sid, Duration::from_secs(5))
        .await
        .expect("collect");

    assert_eq!(events.len(), 1, "should have exactly one event");
    assert_eq!(
        events[0].id, newer_id,
        "surviving event should be the newer one"
    );
    assert!(
        events[0].content.contains("Newer"),
        "content should be from the newer event"
    );

    client.disconnect().await.expect("disconnect");
}
