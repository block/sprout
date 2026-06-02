//! Mesh hole-punch signaling: the relay's only role in the v1 direct-iroh mesh.
//!
//! v1 mesh is "Sprout-coordinated direct iroh" — no server-side iroh relay/proxy.
//! The relay's entire job for connectivity is: when a member asks to dial a peer
//! it discovered via kind:30621, validate that BOTH ends are relay members, then
//! emit a *paired* live "call-me-now" so both desktops hole-punch at the same
//! moment. The relay never sees or stores the bulk iroh traffic — only this tiny
//! control-plane exchange.
//!
//! Flow:
//!   desktop A (member) reads 30621 → already holds B's `EndpointAddr` →
//!   publishes KIND_MESH_CONNECT_REQUEST (24621) `#p=B` with both endpoint addrs →
//!   relay validates B is a member → mints two relay-signed KIND_MESH_CALL_ME_NOW
//!   (24622): one `#p=A` carrying B's addr, one `#p=B` carrying A's addr →
//!   both fan out over the existing channel-less ephemeral path (local + Redis) →
//!   both desktops dial simultaneously → direct QUIC.
//!
//! The relay is ENDPOINT-STATELESS here: the requester supplies both dial hints
//! (it read them from the relay-signed 30621), so the relay only validates
//! membership and pairs. Endpoint addrs are dial hints, never auth — membership
//! is the gate.

use std::sync::Arc;

use nostr::{EventBuilder, Kind, Tag};
use sprout_core::event::StoredEvent;
use sprout_core::kind::KIND_MESH_CALL_ME_NOW;

use crate::api::relay_members::{check_relay_membership, MembershipDecision};
use crate::state::AppState;

/// Parsed `KIND_MESH_CONNECT_REQUEST` (24621) content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectRequest {
    /// Requester's own iroh EndpointAddr (base64 invite token) — sent to the peer.
    pub self_endpoint_addr: String,
    /// Peer's iroh EndpointAddr (base64 invite token), read by the requester from
    /// the peer's kind:30621 serve target — sent back to the requester.
    pub peer_endpoint_addr: String,
    /// Requester's own iroh endpoint id (optional) — correlation/instrumentation
    /// only, never trusted for auth. Copied into the peer's call-me-now.
    pub self_endpoint_id: Option<String>,
    /// Peer's iroh endpoint id (optional) — correlation/instrumentation only.
    /// Copied into the requester's call-me-now so the desktop can target the
    /// exact peer endpoint it picked from 30621 (multi-endpoint disambiguation).
    pub peer_endpoint_id: Option<String>,
    /// Correlates the two halves of one punch attempt.
    pub attempt_id: String,
}

/// Outcome of validating + parsing a connect request, before any relay state is touched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestError {
    /// Content was not valid JSON in the expected shape.
    Malformed(String),
    /// No `#p` tag naming the target peer.
    MissingTarget,
}

/// Parse the JSON content of a 24621 connect request. Pure — no I/O.
pub fn parse_connect_request(content: &str) -> Result<ConnectRequest, RequestError> {
    let v: serde_json::Value = serde_json::from_str(content)
        .map_err(|e| RequestError::Malformed(format!("not JSON: {e}")))?;
    let get = |k: &str| {
        v.get(k)
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let self_endpoint_addr = get("self_endpoint_addr")
        .ok_or_else(|| RequestError::Malformed("self_endpoint_addr".into()))?;
    let peer_endpoint_addr = get("peer_endpoint_addr")
        .ok_or_else(|| RequestError::Malformed("peer_endpoint_addr".into()))?;
    let attempt_id =
        get("attempt_id").ok_or_else(|| RequestError::Malformed("attempt_id".into()))?;
    Ok(ConnectRequest {
        self_endpoint_addr,
        peer_endpoint_addr,
        self_endpoint_id: get("self_endpoint_id"),
        peer_endpoint_id: get("peer_endpoint_id"),
        attempt_id,
    })
}

/// Extract the single `#p` target pubkey (hex) from the request event's tags.
pub fn extract_target_pubkey(event: &nostr::Event) -> Option<String> {
    event.tags.iter().find_map(|t| {
        let s = t.as_slice();
        if s.len() >= 2 && s[0] == "p" {
            Some(s[1].clone())
        } else {
            None
        }
    })
}

/// Build the JSON content for one call-me-now (24622) directed at `recipient`,
/// telling it to dial `peer_endpoint_addr` (optionally `peer_endpoint_id` for
/// multi-endpoint disambiguation). Pure — no I/O.
pub fn call_me_now_content(
    peer_endpoint_addr: &str,
    peer_endpoint_id: Option<&str>,
    attempt_id: &str,
    expires_at: u64,
) -> String {
    let mut obj = serde_json::json!({
        "v": 1,
        "type": "sprout-iroh-call-me-now",
        "peer_endpoint_addr": peer_endpoint_addr,
        "attempt_id": attempt_id,
        "expires_at": expires_at,
    });
    if let Some(eid) = peer_endpoint_id {
        obj["peer_endpoint_id"] = serde_json::Value::String(eid.to_string());
    }
    obj.to_string()
}

/// Pure: does a membership decision admit a peer into the v1 mesh? Direct relay
/// members (or open relays) only — `ViaOwner` (NIP-OA delegated) is intentionally
/// NOT admitted in v1, keeping the mesh trust boundary tighter and legible. This
/// is applied identically to BOTH the requester and the target, so the two ends
/// are symmetric. Isolated as a pure fn so the trust gate is unit-testable
/// without an `AppState`.
pub fn membership_admits_mesh(decision: &MembershipDecision) -> bool {
    matches!(
        decision,
        MembershipDecision::OpenRelay | MembershipDecision::Member
    )
}

/// Seconds a call-me-now is valid; iroh's punch loop runs ~60s, so this bounds
/// how stale a signal a desktop should act on.
pub const CALL_ME_NOW_TTL_SECS: u64 = 60;

/// Handle a verified KIND_MESH_CONNECT_REQUEST (24621) from an authenticated
/// relay member. Validates the target is also a member, then emits the paired
/// call-me-now to both ends. Returns Ok(()) on success or an Err(reason) string
/// suitable for an OK(false) reply (reason is for the requester, not secret).
pub async fn handle_connect_request(
    state: &Arc<AppState>,
    requester_pubkey_hex: &str,
    event: &nostr::Event,
) -> Result<(), String> {
    let req = parse_connect_request(&event.content)
        .map_err(|e| format!("invalid: malformed mesh connect request ({e:?})"))?;

    let target_hex = extract_target_pubkey(event)
        .ok_or_else(|| "invalid: mesh connect request missing #p target".to_string())?;

    if target_hex == requester_pubkey_hex {
        return Err("invalid: cannot mesh-connect to self".to_string());
    }

    // Membership gate, applied SYMMETRICALLY to both ends — direct relay members
    // only, gated purely by relay access. The requester reached this handler via
    // a NIP-42-authed WS, but that auth can be ViaOwner (NIP-OA delegated) when
    // SPROUT_ALLOW_NIP_OA_AUTH is on; v1 mesh excludes delegated identities, so we
    // re-check the requester here with no auth tag (which makes ViaOwner
    // unreachable — only Member/OpenRelay/Denied) to match the target check.
    require_mesh_member(state, requester_pubkey_hex)
        .await
        .map_err(|_| "restricted: delegated identities cannot initiate mesh in v1".to_string())?;

    require_mesh_member(state, &target_hex)
        .await
        .map_err(|_| "restricted: target is not a relay member".to_string())?;

    let expires_at = (chrono::Utc::now().timestamp().max(0) as u64) + CALL_ME_NOW_TTL_SECS;

    // Pair: tell the requester to dial the peer's addr, and the peer to dial the
    // requester's addr. Each is a relay-signed ephemeral #p-addressed event.
    // endpoint_id (if supplied) is copied through for desktop multi-endpoint
    // disambiguation — it is correlation metadata, never trusted for auth.
    let to_requester = build_call_me_now(
        state,
        requester_pubkey_hex,
        &req.peer_endpoint_addr,
        req.peer_endpoint_id.as_deref(),
        &req.attempt_id,
        expires_at,
    )?;
    let to_target = build_call_me_now(
        state,
        &target_hex,
        &req.self_endpoint_addr,
        req.self_endpoint_id.as_deref(),
        &req.attempt_id,
        expires_at,
    )?;

    publish_channelless_ephemeral(state, &to_requester).await;
    publish_channelless_ephemeral(state, &to_target).await;
    Ok(())
}

/// Async: confirm `pubkey_hex` is a direct relay member admissible to the mesh.
/// `None` auth_tag → ViaOwner is unreachable, so only Member/OpenRelay admit;
/// everything else (Denied, ViaOwner-if-it-somehow-appeared, or a check error)
/// FAILS CLOSED. Used symmetrically for requester and target.
async fn require_mesh_member(state: &Arc<AppState>, pubkey_hex: &str) -> Result<(), ()> {
    let bytes = hex::decode(pubkey_hex).map_err(|_| ())?;
    match check_relay_membership(state, &bytes, None).await {
        Ok(d) if membership_admits_mesh(&d) => Ok(()),
        Ok(_) => Err(()),
        Err(e) => {
            tracing::warn!("mesh connect: membership check failed (fail-closed): {e}");
            Err(())
        }
    }
}

/// Mint one relay-signed call-me-now (24622) addressed to `recipient_hex`.
fn build_call_me_now(
    state: &Arc<AppState>,
    recipient_hex: &str,
    peer_endpoint_addr: &str,
    peer_endpoint_id: Option<&str>,
    attempt_id: &str,
    expires_at: u64,
) -> Result<nostr::Event, String> {
    let content = call_me_now_content(peer_endpoint_addr, peer_endpoint_id, attempt_id, expires_at);
    let p_tag = Tag::parse(["p", recipient_hex])
        .map_err(|e| format!("error: failed to build p tag: {e}"))?;
    EventBuilder::new(Kind::Custom(KIND_MESH_CALL_ME_NOW as u16), content)
        .tags([p_tag])
        .sign_with_keys(&state.relay_keypair)
        .map_err(|e| format!("error: failed to sign call-me-now: {e}"))
}

/// Publish a channel-less ephemeral event over the same path NIP-AB pairing uses:
/// Redis fan-out (nil-UUID global routing key) for cross-pod, plus direct local
/// WS fan-out. The recipient's desktop receives it via a REQ on `#p=self kind:24622`.
async fn publish_channelless_ephemeral(state: &Arc<AppState>, event: &nostr::Event) {
    state.mark_local_event(&event.id);
    if let Err(e) = state.pubsub.publish_event(uuid::Uuid::nil(), event).await {
        state.local_event_ids.invalidate(&event.id.to_bytes());
        tracing::warn!(event_id = %event.id, "mesh call-me-now global publish failed: {e}");
    }
    let stored = StoredEvent::new(event.clone(), None);
    let matches = state.sub_registry.fan_out(&stored);
    metrics::histogram!("sprout_fanout_recipients").record(matches.len() as f64);
    if let Ok(event_json) = serde_json::to_string(event) {
        for (target_conn_id, sub_id) in &matches {
            let msg = format!(r#"["EVENT","{sub_id}",{event_json}]"#);
            let _ = state.conn_manager.send_to(*target_conn_id, msg);
        }
    }
}

/// Handle a verified KIND_MESH_STATUS_REPORT (24620) from an authenticated relay
/// member. The member reports its current mesh `/api/status` JSON; the relay
/// sanitizes it and republishes a relay-signed kind:30621 discovery note keyed
/// to the reporter (so members' notes never clobber each other). The report
/// itself is ephemeral — only the relay's projection is durable. Membership is
/// already enforced (the reporter is authenticated on a member-gated WS).
pub async fn handle_status_report(
    state: &Arc<AppState>,
    reporter_pubkey_hex: &str,
    event: &nostr::Event,
) -> Result<(), String> {
    // Same membership symmetry as handle_connect_request: a NIP-OA delegated
    // (ViaOwner) identity is authed on the WS but is NOT a v1 mesh participant.
    // If we let it report, the relay would advertise a serve_target under that
    // pubkey that the connect path (which denies ViaOwner) then refuses — broken
    // discovery. So gate the reporter the same way: direct members only, fail
    // closed. Keeps all three desktop-facing mesh kinds consistent on delegation.
    require_mesh_member(state, reporter_pubkey_hex)
        .await
        .map_err(|_| {
            "restricted: delegated identities cannot report mesh status in v1".to_string()
        })?;

    let payload: serde_json::Value = serde_json::from_str(&event.content)
        .map_err(|e| format!("invalid: mesh status report content is not JSON ({e})"))?;
    crate::mesh_status_publisher::publish_mesh_status_from_payload(
        state,
        reporter_pubkey_hex,
        &payload,
    )
    .await
    .map_err(|e| {
        tracing::warn!("mesh status report publish failed: {e}");
        "error: failed to publish mesh status".to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_request() {
        let c = r#"{"self_endpoint_addr":"AAA","peer_endpoint_addr":"BBB","attempt_id":"x1"}"#;
        let r = parse_connect_request(c).unwrap();
        assert_eq!(r.self_endpoint_addr, "AAA");
        assert_eq!(r.peer_endpoint_addr, "BBB");
        assert_eq!(r.attempt_id, "x1");
    }

    #[test]
    fn parse_rejects_missing_field() {
        let c = r#"{"self_endpoint_addr":"AAA","attempt_id":"x1"}"#;
        assert!(matches!(
            parse_connect_request(c),
            Err(RequestError::Malformed(_))
        ));
    }

    #[test]
    fn parse_rejects_empty_field() {
        let c = r#"{"self_endpoint_addr":"","peer_endpoint_addr":"BBB","attempt_id":"x1"}"#;
        assert!(matches!(
            parse_connect_request(c),
            Err(RequestError::Malformed(_))
        ));
    }

    #[test]
    fn parse_rejects_non_json() {
        assert!(matches!(
            parse_connect_request("not json"),
            Err(RequestError::Malformed(_))
        ));
    }

    #[test]
    fn call_me_now_content_shape() {
        let s = call_me_now_content("ENDPOINT", None, "att-1", 1234);
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["type"], "sprout-iroh-call-me-now");
        assert_eq!(v["peer_endpoint_addr"], "ENDPOINT");
        assert_eq!(v["attempt_id"], "att-1");
        assert_eq!(v["expires_at"], 1234);
        assert_eq!(v["v"], 1);
        // No endpoint id supplied → field omitted entirely.
        assert!(v.get("peer_endpoint_id").is_none());
    }

    #[test]
    fn call_me_now_content_includes_endpoint_id_when_present() {
        let s = call_me_now_content("ENDPOINT", Some("EID-7"), "att-1", 1234);
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["peer_endpoint_id"], "EID-7");
    }

    #[test]
    fn parse_round_trips_optional_endpoint_ids() {
        let c = r#"{"self_endpoint_addr":"A","peer_endpoint_addr":"B","self_endpoint_id":"SI","peer_endpoint_id":"PI","attempt_id":"x"}"#;
        let r = parse_connect_request(c).unwrap();
        assert_eq!(r.self_endpoint_id.as_deref(), Some("SI"));
        assert_eq!(r.peer_endpoint_id.as_deref(), Some("PI"));
        // endpoint ids are optional — absent is fine.
        let c2 = r#"{"self_endpoint_addr":"A","peer_endpoint_addr":"B","attempt_id":"x"}"#;
        let r2 = parse_connect_request(c2).unwrap();
        assert_eq!(r2.self_endpoint_id, None);
        assert_eq!(r2.peer_endpoint_id, None);
    }

    // ── Trust gate: membership_admits_mesh ──────────────────────────────────
    // This is the single pure predicate behind the requester, target, AND
    // reporter gates. v1 admits only direct relay members (or open relays);
    // NIP-OA-delegated (ViaOwner) and Denied are excluded, symmetrically.

    #[test]
    fn member_and_open_relay_are_admitted() {
        assert!(membership_admits_mesh(&MembershipDecision::Member));
        assert!(membership_admits_mesh(&MembershipDecision::OpenRelay));
    }

    #[test]
    fn denied_is_not_admitted() {
        assert!(!membership_admits_mesh(&MembershipDecision::Denied));
    }

    #[test]
    fn via_owner_is_not_admitted_in_v1() {
        // Delegated identities are excluded from v1 mesh on every desktop-facing
        // path (requester / target / reporter all run through this predicate).
        let owner = nostr::Keys::generate().public_key();
        assert!(!membership_admits_mesh(&MembershipDecision::ViaOwner(
            owner
        )));
    }

    #[test]
    fn extract_target_takes_the_p_tag() {
        let keys = nostr::Keys::generate();
        let target = nostr::Keys::generate().public_key().to_hex();
        let event = nostr::EventBuilder::new(nostr::Kind::Custom(24621), "{}")
            .tags([nostr::Tag::parse(["p", &target]).unwrap()])
            .sign_with_keys(&keys)
            .unwrap();
        assert_eq!(
            extract_target_pubkey(&event).as_deref(),
            Some(target.as_str())
        );
    }

    #[test]
    fn extract_target_none_without_p_tag() {
        let keys = nostr::Keys::generate();
        let event = nostr::EventBuilder::new(nostr::Kind::Custom(24621), "{}")
            .sign_with_keys(&keys)
            .unwrap();
        assert_eq!(extract_target_pubkey(&event), None);
    }
}
