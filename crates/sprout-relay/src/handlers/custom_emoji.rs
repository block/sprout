//! Relay-global custom emoji command handler.
//!
//! Sprout models custom emoji like Slack workspace emoji: one relay-owned
//! canonical kind:30030 set, editable by relay members via user-signed command
//! events. Command events are processed directly and are not stored.

use std::sync::Arc;

use nostr::Event;
use sprout_core::kind::{KIND_EMOJI_SET, KIND_EMOJI_SET_D_TAG, KIND_RELAY_EMOJI_COMMAND};
use sprout_core::StoredEvent;
use sprout_sdk::{build_custom_emoji_set, normalize_custom_emoji_shortcode, CustomEmoji};
use tracing::{info, warn};

use crate::handlers::event::dispatch_persistent_event;
use crate::state::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EmojiCommandAction {
    Set,
    Remove,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EmojiCommand {
    action: EmojiCommandAction,
    shortcode: String,
    url: Option<String>,
}

fn extract_tag_value(event: &Event, name: &str) -> Option<String> {
    event.tags.iter().find_map(|tag| {
        let parts = tag.as_slice();
        if parts.first().map(|s| s.as_str()) == Some(name) {
            parts.get(1).map(|s| s.to_string())
        } else {
            None
        }
    })
}

fn parse_emoji_command(event: &Event) -> Result<EmojiCommand, String> {
    let action = match extract_tag_value(event, "action").as_deref() {
        Some("set") => EmojiCommandAction::Set,
        Some("remove") => EmojiCommandAction::Remove,
        Some(other) => return Err(format!("invalid action: {other}")),
        None => return Err("missing action tag".to_string()),
    };

    let emoji_tags: Vec<Vec<String>> = event
        .tags
        .iter()
        .filter_map(|tag| {
            let parts = tag.as_slice();
            (parts.first().map(|s| s.as_str()) == Some("emoji")).then(|| parts.to_vec())
        })
        .collect();

    if emoji_tags.len() != 1 {
        return Err(format!(
            "emoji command must include exactly one emoji tag (got {})",
            emoji_tags.len()
        ));
    }

    let tag = &emoji_tags[0];
    let raw_shortcode = tag
        .get(1)
        .ok_or_else(|| "emoji tag missing shortcode".to_string())?;
    let shortcode = normalize_custom_emoji_shortcode(raw_shortcode).map_err(|e| e.to_string())?;

    match action {
        EmojiCommandAction::Set => {
            if tag.len() != 3 {
                return Err("set command emoji tag must be [emoji, shortcode, url]".to_string());
            }
            let url = tag[2].clone();
            validate_emoji_url(&url)?;
            Ok(EmojiCommand {
                action,
                shortcode,
                url: Some(url),
            })
        }
        EmojiCommandAction::Remove => {
            if tag.len() != 2 {
                return Err("remove command emoji tag must be [emoji, shortcode]".to_string());
            }
            Ok(EmojiCommand {
                action,
                shortcode,
                url: None,
            })
        }
    }
}

fn validate_emoji_url(url: &str) -> Result<(), String> {
    if url.is_empty() {
        return Err("emoji image URL must not be empty".to_string());
    }
    if url.len() > 2048 {
        return Err(format!(
            "emoji image URL exceeds 2048 bytes (got {})",
            url.len()
        ));
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("emoji image URL must start with http:// or https://".to_string());
    }
    Ok(())
}

async fn fetch_current_emojis(state: &Arc<AppState>) -> Result<Vec<CustomEmoji>, String> {
    let relay_pubkey = state.relay_keypair.public_key().to_bytes().to_vec();
    let events = state
        .db
        .query_events(&sprout_db::event::EventQuery {
            kinds: Some(vec![KIND_EMOJI_SET as i32]),
            pubkey: Some(relay_pubkey),
            d_tag: Some(KIND_EMOJI_SET_D_TAG.to_string()),
            global_only: true,
            limit: Some(1),
            ..Default::default()
        })
        .await
        .map_err(|e| format!("database error: {e}"))?;

    let Some(event) = events.first() else {
        return Ok(vec![]);
    };

    let mut emojis = Vec::new();
    for tag in event.event.tags.iter() {
        let parts = tag.as_slice();
        if parts.first().map(|s| s.as_str()) != Some("emoji") {
            continue;
        }
        if parts.len() < 3 {
            continue;
        }
        let shortcode = normalize_custom_emoji_shortcode(&parts[1]).map_err(|e| e.to_string())?;
        let url = parts[2].clone();
        validate_emoji_url(&url)?;
        emojis.push(CustomEmoji { shortcode, url });
    }
    Ok(emojis)
}

async fn next_emoji_set_timestamp(state: &Arc<AppState>) -> u64 {
    let relay_pubkey = state.relay_keypair.public_key().to_bytes().to_vec();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let min_ts = state
        .db
        .query_events(&sprout_db::event::EventQuery {
            kinds: Some(vec![KIND_EMOJI_SET as i32]),
            pubkey: Some(relay_pubkey),
            d_tag: Some(KIND_EMOJI_SET_D_TAG.to_string()),
            global_only: true,
            limit: Some(1),
            ..Default::default()
        })
        .await
        .ok()
        .and_then(|events| events.first().map(|e| e.event.created_at.as_secs() + 1))
        .unwrap_or(now);
    now.max(min_ts)
}

async fn publish_emoji_set(
    state: &Arc<AppState>,
    emojis: &[CustomEmoji],
) -> Result<StoredEvent, String> {
    let ts = next_emoji_set_timestamp(state).await;
    let event = build_custom_emoji_set(emojis)
        .map_err(|e| e.to_string())?
        .custom_created_at(nostr::Timestamp::from(ts))
        .sign_with_keys(&state.relay_keypair)
        .map_err(|e| format!("failed to sign emoji set: {e}"))?;

    let (stored, was_inserted) = state
        .db
        .replace_parameterized_event(&event, KIND_EMOJI_SET_D_TAG, None)
        .await
        .map_err(|e| format!("database error: {e}"))?;

    if was_inserted {
        let relay_pubkey_hex = state.relay_keypair.public_key().to_hex();
        dispatch_persistent_event(state, &stored, KIND_EMOJI_SET, &relay_pubkey_hex).await;
    } else {
        warn!("relay emoji set update was rejected as stale/duplicate");
    }

    Ok(stored)
}

/// Validate and execute a relay-global custom emoji command (kind:9037).
pub async fn handle_custom_emoji_command(
    state: &Arc<AppState>,
    event: &Event,
) -> Result<(), String> {
    let kind = event.kind.as_u16() as u32;
    if kind != KIND_RELAY_EMOJI_COMMAND {
        return Err(format!("unexpected custom emoji command kind: {kind}"));
    }

    // Mirror relay_admin freshness: commands should be freshly signed.
    let event_ts = event.created_at.as_secs() as i64;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    if (event_ts - now).abs() > 120 {
        return Err(format!(
            "event timestamp out of range: created_at={event_ts}, now={now}, delta={}s (max ±120s)",
            event_ts - now
        ));
    }

    let sender_hex = event.pubkey.to_hex();
    if state
        .db
        .get_relay_member(&sender_hex)
        .await
        .map_err(|e| format!("database error: {e}"))?
        .is_none()
    {
        return Err("actor not authorized: must be a relay member".to_string());
    }

    let command = parse_emoji_command(event)?;
    let mut emojis = fetch_current_emojis(state).await?;

    match command.action {
        EmojiCommandAction::Set => {
            let url = command.url.expect("set command parser must provide url");
            let mut updated = false;
            for emoji in &mut emojis {
                if emoji.shortcode == command.shortcode {
                    emoji.url = url.clone();
                    updated = true;
                    break;
                }
            }
            if !updated {
                emojis.push(CustomEmoji {
                    shortcode: command.shortcode.clone(),
                    url,
                });
                emojis.sort_by(|a, b| a.shortcode.cmp(&b.shortcode));
            }
            publish_emoji_set(state, &emojis).await?;
            info!(sender = %sender_hex, shortcode = %command.shortcode, "custom emoji set");
        }
        EmojiCommandAction::Remove => {
            let before = emojis.len();
            emojis.retain(|emoji| emoji.shortcode != command.shortcode);
            if emojis.len() == before {
                return Err(format!("emoji not found: {}", command.shortcode));
            }
            publish_emoji_set(state, &emojis).await?;
            info!(sender = %sender_hex, shortcode = %command.shortcode, "custom emoji removed");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind, Tag};

    fn make_event(tags: Vec<Vec<&'static str>>) -> Event {
        let keys = Keys::generate();
        let tags: Vec<Tag> = tags
            .into_iter()
            .map(|parts| Tag::parse(parts).expect("tag"))
            .collect();
        EventBuilder::new(Kind::Custom(KIND_RELAY_EMOJI_COMMAND as u16), "")
            .tags(tags)
            .sign_with_keys(&keys)
            .expect("sign")
    }

    #[test]
    fn parses_set_command() {
        let event = make_event(vec![
            vec!["action", "set"],
            vec!["emoji", "Party_Parrot", "https://example.com/parrot.png"],
        ]);
        let command = parse_emoji_command(&event).expect("parse");
        assert_eq!(command.action, EmojiCommandAction::Set);
        assert_eq!(command.shortcode, "party_parrot");
        assert_eq!(
            command.url.as_deref(),
            Some("https://example.com/parrot.png")
        );
    }

    #[test]
    fn parses_remove_command() {
        let event = make_event(vec![vec!["action", "remove"], vec!["emoji", "party"]]);
        let command = parse_emoji_command(&event).expect("parse");
        assert_eq!(command.action, EmojiCommandAction::Remove);
        assert_eq!(command.shortcode, "party");
        assert_eq!(command.url, None);
    }

    #[test]
    fn rejects_missing_action() {
        let event = make_event(vec![vec!["emoji", "party", "https://example.com/p.png"]]);
        assert!(parse_emoji_command(&event)
            .unwrap_err()
            .contains("missing action"));
    }

    #[test]
    fn rejects_multiple_emoji_tags() {
        let event = make_event(vec![
            vec!["action", "remove"],
            vec!["emoji", "party"],
            vec!["emoji", "other"],
        ]);
        assert!(parse_emoji_command(&event)
            .unwrap_err()
            .contains("exactly one"));
    }

    #[test]
    fn rejects_invalid_shortcode() {
        let event = make_event(vec![vec!["action", "remove"], vec!["emoji", "bad space"]]);
        assert!(parse_emoji_command(&event)
            .unwrap_err()
            .contains("shortcode"));
    }
}
