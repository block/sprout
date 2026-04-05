// ABOUTME: Conversion functions between serde_json::Value API responses and FFI-safe types.
// ABOUTME: Translates relay JSON responses into the flat records that cross the UniFFI boundary.

use serde_json::Value;

use crate::types::*;

/// Convert a channel JSON object from the REST API to an FFI Channel.
pub(crate) fn json_to_channel(v: &Value) -> Option<Channel> {
    Some(Channel {
        id: v.get("id")?.as_str()?.to_string(),
        name: v.get("name")?.as_str()?.to_string(),
        about: v
            .get("description")
            .and_then(|d| d.as_str())
            .map(|s| s.to_string()),
        topic: v
            .get("topic")
            .and_then(|t| t.as_str())
            .map(|s| s.to_string()),
        channel_type: match v.get("channel_type").and_then(|c| c.as_str()) {
            Some("forum") => ChannelType::Forum,
            Some("dm") => ChannelType::Dm,
            _ => ChannelType::Stream,
        },
        visibility: match v.get("visibility").and_then(|v| v.as_str()) {
            Some("private") => ChannelVisibility::Private,
            _ => ChannelVisibility::Open,
        },
        member_count: v.get("member_count").and_then(|m| m.as_u64()).unwrap_or(0) as u32,
        last_message_at: v
            .get("last_message_at")
            .and_then(|t| t.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.timestamp()),
        is_member: v
            .get("is_member")
            .and_then(|m| m.as_bool())
            .unwrap_or(false),
    })
}

/// Convert a user profile JSON object from the REST API to an FFI UserProfile.
pub(crate) fn json_to_profile(v: &Value) -> Option<UserProfile> {
    Some(UserProfile {
        pubkey: v.get("pubkey")?.as_str()?.to_string(),
        display_name: v
            .get("display_name")
            .and_then(|d| d.as_str())
            .map(|s| s.to_string()),
        picture: v
            .get("avatar_url")
            .and_then(|p| p.as_str())
            .map(|s| s.to_string()),
        about: v
            .get("about")
            .and_then(|a| a.as_str())
            .map(|s| s.to_string()),
        nip05: v
            .get("nip05_handle")
            .and_then(|n| n.as_str())
            .map(|s| s.to_string()),
    })
}

/// Convert a member JSON object to an FFI ChannelMember.
pub(crate) fn json_to_member(v: &Value) -> Option<ChannelMember> {
    Some(ChannelMember {
        pubkey: v.get("pubkey")?.as_str()?.to_string(),
        role: match v.get("role").and_then(|r| r.as_str()) {
            Some("owner") => MemberRole::Owner,
            Some("admin") => MemberRole::Admin,
            Some("guest") => MemberRole::Guest,
            Some("bot") => MemberRole::Bot,
            _ => MemberRole::Member,
        },
        profile: v.get("display_name").and_then(|_| {
            Some(UserProfile {
                pubkey: v.get("pubkey")?.as_str()?.to_string(),
                display_name: v
                    .get("display_name")
                    .and_then(|d| d.as_str())
                    .map(|s| s.to_string()),
                picture: None,
                about: None,
                nip05: None,
            })
        }),
    })
}

/// Convert a search hit JSON object to an FFI SearchResult.
pub(crate) fn json_to_search_result(v: &Value) -> Option<SearchResult> {
    Some(SearchResult {
        event_id: v.get("event_id")?.as_str()?.to_string(),
        channel_id: v.get("channel_id")?.as_str()?.to_string(),
        channel_name: v
            .get("channel_name")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string(),
        content: v
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string(),
        author_pubkey: v
            .get("pubkey")
            .and_then(|p| p.as_str())
            .unwrap_or("")
            .to_string(),
        created_at: v.get("created_at").and_then(|c| c.as_i64()).unwrap_or(0),
    })
}

/// Convert a feed item JSON object to an FFI FeedItem.
pub(crate) fn json_to_feed_item(v: &Value, category: FeedCategory) -> Option<FeedItem> {
    Some(FeedItem {
        event_id: v
            .get("id")
            .or_else(|| v.get("event_id"))?
            .as_str()?
            .to_string(),
        channel_id: v
            .get("channel_id")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string(),
        channel_name: v
            .get("channel_name")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string(),
        content: v
            .get("content")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string(),
        author_pubkey: v
            .get("pubkey")
            .and_then(|p| p.as_str())
            .unwrap_or("")
            .to_string(),
        created_at: v.get("created_at").and_then(|c| c.as_i64()).unwrap_or(0),
        kind: v.get("kind").and_then(|k| k.as_u64()).unwrap_or(9) as u32,
        category,
    })
}

/// Convert a DM conversation JSON to an FFI DmConversation.
pub(crate) fn json_to_dm(v: &Value) -> Option<DmConversation> {
    let participants = v
        .get("participants")
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| {
                    let pubkey = p.as_str().map(|s| s.to_string())?;
                    Some(UserProfile {
                        pubkey,
                        display_name: None,
                        picture: None,
                        about: None,
                        nip05: None,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Some(DmConversation {
        channel_id: v.get("id")?.as_str()?.to_string(),
        participants,
        last_message: None,
    })
}

/// Extract a nostr::Event from a relay JSON response and convert to FFI Message.
pub(crate) fn json_to_message(v: &Value) -> Option<Message> {
    let tags = v.get("tags").and_then(|t| t.as_array())?;

    let channel_id = tags
        .iter()
        .find(|t| t.get(0).and_then(|v| v.as_str()) == Some("h"))
        .and_then(|t| t.get(1))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let reply_to = tags
        .iter()
        .find(|t| {
            t.get(0).and_then(|v| v.as_str()) == Some("e")
                && t.get(3).and_then(|v| v.as_str()) == Some("reply")
        })
        .and_then(|t| t.get(1))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let thread_root = tags
        .iter()
        .find(|t| {
            t.get(0).and_then(|v| v.as_str()) == Some("e")
                && t.get(3).and_then(|v| v.as_str()) == Some("root")
        })
        .and_then(|t| t.get(1))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Extract media from imeta tags.
    let media: Vec<MediaAttachment> = tags
        .iter()
        .filter(|t| t.get(0).and_then(|v| v.as_str()) == Some("imeta"))
        .filter_map(|t| {
            let parts: Vec<&str> = t
                .as_array()?
                .iter()
                .skip(1)
                .filter_map(|v| v.as_str())
                .collect();

            let mut url = None;
            let mut mime_type = None;
            let mut size_bytes = None;
            let mut dimensions = None;
            let mut blurhash = None;
            let mut thumbnail_url = None;

            for part in parts {
                if let Some(val) = part.strip_prefix("url ") {
                    url = Some(val.to_string());
                } else if let Some(val) = part.strip_prefix("m ") {
                    mime_type = Some(val.to_string());
                } else if let Some(val) = part.strip_prefix("size ") {
                    size_bytes = val.parse().ok();
                } else if let Some(val) = part.strip_prefix("dim ") {
                    dimensions = Some(val.to_string());
                } else if let Some(val) = part.strip_prefix("blurhash ") {
                    blurhash = Some(val.to_string());
                } else if let Some(val) = part.strip_prefix("thumb ") {
                    thumbnail_url = Some(val.to_string());
                }
            }

            Some(MediaAttachment {
                url: url?,
                mime_type: mime_type.unwrap_or_else(|| "application/octet-stream".to_string()),
                size_bytes,
                dimensions,
                blurhash,
                thumbnail_url,
            })
        })
        .collect();

    Some(Message {
        event_id: v.get("id")?.as_str()?.to_string(),
        channel_id,
        author_pubkey: v.get("pubkey")?.as_str()?.to_string(),
        content: v.get("content")?.as_str()?.to_string(),
        created_at: v.get("created_at")?.as_i64()?,
        kind: v.get("kind")?.as_u64()? as u32,
        reply_to,
        thread_root,
        reactions: Vec::new(),
        media,
        reply_count: 0,
        author_profile: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_channel_json() {
        let json = serde_json::json!({
            "id": "ch-1",
            "name": "general",
            "channel_type": "stream",
            "visibility": "open",
            "member_count": 10,
            "is_member": true,
        });
        let ch = json_to_channel(&json).unwrap();
        assert_eq!(ch.id, "ch-1");
        assert_eq!(ch.name, "general");
        assert_eq!(ch.member_count, 10);
    }

    #[test]
    fn convert_profile_json() {
        let json = serde_json::json!({
            "pubkey": "aabb",
            "display_name": "Alice",
            "avatar_url": "https://example.com/alice.jpg",
        });
        let p = json_to_profile(&json).unwrap();
        assert_eq!(p.pubkey, "aabb");
        assert_eq!(p.display_name, Some("Alice".to_string()));
        assert_eq!(p.picture, Some("https://example.com/alice.jpg".to_string()));
    }

    #[test]
    fn convert_event_json_to_message() {
        let json = serde_json::json!({
            "id": "evt-1",
            "pubkey": "aabb",
            "kind": 9,
            "content": "hello",
            "created_at": 1000,
            "tags": [
                ["h", "ch-1"],
                ["e", "root-1", "", "root"],
                ["e", "parent-1", "", "reply"],
            ],
            "sig": "deadbeef",
        });
        let msg = json_to_message(&json).unwrap();
        assert_eq!(msg.event_id, "evt-1");
        assert_eq!(msg.channel_id, "ch-1");
        assert_eq!(msg.thread_root, Some("root-1".to_string()));
        assert_eq!(msg.reply_to, Some("parent-1".to_string()));
    }

    #[test]
    fn convert_message_with_imeta() {
        let json = serde_json::json!({
            "id": "evt-2",
            "pubkey": "aabb",
            "kind": 9,
            "content": "check this out",
            "created_at": 2000,
            "tags": [
                ["h", "ch-1"],
                ["imeta", "url https://example.com/img.jpg", "m image/jpeg", "size 1024", "dim 800x600"],
            ],
            "sig": "deadbeef",
        });
        let msg = json_to_message(&json).unwrap();
        assert_eq!(msg.media.len(), 1);
        assert_eq!(msg.media[0].url, "https://example.com/img.jpg");
        assert_eq!(msg.media[0].mime_type, "image/jpeg");
        assert_eq!(msg.media[0].dimensions, Some("800x600".to_string()));
    }
}
