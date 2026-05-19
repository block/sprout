//! Mesh-LLM compute offer envelope (kind:31990 event content).
//!
//! Published by Sprout members willing to share their local LLM/compute with
//! the rest of the relay. Consumers (other Sprout members) subscribe to
//! kind:31990 events scoped to relay membership and pick an offer that
//! matches their request.
//!
//! # Schema
//!
//! The event content is a JSON-serialised [`MeshLlmOffer`]. The event itself
//! is a NIP-33 parameterized-replaceable event addressed by
//! `(pubkey, kind:31990, d_tag)` where `d_tag` is the [`MeshLlmOffer::d_tag`].
//! This means a member can replace their own offer atomically (e.g. when the
//! VRAM cap changes or a model is loaded/unloaded) without leaking dangling
//! stale offers.
//!
//! # Trust model
//!
//! The signing pubkey of the kind:31990 event is the Nostr identity of the
//! offering member; the event flows through the existing NIP-43 fan-out, so
//! only relay members ever see it. The iroh [`endpoint_id`](MeshLlmOffer::endpoint_id)
//! is a separate ed25519 keypair under the same member's control — the
//! Nostr signature on the kind:31990 event is what binds those two
//! identities together.
//!
//! When a consumer connects to the offered iroh endpoint, the consumer's own
//! NIP-98 bearer (signed with its Nostr key, NOT its iroh key) is what the
//! receiving relay uses to gate admission. So the chain of trust is:
//!
//! - The 31990 event proves "Nostr pubkey N offers compute via iroh endpoint E".
//! - The NIP-98 bearer on the iroh connection proves "Nostr pubkey N' is the
//!   connecting party".
//! - Sprout's [`check_relay_membership`] confirms N' is a relay member.
//!
//! There is no need to also bind N' ↔ iroh-client-endpoint cryptographically:
//! once the membership decision allows the connection, the QUIC stream itself
//! is end-to-end-encrypted between the two iroh endpoints. The offering side
//! sees only `(member-pubkey N', iroh-endpoint E')`, both authenticated.

use serde::{Deserialize, Serialize};

/// The full content of a kind:31990 event.
///
/// Serialized to JSON and placed in the event's `content` field. The event's
/// `d` tag should equal [`MeshLlmOffer::d_tag`] so the event is a stable
/// addressable replacement target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeshLlmOffer {
    /// Schema version. Bumped on breaking changes. Current: `1`.
    pub v: u32,

    /// Stable identifier for *this offering node* under the publisher's
    /// pubkey. A member may publish multiple offers (e.g. one per host they
    /// own, or one per GPU); each gets a distinct `d_tag`.
    ///
    /// MUST be ≤64 chars, ASCII alphanumeric + `-` + `_`. The same value
    /// must be used as the kind:31990 event's `d` tag so replaces are
    /// atomic.
    pub d_tag: String,

    /// Iroh endpoint id (ed25519 public key, base32 z-base form as iroh
    /// renders it) of the offering node's iroh endpoint. Consumers dial
    /// this through an iroh `EndpointAddr` constructed from
    /// `(endpoint_id, iroh_relay_url)`.
    pub endpoint_id: String,

    /// Iroh relay URL through which the offering endpoint is reachable.
    ///
    /// This is the *Sprout-hosted* iroh-relay URL — copied verbatim from
    /// the publisher's view of NIP-11 `iroh_relay_url`. The field is
    /// preserved for future cross-relay bridging, but **v1 consumers MUST
    /// ignore offers whose `iroh_relay_url` doesn't match the current
    /// relay's NIP-11 `iroh_relay_url`** (see [`Self::matches_local_relay`]).
    /// This keeps "one relay = one mesh boundary" as an enforced invariant
    /// until cross-relay membership is explicitly designed.
    pub iroh_relay_url: String,

    /// Unix-seconds timestamp at which this offer becomes stale. Consumers
    /// MUST ignore offers where `expires_at <= now` (publishers SHOULD
    /// republish a fresh offer well before this deadline to act as a
    /// heartbeat). Because crashed publishers cannot send the NIP-33
    /// delete-by-replace tombstone, this TTL is the only thing that
    /// removes their offers from the consumer view.
    pub expires_at: u64,

    /// Resource caps the offering side promises to honour for any single
    /// consumer at a time. The publisher should re-publish (replacing the
    /// previous event) whenever these change materially. **These are
    /// claims/UI hints, not authority** — the provider runtime must
    /// enforce its own caps locally; the consumer cannot rely on the
    /// publisher to honour them at admission time.
    pub caps: ResourceCaps,

    /// Models this node is willing to serve. Empty list = "negotiate at
    /// connect time"; non-empty = the consumer should pick one of these.
    #[serde(default)]
    pub models: Vec<ModelOffer>,

    /// Free-form opaque metadata field, reserved for future extensions
    /// (e.g. region, accelerator type, presence-style state).
    ///
    /// Stored as `serde_json::Value` so additions don't require a schema
    /// bump. `deny_unknown_fields` above keeps the *top-level* schema
    /// strict; freeform extension lives here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Value>,
}

/// Resource caps the offering side commits to for a single consumer.
///
/// Caps are *per-consumer* upper bounds — the offering side may host
/// multiple concurrent consumers, each subject to these caps. The
/// `max_concurrency` field expresses how many concurrent consumers the node
/// will accept across all consumers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceCaps {
    /// Max VRAM (megabytes) the offering side will commit to a single
    /// request. `None` = no cap advertised (consumer decides whether to
    /// proceed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_vram_mb: Option<u32>,

    /// Max system RAM (megabytes) the offering side will commit to a
    /// single request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_ram_mb: Option<u32>,

    /// Max number of concurrent consumers the offering node will accept
    /// across all currently-running requests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrency: Option<u32>,
}

/// A single model the offering node is prepared to serve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelOffer {
    /// Model identifier. Convention: HuggingFace-style `org/name[:tag]`,
    /// or `local:<filename>` for ad-hoc local files. Free-form string;
    /// the consumer side is responsible for matching this against its own
    /// requested model.
    pub id: String,

    /// Optional human-readable label for UI surfaces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,

    /// Approximate context window this model serves (tokens). Used for
    /// UI hints; not enforced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_tokens: Option<u32>,
}

impl MeshLlmOffer {
    /// Maximum length of a `d_tag` string. Mirrors NIP-33's general rule
    /// that `d` tags should be short and stable.
    pub const MAX_D_TAG_LEN: usize = 64;

    /// Validate that a `d_tag` is well-formed: ≤64 chars, ASCII
    /// alphanumeric / `-` / `_`.
    pub fn is_valid_d_tag(d_tag: &str) -> bool {
        !d_tag.is_empty()
            && d_tag.len() <= Self::MAX_D_TAG_LEN
            && d_tag
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    }

    /// Returns true if every required field is well-formed for publishing.
    ///
    /// This is a *publisher-side* sanity check; consumers should be
    /// permissive in what they accept as long as serde-deserialization
    /// succeeds (modulo the [`Self::is_expired`] and
    /// [`Self::matches_local_relay`] filters below).
    pub fn is_publishable(&self) -> bool {
        self.v == 1
            && Self::is_valid_d_tag(&self.d_tag)
            && !self.endpoint_id.is_empty()
            && !self.iroh_relay_url.is_empty()
            && self.expires_at > 0
    }

    /// Consumer-side TTL check. Returns `true` when `expires_at <= now`,
    /// in which case the offer must be ignored (crashed publishers can't
    /// send a delete-by-replace tombstone; the TTL is the only reaper).
    ///
    /// `now` is unix-seconds — callers in this crate pass `Timestamp::now()`
    /// or a test clock to avoid pulling `std::time::SystemTime` into the
    /// trust path.
    pub fn is_expired(&self, now: u64) -> bool {
        self.expires_at <= now
    }

    /// Consumer-side same-relay filter for v1 discovery. Returns `true`
    /// when the offer's advertised `iroh_relay_url` matches `current_relay`
    /// after canonicalisation (lower-case scheme/host, trailing slash on
    /// path collapsed, query/fragment dropped).
    ///
    /// **v1 consumers MUST ignore offers where this returns `false`.** The
    /// invariant is "one relay = one mesh boundary"; cross-relay bridging
    /// is reserved for a future explicit design.
    pub fn matches_local_relay(&self, current_relay: &str) -> bool {
        canonical_relay_url(&self.iroh_relay_url) == canonical_relay_url(current_relay)
    }
}

/// Lightweight URL canonicaliser used only for the same-relay filter. Not
/// to be confused with [`sprout_auth::nip98_canonical_url`], which has a
/// different job (computing the `u`-tag value); this one just strips
/// query/fragment, lower-cases scheme/host, and collapses one trailing
/// slash so users who paste `https://r.example.com/iroh/` see it match
/// `https://r.example.com/iroh`.
fn canonical_relay_url(raw: &str) -> String {
    let trimmed = raw.trim();
    // Split off query + fragment. Compose the splits in two steps so the
    // second split operates on the result of the first, not on `trimmed`.
    let no_query = match trimmed.split_once('?') {
        Some((h, _)) => h,
        None => trimmed,
    };
    let no_frag = match no_query.split_once('#') {
        Some((h, _)) => h,
        None => no_query,
    };
    let head = if let Some(stripped) = no_frag.strip_suffix('/') {
        // Only strip *one* trailing slash — don't collapse repeated
        // slashes (those are real path components).
        stripped
    } else {
        no_frag
    };

    // Lower-case the scheme+authority portion; leave the path case-sensitive.
    if let Some(idx) = head.find("://") {
        let (scheme, rest) = head.split_at(idx);
        // rest starts with "://"; authority ends at next '/'.
        let after_proto = &rest[3..];
        let (authority, path) = match after_proto.find('/') {
            Some(p) => after_proto.split_at(p),
            None => (after_proto, ""),
        };
        format!(
            "{}://{}{}",
            scheme.to_ascii_lowercase(),
            authority.to_ascii_lowercase(),
            path,
        )
    } else {
        head.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> MeshLlmOffer {
        MeshLlmOffer {
            v: 1,
            d_tag: "node-1".to_string(),
            endpoint_id: "1234abcd".to_string(),
            iroh_relay_url: "https://relay.example.com/iroh".to_string(),
            expires_at: 2_000_000_000, // far-future fixture
            caps: ResourceCaps {
                max_vram_mb: Some(24_000),
                max_ram_mb: Some(64_000),
                max_concurrency: Some(2),
            },
            models: vec![ModelOffer {
                id: "meta-llama/Llama-3-8B".to_string(),
                label: Some("Llama 3 8B".to_string()),
                context_tokens: Some(8192),
            }],
            extra: None,
        }
    }

    #[test]
    fn round_trip_via_json() {
        let offer = sample();
        let s = serde_json::to_string(&offer).expect("serialise");
        let back: MeshLlmOffer = serde_json::from_str(&s).expect("deserialise");
        assert_eq!(offer, back);
    }

    #[test]
    fn optional_caps_default_to_none() {
        let s = r#"{
            "v": 1,
            "d_tag": "x",
            "endpoint_id": "abc",
            "iroh_relay_url": "https://r/",
            "expires_at": 2000000000,
            "caps": {}
        }"#;
        let offer: MeshLlmOffer = serde_json::from_str(s).expect("deserialise minimal");
        assert!(offer.caps.max_vram_mb.is_none());
        assert!(offer.caps.max_ram_mb.is_none());
        assert!(offer.caps.max_concurrency.is_none());
        assert!(offer.models.is_empty());
    }

    #[test]
    fn unknown_top_level_field_rejected() {
        // deny_unknown_fields catches schema drift.
        let s = r#"{
            "v": 1,
            "d_tag": "x",
            "endpoint_id": "abc",
            "iroh_relay_url": "https://r",
            "expires_at": 2000000000,
            "caps": {},
            "wat": "lol"
        }"#;
        assert!(serde_json::from_str::<MeshLlmOffer>(s).is_err());
    }

    #[test]
    fn unknown_caps_field_rejected() {
        let s = r#"{
            "v": 1,
            "d_tag": "x",
            "endpoint_id": "abc",
            "iroh_relay_url": "https://r",
            "expires_at": 2000000000,
            "caps": { "wat": 7 }
        }"#;
        assert!(serde_json::from_str::<MeshLlmOffer>(s).is_err());
    }

    #[test]
    fn expires_at_required() {
        // expires_at has no serde default; missing it is a hard error
        // (consumers depend on TTL for correctness).
        let s = r#"{
            "v": 1,
            "d_tag": "x",
            "endpoint_id": "abc",
            "iroh_relay_url": "https://r",
            "caps": {}
        }"#;
        assert!(serde_json::from_str::<MeshLlmOffer>(s).is_err());
    }

    #[test]
    fn is_expired_filter() {
        let mut offer = sample();
        offer.expires_at = 1_000;
        assert!(offer.is_expired(2_000), "now > expires_at must expire");
        assert!(offer.is_expired(1_000), "now == expires_at must expire");
        assert!(!offer.is_expired(999), "now < expires_at must not expire");
    }

    #[test]
    fn matches_local_relay_canonicalises() {
        let mut offer = sample();
        offer.iroh_relay_url = "https://relay.example.com/iroh".to_string();
        // exact match
        assert!(offer.matches_local_relay("https://relay.example.com/iroh"));
        // trailing slash on one side
        assert!(offer.matches_local_relay("https://relay.example.com/iroh/"));
        // upper-case host
        assert!(offer.matches_local_relay("HTTPS://Relay.Example.COM/iroh"));
        // query/fragment stripped
        assert!(offer.matches_local_relay("https://relay.example.com/iroh?x=1#y"));
        // different host -> reject
        assert!(!offer.matches_local_relay("https://other.example.com/iroh"));
        // different path -> reject (different mesh boundary)
        assert!(!offer.matches_local_relay("https://relay.example.com/other"));
    }

    #[test]
    fn is_publishable_rejects_zero_expires_at() {
        let mut offer = sample();
        offer.expires_at = 0;
        assert!(!offer.is_publishable());
    }

    #[test]
    fn extra_freeform_passes_through() {
        let offer = MeshLlmOffer {
            extra: Some(serde_json::json!({"region": "us-east", "gpu": "H100"})),
            ..sample()
        };
        let s = serde_json::to_string(&offer).unwrap();
        let back: MeshLlmOffer = serde_json::from_str(&s).unwrap();
        assert_eq!(offer, back);
    }

    #[test]
    fn d_tag_validation() {
        assert!(MeshLlmOffer::is_valid_d_tag("node-1"));
        assert!(MeshLlmOffer::is_valid_d_tag("a"));
        assert!(MeshLlmOffer::is_valid_d_tag(&"a".repeat(64)));
        assert!(!MeshLlmOffer::is_valid_d_tag(""));
        assert!(!MeshLlmOffer::is_valid_d_tag(&"a".repeat(65)));
        assert!(!MeshLlmOffer::is_valid_d_tag("node 1"));
        assert!(!MeshLlmOffer::is_valid_d_tag("node/1"));
        assert!(!MeshLlmOffer::is_valid_d_tag("nodé"));
    }

    #[test]
    fn is_publishable_rejects_bad_d_tag() {
        let mut offer = sample();
        offer.d_tag = "bad tag with spaces".to_string();
        assert!(!offer.is_publishable());
    }

    #[test]
    fn is_publishable_rejects_wrong_version() {
        let mut offer = sample();
        offer.v = 2;
        assert!(!offer.is_publishable());
    }

    #[test]
    fn is_publishable_rejects_empty_endpoint() {
        let mut offer = sample();
        offer.endpoint_id = String::new();
        assert!(!offer.is_publishable());
    }
}
