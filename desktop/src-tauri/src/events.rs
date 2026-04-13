//! Signed-event builders for desktop write operations.
//!
//! Mirrors the sprout-sdk builder patterns but uses nostr 0.37 API
//! (the desktop is excluded from the workspace which pins nostr 0.36).
//!
//! Mental model:
//!   caller params → build_*() → EventBuilder → submit_event() signs + POSTs
//!
//! Each function validates inputs and returns a nostr::EventBuilder.
//! Signing and submission happen in relay::submit_event.

use nostr::{EventBuilder, EventId, Kind, Tag};
use uuid::Uuid;

// ── Constants ────────────────────────────────────────────────────────────────

/// Maximum content size — matches sprout-sdk (64 KiB).
const MAX_CONTENT_BYTES: usize = 64 * 1024;

/// Maximum mention count — matches sprout-sdk.
const MAX_MENTIONS: usize = 50;

/// Maximum emoji length in characters — matches sprout-sdk.
const MAX_EMOJI_CHARS: usize = 64;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn tag(parts: Vec<&str>) -> Result<Tag, String> {
    Tag::parse(parts).map_err(|e| format!("invalid tag: {e}"))
}

fn check_content(content: &str) -> Result<(), String> {
    if content.len() > MAX_CONTENT_BYTES {
        return Err(format!(
            "content exceeds maximum size of {} bytes (got {})",
            MAX_CONTENT_BYTES,
            content.len()
        ));
    }
    Ok(())
}

/// NIP-10 thread reference.
pub struct ThreadRef {
    pub root_event_id: EventId,
    pub parent_event_id: EventId,
}

fn thread_tags(tr: &ThreadRef) -> Result<Vec<Tag>, String> {
    let root = tr.root_event_id.to_hex();
    let parent = tr.parent_event_id.to_hex();
    if root == parent {
        Ok(vec![tag(vec!["e", &root, "", "reply"])?])
    } else {
        Ok(vec![
            tag(vec!["e", &root, "", "root"])?,
            tag(vec!["e", &parent, "", "reply"])?,
        ])
    }
}

fn agent_reply_parent_tag(thread_ref: &ThreadRef, agent_reply_parent_id: &str) -> Result<Tag, String> {
    let expected_parent = thread_ref.parent_event_id.to_hex();
    if agent_reply_parent_id != expected_parent {
        return Err("agent_reply_parent_id must match thread parent_event_id".into());
    }
    tag(vec!["sprout", "agent_reply_parent", agent_reply_parent_id])
}

fn mention_tags(mentions: &[&str]) -> Result<Vec<Tag>, String> {
    if mentions.len() > MAX_MENTIONS {
        return Err(format!("too many mentions (max {MAX_MENTIONS})"));
    }
    let mut seen = std::collections::HashSet::new();
    let mut tags = Vec::new();
    for &hex in mentions {
        let lower = hex.to_ascii_lowercase();
        if seen.insert(lower.clone()) {
            tags.push(tag(vec!["p", &lower])?);
        }
    }
    Ok(tags)
}

/// Validate and append imeta tags. Rejects any tag whose first element is not "imeta"
/// to prevent injection of arbitrary tags (e.g., forged "h", "e", or "p" tags).
fn imeta_tags(media_tags: &[Vec<String>], tags: &mut Vec<Tag>) -> Result<(), String> {
    for mt in media_tags {
        if mt.first().map(String::as_str) != Some("imeta") {
            return Err(format!(
                "media tags must use 'imeta' prefix (got {:?})",
                mt.first()
            ));
        }
        let parts: Vec<&str> = mt.iter().map(String::as_str).collect();
        tags.push(Tag::parse(parts).map_err(|e| format!("invalid imeta tag: {e}"))?);
    }
    Ok(())
}

/// Validate a hex pubkey is exactly 64 hex characters.
fn check_pubkey(pubkey: &str) -> Result<(), String> {
    if pubkey.len() != 64 || !pubkey.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "pubkey must be a 64-character hex string (got {} chars)",
            pubkey.len()
        ));
    }
    Ok(())
}

// ── Channel operations ───────────────────────────────────────────────────────

/// Kind 9007 — create channel.
pub fn build_create_channel(
    channel_id: Uuid,
    name: &str,
    visibility: &str,
    channel_type: &str,
    about: Option<&str>,
    ttl_seconds: Option<i32>,
) -> Result<EventBuilder, String> {
    let mut tags = vec![
        tag(vec!["h", &channel_id.to_string()])?,
        tag(vec!["name", name])?,
        tag(vec!["visibility", visibility])?,
        tag(vec!["channel_type", channel_type])?,
    ];
    if let Some(a) = about {
        tags.push(tag(vec!["about", a])?);
    }
    if let Some(ttl) = ttl_seconds {
        tags.push(tag(vec!["ttl", &ttl.to_string()])?);
    }
    Ok(EventBuilder::new(Kind::Custom(9007), "").tags(tags))
}

/// Kind 9021 — join channel.
pub fn build_join(channel_id: Uuid) -> Result<EventBuilder, String> {
    let tags = vec![tag(vec!["h", &channel_id.to_string()])?];
    Ok(EventBuilder::new(Kind::Custom(9021), "").tags(tags))
}

/// Kind 9022 — leave channel.
pub fn build_leave(channel_id: Uuid) -> Result<EventBuilder, String> {
    let tags = vec![tag(vec!["h", &channel_id.to_string()])?];
    Ok(EventBuilder::new(Kind::Custom(9022), "").tags(tags))
}

/// Kind 9002 — update channel name/description.
pub fn build_update_channel(
    channel_id: Uuid,
    name: Option<&str>,
    about: Option<&str>,
) -> Result<EventBuilder, String> {
    if name.is_none() && about.is_none() {
        return Err("at least one of name or about must be provided".into());
    }
    let mut tags = vec![tag(vec!["h", &channel_id.to_string()])?];
    if let Some(n) = name {
        tags.push(tag(vec!["name", n])?);
    }
    if let Some(a) = about {
        tags.push(tag(vec!["about", a])?);
    }
    Ok(EventBuilder::new(Kind::Custom(9002), "").tags(tags))
}

/// Kind 9002 — set topic.
pub fn build_set_topic(channel_id: Uuid, topic: &str) -> Result<EventBuilder, String> {
    let tags = vec![
        tag(vec!["h", &channel_id.to_string()])?,
        tag(vec!["topic", topic])?,
    ];
    Ok(EventBuilder::new(Kind::Custom(9002), "").tags(tags))
}

/// Kind 9002 — set purpose.
pub fn build_set_purpose(channel_id: Uuid, purpose: &str) -> Result<EventBuilder, String> {
    let tags = vec![
        tag(vec!["h", &channel_id.to_string()])?,
        tag(vec!["purpose", purpose])?,
    ];
    Ok(EventBuilder::new(Kind::Custom(9002), "").tags(tags))
}

/// Kind 9002 — archive.
pub fn build_archive(channel_id: Uuid) -> Result<EventBuilder, String> {
    let tags = vec![
        tag(vec!["h", &channel_id.to_string()])?,
        tag(vec!["archived", "true"])?,
    ];
    Ok(EventBuilder::new(Kind::Custom(9002), "").tags(tags))
}

/// Kind 9002 — unarchive.
pub fn build_unarchive(channel_id: Uuid) -> Result<EventBuilder, String> {
    let tags = vec![
        tag(vec!["h", &channel_id.to_string()])?,
        tag(vec!["archived", "false"])?,
    ];
    Ok(EventBuilder::new(Kind::Custom(9002), "").tags(tags))
}

/// Kind 9008 — delete channel.
pub fn build_delete_channel(channel_id: Uuid) -> Result<EventBuilder, String> {
    let tags = vec![tag(vec!["h", &channel_id.to_string()])?];
    Ok(EventBuilder::new(Kind::Custom(9008), "").tags(tags))
}

// ── Membership ───────────────────────────────────────────────────────────────

/// Kind 9000 — add member.
pub fn build_add_member(
    channel_id: Uuid,
    target_pubkey: &str,
    role: Option<&str>,
) -> Result<EventBuilder, String> {
    check_pubkey(target_pubkey)?;
    let mut tags = vec![
        tag(vec!["h", &channel_id.to_string()])?,
        tag(vec!["p", &target_pubkey.to_ascii_lowercase()])?,
    ];
    if let Some(r) = role {
        tags.push(tag(vec!["role", r])?);
    }
    Ok(EventBuilder::new(Kind::Custom(9000), "").tags(tags))
}

/// Kind 9001 — remove member.
pub fn build_remove_member(channel_id: Uuid, target_pubkey: &str) -> Result<EventBuilder, String> {
    check_pubkey(target_pubkey)?;
    let tags = vec![
        tag(vec!["h", &channel_id.to_string()])?,
        tag(vec!["p", &target_pubkey.to_ascii_lowercase()])?,
    ];
    Ok(EventBuilder::new(Kind::Custom(9001), "").tags(tags))
}

// ── Messages ─────────────────────────────────────────────────────────────────

/// Kind 9 — stream message.
pub fn build_message(
    channel_id: Uuid,
    content: &str,
    thread_ref: Option<&ThreadRef>,
    agent_reply_parent_id: Option<&str>,
    mentions: &[&str],
    media_tags: &[Vec<String>],
) -> Result<EventBuilder, String> {
    check_content(content)?;
    let mut tags = vec![tag(vec!["h", &channel_id.to_string()])?];
    if let Some(tr) = thread_ref {
        tags.extend(thread_tags(tr)?);
        if let Some(agent_parent) = agent_reply_parent_id {
            tags.push(agent_reply_parent_tag(tr, agent_parent)?);
        }
    } else if agent_reply_parent_id.is_some() {
        return Err("agent_reply_parent_id requires thread_ref".into());
    }
    tags.extend(mention_tags(mentions)?);
    imeta_tags(media_tags, &mut tags)?;
    Ok(EventBuilder::new(Kind::Custom(9), content).tags(tags))
}

/// Kind 45001 — forum post.
pub fn build_forum_post(
    channel_id: Uuid,
    content: &str,
    mentions: &[&str],
    media_tags: &[Vec<String>],
) -> Result<EventBuilder, String> {
    check_content(content)?;
    let mut tags = vec![tag(vec!["h", &channel_id.to_string()])?];
    tags.extend(mention_tags(mentions)?);
    imeta_tags(media_tags, &mut tags)?;
    Ok(EventBuilder::new(Kind::Custom(45001), content).tags(tags))
}

/// Kind 45003 — forum comment.
pub fn build_forum_comment(
    channel_id: Uuid,
    content: &str,
    thread_ref: &ThreadRef,
    mentions: &[&str],
    media_tags: &[Vec<String>],
) -> Result<EventBuilder, String> {
    check_content(content)?;
    let mut tags = vec![tag(vec!["h", &channel_id.to_string()])?];
    tags.extend(thread_tags(thread_ref)?);
    tags.extend(mention_tags(mentions)?);
    imeta_tags(media_tags, &mut tags)?;
    Ok(EventBuilder::new(Kind::Custom(45003), content).tags(tags))
}

/// Kind 40003 — edit a message.
pub fn build_message_edit(
    channel_id: Uuid,
    target_event_id: EventId,
    content: &str,
) -> Result<EventBuilder, String> {
    check_content(content)?;
    let tags = vec![
        tag(vec!["h", &channel_id.to_string()])?,
        tag(vec!["e", &target_event_id.to_hex()])?,
    ];
    Ok(EventBuilder::new(Kind::Custom(40003), content).tags(tags))
}

/// Kind 5 — NIP-09 deletion (messages).
pub fn build_delete_compat(target_event_id: EventId) -> Result<EventBuilder, String> {
    let tags = vec![tag(vec!["e", &target_event_id.to_hex()])?];
    Ok(EventBuilder::new(Kind::Custom(5), "").tags(tags))
}

// ── Reactions ────────────────────────────────────────────────────────────────

/// Kind 7 — NIP-25 reaction.
pub fn build_reaction(target_event_id: EventId, emoji: &str) -> Result<EventBuilder, String> {
    if emoji.chars().count() > MAX_EMOJI_CHARS {
        return Err(format!(
            "emoji exceeds maximum length of {MAX_EMOJI_CHARS} characters"
        ));
    }
    let tags = vec![tag(vec!["e", &target_event_id.to_hex()])?];
    Ok(EventBuilder::new(Kind::Custom(7), emoji).tags(tags))
}

/// Kind 5 — delete a reaction event.
pub fn build_remove_reaction(reaction_event_id: EventId) -> Result<EventBuilder, String> {
    let tags = vec![tag(vec!["e", &reaction_event_id.to_hex()])?];
    Ok(EventBuilder::new(Kind::Custom(5), "").tags(tags))
}

// ── Canvas ───────────────────────────────────────────────────────────────────

/// Kind 40100 — set canvas.
pub fn build_set_canvas(channel_id: Uuid, content: &str) -> Result<EventBuilder, String> {
    check_content(content)?;
    let tags = vec![tag(vec!["h", &channel_id.to_string()])?];
    Ok(EventBuilder::new(Kind::Custom(40100), content).tags(tags))
}

// ── Profile ──────────────────────────────────────────────────────────────────

/// Kind 0 — NIP-01 profile metadata (full snapshot).
pub fn build_profile(
    display_name: Option<&str>,
    name: Option<&str>,
    picture: Option<&str>,
    about: Option<&str>,
    nip05: Option<&str>,
) -> Result<EventBuilder, String> {
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
    Ok(EventBuilder::new(Kind::Custom(0), content))
}

// ── Social notes ────────────────────────────────────────────────────────────

/// Kind 1 — NIP-01 short text note (global, no channel scope).
pub fn build_note(
    content: &str,
    reply_to_event_id: Option<EventId>,
) -> Result<EventBuilder, String> {
    check_content(content)?;
    let mut tags = Vec::new();
    if let Some(parent) = reply_to_event_id {
        tags.push(tag(vec!["e", &parent.to_hex(), "", "reply"])?);
    }
    Ok(EventBuilder::new(Kind::TextNote, content).tags(tags))
}

/// Maximum contacts per contact list event.
const MAX_CONTACTS: usize = 10_000;

/// Kind 3 — NIP-02 contact list (replaceable, full snapshot).
pub fn build_contact_list(
    contacts: &[(&str, Option<&str>, Option<&str>)],
) -> Result<EventBuilder, String> {
    if contacts.len() > MAX_CONTACTS {
        return Err(format!(
            "too many contacts (max {MAX_CONTACTS}, got {})",
            contacts.len()
        ));
    }
    let mut seen = std::collections::HashSet::new();
    let mut tags = Vec::new();
    for &(pubkey, relay_url, petname) in contacts {
        check_pubkey(pubkey)?;
        let lower = pubkey.to_ascii_lowercase();
        if seen.insert(lower.clone()) {
            tags.push(tag(vec![
                "p",
                &lower,
                relay_url.unwrap_or(""),
                petname.unwrap_or(""),
            ])?);
        }
    }
    Ok(EventBuilder::new(Kind::ContactList, "").tags(tags))
}
