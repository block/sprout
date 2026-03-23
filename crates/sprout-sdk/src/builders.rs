//! The 23 typed event builder functions.
//!
//! All functions return `Result<nostr::EventBuilder, SdkError>`.
//! The caller signs: `builder.sign_with_keys(&keys)?`.

use nostr::{EventBuilder, Kind, Tag};
use uuid::Uuid;

use crate::{DiffMeta, SdkError, ThreadRef, VoteDirection};

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Parse a tag slice, mapping errors to `SdkError::InvalidTag`.
fn tag(parts: &[&str]) -> Result<Tag, SdkError> {
    Tag::parse(parts).map_err(|e| SdkError::InvalidTag(e.to_string()))
}

/// Validate content byte length.
fn check_content(content: &str, max: usize) -> Result<(), SdkError> {
    let got = content.len();
    if got > max {
        return Err(SdkError::ContentTooLarge { max, got });
    }
    Ok(())
}

/// Validate hex string has at least `min_len` hex characters.
fn check_hex_len(s: &str, min_len: usize, field: &str) -> Result<(), SdkError> {
    if s.len() < min_len || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(SdkError::InvalidDiffMeta(format!(
            "{field} must be at least {min_len} hex characters (got {:?})",
            s
        )));
    }
    Ok(())
}

/// Emit NIP-10 e-tags for a `ThreadRef`.
fn thread_tags(thread_ref: &ThreadRef, tags: &mut Vec<Tag>) -> Result<(), SdkError> {
    let root = thread_ref.root_event_id.to_hex();
    let parent = thread_ref.parent_event_id.to_hex();
    if root == parent {
        // Direct reply
        tags.push(tag(&["e", &root, "", "reply"])?);
    } else {
        // Nested reply
        tags.push(tag(&["e", &root, "", "root"])?);
        tags.push(tag(&["e", &parent, "", "reply"])?);
    }
    Ok(())
}

/// Deduplicate and cap mentions, emitting p-tags.
fn mention_tags(mentions: &[&str], tags: &mut Vec<Tag>) -> Result<(), SdkError> {
    if mentions.len() > 50 {
        return Err(SdkError::TooManyMentions);
    }
    let mut seen = std::collections::HashSet::new();
    for &hex in mentions {
        let lower = hex.to_ascii_lowercase();
        if seen.insert(lower.clone()) {
            tags.push(tag(&["p", &lower])?);
        }
    }
    Ok(())
}

/// Emit imeta tags from raw tag vectors.
fn imeta_tags(media_tags: &[Vec<String>], tags: &mut Vec<Tag>) -> Result<(), SdkError> {
    for mt in media_tags {
        let parts: Vec<&str> = mt.iter().map(String::as_str).collect();
        tags.push(Tag::parse(&parts).map_err(|e| SdkError::InvalidTag(e.to_string()))?);
    }
    Ok(())
}

// ── Builder 1: build_message ─────────────────────────────────────────────────

/// Build a stream message (kind 9).
///
/// - `channel_id`: target channel UUID
/// - `content`: message text (max 64 KiB)
/// - `thread_ref`: optional NIP-10 reply context
/// - `mentions`: pubkey hex strings to p-tag (deduped, max 50)
/// - `broadcast`: if true, adds `["broadcast", "1"]` tag
/// - `media_tags`: raw imeta tag vectors
pub fn build_message(
    channel_id: Uuid,
    content: &str,
    thread_ref: Option<&ThreadRef>,
    mentions: &[&str],
    broadcast: bool,
    media_tags: &[Vec<String>],
) -> Result<EventBuilder, SdkError> {
    check_content(content, 64 * 1024)?;
    let mut tags = vec![tag(&["h", &channel_id.to_string()])?];
    if let Some(tr) = thread_ref {
        thread_tags(tr, &mut tags)?;
    }
    mention_tags(mentions, &mut tags)?;
    if broadcast {
        tags.push(tag(&["broadcast", "1"])?);
    }
    imeta_tags(media_tags, &mut tags)?;
    Ok(EventBuilder::new(Kind::Custom(9), content, tags))
}

// ── Builder 2: build_forum_post ───────────────────────────────────────────────

/// Build a forum post thread root (kind 45001).
pub fn build_forum_post(
    channel_id: Uuid,
    content: &str,
    mentions: &[&str],
    media_tags: &[Vec<String>],
) -> Result<EventBuilder, SdkError> {
    check_content(content, 64 * 1024)?;
    let mut tags = vec![tag(&["h", &channel_id.to_string()])?];
    mention_tags(mentions, &mut tags)?;
    imeta_tags(media_tags, &mut tags)?;
    Ok(EventBuilder::new(Kind::Custom(45001), content, tags))
}

// ── Builder 3: build_forum_comment ───────────────────────────────────────────

/// Build a forum comment reply (kind 45003).
pub fn build_forum_comment(
    channel_id: Uuid,
    content: &str,
    thread_ref: &ThreadRef,
    mentions: &[&str],
    media_tags: &[Vec<String>],
) -> Result<EventBuilder, SdkError> {
    check_content(content, 64 * 1024)?;
    let mut tags = vec![tag(&["h", &channel_id.to_string()])?];
    thread_tags(thread_ref, &mut tags)?;
    mention_tags(mentions, &mut tags)?;
    imeta_tags(media_tags, &mut tags)?;
    Ok(EventBuilder::new(Kind::Custom(45003), content, tags))
}

// ── Builder 4: build_diff_message ────────────────────────────────────────────

/// Build a diff/patch message (kind 40008).
pub fn build_diff_message(
    channel_id: Uuid,
    content: &str,
    diff_meta: &DiffMeta,
    thread_ref: Option<&ThreadRef>,
) -> Result<EventBuilder, SdkError> {
    check_content(content, 60 * 1024)?;

    // Validate DiffMeta
    if !diff_meta.repo_url.starts_with("http://") && !diff_meta.repo_url.starts_with("https://") {
        return Err(SdkError::InvalidDiffMeta(
            "repo_url must start with http:// or https://".into(),
        ));
    }
    check_hex_len(&diff_meta.commit_sha, 7, "commit_sha")?;
    if let Some(ref pc) = diff_meta.parent_commit {
        check_hex_len(pc, 7, "parent_commit")?;
    }
    match &diff_meta.branch {
        Some((src, tgt)) if src.is_empty() || tgt.is_empty() => {
            return Err(SdkError::InvalidDiffMeta(
                "branch requires both source and target to be non-empty".into(),
            ));
        }
        _ => {}
    }
    if let Some(pr) = diff_meta.pr_number {
        if pr == 0 {
            return Err(SdkError::InvalidDiffMeta(
                "pr_number must be positive".into(),
            ));
        }
    }

    let mut tags = vec![
        tag(&["h", &channel_id.to_string()])?,
        tag(&["repo", &diff_meta.repo_url])?,
        tag(&["commit", &diff_meta.commit_sha])?,
    ];
    if let Some(ref fp) = diff_meta.file_path {
        tags.push(tag(&["file", fp])?);
    }
    if let Some(ref pc) = diff_meta.parent_commit {
        tags.push(tag(&["parent-commit", pc])?);
    }
    if let Some((ref src, ref tgt)) = diff_meta.branch {
        tags.push(tag(&["branch", src, tgt])?);
    }
    if let Some(pr) = diff_meta.pr_number {
        tags.push(tag(&["pr", &pr.to_string()])?);
    }
    if let Some(ref lang) = diff_meta.language {
        tags.push(tag(&["l", lang])?);
    }
    if let Some(ref desc) = diff_meta.description {
        tags.push(tag(&["description", desc])?);
    }
    if diff_meta.truncated {
        tags.push(tag(&["truncated", "true"])?);
    }
    if let Some(ref alt) = diff_meta.alt_text {
        tags.push(tag(&["alt", alt])?);
    }
    if let Some(tr) = thread_ref {
        thread_tags(tr, &mut tags)?;
    }
    Ok(EventBuilder::new(Kind::Custom(40008), content, tags))
}

// ── Builder 5: build_edit ────────────────────────────────────────────────────

/// Build an edit event targeting an existing message (kind 40003).
pub fn build_edit(
    channel_id: Uuid,
    target_event_id: nostr::EventId,
    new_content: &str,
) -> Result<EventBuilder, SdkError> {
    check_content(new_content, 64 * 1024)?;
    let tags = vec![
        tag(&["h", &channel_id.to_string()])?,
        tag(&["e", &target_event_id.to_hex()])?,
    ];
    Ok(EventBuilder::new(Kind::Custom(40003), new_content, tags))
}

// ── Builder 6: build_delete_message ──────────────────────────────────────────

/// Build a Sprout-native delete event (kind 9005).
pub fn build_delete_message(
    channel_id: Uuid,
    target_event_id: nostr::EventId,
) -> Result<EventBuilder, SdkError> {
    let tags = vec![
        tag(&["h", &channel_id.to_string()])?,
        tag(&["e", &target_event_id.to_hex()])?,
    ];
    Ok(EventBuilder::new(Kind::Custom(9005), "", tags))
}

// ── Builder 7: build_delete_compat ───────────────────────────────────────────

/// Build a NIP-09 compatible deletion event (kind 5).
pub fn build_delete_compat(target_event_id: nostr::EventId) -> Result<EventBuilder, SdkError> {
    let tags = vec![tag(&["e", &target_event_id.to_hex()])?];
    Ok(EventBuilder::new(Kind::Custom(5), "", tags))
}

// ── Builder 8: build_vote ────────────────────────────────────────────────────

/// Build a forum vote event (kind 45002). Content is `"+"` or `"-"`.
pub fn build_vote(
    channel_id: Uuid,
    target_event_id: nostr::EventId,
    direction: VoteDirection,
) -> Result<EventBuilder, SdkError> {
    let content = match direction {
        VoteDirection::Up => "+",
        VoteDirection::Down => "-",
    };
    let tags = vec![
        tag(&["h", &channel_id.to_string()])?,
        tag(&["e", &target_event_id.to_hex()])?,
    ];
    Ok(EventBuilder::new(Kind::Custom(45002), content, tags))
}

// ── Builder 9: build_reaction ────────────────────────────────────────────────

/// Build a NIP-25 reaction event (kind 7). Emoji max 64 chars.
pub fn build_reaction(
    target_event_id: nostr::EventId,
    emoji: &str,
) -> Result<EventBuilder, SdkError> {
    if emoji.chars().count() > 64 {
        return Err(SdkError::EmojiTooLong);
    }
    let tags = vec![tag(&["e", &target_event_id.to_hex()])?];
    Ok(EventBuilder::new(Kind::Custom(7), emoji, tags))
}

// ── Builder 10: build_remove_reaction ────────────────────────────────────────

/// Build a deletion event targeting a reaction (kind 5).
pub fn build_remove_reaction(reaction_event_id: nostr::EventId) -> Result<EventBuilder, SdkError> {
    let tags = vec![tag(&["e", &reaction_event_id.to_hex()])?];
    Ok(EventBuilder::new(Kind::Custom(5), "", tags))
}

// ── Builder 11: build_set_canvas ─────────────────────────────────────────────

/// Build a canvas update event (kind 40100).
pub fn build_set_canvas(channel_id: Uuid, content: &str) -> Result<EventBuilder, SdkError> {
    let tags = vec![tag(&["h", &channel_id.to_string()])?];
    Ok(EventBuilder::new(Kind::Custom(40100), content, tags))
}

// ── Builder 12: build_profile ────────────────────────────────────────────────

/// Build a NIP-01 profile metadata event (kind 0).
///
/// Only present (Some) fields are included in the JSON object.
pub fn build_profile(
    display_name: Option<&str>,
    name: Option<&str>,
    picture: Option<&str>,
    about: Option<&str>,
    nip05: Option<&str>,
) -> Result<EventBuilder, SdkError> {
    let mut map = serde_json::Map::new();
    if let Some(v) = display_name {
        map.insert("display_name".into(), serde_json::Value::String(v.into()));
    }
    if let Some(v) = name {
        map.insert("name".into(), serde_json::Value::String(v.into()));
    }
    if let Some(v) = picture {
        map.insert("picture".into(), serde_json::Value::String(v.into()));
    }
    if let Some(v) = about {
        map.insert("about".into(), serde_json::Value::String(v.into()));
    }
    if let Some(v) = nip05 {
        map.insert("nip05".into(), serde_json::Value::String(v.into()));
    }
    let content = serde_json::Value::Object(map).to_string();
    Ok(EventBuilder::new(Kind::Custom(0), content, []))
}

// ── Builder 13: build_add_member ─────────────────────────────────────────────

/// Build a NIP-29 add-member event (kind 9000).
pub fn build_add_member(
    channel_id: Uuid,
    target_pubkey: &str,
    role: Option<&str>,
) -> Result<EventBuilder, SdkError> {
    let mut tags = vec![
        tag(&["h", &channel_id.to_string()])?,
        tag(&["p", &target_pubkey.to_ascii_lowercase()])?,
    ];
    if let Some(r) = role {
        tags.push(tag(&["role", r])?);
    }
    Ok(EventBuilder::new(Kind::Custom(9000), "", tags))
}

// ── Builder 14: build_remove_member ──────────────────────────────────────────

/// Build a NIP-29 remove-member event (kind 9001).
pub fn build_remove_member(
    channel_id: Uuid,
    target_pubkey: &str,
) -> Result<EventBuilder, SdkError> {
    let tags = vec![
        tag(&["h", &channel_id.to_string()])?,
        tag(&["p", &target_pubkey.to_ascii_lowercase()])?,
    ];
    Ok(EventBuilder::new(Kind::Custom(9001), "", tags))
}

// ── Builder 15: build_leave ──────────────────────────────────────────────────

/// Build a NIP-29 leave-request event (kind 9022).
pub fn build_leave(channel_id: Uuid) -> Result<EventBuilder, SdkError> {
    let tags = vec![tag(&["h", &channel_id.to_string()])?];
    Ok(EventBuilder::new(Kind::Custom(9022), "", tags))
}

// ── Builder 16: build_update_channel ─────────────────────────────────────────

/// Build a NIP-29 edit-metadata event for name/about (kind 9002).
pub fn build_update_channel(
    channel_id: Uuid,
    name: Option<&str>,
    about: Option<&str>,
) -> Result<EventBuilder, SdkError> {
    let mut tags = vec![tag(&["h", &channel_id.to_string()])?];
    if let Some(n) = name {
        tags.push(tag(&["name", n])?);
    }
    if let Some(a) = about {
        tags.push(tag(&["about", a])?);
    }
    Ok(EventBuilder::new(Kind::Custom(9002), "", tags))
}

// ── Builder 17: build_set_topic ──────────────────────────────────────────────

/// Build a NIP-29 edit-metadata event for topic (kind 9002).
pub fn build_set_topic(channel_id: Uuid, topic: &str) -> Result<EventBuilder, SdkError> {
    let tags = vec![
        tag(&["h", &channel_id.to_string()])?,
        tag(&["topic", topic])?,
    ];
    Ok(EventBuilder::new(Kind::Custom(9002), "", tags))
}

// ── Builder 18: build_set_purpose ────────────────────────────────────────────

/// Build a NIP-29 edit-metadata event for purpose (kind 9002).
pub fn build_set_purpose(channel_id: Uuid, purpose: &str) -> Result<EventBuilder, SdkError> {
    let tags = vec![
        tag(&["h", &channel_id.to_string()])?,
        tag(&["purpose", purpose])?,
    ];
    Ok(EventBuilder::new(Kind::Custom(9002), "", tags))
}

// ── Builder 19: build_create_channel ─────────────────────────────────────────

/// Build a NIP-29 create-group event (kind 9007).
pub fn build_create_channel(
    channel_id: Uuid,
    name: &str,
    visibility: Option<&str>,
    channel_type: Option<&str>,
    about: Option<&str>,
) -> Result<EventBuilder, SdkError> {
    let mut tags = vec![tag(&["h", &channel_id.to_string()])?, tag(&["name", name])?];
    if let Some(v) = visibility {
        tags.push(tag(&["visibility", v])?);
    }
    if let Some(ct) = channel_type {
        tags.push(tag(&["channel_type", ct])?);
    }
    if let Some(a) = about {
        tags.push(tag(&["about", a])?);
    }
    Ok(EventBuilder::new(Kind::Custom(9007), "", tags))
}

// ── Builder 20: build_join ───────────────────────────────────────────────────

/// Build a NIP-29 join-request event (kind 9021).
pub fn build_join(channel_id: Uuid) -> Result<EventBuilder, SdkError> {
    let tags = vec![tag(&["h", &channel_id.to_string()])?];
    Ok(EventBuilder::new(Kind::Custom(9021), "", tags))
}

// ── Builder 21: build_archive ────────────────────────────────────────────────

/// Build a NIP-29 archive event (kind 9002, `["archived", "true"]`).
pub fn build_archive(channel_id: Uuid) -> Result<EventBuilder, SdkError> {
    let tags = vec![
        tag(&["h", &channel_id.to_string()])?,
        tag(&["archived", "true"])?,
    ];
    Ok(EventBuilder::new(Kind::Custom(9002), "", tags))
}

// ── Builder 22: build_unarchive ──────────────────────────────────────────────

/// Build a NIP-29 unarchive event (kind 9002, `["archived", "false"]`).
pub fn build_unarchive(channel_id: Uuid) -> Result<EventBuilder, SdkError> {
    let tags = vec![
        tag(&["h", &channel_id.to_string()])?,
        tag(&["archived", "false"])?,
    ];
    Ok(EventBuilder::new(Kind::Custom(9002), "", tags))
}

// ── Builder 23: build_delete_channel ─────────────────────────────────────────

/// Build a NIP-29 delete-group event (kind 9008).
pub fn build_delete_channel(channel_id: Uuid) -> Result<EventBuilder, SdkError> {
    let tags = vec![tag(&["h", &channel_id.to_string()])?];
    Ok(EventBuilder::new(Kind::Custom(9008), "", tags))
}

// ── Helper: extract_channel_id ───────────────────────────────────────────────

/// Extract the channel UUID from an event's `h` tag.
///
/// Returns `None` if no `h` tag is present or the value is not a valid UUID.
pub fn extract_channel_id(event: &nostr::Event) -> Option<Uuid> {
    event.tags.iter().find_map(|t| {
        let vec = t.as_slice();
        if vec.first().map(|s| s.as_str()) == Some("h") {
            vec.get(1).and_then(|v| Uuid::parse_str(v.as_str()).ok())
        } else {
            None
        }
    })
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventId, Keys};

    fn keys() -> Keys {
        Keys::generate()
    }

    fn sign(b: EventBuilder) -> nostr::Event {
        b.sign_with_keys(&keys()).expect("sign")
    }

    fn event_id() -> EventId {
        let k = keys();
        EventBuilder::new(Kind::Custom(1), "x", [])
            .sign_with_keys(&k)
            .expect("sign")
            .id
    }

    fn uuid() -> Uuid {
        Uuid::new_v4()
    }

    fn tag_values(event: &nostr::Event, key: &str) -> Vec<String> {
        event
            .tags
            .iter()
            .filter_map(|t| {
                let s = t.as_slice();
                if s.first().map(|v| v.as_str()) == Some(key) {
                    s.get(1).map(|v| v.to_string())
                } else {
                    None
                }
            })
            .collect()
    }

    fn has_tag(event: &nostr::Event, key: &str, val: &str) -> bool {
        event.tags.iter().any(|t| {
            let s = t.as_slice();
            s.first().map(|v| v.as_str()) == Some(key) && s.get(1).map(|v| v.as_str()) == Some(val)
        })
    }

    // ── build_message ────────────────────────────────────────────────────────

    #[test]
    fn message_happy_path() {
        let cid = uuid();
        let ev = sign(build_message(cid, "hello", None, &[], false, &[]).unwrap());
        assert_eq!(ev.kind.as_u16(), 9);
        assert_eq!(ev.content, "hello");
        assert!(has_tag(&ev, "h", &cid.to_string()));
    }

    #[test]
    fn message_direct_reply() {
        let cid = uuid();
        let eid = event_id();
        let tr = ThreadRef {
            root_event_id: eid,
            parent_event_id: eid,
        };
        let ev = sign(build_message(cid, "reply", Some(&tr), &[], false, &[]).unwrap());
        // Direct reply: only one e-tag with "reply" marker
        let e_tags: Vec<_> = ev
            .tags
            .iter()
            .filter(|t| t.as_slice().first().map(|v| v.as_str()) == Some("e"))
            .collect();
        assert_eq!(e_tags.len(), 1);
        assert_eq!(
            e_tags[0].as_slice().get(3).map(|v| v.as_str()),
            Some("reply")
        );
    }

    #[test]
    fn message_nested_reply() {
        let cid = uuid();
        let root = event_id();
        let parent = event_id();
        let tr = ThreadRef {
            root_event_id: root,
            parent_event_id: parent,
        };
        let ev = sign(build_message(cid, "nested", Some(&tr), &[], false, &[]).unwrap());
        let e_tags: Vec<_> = ev
            .tags
            .iter()
            .filter(|t| t.as_slice().first().map(|v| v.as_str()) == Some("e"))
            .collect();
        assert_eq!(e_tags.len(), 2);
        let markers: Vec<_> = e_tags
            .iter()
            .filter_map(|t| t.as_slice().get(3).map(|v| v.as_str()))
            .collect();
        assert!(markers.contains(&"root"));
        assert!(markers.contains(&"reply"));
    }

    #[test]
    fn message_broadcast_flag() {
        let cid = uuid();
        let ev = sign(build_message(cid, "hi", None, &[], true, &[]).unwrap());
        assert!(has_tag(&ev, "broadcast", "1"));
    }

    #[test]
    fn message_mentions_deduped() {
        let cid = uuid();
        let hex = "abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234";
        let ev = sign(build_message(cid, "hi", None, &[hex, hex], false, &[]).unwrap());
        let p_tags = tag_values(&ev, "p");
        assert_eq!(p_tags.len(), 1);
    }

    #[test]
    fn message_too_many_mentions() {
        let cid = uuid();
        let hex = "abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234";
        let _mentions: Vec<&str> = (0..51).map(|_| hex).collect();
        // All same hex so dedup would reduce to 1, but the check is on raw len
        // Let's use 51 distinct-ish values by varying the first char
        let hexes: Vec<String> = (0..51u8)
            .map(|i| {
                format!(
                    "{:02x}cd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd12",
                    i
                )
            })
            .collect();
        let refs: Vec<&str> = hexes.iter().map(|s| s.as_str()).collect();
        let result = build_message(cid, "hi", None, &refs, false, &[]);
        assert!(matches!(result, Err(SdkError::TooManyMentions)));
    }

    #[test]
    fn message_content_too_large() {
        let cid = uuid();
        let big = "x".repeat(64 * 1024 + 1);
        let result = build_message(cid, &big, None, &[], false, &[]);
        assert!(matches!(result, Err(SdkError::ContentTooLarge { .. })));
    }

    #[test]
    fn message_max_content_ok() {
        let cid = uuid();
        let max = "x".repeat(64 * 1024);
        assert!(build_message(cid, &max, None, &[], false, &[]).is_ok());
    }

    // ── build_forum_post ─────────────────────────────────────────────────────

    #[test]
    fn forum_post_happy_path() {
        let cid = uuid();
        let ev = sign(build_forum_post(cid, "post body", &[], &[]).unwrap());
        assert_eq!(ev.kind.as_u16(), 45001);
        assert!(has_tag(&ev, "h", &cid.to_string()));
    }

    #[test]
    fn forum_post_content_too_large() {
        let cid = uuid();
        let big = "x".repeat(64 * 1024 + 1);
        assert!(matches!(
            build_forum_post(cid, &big, &[], &[]),
            Err(SdkError::ContentTooLarge { .. })
        ));
    }

    // ── build_forum_comment ──────────────────────────────────────────────────

    #[test]
    fn forum_comment_happy_path() {
        let cid = uuid();
        let eid = event_id();
        let tr = ThreadRef {
            root_event_id: eid,
            parent_event_id: eid,
        };
        let ev = sign(build_forum_comment(cid, "comment", &tr, &[], &[]).unwrap());
        assert_eq!(ev.kind.as_u16(), 45003);
        assert!(has_tag(&ev, "h", &cid.to_string()));
    }

    // ── build_diff_message ───────────────────────────────────────────────────

    fn good_diff_meta() -> DiffMeta {
        DiffMeta {
            repo_url: "https://github.com/example/repo".into(),
            commit_sha: "abc1234".into(),
            file_path: Some("src/main.rs".into()),
            parent_commit: None,
            branch: None,
            pr_number: None,
            language: Some("rust".into()),
            description: None,
            truncated: false,
            alt_text: None,
        }
    }

    #[test]
    fn diff_message_happy_path() {
        let cid = uuid();
        let ev = sign(build_diff_message(cid, "diff content", &good_diff_meta(), None).unwrap());
        assert_eq!(ev.kind.as_u16(), 40008);
        assert!(has_tag(&ev, "repo", "https://github.com/example/repo"));
        assert!(has_tag(&ev, "commit", "abc1234"));
        assert!(has_tag(&ev, "l", "rust"));
    }

    #[test]
    fn diff_message_bad_repo_url() {
        let cid = uuid();
        let mut meta = good_diff_meta();
        meta.repo_url = "ftp://bad.url".into();
        assert!(matches!(
            build_diff_message(cid, "x", &meta, None),
            Err(SdkError::InvalidDiffMeta(_))
        ));
    }

    #[test]
    fn diff_message_short_commit_sha() {
        let cid = uuid();
        let mut meta = good_diff_meta();
        meta.commit_sha = "abc12".into(); // only 5 chars
        assert!(matches!(
            build_diff_message(cid, "x", &meta, None),
            Err(SdkError::InvalidDiffMeta(_))
        ));
    }

    #[test]
    fn diff_message_invalid_commit_sha_chars() {
        let cid = uuid();
        let mut meta = good_diff_meta();
        meta.commit_sha = "xyz1234".into(); // 'x', 'y', 'z' not hex
        assert!(matches!(
            build_diff_message(cid, "x", &meta, None),
            Err(SdkError::InvalidDiffMeta(_))
        ));
    }

    #[test]
    fn diff_message_branch_only_source() {
        let cid = uuid();
        let mut meta = good_diff_meta();
        meta.branch = Some(("main".into(), "".into())); // target empty
        assert!(matches!(
            build_diff_message(cid, "x", &meta, None),
            Err(SdkError::InvalidDiffMeta(_))
        ));
    }

    #[test]
    fn diff_message_pr_zero() {
        let cid = uuid();
        let mut meta = good_diff_meta();
        meta.pr_number = Some(0);
        assert!(matches!(
            build_diff_message(cid, "x", &meta, None),
            Err(SdkError::InvalidDiffMeta(_))
        ));
    }

    #[test]
    fn diff_message_content_too_large() {
        let cid = uuid();
        let big = "x".repeat(60 * 1024 + 1);
        assert!(matches!(
            build_diff_message(cid, &big, &good_diff_meta(), None),
            Err(SdkError::ContentTooLarge { .. })
        ));
    }

    #[test]
    fn diff_message_all_optional_fields() {
        let cid = uuid();
        let meta = DiffMeta {
            repo_url: "https://github.com/example/repo".into(),
            commit_sha: "abc1234def".into(),
            file_path: Some("src/lib.rs".into()),
            parent_commit: Some("1234567".into()),
            branch: Some(("feature".into(), "main".into())),
            pr_number: Some(42),
            language: Some("rust".into()),
            description: Some("fix bug".into()),
            truncated: true,
            alt_text: Some("patch for bug fix".into()),
        };
        let ev = sign(build_diff_message(cid, "diff", &meta, None).unwrap());
        assert!(has_tag(&ev, "file", "src/lib.rs"));
        assert!(has_tag(&ev, "parent-commit", "1234567"));
        assert!(has_tag(&ev, "pr", "42"));
        assert!(has_tag(&ev, "truncated", "true"));
        assert!(has_tag(&ev, "alt", "patch for bug fix"));
    }

    // ── build_edit ───────────────────────────────────────────────────────────

    #[test]
    fn edit_happy_path() {
        let cid = uuid();
        let eid = event_id();
        let ev = sign(build_edit(cid, eid, "new content").unwrap());
        assert_eq!(ev.kind.as_u16(), 40003);
        assert!(has_tag(&ev, "e", &eid.to_hex()));
    }

    #[test]
    fn edit_content_too_large() {
        let cid = uuid();
        let eid = event_id();
        let big = "x".repeat(64 * 1024 + 1);
        assert!(matches!(
            build_edit(cid, eid, &big),
            Err(SdkError::ContentTooLarge { .. })
        ));
    }

    // ── build_delete_message ─────────────────────────────────────────────────

    #[test]
    fn delete_message_happy_path() {
        let cid = uuid();
        let eid = event_id();
        let ev = sign(build_delete_message(cid, eid).unwrap());
        assert_eq!(ev.kind.as_u16(), 9005);
        assert!(has_tag(&ev, "h", &cid.to_string()));
        assert!(has_tag(&ev, "e", &eid.to_hex()));
        assert_eq!(ev.content, "");
    }

    // ── build_delete_compat ──────────────────────────────────────────────────

    #[test]
    fn delete_compat_happy_path() {
        let eid = event_id();
        let ev = sign(build_delete_compat(eid).unwrap());
        assert_eq!(ev.kind.as_u16(), 5);
        assert!(has_tag(&ev, "e", &eid.to_hex()));
        assert_eq!(ev.content, "");
    }

    // ── build_vote ───────────────────────────────────────────────────────────

    #[test]
    fn vote_up() {
        let cid = uuid();
        let eid = event_id();
        let ev = sign(build_vote(cid, eid, VoteDirection::Up).unwrap());
        assert_eq!(ev.kind.as_u16(), 45002);
        assert_eq!(ev.content, "+");
    }

    #[test]
    fn vote_down() {
        let cid = uuid();
        let eid = event_id();
        let ev = sign(build_vote(cid, eid, VoteDirection::Down).unwrap());
        assert_eq!(ev.content, "-");
    }

    // ── build_reaction ───────────────────────────────────────────────────────

    #[test]
    fn reaction_happy_path() {
        let eid = event_id();
        let ev = sign(build_reaction(eid, "👍").unwrap());
        assert_eq!(ev.kind.as_u16(), 7);
        assert_eq!(ev.content, "👍");
    }

    #[test]
    fn reaction_emoji_too_long() {
        let eid = event_id();
        let long_emoji = "a".repeat(65);
        assert!(matches!(
            build_reaction(eid, &long_emoji),
            Err(SdkError::EmojiTooLong)
        ));
    }

    #[test]
    fn reaction_emoji_max_len_ok() {
        let eid = event_id();
        let max_emoji = "a".repeat(64);
        assert!(build_reaction(eid, &max_emoji).is_ok());
    }

    // ── build_remove_reaction ────────────────────────────────────────────────

    #[test]
    fn remove_reaction_happy_path() {
        let eid = event_id();
        let ev = sign(build_remove_reaction(eid).unwrap());
        assert_eq!(ev.kind.as_u16(), 5);
        assert!(has_tag(&ev, "e", &eid.to_hex()));
    }

    // ── build_set_canvas ─────────────────────────────────────────────────────

    #[test]
    fn set_canvas_happy_path() {
        let cid = uuid();
        let ev = sign(build_set_canvas(cid, "# Canvas\nHello").unwrap());
        assert_eq!(ev.kind.as_u16(), 40100);
        assert!(has_tag(&ev, "h", &cid.to_string()));
        assert_eq!(ev.content, "# Canvas\nHello");
    }

    // ── build_profile ────────────────────────────────────────────────────────

    #[test]
    fn profile_all_fields() {
        let ev = sign(
            build_profile(
                Some("Alice"),
                Some("alice"),
                Some("https://example.com/pic.jpg"),
                Some("Hello world"),
                Some("alice@example.com"),
            )
            .unwrap(),
        );
        assert_eq!(ev.kind.as_u16(), 0);
        let v: serde_json::Value = serde_json::from_str(&ev.content).unwrap();
        assert_eq!(v["display_name"], "Alice");
        assert_eq!(v["name"], "alice");
        assert_eq!(v["nip05"], "alice@example.com");
    }

    #[test]
    fn profile_some_fields() {
        let ev = sign(build_profile(Some("Bob"), None, None, None, None).unwrap());
        let v: serde_json::Value = serde_json::from_str(&ev.content).unwrap();
        assert_eq!(v["display_name"], "Bob");
        assert!(
            v.get("name").is_none()
                || !v["name"].is_null() && v.get("name") == Some(&serde_json::Value::Null)
                || !v.as_object().unwrap().contains_key("name")
        );
    }

    #[test]
    fn profile_no_fields() {
        let ev = sign(build_profile(None, None, None, None, None).unwrap());
        let v: serde_json::Value = serde_json::from_str(&ev.content).unwrap();
        assert!(v.as_object().unwrap().is_empty());
    }

    // ── build_add_member ─────────────────────────────────────────────────────

    #[test]
    fn add_member_with_role() {
        let cid = uuid();
        let pubkey = "abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234";
        let ev = sign(build_add_member(cid, pubkey, Some("admin")).unwrap());
        assert_eq!(ev.kind.as_u16(), 9000);
        assert!(has_tag(&ev, "p", pubkey));
        assert!(has_tag(&ev, "role", "admin"));
    }

    #[test]
    fn add_member_without_role() {
        let cid = uuid();
        let pubkey = "abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234";
        let ev = sign(build_add_member(cid, pubkey, None).unwrap());
        assert_eq!(ev.kind.as_u16(), 9000);
        assert!(tag_values(&ev, "role").is_empty());
    }

    // ── build_remove_member ──────────────────────────────────────────────────

    #[test]
    fn remove_member_happy_path() {
        let cid = uuid();
        let pubkey = "abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234abcd1234";
        let ev = sign(build_remove_member(cid, pubkey).unwrap());
        assert_eq!(ev.kind.as_u16(), 9001);
        assert!(has_tag(&ev, "p", pubkey));
    }

    // ── build_leave ──────────────────────────────────────────────────────────

    #[test]
    fn leave_happy_path() {
        let cid = uuid();
        let ev = sign(build_leave(cid).unwrap());
        assert_eq!(ev.kind.as_u16(), 9022);
        assert!(has_tag(&ev, "h", &cid.to_string()));
    }

    // ── build_update_channel ─────────────────────────────────────────────────

    #[test]
    fn update_channel_name_and_about() {
        let cid = uuid();
        let ev = sign(build_update_channel(cid, Some("new-name"), Some("new about")).unwrap());
        assert_eq!(ev.kind.as_u16(), 9002);
        assert!(has_tag(&ev, "name", "new-name"));
        assert!(has_tag(&ev, "about", "new about"));
    }

    #[test]
    fn update_channel_no_fields() {
        let cid = uuid();
        let ev = sign(build_update_channel(cid, None, None).unwrap());
        assert_eq!(ev.kind.as_u16(), 9002);
        // Only h-tag
        assert_eq!(ev.tags.len(), 1);
    }

    // ── build_set_topic ──────────────────────────────────────────────────────

    #[test]
    fn set_topic_happy_path() {
        let cid = uuid();
        let ev = sign(build_set_topic(cid, "Rust async patterns").unwrap());
        assert_eq!(ev.kind.as_u16(), 9002);
        assert!(has_tag(&ev, "topic", "Rust async patterns"));
    }

    // ── build_set_purpose ────────────────────────────────────────────────────

    #[test]
    fn set_purpose_happy_path() {
        let cid = uuid();
        let ev = sign(build_set_purpose(cid, "Team coordination").unwrap());
        assert_eq!(ev.kind.as_u16(), 9002);
        assert!(has_tag(&ev, "purpose", "Team coordination"));
    }

    // ── build_create_channel ─────────────────────────────────────────────────

    #[test]
    fn create_channel_all_fields() {
        let cid = uuid();
        let ev = sign(
            build_create_channel(
                cid,
                "general",
                Some("public"),
                Some("text"),
                Some("General chat"),
            )
            .unwrap(),
        );
        assert_eq!(ev.kind.as_u16(), 9007);
        assert!(has_tag(&ev, "name", "general"));
        assert!(has_tag(&ev, "visibility", "public"));
        assert!(has_tag(&ev, "channel_type", "text"));
        assert!(has_tag(&ev, "about", "General chat"));
    }

    #[test]
    fn create_channel_minimal() {
        let cid = uuid();
        let ev = sign(build_create_channel(cid, "dev", None, None, None).unwrap());
        assert_eq!(ev.kind.as_u16(), 9007);
        assert!(has_tag(&ev, "name", "dev"));
    }

    // ── build_join ───────────────────────────────────────────────────────────

    #[test]
    fn join_happy_path() {
        let cid = uuid();
        let ev = sign(build_join(cid).unwrap());
        assert_eq!(ev.kind.as_u16(), 9021);
        assert!(has_tag(&ev, "h", &cid.to_string()));
    }

    // ── build_archive / build_unarchive ──────────────────────────────────────

    #[test]
    fn archive_happy_path() {
        let cid = uuid();
        let ev = sign(build_archive(cid).unwrap());
        assert_eq!(ev.kind.as_u16(), 9002);
        assert!(has_tag(&ev, "archived", "true"));
    }

    #[test]
    fn unarchive_happy_path() {
        let cid = uuid();
        let ev = sign(build_unarchive(cid).unwrap());
        assert_eq!(ev.kind.as_u16(), 9002);
        assert!(has_tag(&ev, "archived", "false"));
    }

    // ── build_delete_channel ─────────────────────────────────────────────────

    #[test]
    fn delete_channel_happy_path() {
        let cid = uuid();
        let ev = sign(build_delete_channel(cid).unwrap());
        assert_eq!(ev.kind.as_u16(), 9008);
        assert!(has_tag(&ev, "h", &cid.to_string()));
    }

    // ── extract_channel_id ───────────────────────────────────────────────────

    #[test]
    fn extract_channel_id_present() {
        let cid = uuid();
        let ev = sign(build_join(cid).unwrap());
        assert_eq!(extract_channel_id(&ev), Some(cid));
    }

    #[test]
    fn extract_channel_id_absent() {
        let eid = event_id();
        let ev = sign(build_delete_compat(eid).unwrap());
        assert_eq!(extract_channel_id(&ev), None);
    }

    #[test]
    fn extract_channel_id_invalid_uuid() {
        // Build an event with a malformed h-tag value
        let tags = vec![Tag::parse(&["h", "not-a-uuid"]).unwrap()];
        let ev = EventBuilder::new(Kind::Custom(9), "x", tags)
            .sign_with_keys(&keys())
            .unwrap();
        assert_eq!(extract_channel_id(&ev), None);
    }
}
