//! NIP-29 and NIP-25 side-effect handlers.

use std::sync::Arc;

use nostr::{Event, EventBuilder, Kind, Tag};
use tracing::{info, warn};
use uuid::Uuid;

use sprout_db::channel::MemberRole;

use crate::state::AppState;

/// Check if a kind is an admin kind (9000-9022) that needs pre-storage validation.
pub fn is_admin_kind(kind: u32) -> bool {
    matches!(kind, 9000..=9022)
}

/// Check if a kind triggers side effects after storage.
pub fn is_side_effect_kind(kind: u32) -> bool {
    matches!(kind, 7 | 9000..=9022 | 41001..=41003 | 40099)
}

/// Dispatch side effects for a stored event.
pub async fn handle_side_effects(
    kind: u32,
    event: &Event,
    state: &Arc<AppState>,
) -> anyhow::Result<()> {
    match kind {
        9000 => handle_put_user(event, state).await,
        9001 => handle_remove_user(event, state).await,
        9002 => handle_edit_metadata(event, state).await,
        9005 => handle_delete_event_side_effect(event, state).await,
        9007 => handle_create_group(event, state).await,
        9008 => handle_delete_group(event, state).await,
        9009 | 9021 => {
            warn!(
                kind = kind,
                "NIP-29 kind {kind} handler deferred to future phase"
            );
            Ok(())
        }
        9022 => handle_leave_request(event, state).await,
        7 => handle_reaction(event, state).await,
        _ => Ok(()),
    }
}

/// Validate an admin kind event BEFORE storage.
pub async fn validate_admin_event(
    kind: u32,
    event: &Event,
    state: &Arc<AppState>,
) -> anyhow::Result<()> {
    // CREATE_GROUP doesn't need an existing channel — skip h-tag extraction
    if kind == 9007 {
        return Ok(());
    }

    // Extract channel from h tag
    let channel_id =
        extract_h_tag_channel(event).ok_or_else(|| anyhow::anyhow!("missing or invalid h tag"))?;

    let actor_bytes = event.pubkey.serialize().to_vec();

    // Reject mutations on archived channels.
    let channel = state
        .db
        .get_channel(channel_id)
        .await
        .map_err(|_| anyhow::anyhow!("channel not found"))?;
    if channel.archived_at.is_some() {
        return Err(anyhow::anyhow!("channel is archived"));
    }

    match kind {
        9000 => {
            // PUT_USER: open channels allow any member; private requires owner/admin
            if channel.visibility == "private" {
                // Check actor is owner/admin
                let members = state.db.get_members(channel_id).await?;
                let actor_member = members.iter().find(|m| m.pubkey == actor_bytes);
                match actor_member {
                    Some(m) if m.role == "owner" || m.role == "admin" => Ok(()),
                    _ => Err(anyhow::anyhow!("actor not authorized")),
                }
            } else {
                // Open channel: any authenticated user can add
                Ok(())
            }
        }
        9001 => {
            // REMOVE_USER: self-remove allowed unless actor is the last owner; removing others requires owner/admin
            let target_pubkey =
                extract_p_tag(event).ok_or_else(|| anyhow::anyhow!("missing p tag"))?;
            if target_pubkey == actor_bytes {
                // Self-removal: must be an active member, and cannot be the last owner.
                let members = state.db.get_members(channel_id).await?;
                let actor_member = members.iter().find(|m| m.pubkey == actor_bytes);
                match actor_member {
                    None => {
                        return Err(anyhow::anyhow!("actor is not an active member"));
                    }
                    Some(m) if m.role == "owner" => {
                        let owner_count = members.iter().filter(|m| m.role == "owner").count();
                        if owner_count <= 1 {
                            return Err(anyhow::anyhow!("cannot remove the last owner"));
                        }
                    }
                    _ => {}
                }
                Ok(())
            } else {
                let members = state.db.get_members(channel_id).await?;
                let actor_member = members.iter().find(|m| m.pubkey == actor_bytes);
                match actor_member {
                    Some(m) if m.role == "owner" || m.role == "admin" => Ok(()),
                    _ => Err(anyhow::anyhow!("actor not authorized")),
                }
            }
        }
        9002 => {
            // EDIT_METADATA: name/about require owner/admin; topic/purpose allow any member
            let has_name_or_about = event.tags.iter().any(|t| {
                let k = t.kind().to_string();
                k == "name" || k == "about"
            });
            if has_name_or_about {
                let members = state.db.get_members(channel_id).await?;
                let actor_member = members.iter().find(|m| m.pubkey == actor_bytes);
                match actor_member {
                    Some(m) if m.role == "owner" || m.role == "admin" => Ok(()),
                    _ => Err(anyhow::anyhow!(
                        "actor not authorized for name/about changes"
                    )),
                }
            } else {
                // topic/purpose: any member
                let is_member = state.db.is_member(channel_id, &actor_bytes).await?;
                if is_member {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("not a member"))
                }
            }
        }
        9005 => {
            // DELETE_EVENT: event author OR channel owner/admin.
            // Extract target event from e tag to check authorship.
            let target_id = event
                .tags
                .iter()
                .find_map(|tag| {
                    if tag.kind().to_string() == "e" {
                        tag.content().and_then(|v| hex::decode(v).ok())
                    } else {
                        None
                    }
                })
                .ok_or_else(|| anyhow::anyhow!("missing e tag for target event"))?;

            // Verify the target event belongs to the h-tag channel BEFORE storage.
            // Without this, an admin of channel A could craft h=A, e=<event-in-B>
            // and the relay would store the invalid admin event.
            if let Ok(Some(target_event)) = state.db.get_event_by_id(&target_id).await {
                match target_event.channel_id {
                    Some(target_ch) if target_ch != channel_id => {
                        return Err(anyhow::anyhow!(
                            "target event belongs to a different channel"
                        ));
                    }
                    None => {
                        return Err(anyhow::anyhow!("target event has no channel"));
                    }
                    _ => {} // Same channel — OK
                }

                // Check if actor is the event author.
                // For relay-signed REST messages, the real author is in the p tag.
                let author = effective_message_author(
                    &target_event.event,
                    &state.relay_keypair.public_key(),
                );
                if author == actor_bytes {
                    return Ok(()); // Author can always delete their own messages
                }
            }

            // Not the author — must be owner/admin.
            let members = state.db.get_members(channel_id).await?;
            let actor_member = members.iter().find(|m| m.pubkey == actor_bytes);
            match actor_member {
                Some(m) if m.role == "owner" || m.role == "admin" => Ok(()),
                _ => Err(anyhow::anyhow!(
                    "must be event author or channel owner/admin"
                )),
            }
        }
        9008 => {
            // DELETE_GROUP: owner only
            let members = state.db.get_members(channel_id).await?;
            let actor_member = members.iter().find(|m| m.pubkey == actor_bytes);
            match actor_member {
                Some(m) if m.role == "owner" => Ok(()),
                _ => Err(anyhow::anyhow!("only owner can delete group")),
            }
        }
        9022 => {
            // LEAVE_REQUEST: must be an active member, and cannot be the last owner.
            let members = state.db.get_members(channel_id).await?;
            let actor_member = members.iter().find(|m| m.pubkey == actor_bytes);
            match actor_member {
                None => {
                    return Err(anyhow::anyhow!("actor is not an active member"));
                }
                Some(m) if m.role == "owner" => {
                    let owner_count = members.iter().filter(|m| m.role == "owner").count();
                    if owner_count <= 1 {
                        return Err(anyhow::anyhow!("cannot remove the last owner"));
                    }
                }
                _ => {}
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Emit a system message (kind 40099) signed by the relay keypair.
pub async fn emit_system_message(
    state: &Arc<AppState>,
    channel_id: Uuid,
    content: serde_json::Value,
) -> anyhow::Result<()> {
    let channel_tag = Tag::parse(&["h", &channel_id.to_string()])?;

    let event = EventBuilder::new(Kind::Custom(40099), content.to_string(), [channel_tag])
        .sign_with_keys(&state.relay_keypair)
        .map_err(|e| anyhow::anyhow!("failed to sign system message: {e}"))?;

    let _ = state.db.insert_event(&event, Some(channel_id)).await;

    // Fan out to subscribers
    if let Err(e) = state.pubsub.publish_event(channel_id, &event).await {
        warn!("System message fan-out failed: {e}");
    }

    Ok(())
}

// ── NIP-29 Handlers ──────────────────────────────────────────────────────────

async fn handle_put_user(event: &Event, state: &Arc<AppState>) -> anyhow::Result<()> {
    let channel_id =
        extract_h_tag_channel(event).ok_or_else(|| anyhow::anyhow!("missing h tag"))?;
    let target_pubkey = extract_p_tag(event).ok_or_else(|| anyhow::anyhow!("missing p tag"))?;
    let role_str = extract_tag_value(event, "role").unwrap_or_else(|| "member".to_string());
    let role: MemberRole = role_str
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid role: {role_str}"))?;

    let actor_bytes = event.pubkey.serialize().to_vec();

    state
        .db
        .add_member(channel_id, &target_pubkey, role, Some(&actor_bytes))
        .await?;

    let actor_hex = nostr::util::hex::encode(&actor_bytes);
    let target_hex = nostr::util::hex::encode(&target_pubkey);
    emit_system_message(
        state,
        channel_id,
        serde_json::json!({
            "type": "member_joined",
            "actor": actor_hex,
            "target": target_hex,
        }),
    )
    .await?;

    info!(channel = %channel_id, target = %target_hex, "NIP-29 PUT_USER processed");
    Ok(())
}

async fn handle_remove_user(event: &Event, state: &Arc<AppState>) -> anyhow::Result<()> {
    let channel_id =
        extract_h_tag_channel(event).ok_or_else(|| anyhow::anyhow!("missing h tag"))?;
    let target_pubkey = extract_p_tag(event).ok_or_else(|| anyhow::anyhow!("missing p tag"))?;
    let actor_bytes = event.pubkey.serialize().to_vec();

    // Guard: prevent last-owner orphaning on self-removal (kind 9001).
    if target_pubkey == actor_bytes {
        let members = state.db.get_members(channel_id).await?;
        let owner_count = members.iter().filter(|m| m.role == "owner").count();
        let actor_is_owner = members
            .iter()
            .any(|m| m.pubkey == actor_bytes && m.role == "owner");
        if actor_is_owner && owner_count <= 1 {
            return Err(anyhow::anyhow!(
                "cannot remove the last owner — transfer ownership first"
            ));
        }
    }

    state
        .db
        .remove_member(channel_id, &target_pubkey, &actor_bytes)
        .await?;

    let actor_hex = nostr::util::hex::encode(&actor_bytes);
    let target_hex = nostr::util::hex::encode(&target_pubkey);
    let msg_type = if target_pubkey == actor_bytes {
        "member_left"
    } else {
        "member_removed"
    };
    emit_system_message(
        state,
        channel_id,
        serde_json::json!({
            "type": msg_type,
            "actor": actor_hex,
            "target": target_hex,
        }),
    )
    .await?;

    Ok(())
}

async fn handle_edit_metadata(event: &Event, state: &Arc<AppState>) -> anyhow::Result<()> {
    let channel_id =
        extract_h_tag_channel(event).ok_or_else(|| anyhow::anyhow!("missing h tag"))?;
    let actor_bytes = event.pubkey.serialize().to_vec();
    let actor_hex = nostr::util::hex::encode(&actor_bytes);

    for tag in event.tags.iter() {
        let key = tag.kind().to_string();
        if let Some(val) = tag.content() {
            match key.as_str() {
                "name" => {
                    state
                        .db
                        .update_channel(
                            channel_id,
                            sprout_db::channel::ChannelUpdate {
                                name: Some(val.to_string()),
                                description: None,
                            },
                        )
                        .await?;
                }
                "about" => {
                    state
                        .db
                        .update_channel(
                            channel_id,
                            sprout_db::channel::ChannelUpdate {
                                name: None,
                                description: Some(val.to_string()),
                            },
                        )
                        .await?;
                }
                "topic" => {
                    state.db.set_topic(channel_id, val, &actor_bytes).await?;
                    emit_system_message(
                        state,
                        channel_id,
                        serde_json::json!({
                            "type": "topic_changed", "actor": actor_hex, "topic": val
                        }),
                    )
                    .await?;
                }
                "purpose" => {
                    state.db.set_purpose(channel_id, val, &actor_bytes).await?;
                    emit_system_message(
                        state,
                        channel_id,
                        serde_json::json!({
                            "type": "purpose_changed", "actor": actor_hex, "purpose": val
                        }),
                    )
                    .await?;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

async fn handle_delete_event_side_effect(
    event: &Event,
    state: &Arc<AppState>,
) -> anyhow::Result<()> {
    let channel_id =
        extract_h_tag_channel(event).ok_or_else(|| anyhow::anyhow!("missing h tag"))?;

    // Extract target event ID from e tag
    let target_id = event
        .tags
        .iter()
        .find_map(|tag| {
            if tag.kind().to_string() == "e" {
                tag.content().and_then(|v| {
                    let bytes = hex::decode(v).ok()?;
                    if bytes.len() == 32 {
                        Some(bytes)
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        })
        .ok_or_else(|| anyhow::anyhow!("missing e tag for target event"))?;

    // Verify the target event belongs to the same channel as the h-tag.
    // Without this check, an admin of channel A could delete events in channel B
    // by sending h=A, e=<event-in-B>.
    if let Some(target_event) = state
        .db
        .get_event_by_id_including_deleted(&target_id)
        .await
        .map_err(|e| anyhow::anyhow!("get_event_by_id failed: {e}"))?
    {
        match target_event.channel_id {
            Some(target_ch) if target_ch != channel_id => {
                return Err(anyhow::anyhow!(
                    "target event belongs to a different channel"
                ));
            }
            None => {
                return Err(anyhow::anyhow!("target event has no channel"));
            }
            _ => {} // Same channel — OK
        }
    }

    // Look up thread metadata so we can pass parent/root IDs to the
    // transactional delete function.
    let meta = state
        .db
        .get_thread_metadata_by_event(&target_id)
        .await
        .map_err(|e| anyhow::anyhow!("get_thread_metadata failed: {e}"))?;

    let parent_id = meta.as_ref().and_then(|m| m.parent_event_id.clone());
    let root_id = meta.as_ref().and_then(|m| m.root_event_id.clone());

    // Atomically soft-delete the event and decrement thread counters in one transaction.
    let deleted = state
        .db
        .soft_delete_event_and_update_thread(&target_id, parent_id.as_deref(), root_id.as_deref())
        .await
        .map_err(|e| anyhow::anyhow!("soft_delete_event failed: {e}"))?;

    if !deleted {
        warn!(target_event = %hex::encode(&target_id), "event already deleted or not found");
        return Ok(()); // No-op: skip system message to avoid false audit records.
    }

    let actor_hex = nostr::util::hex::encode(event.pubkey.serialize());
    emit_system_message(
        state,
        channel_id,
        serde_json::json!({
            "type": "message_deleted",
            "actor": actor_hex,
            "target_event_id": hex::encode(&target_id),
        }),
    )
    .await?;

    info!(target_event = %hex::encode(&target_id), "NIP-29 DELETE_EVENT processed");
    Ok(())
}

async fn handle_create_group(event: &Event, state: &Arc<AppState>) -> anyhow::Result<()> {
    let name =
        extract_tag_value(event, "name").ok_or_else(|| anyhow::anyhow!("missing name tag"))?;
    let visibility_str =
        extract_tag_value(event, "visibility").unwrap_or_else(|| "open".to_string());
    let channel_type_str =
        extract_tag_value(event, "channel_type").unwrap_or_else(|| "stream".to_string());

    let visibility: sprout_db::channel::ChannelVisibility = visibility_str
        .parse()
        .unwrap_or(sprout_db::channel::ChannelVisibility::Open);
    let channel_type: sprout_db::channel::ChannelType = channel_type_str
        .parse()
        .unwrap_or(sprout_db::channel::ChannelType::Stream);

    let actor_bytes = event.pubkey.serialize().to_vec();
    let channel = state
        .db
        .create_channel(&name, channel_type, visibility, None, &actor_bytes)
        .await?;

    let actor_hex = nostr::util::hex::encode(&actor_bytes);
    emit_system_message(
        state,
        channel.id,
        serde_json::json!({
            "type": "channel_created", "actor": actor_hex
        }),
    )
    .await?;

    info!(channel_id = %channel.id, name = %name, "NIP-29 CREATE_GROUP processed");
    Ok(())
}

async fn handle_delete_group(event: &Event, state: &Arc<AppState>) -> anyhow::Result<()> {
    let channel_id =
        extract_h_tag_channel(event).ok_or_else(|| anyhow::anyhow!("missing h tag"))?;
    let actor_bytes = event.pubkey.serialize().to_vec();

    // Soft-delete the channel.
    let deleted = state
        .db
        .soft_delete_channel(channel_id)
        .await
        .map_err(|e| anyhow::anyhow!("soft_delete_channel failed: {e}"))?;

    if !deleted {
        warn!(channel = %channel_id, "channel already deleted or not found");
    }

    let actor_hex = nostr::util::hex::encode(&actor_bytes);
    emit_system_message(
        state,
        channel_id,
        serde_json::json!({
            "type": "channel_deleted", "actor": actor_hex
        }),
    )
    .await?;

    info!(channel = %channel_id, "NIP-29 DELETE_GROUP processed");
    Ok(())
}

async fn handle_leave_request(event: &Event, state: &Arc<AppState>) -> anyhow::Result<()> {
    // Kind 9022: functionally identical to self-remove via kind 9001
    let channel_id =
        extract_h_tag_channel(event).ok_or_else(|| anyhow::anyhow!("missing h tag"))?;
    let actor_bytes = event.pubkey.serialize().to_vec();

    // Guard: prevent last-owner orphaning on leave.
    let members = state.db.get_members(channel_id).await?;
    let owner_count = members.iter().filter(|m| m.role == "owner").count();
    let actor_is_owner = members
        .iter()
        .any(|m| m.pubkey == actor_bytes && m.role == "owner");
    if actor_is_owner && owner_count <= 1 {
        return Err(anyhow::anyhow!(
            "cannot remove the last owner — transfer ownership first"
        ));
    }

    state
        .db
        .remove_member(channel_id, &actor_bytes, &actor_bytes)
        .await?;

    let actor_hex = nostr::util::hex::encode(&actor_bytes);
    emit_system_message(
        state,
        channel_id,
        serde_json::json!({
            "type": "member_left",
            "actor": actor_hex,
        }),
    )
    .await?;

    Ok(())
}

async fn handle_reaction(event: &Event, state: &Arc<AppState>) -> anyhow::Result<()> {
    // Extract target event from last e tag (NIP-25)
    let target_hex = event
        .tags
        .iter()
        .rev()
        .find_map(|tag| {
            if tag.kind().to_string() == "e" {
                tag.content().and_then(|v| {
                    if v.len() == 64 && v.chars().all(|c| c.is_ascii_hexdigit()) {
                        Some(v.to_string())
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        })
        .ok_or_else(|| anyhow::anyhow!("missing e tag for reaction target"))?;

    let target_id = hex::decode(&target_hex)?;

    // Look up target event to get created_at for partitioned table lookup
    let target_event = state
        .db
        .get_event_by_id(&target_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("reaction target event not found"))?;

    // Reject reactions on archived channels.
    if let Some(channel_id) = target_event.channel_id {
        let channel = state
            .db
            .get_channel(channel_id)
            .await
            .map_err(|_| anyhow::anyhow!("channel not found"))?;
        if channel.archived_at.is_some() {
            return Err(anyhow::anyhow!("channel is archived"));
        }
    }

    let event_created_at =
        chrono::DateTime::from_timestamp(target_event.event.created_at.as_u64() as i64, 0)
            .unwrap_or_else(chrono::Utc::now);

    let pubkey_bytes = event.pubkey.serialize().to_vec();
    let emoji = if event.content.is_empty() {
        "+"
    } else {
        &event.content
    };

    state
        .db
        .add_reaction(&target_id, event_created_at, &pubkey_bytes, emoji)
        .await?;

    info!(target = %target_hex, emoji = %emoji, "NIP-25 reaction processed");
    Ok(())
}

// ── Tag Helpers ──────────────────────────────────────────────────────────────

/// Extract channel UUID from `h` tag (NIP-29 group ID).
fn extract_h_tag_channel(event: &Event) -> Option<Uuid> {
    for tag in event.tags.iter() {
        if tag.kind().to_string() == "h" {
            if let Some(val) = tag.content() {
                if let Ok(id) = val.parse::<Uuid>() {
                    return Some(id);
                }
            }
        }
    }
    None
}

/// Extract target pubkey from first `p` tag.
fn extract_p_tag(event: &Event) -> Option<Vec<u8>> {
    for tag in event.tags.iter() {
        if tag.kind().to_string() == "p" {
            if let Some(val) = tag.content() {
                if let Ok(bytes) = hex::decode(val) {
                    if bytes.len() == 32 {
                        return Some(bytes);
                    }
                }
            }
        }
    }
    None
}

/// Extract the effective message author from a stored event.
///
/// REST-created messages are signed by the relay keypair and attribute the real
/// sender via a `p` tag. For user-signed events (WebSocket), `event.pubkey` is
/// the author. Returns the correct author bytes in both cases.
fn effective_message_author(event: &Event, relay_pubkey: &nostr::PublicKey) -> Vec<u8> {
    if event.pubkey == *relay_pubkey {
        for tag in event.tags.iter() {
            if tag.kind().to_string() == "p" {
                if let Some(hex) = tag.content() {
                    if let Ok(bytes) = hex::decode(hex) {
                        if bytes.len() == 32 {
                            return bytes;
                        }
                    }
                }
            }
        }
    }
    event.pubkey.serialize().to_vec()
}

/// Extract value of a named tag.
fn extract_tag_value(event: &Event, tag_name: &str) -> Option<String> {
    for tag in event.tags.iter() {
        if tag.kind().to_string() == tag_name {
            return tag.content().map(|s| s.to_string());
        }
    }
    None
}
