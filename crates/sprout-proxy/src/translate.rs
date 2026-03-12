//! Event translation between Sprout internal format and NIP-28 standard format.
//!
//! # Overview
//!
//! The proxy sits between standard Nostr clients (NIP-28) and the Sprout relay.
//! Sprout stores messages as kind:40001 events with `#h <uuid>` channel tags.
//! NIP-28 clients expect kind:42 events with `#e <kind40_event_id>` channel tags.
//!
//! This module handles bidirectional translation:
//!
//! - **Outbound** (Sprout → client): kind:40001 + `#h(uuid)` → kind:42 + `#e(event_id)`
//! - **Inbound** (client → Sprout): kind:42 + `#e(event_id)` → kind:40001 + `#h(uuid)`
//!
//! All translated events are re-signed with deterministic shadow keys so that
//! each external user maps to a consistent shadow pubkey across sessions.

use std::sync::Arc;

use nostr::prelude::*;
use sprout_core::kind::{
    event_kind_u32, KIND_STREAM_MESSAGE, KIND_STREAM_MESSAGE_EDIT, KIND_STREAM_MESSAGE_V2,
};
use uuid::Uuid;

use crate::channel_map::ChannelMap;
use crate::kind_translator::KindTranslator;
use crate::shadow_keys::ShadowKeyManager;
use crate::ProxyError;

// ─── Translator ──────────────────────────────────────────────────────────────

/// Translates events and filters between Sprout internal format and NIP-28
/// standard format.
///
/// All translated events are re-signed with shadow keys to preserve per-user
/// identity while hiding internal key material from external clients.
pub struct Translator {
    kind_translator: KindTranslator,
    shadow_keys: Arc<ShadowKeyManager>,
    channel_map: Arc<ChannelMap>,
}

impl Translator {
    /// Create a new [`Translator`].
    pub fn new(shadow_keys: Arc<ShadowKeyManager>, channel_map: Arc<ChannelMap>) -> Self {
        Self {
            kind_translator: KindTranslator::new(),
            shadow_keys,
            channel_map,
        }
    }
}

// ─── Outbound translation (Sprout → NIP-28 client) ───────────────────────────

impl Translator {
    /// Translate a Sprout event to NIP-28 format for delivery to external clients.
    ///
    /// Returns `Ok(Some(event))` on success, `Ok(None)` if the event should be
    /// silently dropped (unknown kind, no channel tag, etc.), or an error if the
    /// event references a channel the client is not allowed to see.
    ///
    /// # Translation rules
    ///
    /// - kind:40001 / kind:40002 → kind:42
    /// - `#h <uuid>` tag → `#e <kind40_event_id>` tag
    /// - All other tags are preserved unchanged
    /// - Re-signed with the shadow key for the original author's pubkey
    /// - V2 rich content (JSON with `"text"` field) is unwrapped to plain text
    pub fn translate_outbound(
        &self,
        event: &Event,
        allowed_channels: &[Uuid],
    ) -> Result<Option<Event>, ProxyError> {
        let kind_u32 = event_kind_u32(event);

        // Only translate stream messages and edits; pass through or drop everything else.
        let is_stream_message =
            kind_u32 == KIND_STREAM_MESSAGE || kind_u32 == KIND_STREAM_MESSAGE_V2;
        let is_edit = kind_u32 == KIND_STREAM_MESSAGE_EDIT;

        if !is_stream_message && !is_edit {
            if !self.kind_translator.is_translatable(kind_u32) {
                // Unknown / non-translatable kind — pass through as-is.
                return Ok(Some(event.clone()));
            }
            // Translatable but unhandled kind — drop silently.
            return Ok(None);
        }

        // Extract channel UUID from the `#h` tag.
        let uuid_str = event
            .tags
            .iter()
            .find(|t| {
                let s = t.as_slice();
                s.len() >= 2 && s[0] == "h"
            })
            .and_then(|t| t.as_slice().get(1).cloned());

        let uuid_str = match uuid_str {
            Some(s) => s,
            None => return Ok(None), // No channel tag — drop silently.
        };

        let uuid: Uuid = uuid_str
            .parse()
            .map_err(|_| ProxyError::ChannelNotFound(uuid_str.clone()))?;

        // Enforce channel-level access control.
        if !allowed_channels.contains(&uuid) {
            return Err(ProxyError::PermissionDenied(format!(
                "channel {} not in invite scope",
                uuid
            )));
        }

        // Resolve the kind:40 event ID for this channel.
        let channel_info = self
            .channel_map
            .lookup_by_uuid(&uuid)
            .ok_or_else(|| ProxyError::ChannelNotFound(uuid.to_string()))?;

        // Build translated tag list: replace the channel `#h` with `#e`, keep everything else.
        let mut new_tags: Vec<Tag> = Vec::new();
        new_tags.push(
            Tag::parse(&["e", &channel_info.kind40_event_id]).expect("e tag is always valid"),
        );
        for tag in event.tags.iter() {
            let s = tag.as_slice();
            // Only strip the specific `#h` tag whose value matches the channel UUID.
            // Preserve any other `#h` tags.
            if s.first().map(|v| v.as_str()) == Some("h")
                && s.get(1).map(|v| v.as_str()) == Some(uuid_str.as_str())
            {
                continue; // Drop only the channel `#h` tag.
            }
            new_tags.push(tag.clone());
        }

        // Translate kind number.
        let standard_kind = self.kind_translator.to_standard(kind_u32);

        // Unwrap V2 rich content to plain text; V1 and edit content passes through.
        let content = if kind_u32 == KIND_STREAM_MESSAGE_V2 {
            extract_plain_text(&event.content)
        } else {
            event.content.clone()
        };

        // Re-sign with the shadow key for the original author.
        let shadow_keys = self.shadow_keys.get_or_create(&event.pubkey.to_hex())?;

        let translated = EventBuilder::new(
            Kind::Custom(u16::try_from(standard_kind).expect("standard kind must fit in u16")),
            content,
            new_tags,
        )
        .custom_created_at(event.created_at)
        .sign_with_keys(&shadow_keys)
        .map_err(|e| ProxyError::Upstream(format!("outbound signing failed: {e}")))?;

        Ok(Some(translated))
    }
}

// ─── Inbound translation (NIP-28 client → Sprout) ────────────────────────────

impl Translator {
    /// Translate a NIP-28 event from an external client into Sprout format.
    ///
    /// # Translation rules
    ///
    /// - kind:42 (or kind:1) → kind:40001
    /// - `#e <kind40_event_id>` tag → `#h <uuid>` tag
    /// - All other tags are preserved unchanged
    /// - Re-signed with the shadow key for `external_pubkey`
    ///
    /// Returns an error if the event kind is not accepted, the `#e` tag is
    /// missing or references an unknown channel, or the channel is not in the
    /// client's invite scope.
    pub fn translate_inbound(
        &self,
        event: &Event,
        external_pubkey: &str,
        allowed_channels: &[Uuid],
    ) -> Result<Event, ProxyError> {
        let kind_u32 = event.kind.as_u16() as u32;

        // Accept kind:42 (channel message), kind:41 (channel metadata edit),
        // and kind:1 (text note). Everything else is rejected.
        if kind_u32 != 42 && kind_u32 != 41 && kind_u32 != 1 {
            return Err(ProxyError::PermissionDenied(format!(
                "kind {} not accepted by proxy (expected 1, 41, or 42)",
                kind_u32
            )));
        }

        // Find the channel-reference `#e` tag by looking up each `#e` value
        // against the channel map. This is more robust than assuming the first
        // `#e` tag is always the channel reference — reply/thread `#e` tags may
        // appear in any order depending on the client.
        let (event_id_str, channel_info) = event
            .tags
            .iter()
            .filter(|t| {
                let s = t.as_slice();
                s.len() >= 2 && s[0] == "e"
            })
            .find_map(|t| {
                let eid = t.as_slice().get(1)?;
                let info = self.channel_map.lookup_by_event_id(eid)?;
                Some((eid.clone(), info))
            })
            .ok_or_else(|| {
                ProxyError::PermissionDenied(
                    "kind:42 event must have an #e tag referencing a known channel".into(),
                )
            })?;

        // Enforce channel-level access control.
        if !allowed_channels.contains(&channel_info.uuid) {
            return Err(ProxyError::PermissionDenied(format!(
                "channel {} not in invite scope",
                channel_info.uuid
            )));
        }

        // Build translated tag list: replace the channel `#e` with `#h`, keep everything else.
        let mut new_tags: Vec<Tag> = Vec::new();
        new_tags.push(
            Tag::parse(&["h", &channel_info.uuid.to_string()]).expect("h tag is always valid"),
        );
        for tag in event.tags.iter() {
            let s = tag.as_slice();
            // Only strip the specific `#e` tag whose value matches the channel event ID.
            // Preserve other `#e` tags (e.g. NIP-10 reply threading).
            if s.first().map(|v| v.as_str()) == Some("e")
                && s.get(1).map(|v| v.as_str()) == Some(event_id_str.as_str())
            {
                continue; // Drop only the channel `#e` tag.
            }
            new_tags.push(tag.clone());
        }

        // Translate kind number.
        let sprout_kind = self.kind_translator.to_sprout(kind_u32);

        // Re-sign with the shadow key for the external user.
        let shadow_keys = self.shadow_keys.get_or_create(external_pubkey)?;

        let translated = EventBuilder::new(
            Kind::Custom(u16::try_from(sprout_kind).expect("sprout kind must fit in u16")),
            &event.content,
            new_tags,
        )
        .custom_created_at(event.created_at)
        .sign_with_keys(&shadow_keys)
        .map_err(|e| ProxyError::Upstream(format!("inbound signing failed: {e}")))?;

        Ok(translated)
    }
}

// ─── Filter translation ───────────────────────────────────────────────────────

impl Translator {
    /// Translate a NIP-28 REQ filter to Sprout format.
    ///
    /// - kind:42 / kind:1 → kind:40001
    /// - Injects `#h` tag filters from `allowed_channels` so the client can
    ///   only receive events from channels they have access to.
    ///
    /// The returned filter is ready to be forwarded to the upstream Sprout relay.
    pub fn translate_filter_inbound(
        &self,
        filter: &Filter,
        allowed_channels: &[Uuid],
    ) -> Filter {
        // Start with a clone and rebuild the kinds set.
        let mut f = filter.clone();

        if let Some(ref kinds) = filter.kinds {
            let new_kinds: Vec<Kind> = kinds
                .iter()
                .map(|k| {
                    let k_u32 = k.as_u16() as u32;
                    let sprout_k = self.kind_translator.to_sprout(k_u32);
                    Kind::Custom(u16::try_from(sprout_k).expect("sprout kind must fit in u16"))
                })
                .collect();
            // Rebuild via the builder to stay consistent with nostr's internal state.
            f = f.remove_kinds(kinds.iter().cloned()).kinds(new_kinds);
        }

        // Check for client-supplied #e channel filters and translate to #h UUIDs.
        //
        // NIP-28 clients filter by channel using `#e <kind40_event_id>`. Sprout
        // uses `#h <uuid>` instead. If the client specified `#e` values, resolve
        // them to UUIDs (intersected with allowed_channels) and use those for the
        // `#h` injection. The `#e` filter must be removed from the translated
        // filter — if both `#e` and `#h` were present, the relay would AND them,
        // and since Sprout events carry `#h` but not `#e`, zero events would match.
        let e_tag_key = SingleLetterTag::lowercase(Alphabet::E);
        let had_e_filter = f.generic_tags.contains_key(&e_tag_key);
        let client_channel_uuids: Vec<String> =
            if let Some(e_values) = f.generic_tags.get(&e_tag_key) {
                e_values
                    .iter()
                    .filter_map(|event_id| self.channel_map.lookup_by_event_id(event_id))
                    .filter(|info| allowed_channels.contains(&info.uuid))
                    .map(|info| info.uuid.to_string())
                    .collect()
            } else {
                Vec::new()
            };

        // Remove the #e tag filter — Sprout uses #h, not #e.
        f.generic_tags.remove(&e_tag_key);

        // Inject #h tag constraints from the allowed channel list.
        // Three cases:
        //   1. Client specified #e and some resolved → use those (already intersected)
        //   2. Client specified #e but NONE resolved → deny-all (sentinel UUID)
        //   3. Client didn't specify #e → use all allowed_channels (or sentinel if empty)
        //
        // Case 2 is critical: an explicit filter that resolves to nothing must
        // return zero results, not widen to all channels.
        let sentinel = vec!["00000000-0000-0000-0000-000000000000".to_string()];
        let uuid_strings: Vec<String> = if !client_channel_uuids.is_empty() {
            // Case 1: client asked for specific channels, some resolved
            client_channel_uuids
        } else if had_e_filter {
            // Case 2: client asked for specific channels, none resolved → deny all
            sentinel
        } else if allowed_channels.is_empty() {
            // Case 3 with empty scope → deny all
            sentinel
        } else {
            // Case 3: no #e filter, use full allowed scope
            allowed_channels.iter().map(|u| u.to_string()).collect()
        };
        f = f.custom_tag(SingleLetterTag::lowercase(Alphabet::H), uuid_strings);

        f
    }

    /// Translate a Sprout REQ filter to NIP-28 format (for outbound subscription
    /// forwarding, e.g. when the proxy subscribes on behalf of a client).
    ///
    /// - kind:40001 / kind:40002 → kind:42
    pub fn translate_filter_outbound(&self, filter: &Filter) -> Filter {
        let mut f = filter.clone();

        if let Some(ref kinds) = filter.kinds {
            let new_kinds: Vec<Kind> = kinds
                .iter()
                .map(|k| {
                    let k_u32 = k.as_u16() as u32;
                    let standard_k = self.kind_translator.to_standard(k_u32);
                    Kind::Custom(
                        u16::try_from(standard_k).expect("standard kind must fit in u16"),
                    )
                })
                .collect();
            f = f.remove_kinds(kinds.iter().cloned()).kinds(new_kinds);
        }

        f
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Extract plain text from V2 rich content JSON.
///
/// V2 content is a JSON object with a `"text"` field. Falls back to the raw
/// content string if parsing fails or the field is absent.
fn extract_plain_text(content: &str) -> String {
    serde_json::from_str::<serde_json::Value>(content)
        .ok()
        .and_then(|v| v.get("text").and_then(|t| t.as_str()).map(String::from))
        .unwrap_or_else(|| content.to_string())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channel_map::{ChannelDto, ChannelMap};
    use sprout_core::kind::KIND_STREAM_MESSAGE;

    // ── Test fixtures ────────────────────────────────────────────────────────

    const TEST_UUID: &str = "550e8400-e29b-41d4-a716-446655440000";
    const TEST_SALT: &[u8] = b"test-salt-for-translate-tests";

    fn test_channel_map() -> ChannelMap {
        let keys = Keys::generate();
        let map = ChannelMap::new(keys);
        let dto = ChannelDto {
            id: TEST_UUID.to_string(),
            name: "test-channel".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            visibility: "open".to_string(),
            description: "A test channel".to_string(),
            created_by: "0101010101010101010101010101010101010101010101010101010101010101"
                .to_string(),
        };
        map.register(&dto).expect("test channel registration must succeed");
        map
    }

    fn make_translator() -> (Translator, String) {
        let channel_map = Arc::new(test_channel_map());
        let shadow_mgr = Arc::new(
            ShadowKeyManager::new(TEST_SALT).expect("shadow key manager creation must succeed"),
        );
        let translator = Translator::new(shadow_mgr, channel_map.clone());

        // Retrieve the kind:40 event ID for the test channel.
        let uuid: Uuid = TEST_UUID.parse().unwrap();
        let info = channel_map.lookup_by_uuid(&uuid).unwrap();
        (translator, info.kind40_event_id)
    }

    fn allowed() -> Vec<Uuid> {
        vec![TEST_UUID.parse().unwrap()]
    }

    fn no_channels() -> Vec<Uuid> {
        vec![]
    }

    // ── Test 1: Outbound — kind:40001 + #h → kind:42 + #e ───────────────────

    #[test]
    fn outbound_translates_stream_message() {
        let (translator, kind40_event_id) = make_translator();
        let author_keys = Keys::generate();

        // Build a synthetic kind:40001 event with an #h tag.
        let h_tag = Tag::parse(&["h", TEST_UUID]).unwrap();
        let sprout_event = EventBuilder::new(
            Kind::Custom(KIND_STREAM_MESSAGE as u16),
            "hello world",
            [h_tag],
        )
        .sign_with_keys(&author_keys)
        .unwrap();

        let result = translator
            .translate_outbound(&sprout_event, &allowed())
            .expect("outbound translation must not error");

        let translated = result.expect("should produce a translated event");

        // Kind must be 42.
        assert_eq!(translated.kind.as_u16(), 42, "translated kind must be 42");

        // Must have an #e tag pointing to the kind:40 event ID.
        let has_e_tag = translated.tags.iter().any(|t| {
            let s = t.as_slice();
            s.len() >= 2 && s[0] == "e" && s[1] == kind40_event_id
        });
        assert!(has_e_tag, "translated event must have #e tag with kind:40 event ID");

        // Must NOT have an #h tag.
        let has_h_tag = translated.tags.iter().any(|t| {
            t.as_slice().first().map(|v| v.as_str()) == Some("h")
        });
        assert!(!has_h_tag, "translated event must not retain #h tag");

        // Content must be preserved.
        assert_eq!(translated.content, "hello world");

        // Signature must be valid.
        translated.verify().expect("translated event signature must be valid");
    }

    // ── Test 2: Inbound — kind:42 + #e → kind:40001 + #h ───────────────────

    #[test]
    fn inbound_translates_channel_message() {
        let (translator, kind40_event_id) = make_translator();
        let client_keys = Keys::generate();
        let external_pubkey = client_keys.public_key().to_hex();

        // Build a synthetic kind:42 event with an #e tag.
        let e_tag = Tag::parse(&["e", &kind40_event_id]).unwrap();
        let nip28_event = EventBuilder::new(Kind::Custom(42), "hello from client", [e_tag])
            .sign_with_keys(&client_keys)
            .unwrap();

        let translated = translator
            .translate_inbound(&nip28_event, &external_pubkey, &allowed())
            .expect("inbound translation must not error");

        // Kind must be KIND_STREAM_MESSAGE.
        assert_eq!(
            translated.kind.as_u16(),
            KIND_STREAM_MESSAGE as u16,
            "translated kind must be KIND_STREAM_MESSAGE"
        );

        // Must have an #h tag pointing to the channel UUID.
        let has_h_tag = translated.tags.iter().any(|t| {
            let s = t.as_slice();
            s.len() >= 2 && s[0] == "h" && s[1] == TEST_UUID
        });
        assert!(has_h_tag, "translated event must have #h tag with channel UUID");

        // Must NOT have an #e tag.
        let has_e_tag = translated.tags.iter().any(|t| {
            t.as_slice().first().map(|v| v.as_str()) == Some("e")
        });
        assert!(!has_e_tag, "translated event must not retain #e tag");

        // Content must be preserved.
        assert_eq!(translated.content, "hello from client");

        // Signature must be valid.
        translated.verify().expect("translated event signature must be valid");
    }

    // ── Test 3: Outbound — channel not in allowed_channels → PermissionDenied

    #[test]
    fn outbound_rejects_channel_not_in_scope() {
        let (translator, _) = make_translator();
        let author_keys = Keys::generate();

        let h_tag = Tag::parse(&["h", TEST_UUID]).unwrap();
        let sprout_event = EventBuilder::new(
            Kind::Custom(KIND_STREAM_MESSAGE as u16),
            "secret",
            [h_tag],
        )
        .sign_with_keys(&author_keys)
        .unwrap();

        let result = translator.translate_outbound(&sprout_event, &no_channels());

        assert!(
            matches!(result, Err(ProxyError::PermissionDenied(_))),
            "expected PermissionDenied, got: {:?}",
            result
        );
    }

    // ── Test 4: Inbound — channel not in allowed_channels → PermissionDenied

    #[test]
    fn inbound_rejects_channel_not_in_scope() {
        let (translator, kind40_event_id) = make_translator();
        let client_keys = Keys::generate();
        let external_pubkey = client_keys.public_key().to_hex();

        let e_tag = Tag::parse(&["e", &kind40_event_id]).unwrap();
        let nip28_event = EventBuilder::new(Kind::Custom(42), "sneaky", [e_tag])
            .sign_with_keys(&client_keys)
            .unwrap();

        let result = translator.translate_inbound(&nip28_event, &external_pubkey, &no_channels());

        assert!(
            matches!(result, Err(ProxyError::PermissionDenied(_))),
            "expected PermissionDenied, got: {:?}",
            result
        );
    }

    // ── Test 5: V2 content extraction ────────────────────────────────────────

    #[test]
    fn v2_content_plain_text_extracted() {
        // extract_plain_text is a private helper; test it indirectly via outbound.
        let (translator, _) = make_translator();
        let author_keys = Keys::generate();

        let v2_content = r#"{"text":"hello v2","attachments":[]}"#;
        let h_tag = Tag::parse(&["h", TEST_UUID]).unwrap();
        let sprout_event = EventBuilder::new(
            Kind::Custom(KIND_STREAM_MESSAGE_V2 as u16),
            v2_content,
            [h_tag],
        )
        .sign_with_keys(&author_keys)
        .unwrap();

        let translated = translator
            .translate_outbound(&sprout_event, &allowed())
            .unwrap()
            .expect("should produce translated event");

        assert_eq!(
            translated.content, "hello v2",
            "V2 rich content must be unwrapped to plain text"
        );
    }

    #[test]
    fn v2_content_fallback_on_non_json() {
        // When V2 content is not valid JSON, fall back to raw content.
        let content = extract_plain_text("not json at all");
        assert_eq!(content, "not json at all");
    }

    // ── Test 6: Filter translation — kind:42 → kind:40001 ───────────────────

    #[test]
    fn filter_inbound_translates_kind() {
        let (translator, _) = make_translator();

        let filter = Filter::new().kind(Kind::Custom(42));
        let translated = translator.translate_filter_inbound(&filter, &allowed());

        // The translated filter must contain KIND_STREAM_MESSAGE, not 42.
        let kinds = translated.kinds.as_ref().expect("filter must have kinds");
        assert!(
            kinds.contains(&Kind::Custom(KIND_STREAM_MESSAGE as u16)),
            "filter must contain KIND_STREAM_MESSAGE after translation"
        );
        assert!(
            !kinds.contains(&Kind::Custom(42)),
            "filter must not contain kind:42 after translation"
        );

        // Must have injected #h tag constraints.
        let h_tag = SingleLetterTag::lowercase(Alphabet::H);
        let has_h_filter = translated.generic_tags.contains_key(&h_tag);
        assert!(has_h_filter, "filter must have #h tag constraints injected");
    }

    #[test]
    fn filter_outbound_translates_kind() {
        let (translator, _) = make_translator();

        let filter = Filter::new().kind(Kind::Custom(KIND_STREAM_MESSAGE as u16));
        let translated = translator.translate_filter_outbound(&filter);

        let kinds = translated.kinds.as_ref().expect("filter must have kinds");
        assert!(
            kinds.contains(&Kind::Custom(42)),
            "filter must contain kind:42 after outbound translation"
        );
        assert!(
            !kinds.contains(&Kind::Custom(KIND_STREAM_MESSAGE as u16)),
            "filter must not contain KIND_STREAM_MESSAGE after outbound translation"
        );
    }

    // ── Test 6b: Filter — #e channel ref translates to #h UUID (FIX A) ─────

    #[test]
    fn filter_inbound_translates_e_tag_to_h() {
        let (translator, kind40_event_id) = make_translator();

        // Build a filter that includes a #e tag referencing the test channel's
        // kind:40 event ID — this is how NIP-28 clients subscribe to a channel.
        let e_tag_key = SingleLetterTag::lowercase(Alphabet::E);
        let filter = Filter::new()
            .kind(Kind::Custom(42))
            .custom_tag(e_tag_key.clone(), [kind40_event_id.clone()]);

        let translated = translator.translate_filter_inbound(&filter, &allowed());

        // The #e filter must be removed from the translated filter.
        assert!(
            !translated.generic_tags.contains_key(&e_tag_key),
            "#e tag filter must be removed from translated filter (would cause zero matches on Sprout relay)"
        );

        // The #h filter must be present and contain the channel UUID.
        let h_tag_key = SingleLetterTag::lowercase(Alphabet::H);
        let h_values = translated
            .generic_tags
            .get(&h_tag_key)
            .expect("translated filter must have #h tag constraint");

        assert!(
            h_values.contains(TEST_UUID),
            "#h filter must contain the channel UUID resolved from #e, got: {:?}",
            h_values
        );

        // The translated filter must NOT contain all allowed_channels blindly —
        // it should contain exactly the channel(s) resolved from the #e values.
        assert_eq!(
            h_values.len(),
            1,
            "#h filter must contain exactly the one channel resolved from #e, got: {:?}",
            h_values
        );
    }

    // ── Test 6c: Filter — #e with unknown event ID falls back to allowed_channels

    #[test]
    fn filter_inbound_e_tag_unknown_event_id_denies_all() {
        let (translator, _) = make_translator();

        // Use an event ID that doesn't exist in the channel map.
        let unknown_event_id = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";
        let e_tag_key = SingleLetterTag::lowercase(Alphabet::E);
        let filter = Filter::new()
            .kind(Kind::Custom(42))
            .custom_tag(e_tag_key.clone(), [unknown_event_id]);

        let translated = translator.translate_filter_inbound(&filter, &allowed());

        // The #e filter must be removed.
        assert!(
            !translated.generic_tags.contains_key(&e_tag_key),
            "#e tag filter must be removed even when event ID is unknown"
        );

        // Since the client explicitly specified #e values but none resolved,
        // the filter must deny all — inject the sentinel UUID, NOT fall back
        // to all allowed_channels. Explicit filter → fail closed.
        let h_tag_key = SingleLetterTag::lowercase(Alphabet::H);
        let h_values = translated
            .generic_tags
            .get(&h_tag_key)
            .expect("translated filter must have #h tag constraint");

        let sentinel = "00000000-0000-0000-0000-000000000000";
        assert!(
            h_values.contains(sentinel),
            "unknown #e must inject sentinel UUID (deny-all), got: {:?}",
            h_values
        );
        assert!(
            !h_values.contains(TEST_UUID),
            "unknown #e must NOT fall back to allowed channels"
        );
    }

    // ── Test 7: Inbound — reply #e tags are preserved (FIX 1) ───────────────

    #[test]
    fn inbound_preserves_reply_e_tags() {
        let (translator, kind40_event_id) = make_translator();
        let client_keys = Keys::generate();
        let external_pubkey = client_keys.public_key().to_hex();

        // A reply event has two #e tags: one for the channel, one for the
        // message being replied to (NIP-10 threading).
        let reply_event_id =
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let channel_e_tag = Tag::parse(&["e", &kind40_event_id]).unwrap();
        let reply_e_tag = Tag::parse(&["e", reply_event_id]).unwrap();

        let nip28_event = EventBuilder::new(
            Kind::Custom(42),
            "replying to a message",
            [channel_e_tag, reply_e_tag],
        )
        .sign_with_keys(&client_keys)
        .unwrap();

        let translated = translator
            .translate_inbound(&nip28_event, &external_pubkey, &allowed())
            .expect("inbound translation must not error");

        // Must have the #h tag for the channel.
        let has_h_tag = translated.tags.iter().any(|t| {
            let s = t.as_slice();
            s.len() >= 2 && s[0] == "h" && s[1] == TEST_UUID
        });
        assert!(has_h_tag, "translated event must have #h tag with channel UUID");

        // The channel #e tag must be gone.
        let has_channel_e = translated.tags.iter().any(|t| {
            let s = t.as_slice();
            s.len() >= 2 && s[0] == "e" && s[1] == kind40_event_id
        });
        assert!(!has_channel_e, "channel #e tag must be stripped");

        // The reply #e tag must be preserved.
        let has_reply_e = translated.tags.iter().any(|t| {
            let s = t.as_slice();
            s.len() >= 2 && s[0] == "e" && s[1] == reply_event_id
        });
        assert!(has_reply_e, "reply #e tag must be preserved for NIP-10 threading");

        translated.verify().expect("translated event signature must be valid");
    }

    // ── Test 8: Outbound — non-channel #h tags are preserved (FIX 2) ────────

    #[test]
    fn outbound_preserves_non_channel_h_tags() {
        let (translator, kind40_event_id) = make_translator();
        let author_keys = Keys::generate();

        // An event with the channel #h tag AND an unrelated #h tag.
        let channel_h_tag = Tag::parse(&["h", TEST_UUID]).unwrap();
        let other_h_tag = Tag::parse(&["h", "some-other-value"]).unwrap();

        let sprout_event = EventBuilder::new(
            Kind::Custom(KIND_STREAM_MESSAGE as u16),
            "message with extra h tag",
            [channel_h_tag, other_h_tag],
        )
        .sign_with_keys(&author_keys)
        .unwrap();

        let result = translator
            .translate_outbound(&sprout_event, &allowed())
            .expect("outbound translation must not error");

        let translated = result.expect("should produce a translated event");

        // The channel #h tag must be replaced by #e.
        let has_channel_e = translated.tags.iter().any(|t| {
            let s = t.as_slice();
            s.len() >= 2 && s[0] == "e" && s[1] == kind40_event_id
        });
        assert!(has_channel_e, "channel #e tag must be present");

        // The channel #h tag must be gone.
        let has_channel_h = translated.tags.iter().any(|t| {
            let s = t.as_slice();
            s.len() >= 2 && s[0] == "h" && s[1] == TEST_UUID
        });
        assert!(!has_channel_h, "channel #h tag must be stripped");

        // The unrelated #h tag must be preserved.
        let has_other_h = translated.tags.iter().any(|t| {
            let s = t.as_slice();
            s.len() >= 2 && s[0] == "h" && s[1] == "some-other-value"
        });
        assert!(has_other_h, "non-channel #h tag must be preserved");

        translated.verify().expect("translated event signature must be valid");
    }

    // ── Test 9: Filter — empty allowed_channels injects deny-all (FIX 3) ────

    #[test]
    fn empty_allowed_channels_denies_all() {
        let (translator, _) = make_translator();

        let filter = Filter::new().kind(Kind::Custom(42));
        let translated = translator.translate_filter_inbound(&filter, &no_channels());

        // Must have an #h tag constraint.
        let h_tag = SingleLetterTag::lowercase(Alphabet::H);
        let h_values = translated
            .generic_tags
            .get(&h_tag)
            .expect("filter must have #h tag constraint even with empty allowed_channels");

        // The injected value must be the impossible sentinel UUID.
        assert!(
            h_values.contains("00000000-0000-0000-0000-000000000000"),
            "empty allowed_channels must inject impossible sentinel UUID, got: {:?}",
            h_values
        );
    }

    // ── Test 10: Inbound — kind:41 (edit) → kind:40003 ─────────────────────

    #[test]
    fn inbound_translates_edit_message() {
        use sprout_core::kind::KIND_STREAM_MESSAGE_EDIT;

        let (translator, kind40_event_id) = make_translator();
        let external_keys = Keys::generate();

        // Build a kind:41 event with #e referencing the channel.
        let e_tag = Tag::parse(&["e", &kind40_event_id]).unwrap();
        let nip28_event = EventBuilder::new(Kind::ChannelMetadata, "updated metadata", [e_tag])
            .sign_with_keys(&external_keys)
            .unwrap();

        let translated = translator
            .translate_inbound(&nip28_event, &external_keys.public_key().to_hex(), &allowed())
            .expect("inbound kind:41 must translate");

        // kind:41 → KIND_STREAM_MESSAGE_EDIT (40003)
        assert_eq!(
            translated.kind.as_u16() as u32,
            KIND_STREAM_MESSAGE_EDIT,
            "kind:41 must translate to KIND_STREAM_MESSAGE_EDIT"
        );

        // Must have #h tag (channel UUID), not #e.
        let has_h = translated
            .tags
            .iter()
            .any(|t| t.as_slice().first().map(|v| v.as_str()) == Some("h"));
        assert!(has_h, "translated edit must have #h tag");

        translated
            .verify()
            .expect("translated edit signature must be valid");
    }

    // ── Test 11: Inbound — rejects unknown kinds ─────────────────────────────

    #[test]
    fn inbound_rejects_unknown_kind() {
        let (translator, kind40_event_id) = make_translator();
        let external_keys = Keys::generate();

        let e_tag = Tag::parse(&["e", &kind40_event_id]).unwrap();
        let event = EventBuilder::new(Kind::Custom(9999), "nope", [e_tag])
            .sign_with_keys(&external_keys)
            .unwrap();

        let result =
            translator.translate_inbound(&event, &external_keys.public_key().to_hex(), &allowed());
        assert!(result.is_err(), "kind:9999 must be rejected inbound");
    }

    // ── Test 12: Outbound — kind:40003 (edit) → kind:41 (FIX 4) ─────────────

    #[test]
    fn outbound_translates_edit_message() {
        use sprout_core::kind::KIND_STREAM_MESSAGE_EDIT;

        let (translator, _) = make_translator();
        let author_keys = Keys::generate();

        let h_tag = Tag::parse(&["h", TEST_UUID]).unwrap();
        let sprout_event = EventBuilder::new(
            Kind::Custom(KIND_STREAM_MESSAGE_EDIT as u16),
            "edited content",
            [h_tag],
        )
        .sign_with_keys(&author_keys)
        .unwrap();

        let result = translator
            .translate_outbound(&sprout_event, &allowed())
            .expect("outbound translation of edit must not error");

        let translated = result.expect("edit must produce a translated event, not None");

        // kind:40003 must translate to kind:41 (NIP-28 channel edit).
        assert_eq!(
            translated.kind.as_u16(),
            41,
            "kind:40003 must translate to kind:41"
        );

        // Content must be preserved.
        assert_eq!(translated.content, "edited content");

        // Must have #e tag (channel reference), not #h.
        let has_e_tag = translated.tags.iter().any(|t| {
            t.as_slice().first().map(|v| v.as_str()) == Some("e")
        });
        assert!(has_e_tag, "translated edit must have #e tag");

        let has_h_tag = translated.tags.iter().any(|t| {
            t.as_slice().first().map(|v| v.as_str()) == Some("h")
        });
        assert!(!has_h_tag, "translated edit must not retain #h tag");

        translated.verify().expect("translated edit signature must be valid");
    }
}
