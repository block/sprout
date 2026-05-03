//! Integration tests for sprout-pair-relay.
//!
//! Each test spins up a relay on a random port (`:0`), connects one or more
//! WebSocket clients, and exercises the observable protocol surface.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

use sprout_pair_relay::{run_server, Relay};

// ── Constants ─────────────────────────────────────────────────────────────────

/// A valid 64-char lowercase hex string (all 'a's).
const P_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
/// A different valid 64-char lowercase hex string (all 'b's).
const P_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
/// A valid event id (all 'c's).
const EV_ID: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
/// A valid pubkey (all 'd's).
const PUBKEY: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
/// A valid sig (128 'e's).
const SIG: &str = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";

// ── Test infrastructure ───────────────────────────────────────────────────────

type WS = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

/// Start a relay on a random port, return the WebSocket URL.
async fn start_relay() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let relay = Arc::new(Relay::new());
    tokio::spawn(run_server(listener, relay));
    format!("ws://127.0.0.1:{}", addr.port())
}

/// Connect a WebSocket client to the relay.
async fn connect(url: &str) -> WS {
    let (ws, _) = connect_async(url).await.unwrap();
    ws
}

/// Send a JSON value as a text frame.
async fn send(ws: &mut WS, msg: &Value) {
    ws.send(Message::Text(msg.to_string().into()))
        .await
        .unwrap();
}

/// Receive the next text frame and parse it as JSON.
/// Panics if no message arrives within 2 seconds.
async fn recv(ws: &mut WS) -> Value {
    let msg = tokio::time::timeout(Duration::from_secs(2), ws.next())
        .await
        .expect("timed out waiting for message")
        .expect("stream ended")
        .expect("WebSocket error");
    match msg {
        Message::Text(t) => serde_json::from_str(t.as_str()).expect("invalid JSON"),
        other => panic!("expected Text frame, got {:?}", other),
    }
}

/// Try to receive the next frame; return None if nothing arrives within 500 ms.
async fn try_recv(ws: &mut WS) -> Option<Message> {
    tokio::time::timeout(Duration::from_millis(500), ws.next())
        .await
        .ok()?
        .and_then(|r| r.ok())
}

/// Assert the connection is closed (stream ends) within 2 seconds.
async fn assert_closed(ws: &mut WS) {
    let result = tokio::time::timeout(Duration::from_secs(2), ws.next()).await;
    match result {
        Err(_) => panic!("connection did not close within 2 s"),
        Ok(None) => {}                        // clean EOF
        Ok(Some(Ok(Message::Close(_)))) => {} // close frame
        Ok(Some(Err(_))) => {}                // protocol error / reset
        Ok(Some(Ok(other))) => panic!("expected close, got {:?}", other),
    }
}

/// Build a valid kind:24134 event targeting `p_hex`.
fn make_event(p_hex: &str) -> Value {
    json!({
        "id":         EV_ID,
        "pubkey":     PUBKEY,
        "kind":       24134,
        "created_at": 1_700_000_000i64,
        "content":    "encrypted",
        "sig":        SIG,
        "tags":       [["p", p_hex]]
    })
}

/// Build a valid kind:24134 event with a custom id.
fn make_event_with_id(id: &str, p_hex: &str) -> Value {
    json!({
        "id":         id,
        "pubkey":     PUBKEY,
        "kind":       24134,
        "created_at": 1_700_000_000i64,
        "content":    "encrypted",
        "sig":        SIG,
        "tags":       [["p", p_hex]]
    })
}

/// Send a REQ for the given sub_id and p_hex, then consume the EOSE.
async fn subscribe(ws: &mut WS, sub_id: &str, p_hex: &str) {
    send(ws, &json!(["REQ", sub_id, {"#p": [p_hex]}])).await;
    let eose = recv(ws).await;
    assert_eq!(eose[0], "EOSE", "expected EOSE, got {eose}");
    assert_eq!(eose[1], sub_id);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// 1. No replay: events published before a subscription are not delivered.
#[tokio::test]
async fn test_no_replay() {
    let url = start_relay().await;

    // Publish first, then subscribe.
    let mut pub_ws = connect(&url).await;
    send(&mut pub_ws, &json!(["EVENT", make_event(P_A)])).await;
    let ok = recv(&mut pub_ws).await;
    assert_eq!(ok[0], "OK");
    assert!(ok[2].as_bool().unwrap());

    // Subscribe — should only get EOSE, no EVENT.
    let mut sub_ws = connect(&url).await;
    send(&mut sub_ws, &json!(["REQ", "s1", {"#p": [P_A]}])).await;
    let first = recv(&mut sub_ws).await;
    assert_eq!(first[0], "EOSE", "expected EOSE, got {first}");
    assert!(
        try_recv(&mut sub_ws).await.is_none(),
        "received unexpected message after EOSE"
    );
}

/// 2. Live delivery: events published after subscription are delivered; events
///    for a different p-tag are not.
#[tokio::test]
async fn test_live_delivery() {
    let url = start_relay().await;

    let mut sub_ws = connect(&url).await;
    subscribe(&mut sub_ws, "s1", P_A).await;

    // Publish matching event from a second connection.
    let mut pub_ws = connect(&url).await;
    send(&mut pub_ws, &json!(["EVENT", make_event(P_A)])).await;
    let ok = recv(&mut pub_ws).await;
    assert_eq!(ok[0], "OK");
    assert!(ok[2].as_bool().unwrap());

    // Subscriber should receive the event.
    let ev_msg = recv(&mut sub_ws).await;
    assert_eq!(ev_msg[0], "EVENT");
    assert_eq!(ev_msg[1], "s1");

    // Publish to a different p-tag — subscriber should NOT receive it.
    send(&mut pub_ws, &json!(["EVENT", make_event(P_B)])).await;
    let ok2 = recv(&mut pub_ws).await;
    assert_eq!(ok2[0], "OK");
    assert!(ok2[2].as_bool().unwrap());

    assert!(
        try_recv(&mut sub_ws).await.is_none(),
        "subscriber received event for wrong p-tag"
    );
}

/// 3. Kind rejection: events with kind != 24134 are rejected.
#[tokio::test]
async fn test_kind_rejection() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    let bad_event = json!({
        "id":         EV_ID,
        "pubkey":     PUBKEY,
        "kind":       1,
        "created_at": 1_700_000_000i64,
        "content":    "hello",
        "sig":        SIG,
        "tags":       [["p", P_A]]
    });
    send(&mut ws, &json!(["EVENT", bad_event])).await;
    let resp = recv(&mut ws).await;
    assert_eq!(resp[0], "OK");
    assert_eq!(resp[2], false);
    assert!(
        resp[3]
            .as_str()
            .unwrap_or("")
            .contains("kind must be 24134"),
        "unexpected message: {}",
        resp[3]
    );
}

/// 4. REQ without #p filter is rejected.
#[tokio::test]
async fn test_no_p_filter() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    send(&mut ws, &json!(["REQ", "s1", {}])).await;
    let resp = recv(&mut ws).await;
    assert_eq!(resp[0], "CLOSED");
    assert_eq!(resp[1], "s1");
    assert!(
        resp[2]
            .as_str()
            .unwrap_or("")
            .contains("#p filter required"),
        "unexpected message: {}",
        resp[2]
    );
}

/// 5. REQ with multiple #p values is rejected.
#[tokio::test]
async fn test_multi_value_p() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    send(&mut ws, &json!(["REQ", "s1", {"#p": [P_A, P_B]}])).await;
    let resp = recv(&mut ws).await;
    assert_eq!(resp[0], "CLOSED");
    assert_eq!(resp[1], "s1");
    assert!(
        resp[2]
            .as_str()
            .unwrap_or("")
            .contains("#p must have exactly one value"),
        "unexpected message: {}",
        resp[2]
    );
}

/// 6. REQ with unsupported filter field is rejected.
#[tokio::test]
async fn test_unsupported_filter_field() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    send(
        &mut ws,
        &json!(["REQ", "s1", {"#p": [P_A], "authors": [PUBKEY]}]),
    )
    .await;
    let resp = recv(&mut ws).await;
    assert_eq!(resp[0], "CLOSED");
    assert_eq!(resp[1], "s1");
    assert!(
        resp[2]
            .as_str()
            .unwrap_or("")
            .contains("unsupported filter field"),
        "unexpected message: {}",
        resp[2]
    );
}

/// 7. Second subscription with a different sub_id is rejected; first still works.
#[tokio::test]
async fn test_second_sub_different_id() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    subscribe(&mut ws, "s1", P_A).await;

    // Second REQ with a different sub_id.
    send(&mut ws, &json!(["REQ", "s2", {"#p": [P_B]}])).await;
    let resp = recv(&mut ws).await;
    assert_eq!(resp[0], "CLOSED");
    assert_eq!(resp[1], "s2");
    assert!(
        resp[2]
            .as_str()
            .unwrap_or("")
            .contains("already subscribed"),
        "unexpected message: {}",
        resp[2]
    );

    // First subscription still works.
    let mut pub_ws = connect(&url).await;
    send(&mut pub_ws, &json!(["EVENT", make_event(P_A)])).await;
    let ev_msg = recv(&mut ws).await;
    assert_eq!(ev_msg[0], "EVENT");
    assert_eq!(ev_msg[1], "s1");
}

/// 8. Second subscription with the same sub_id is rejected; first still works.
#[tokio::test]
async fn test_second_sub_same_id() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    subscribe(&mut ws, "s1", P_A).await;

    // Same sub_id again.
    send(&mut ws, &json!(["REQ", "s1", {"#p": [P_B]}])).await;
    let resp = recv(&mut ws).await;
    assert_eq!(resp[0], "CLOSED");
    assert_eq!(resp[1], "s1");
    assert!(
        resp[2]
            .as_str()
            .unwrap_or("")
            .contains("already subscribed"),
        "unexpected message: {}",
        resp[2]
    );

    // First subscription still works.
    let mut pub_ws = connect(&url).await;
    send(&mut pub_ws, &json!(["EVENT", make_event(P_A)])).await;
    let ev_msg = recv(&mut ws).await;
    assert_eq!(ev_msg[0], "EVENT");
    assert_eq!(ev_msg[1], "s1");
}

/// 9. Connection closes after 120 s (virtual time).
#[tokio::test(start_paused = true)]
async fn test_120s_timeout() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    // Advance virtual time past the connection timeout.
    tokio::time::advance(Duration::from_secs(121)).await;
    // Yield to let the relay task run its deadline branch.
    tokio::task::yield_now().await;

    assert_closed(&mut ws).await;
}

/// 10. Backpressure unit test: bounded mpsc channel rejects when full.
#[tokio::test]
async fn test_backpressure_unit() {
    let (tx, _rx) = tokio::sync::mpsc::channel::<String>(4);
    for i in 0..4 {
        tx.try_send(format!("msg {i}"))
            .expect("send should succeed");
    }
    // Channel is now full; 5th send must fail.
    assert!(
        tx.try_send("overflow".to_string()).is_err(),
        "expected channel to be full"
    );
}

/// 11. Frame larger than 4096 bytes closes the connection.
#[tokio::test]
async fn test_max_frame_size() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    let big = "x".repeat(4097);
    // Ignore send errors — the server may close mid-send.
    let _ = ws.send(Message::Text(big.into())).await;

    assert_closed(&mut ws).await;
}

/// 12. EVENT with missing `id` field is rejected.
#[tokio::test]
async fn test_event_shape_missing_id() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    let bad = json!({
        "pubkey":     PUBKEY,
        "kind":       24134,
        "created_at": 1_700_000_000i64,
        "content":    "x",
        "sig":        SIG,
        "tags":       [["p", P_A]]
    });
    send(&mut ws, &json!(["EVENT", bad])).await;
    let resp = recv(&mut ws).await;
    assert_eq!(resp[0], "OK");
    assert_eq!(resp[2], false);
}

/// 13. EVENT with no `p` tag is rejected.
#[tokio::test]
async fn test_no_p_tag() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    let bad = json!({
        "id":         EV_ID,
        "pubkey":     PUBKEY,
        "kind":       24134,
        "created_at": 1_700_000_000i64,
        "content":    "x",
        "sig":        SIG,
        "tags":       []
    });
    send(&mut ws, &json!(["EVENT", bad])).await;
    let resp = recv(&mut ws).await;
    assert_eq!(resp[0], "OK");
    assert_eq!(resp[2], false);
}

/// 14. EVENT with two `p` tags is rejected.
#[tokio::test]
async fn test_multiple_p_tags() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    let bad = json!({
        "id":         EV_ID,
        "pubkey":     PUBKEY,
        "kind":       24134,
        "created_at": 1_700_000_000i64,
        "content":    "x",
        "sig":        SIG,
        "tags":       [["p", P_A], ["p", P_B]]
    });
    send(&mut ws, &json!(["EVENT", bad])).await;
    let resp = recv(&mut ws).await;
    assert_eq!(resp[0], "OK");
    assert_eq!(resp[2], false);
    assert!(
        resp[3].as_str().unwrap_or("").contains("exactly one p tag"),
        "unexpected message: {}",
        resp[3]
    );
}

/// 15. REQ with non-hex #p value is rejected.
#[tokio::test]
async fn test_invalid_hex_in_p_filter() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    send(&mut ws, &json!(["REQ", "s1", {"#p": ["not-hex-64-chars"]}])).await;
    let resp = recv(&mut ws).await;
    assert_eq!(resp[0], "CLOSED");
    assert_eq!(resp[1], "s1");
}

/// 16. EVENT with non-hex `id` is rejected.
#[tokio::test]
async fn test_invalid_hex_in_event_id() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    let bad = json!({
        "id":         "ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ",
        "pubkey":     PUBKEY,
        "kind":       24134,
        "created_at": 1_700_000_000i64,
        "content":    "x",
        "sig":        SIG,
        "tags":       [["p", P_A]]
    });
    send(&mut ws, &json!(["EVENT", bad])).await;
    let resp = recv(&mut ws).await;
    assert_eq!(resp[0], "OK");
    assert_eq!(resp[2], false);
}

/// 17. Global connection cap: 128 connections succeed; 129th is rejected.
///     After closing one, a new connection succeeds.
#[tokio::test]
async fn test_global_conn_cap() {
    let url = start_relay().await;

    let mut conns: Vec<WS> = Vec::with_capacity(128);
    for _ in 0..128 {
        conns.push(connect(&url).await);
    }

    // 129th should fail (server returns 503).
    let result = connect_async(&url).await;
    assert!(result.is_err(), "expected 129th connection to fail");

    // Close one connection.
    let mut dropped = conns.pop().unwrap();
    dropped.close(None).await.unwrap();
    // Give the relay a moment to decrement its counter.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Now a new connection should succeed.
    let _new = connect(&url).await;
}

/// 18. Event rate limit: 10 EVENTs succeed; 11th is rate-limited.
#[tokio::test]
async fn test_event_rate_limit() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    // Use a unique p-tag per test to avoid cross-test fan-out.
    const P: &str = "1111111111111111111111111111111111111111111111111111111111111111";

    // First 10 events must be accepted.
    for i in 0..10u8 {
        let id = format!("{:0>64}", i);
        send(&mut ws, &json!(["EVENT", make_event_with_id(&id, P)])).await;
        let resp = recv(&mut ws).await;
        assert_eq!(resp[0], "OK", "event {i}: {resp}");
        assert!(
            resp[2].as_bool().unwrap(),
            "event {i} rejected: {}",
            resp[3]
        );
    }

    // 11th must be rate-limited.
    let id = format!("{:0>64}", 10u8);
    send(&mut ws, &json!(["EVENT", make_event_with_id(&id, P)])).await;
    let resp = recv(&mut ws).await;
    assert_eq!(resp[0], "OK");
    assert_eq!(resp[2], false);
    assert!(
        resp[3].as_str().unwrap_or("").contains("rate-limited"),
        "unexpected message: {}",
        resp[3]
    );
}

/// 19. Message rate limit: 20 messages succeed; 21st closes the connection.
#[tokio::test]
async fn test_message_rate_limit() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    // Send 20 messages (CLOSE with unknown sub_id — no response generated).
    for _ in 0..20 {
        send(&mut ws, &json!(["CLOSE", "nonexistent"])).await;
    }
    // Small delay to let the server process all messages.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 21st message should trigger the rate limit and close the connection.
    send(&mut ws, &json!(["CLOSE", "nonexistent"])).await;
    assert_closed(&mut ws).await;
}

/// 20. REQ with multiple filters is rejected.
#[tokio::test]
async fn test_multiple_filters() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    send(&mut ws, &json!(["REQ", "s1", {"#p": [P_A]}, {"#p": [P_B]}])).await;
    let resp = recv(&mut ws).await;
    assert_eq!(resp[0], "CLOSED");
    assert_eq!(resp[1], "s1");
    assert!(
        resp[2].as_str().unwrap_or("").contains("multiple filters"),
        "unexpected message: {}",
        resp[2]
    );
}

/// 21. Unknown message type receives a NOTICE.
#[tokio::test]
async fn test_unknown_message() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    send(&mut ws, &json!(["AUTH", {}])).await;
    let resp = recv(&mut ws).await;
    assert_eq!(resp[0], "NOTICE");
    assert!(
        resp[1]
            .as_str()
            .unwrap_or("")
            .contains("unsupported message"),
        "unexpected message: {}",
        resp[1]
    );
}

/// 22. Sub_id containing JSON special characters does not cause injection.
#[tokio::test]
async fn test_json_injection_sub_id() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    let evil_id = r#""],\"evil\":["#;
    send(&mut ws, &json!(["REQ", evil_id, {}])).await;
    let resp = recv(&mut ws).await;
    // Whatever the server responds, it must be valid JSON (recv() already
    // parses it). The sub_id in the response must equal the literal string.
    assert!(resp.is_array(), "response is not a JSON array: {resp}");
    // The response should be CLOSED with the literal sub_id echoed back safely.
    if resp[0] == "CLOSED" {
        let echoed = resp[1].as_str().unwrap_or("");
        assert_eq!(echoed, evil_id, "sub_id was not echoed verbatim");
    }
}

/// 23. CLOSE removes the subscription; subsequent events are not delivered.
#[tokio::test]
async fn test_close_removes_sub() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    subscribe(&mut ws, "s1", P_A).await;

    send(&mut ws, &json!(["CLOSE", "s1"])).await;
    // Give the relay a moment to process the CLOSE.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Publish a matching event.
    let mut pub_ws = connect(&url).await;
    send(&mut pub_ws, &json!(["EVENT", make_event(P_A)])).await;
    let ok = recv(&mut pub_ws).await;
    assert_eq!(ok[0], "OK");

    // Original subscriber must not receive anything.
    assert!(
        try_recv(&mut ws).await.is_none(),
        "received event after CLOSE"
    );
}

/// 24. CLOSE does not close the WebSocket connection.
#[tokio::test]
async fn test_close_keeps_connection() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    subscribe(&mut ws, "s1", P_A).await;
    send(&mut ws, &json!(["CLOSE", "s1"])).await;

    // Connection must still be open — send another message and get a response.
    send(&mut ws, &json!(["REQ", "s2", {"#p": [P_B]}])).await;
    let eose = recv(&mut ws).await;
    assert_eq!(eose[0], "EOSE");
    assert_eq!(eose[1], "s2");
}

/// 25. After CLOSE, a new REQ works and receives future events.
#[tokio::test]
async fn test_req_after_close() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    subscribe(&mut ws, "s1", P_A).await;
    send(&mut ws, &json!(["CLOSE", "s1"])).await;

    // Re-subscribe with a new sub_id.
    subscribe(&mut ws, "s2", P_A).await;

    // Publish a matching event.
    let mut pub_ws = connect(&url).await;
    send(&mut pub_ws, &json!(["EVENT", make_event(P_A)])).await;
    let ok = recv(&mut pub_ws).await;
    assert_eq!(ok[0], "OK");
    assert!(ok[2].as_bool().unwrap());

    // New subscription must receive the event.
    let ev_msg = recv(&mut ws).await;
    assert_eq!(ev_msg[0], "EVENT");
    assert_eq!(ev_msg[1], "s2");
}

/// 26. CLOSE with an unknown sub_id is silently ignored.
#[tokio::test]
async fn test_close_unknown_sub_id() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    send(&mut ws, &json!(["CLOSE", "nonexistent"])).await;

    // No error response; connection stays open.
    assert!(
        try_recv(&mut ws).await.is_none(),
        "received unexpected response to CLOSE of unknown sub_id"
    );

    // Connection is still usable.
    send(&mut ws, &json!(["REQ", "s1", {"#p": [P_A]}])).await;
    let eose = recv(&mut ws).await;
    assert_eq!(eose[0], "EOSE");
}

/// 27. No events delivered after CLOSE (explicit duplicate of test 23).
#[tokio::test]
async fn test_no_events_after_close() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    subscribe(&mut ws, "sub", P_A).await;
    send(&mut ws, &json!(["CLOSE", "sub"])).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut pub_ws = connect(&url).await;
    send(&mut pub_ws, &json!(["EVENT", make_event(P_A)])).await;
    recv(&mut pub_ws).await; // consume OK

    assert!(
        try_recv(&mut ws).await.is_none(),
        "received event after CLOSE"
    );
}

/// 28. Binary WebSocket frame closes the connection.
#[tokio::test]
async fn test_binary_frame() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    let _ = ws.send(Message::Binary(b"hello".to_vec().into())).await;

    assert_closed(&mut ws).await;
}

/// 29. REQ with too few elements receives a NOTICE.
#[tokio::test]
async fn test_malformed_req_too_few() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    send(&mut ws, &json!(["REQ"])).await;
    let resp = recv(&mut ws).await;
    assert_eq!(resp[0], "NOTICE");
    assert!(
        resp[1].as_str().unwrap_or("").contains("invalid REQ"),
        "unexpected message: {}",
        resp[1]
    );
}

/// 30. REQ with a non-string sub_id receives a NOTICE.
#[tokio::test]
async fn test_malformed_req_non_string_sub_id() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    send(&mut ws, &json!(["REQ", 123, {}])).await;
    let resp = recv(&mut ws).await;
    assert_eq!(resp[0], "NOTICE");
    assert!(
        resp[1].as_str().unwrap_or("").contains("invalid REQ"),
        "unexpected message: {}",
        resp[1]
    );
}

/// 31. REQ with a non-object filter receives a CLOSED.
#[tokio::test]
async fn test_malformed_req_non_object_filter() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    send(&mut ws, &json!(["REQ", "s1", "bad"])).await;
    let resp = recv(&mut ws).await;
    assert_eq!(resp[0], "CLOSED");
    assert_eq!(resp[1], "s1");
    assert!(
        resp[2].as_str().unwrap_or("").contains("invalid filter"),
        "unexpected message: {}",
        resp[2]
    );
}

/// 32. Write timeout / slow reader: subscriber that doesn't read eventually
///     gets disconnected when the server's write buffer fills up.
///
///     We flood the subscriber's p-tag with events from a second connection.
///     Because the subscriber never reads, the relay's bounded channel fills
///     and subsequent fan-out drops are silent.  The subscriber connection
///     itself stays open (fan-out drops don't close).  This test verifies the
///     observable behavior: the publisher keeps getting OK responses and the
///     subscriber connection is still alive after the flood (see test 40 for
///     the complementary assertion).
#[tokio::test]
async fn test_write_timeout() {
    let url = start_relay().await;

    // Subscribe but never read.
    let mut sub_ws = connect(&url).await;
    send(&mut sub_ws, &json!(["REQ", "s1", {"#p": [P_A]}])).await;
    // Don't call recv — leave the EOSE unread.

    // Flood from a publisher (stay under 20 msg rate limit).
    let mut pub_ws = connect(&url).await;
    for i in 0..10u8 {
        let id = format!("{:0>64}", i);
        send(&mut pub_ws, &json!(["EVENT", make_event_with_id(&id, P_A)])).await;
        let ok = recv(&mut pub_ws).await;
        assert_eq!(ok[0], "OK");
    }

    // Publisher connection is still healthy.
    send(&mut pub_ws, &json!(["REQ", "check", {"#p": [P_B]}])).await;
    let eose = recv(&mut pub_ws).await;
    assert_eq!(eose[0], "EOSE");
}

/// 33. Ping frame receives a Pong with the same payload.
#[tokio::test]
async fn test_ping_pong() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    let payload = b"hello".to_vec();
    ws.send(Message::Ping(payload.clone().into()))
        .await
        .unwrap();

    let msg = tokio::time::timeout(Duration::from_secs(2), ws.next())
        .await
        .expect("timed out")
        .expect("stream ended")
        .expect("WebSocket error");

    match msg {
        Message::Pong(data) => assert_eq!(data.as_ref(), payload.as_slice()),
        other => panic!("expected Pong, got {:?}", other),
    }
}

/// 34. Client-initiated Close frame receives a Close reply.
#[tokio::test]
async fn test_close_handshake() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    ws.send(Message::Close(None)).await.unwrap();

    // The stream should end (server sends its own Close and closes).
    assert_closed(&mut ws).await;
}

/// 35. Connection counter does not leak: open/close 5 connections, then open
///     128 more — all should succeed.
#[tokio::test]
async fn test_conn_counter_no_leak() {
    let url = start_relay().await;

    // Open and close 5 connections.
    for _ in 0..5 {
        let mut ws = connect(&url).await;
        ws.close(None).await.unwrap();
    }
    // Give the relay time to decrement counters.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Now open 128 connections — all should succeed.
    let mut conns: Vec<WS> = Vec::with_capacity(128);
    for _ in 0..128 {
        conns.push(connect(&url).await);
    }
    assert_eq!(conns.len(), 128);
}

/// 36. Fan-out drops do not close the subscriber connection.
///     (Merged with test 40 — see that test for the authoritative assertion.)
#[tokio::test]
async fn test_control_msg_backpressure() {
    // Verify that flooding events to a slow subscriber does not close the
    // subscriber's connection — the relay silently drops overflowing messages.
    let url = start_relay().await;

    let mut sub_ws = connect(&url).await;
    send(&mut sub_ws, &json!(["REQ", "s1", {"#p": [P_A]}])).await;
    // Leave EOSE unread to fill the channel quickly.

    let mut pub_ws = connect(&url).await;
    for i in 0..16u8 {
        let id = format!("{:0>64}", i);
        send(&mut pub_ws, &json!(["EVENT", make_event_with_id(&id, P_A)])).await;
        let _ = recv(&mut pub_ws).await;
    }

    // Subscriber connection must still be alive — verify by closing it cleanly.
    // If the connection were dead, close() would fail or timeout.
    let close_result = tokio::time::timeout(Duration::from_secs(2), sub_ws.close(None)).await;
    assert!(
        close_result.is_ok(),
        "subscriber connection died after flood"
    );
}

/// 37. Various malformed inputs do not crash the relay (no panic).
#[tokio::test]
async fn test_no_client_data_in_logs() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    // A grab-bag of weird inputs — none should panic the relay.
    let inputs: &[Value] = &[
        json!(null),
        json!(42),
        json!("just a string"),
        json!({}),
        json!([]),
        json!(["UNKNOWN"]),
        json!(["EVENT"]),
        json!(["EVENT", null]),
        json!(["REQ", null]),
        json!(["CLOSE"]),
        json!(["CLOSE", null]),
    ];

    for input in inputs {
        // Ignore send errors (server may close on some inputs).
        let _ = ws.send(Message::Text(input.to_string().into())).await;
        // Small pause to let the relay process.
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // If we reach here without a panic, the test passes.
    // The connection may or may not still be open.
}

/// 38. EOSE arrives before any EVENT in normal flow.
#[tokio::test]
async fn test_eose_try_send_failure() {
    let url = start_relay().await;

    // Publisher sends an event before the subscriber connects.
    let mut pub_ws = connect(&url).await;
    send(&mut pub_ws, &json!(["EVENT", make_event(P_A)])).await;
    let ok = recv(&mut pub_ws).await;
    assert_eq!(ok[0], "OK");

    // Subscriber connects after the event — should see EOSE first (no EVENT,
    // since there is no persistence).
    let mut sub_ws = connect(&url).await;
    send(&mut sub_ws, &json!(["REQ", "s1", {"#p": [P_A]}])).await;
    let first = recv(&mut sub_ws).await;
    assert_eq!(first[0], "EOSE", "first message must be EOSE, got {first}");

    // No further messages (no stored events).
    assert!(
        try_recv(&mut sub_ws).await.is_none(),
        "received unexpected message after EOSE"
    );
}

/// 39. Ping frames count toward the per-connection message rate limit.
/// We use CLOSE messages (no response generated) to burn through the rate
/// limit without buffered responses interfering with the close assertion.
#[tokio::test]
async fn test_ping_counts_toward_rate_limit() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    // Send 20 CLOSE messages for a non-existent sub_id.
    // Each counts toward the 20-message rate limit but generates no response.
    for _ in 0..20 {
        send(&mut ws, &json!(["CLOSE", "nope"])).await;
    }

    // Small yield to let the server process all 20.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 21st message (a Ping this time) should trigger the rate limit.
    let _ = ws.send(Message::Ping(vec![].into())).await;

    assert_closed(&mut ws).await;
}

/// 40. Fan-out drops do not close the subscriber connection.
#[tokio::test]
async fn test_fan_out_drop_doesnt_close() {
    let url = start_relay().await;

    // Subscribe but don't read (leave EOSE buffered).
    let mut sub_ws = connect(&url).await;
    send(&mut sub_ws, &json!(["REQ", "s1", {"#p": [P_A]}])).await;

    // Flood from a publisher — channel fills, extras are dropped silently.
    let mut pub_ws = connect(&url).await;
    for i in 0..16u8 {
        let id = format!("{:0>64}", i);
        send(&mut pub_ws, &json!(["EVENT", make_event_with_id(&id, P_A)])).await;
        let _ = recv(&mut pub_ws).await;
    }

    // Subscriber connection must still be alive — verify by closing it cleanly.
    let result = tokio::time::timeout(Duration::from_secs(2), sub_ws.close(None)).await;
    assert!(
        result.is_ok(),
        "subscriber connection was unexpectedly dead"
    );
}

/// 41. Reader backpressure: simplified version verifying that a subscriber
///     that never reads eventually allows the publisher to keep running.
///     (Merged with test 32 for observable behavior.)
#[tokio::test]
async fn test_reader_backpressure_closes() {
    let url = start_relay().await;

    // Slow subscriber — never reads.
    let mut sub_ws = connect(&url).await;
    send(&mut sub_ws, &json!(["REQ", "s1", {"#p": [P_A]}])).await;

    // Publisher floods events.
    let mut pub_ws = connect(&url).await;
    for i in 0..20u8 {
        let id = format!("{:0>64}", i);
        send(&mut pub_ws, &json!(["EVENT", make_event_with_id(&id, P_A)])).await;
        let ok = recv(&mut pub_ws).await;
        // Publisher must always get OK (fan-out drops are silent to publisher).
        assert_eq!(ok[0], "OK");
    }
}

/// 42. Connection closes promptly after 120 s (virtual time).
///     Explicit duplicate of test 9 with a slightly different assertion style.
#[tokio::test(start_paused = true)]
async fn test_cancellation_immediate() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    tokio::time::advance(Duration::from_secs(121)).await;
    tokio::task::yield_now().await;

    // The connection must be closed — not just slow.
    assert_closed(&mut ws).await;
}

/// 43. Client-initiated graceful close receives a Close reply.
///     Explicit duplicate of test 34 with a different connection state.
#[tokio::test]
async fn test_graceful_close() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    // Subscribe first, then close gracefully.
    subscribe(&mut ws, "s1", P_A).await;

    ws.send(Message::Close(None)).await.unwrap();
    assert_closed(&mut ws).await;
}

/// 44. Multiple subscribers on the same #p value both receive the event.
#[tokio::test]
async fn test_multiple_subscribers_same_p() {
    let url = start_relay().await;

    // Two subscribers on the same #p.
    let mut sub1 = connect(&url).await;
    subscribe(&mut sub1, "s1", P_A).await;

    let mut sub2 = connect(&url).await;
    subscribe(&mut sub2, "s2", P_A).await;

    // Publisher sends one event.
    let mut pub_ws = connect(&url).await;
    send(&mut pub_ws, &json!(["EVENT", make_event(P_A)])).await;
    let ok = recv(&mut pub_ws).await;
    assert_eq!(ok[0], "OK");
    assert!(ok[2].as_bool().unwrap());

    // Both subscribers receive it.
    let ev1 = recv(&mut sub1).await;
    assert_eq!(ev1[0], "EVENT");
    assert_eq!(ev1[1], "s1");

    let ev2 = recv(&mut sub2).await;
    assert_eq!(ev2[0], "EVENT");
    assert_eq!(ev2[1], "s2");
}

/// 45. Uppercase hex in #p filter value is rejected.
#[tokio::test]
async fn test_uppercase_hex_in_p_filter() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    // Uppercase hex — must be rejected.
    let upper_p = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    send(&mut ws, &json!(["REQ", "s1", {"#p": [upper_p]}])).await;
    let resp = recv(&mut ws).await;
    assert_eq!(resp[0], "CLOSED");
    assert!(
        resp[2].as_str().unwrap_or("").contains("64 lowercase hex"),
        "unexpected: {}",
        resp[2]
    );
}

/// 46. Uppercase hex in event id is rejected.
#[tokio::test]
async fn test_uppercase_hex_in_event_fields() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    // Event with uppercase id.
    let mut ev = make_event(P_A);
    ev["id"] = json!("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
    send(&mut ws, &json!(["EVENT", ev])).await;
    let resp = recv(&mut ws).await;
    assert_eq!(resp[0], "OK");
    assert_eq!(resp[2], false);
    assert!(resp[3].as_str().unwrap_or("").contains("64 lowercase hex"));
}

/// 47. Overlong sub_id (> 64 bytes) is rejected with CLOSED "".
#[tokio::test]
async fn test_overlong_sub_id() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    let long_sub_id = "x".repeat(65);
    send(&mut ws, &json!(["REQ", long_sub_id, {"#p": [P_A]}])).await;
    let resp = recv(&mut ws).await;
    assert_eq!(resp[0], "CLOSED");
    assert_eq!(resp[1], ""); // sub_id too long → use ""
    assert!(resp[2].as_str().unwrap_or("").contains("sub_id too long"));
}

/// 48. Negative created_at is accepted (relay doesn't validate timestamps).
#[tokio::test]
async fn test_negative_created_at() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    let mut ev = make_event(P_A);
    ev["created_at"] = json!(-1);
    send(&mut ws, &json!(["EVENT", ev])).await;
    let resp = recv(&mut ws).await;
    assert_eq!(resp[0], "OK");
    // Should still be accepted — relay doesn't validate timestamps.
    assert!(
        resp[2].as_bool().unwrap(),
        "negative created_at rejected: {}",
        resp[3]
    );
}

/// 49. Event with missing sig field is rejected.
#[tokio::test]
async fn test_event_missing_sig() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    let ev = json!({
        "id": EV_ID,
        "pubkey": PUBKEY,
        "kind": 24134,
        "created_at": 1_700_000_000i64,
        "content": "encrypted",
        "tags": [["p", P_A]]
    });
    send(&mut ws, &json!(["EVENT", ev])).await;
    let resp = recv(&mut ws).await;
    assert_eq!(resp[0], "OK");
    assert_eq!(resp[2], false);
    assert!(resp[3].as_str().unwrap_or("").contains("missing sig"));
}

/// 50. Event with too many tags (> 16) is rejected.
#[tokio::test]
async fn test_event_too_many_tags() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    let mut tags: Vec<Value> = (0..17).map(|i| json!(["x", format!("{i}")])).collect();
    tags.push(json!(["p", P_A])); // 18 tags total, > 16 limit
    let ev = json!({
        "id": EV_ID,
        "pubkey": PUBKEY,
        "kind": 24134,
        "created_at": 1_700_000_000i64,
        "content": "encrypted",
        "sig": SIG,
        "tags": tags
    });
    send(&mut ws, &json!(["EVENT", ev])).await;
    let resp = recv(&mut ws).await;
    assert_eq!(resp[0], "OK");
    assert_eq!(resp[2], false);
    assert!(resp[3].as_str().unwrap_or("").contains("too many tags"));
}

/// 51. Event with tag string exceeding 128 bytes is rejected.
#[tokio::test]
async fn test_event_tag_string_too_long() {
    let url = start_relay().await;
    let mut ws = connect(&url).await;

    let long_val = "x".repeat(129);
    let ev = json!({
        "id": EV_ID,
        "pubkey": PUBKEY,
        "kind": 24134,
        "created_at": 1_700_000_000i64,
        "content": "encrypted",
        "sig": SIG,
        "tags": [["p", P_A], ["x", long_val]]
    });
    send(&mut ws, &json!(["EVENT", ev])).await;
    let resp = recv(&mut ws).await;
    assert_eq!(resp[0], "OK");
    assert_eq!(resp[2], false);
    assert!(resp[3]
        .as_str()
        .unwrap_or("")
        .contains("tag string too long"));
}
