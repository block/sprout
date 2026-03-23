//! NIP-29 and NIP-25 side-effect handlers.

use std::sync::Arc;

use nostr::{Event, EventBuilder, Kind, Tag};
use tracing::{info, warn};
use uuid::Uuid;

use sprout_core::kind::{
    event_kind_u32, KIND_MEMBER_ADDED_NOTIFICATION, KIND_MEMBER_REMOVED_NOTIFICATION,
    KIND_NIP29_GROUP_ADMINS, KIND_NIP29_GROUP_MEMBERS, KIND_NIP29_GROUP_METADATA, KIND_REACTION,
};
use sprout_db::channel::MemberRole;

use super::event::dispatch_persistent_event;
use crate::state::AppState;

/// Check if a kind is an admin kind (9000-9022) that needs pre-storage validation.
pub fn is_admin_kind(kind: u32) -> bool {
    matches!(kind, 9000..=9022)
}

/// Check if a kind triggers side effects after storage.
///
/// NOTE: kind:7 (reaction) is intentionally excluded — dedup and DB writes are
/// handled in `ingest_event()` before storage so we can short-circuit on
/// duplicates without storing the event at all.
pub fn is_side_effect_kind(kind: u32) -> bool {
    matches!(kind, 0 | 5 | 9000..=9022 | 41001..=41003 | 40099)
}

/// Dispatch side effects for a stored event.
pub async fn handle_side_effects(
    kind: u32,
    event: &Event,
    state: &Arc<AppState>,
) -> anyhow::Result<()> {
    match kind {
        0 => handle_kind0_profile(event, state).await,
        5 => handle_standard_deletion_event(event, state).await,
        9000 => handle_put_user(event, state).await,
        9001 => handle_remove_user(event, state).await,
        9002 => handle_edit_metadata(event, state).await,
        9005 => handle_delete_event_side_effect(event, state).await,
        9007 => handle_create_group(event, state).await,
        9008 => handle_delete_group(event, state).await,
        9009 => {
            warn!(
                kind = kind,
                "NIP-29 kind 9009 handler deferred to future phase"
            );
            Ok(())
        }
        9021 => handle_join_request(event, state).await,
        9022 => handle_leave_request(event, state).await,
        // kind:7 (reaction) handled inline in ingest_event() before storage.
        _ => Ok(()),
    }
}

/// Validate a standard NIP-09 deletion event before it is stored.
///
/// Sprout accepts standard deletions for self-authored events only. Channel
/// admin deletions continue to use kind 9005.
pub async fn validate_standard_deletion_event(
    event: &Event,
    state: &Arc<AppState>,
) -> anyhow::Result<()> {
    let actor_bytes = effective_message_author(event, &state.relay_keypair.public_key());
    let target_ids = extract_target_event_ids(event);

    if target_ids.is_empty() {
        return Err(anyhow::anyhow!("missing e tag for target event"));
    }

    for target_id in target_ids {
        let target_event = state
            .db
            .get_event_by_id_including_deleted(&target_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("target event not found"))?;

        let target_author =
            effective_message_author(&target_event.event, &state.relay_keypair.public_key());
        if target_author != actor_bytes {
            return Err(anyhow::anyhow!("must be event author"));
        }
    }

    Ok(())
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

    // Reject mutations on archived channels — except kind:9002 with archived=false
    // (unarchive), which must be allowed through so the channel can be restored.
    let channel = state
        .db
        .get_channel(channel_id)
        .await
        .map_err(|_| anyhow::anyhow!("channel not found"))?;
    let is_unarchive_request = kind == 9002
        && event.tags.iter().any(|t| {
            let parts = t.as_slice();
            parts.len() >= 2 && parts[0] == "archived" && parts[1] == "false"
        });
    if channel.archived_at.is_some() && !is_unarchive_request {
        return Err(anyhow::anyhow!("channel is archived"));
    }

    match kind {
        9000 => {
            // Validate role tag if present
            let role_str = extract_tag_value(event, "role").unwrap_or_else(|| "member".to_string());
            if role_str.parse::<sprout_db::channel::MemberRole>().is_err() {
                return Err(anyhow::anyhow!("invalid role: {role_str}"));
            }

            // PUT_USER: open channels allow any authenticated user; private requires owner/admin.
            // Policy check applies to both open and private channels.
            if channel.visibility == "private" {
                let members = state.db.get_members(channel_id).await?;
                let actor_member = members.iter().find(|m| m.pubkey == actor_bytes);
                match actor_member {
                    Some(m) if m.role == "owner" || m.role == "admin" => {}
                    _ => return Err(anyhow::anyhow!("actor not authorized")),
                }
            }

            // Extract target pubkey from p tag
            let target_pubkey =
                extract_p_tag(event).ok_or_else(|| anyhow::anyhow!("missing p tag"))?;

            // Self-add: always allowed regardless of policy.
            if target_pubkey == actor_bytes {
                return Ok(());
            }

            // Third-party add: check channel_add_policy on the target.
            if let Some((policy, owner)) = state.db.get_agent_channel_policy(&target_pubkey).await?
            {
                match policy.as_str() {
                    "owner_only" => {
                        let owner_bytes = owner.ok_or_else(|| {
                            anyhow::anyhow!("policy:owner_only — agent has no owner set")
                        })?;
                        if actor_bytes != owner_bytes {
                            return Err(anyhow::anyhow!(
                                "policy:owner_only — only the agent owner can add this agent"
                            ));
                        }
                    }
                    "nobody" => {
                        return Err(anyhow::anyhow!(
                            "policy:nobody — this agent has disabled external channel additions"
                        ));
                    }
                    // "anyone" or any unknown value → allow.
                    // NOTE: DB ENUM constraint prevents unknown values from being stored.
                    // If a new policy value is added to the ENUM, update this match.
                    _ => {}
                }
            }

            Ok(())
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
            // EDIT_METADATA: require at least one recognized metadata tag.
            const RECOGNIZED_TAGS: &[&str] = &["name", "about", "archived", "topic", "purpose"];
            let has_recognized = event
                .tags
                .iter()
                .any(|t| RECOGNIZED_TAGS.contains(&t.kind().to_string().as_str()));
            if !has_recognized {
                return Err(anyhow::anyhow!(
                    "kind:9002 must include at least one metadata tag (name, about, archived, topic, purpose)"
                ));
            }

            // Validate archived values before storage.
            for t in event.tags.iter() {
                if t.kind().to_string() == "archived" {
                    match t.content() {
                        Some("true") | Some("false") => {}
                        Some(v) => {
                            return Err(anyhow::anyhow!(
                                "invalid archived value: {v} (must be \"true\" or \"false\")"
                            ));
                        }
                        None => {
                            return Err(anyhow::anyhow!("archived tag must have a value"));
                        }
                    }
                }
            }

            // name/about/archived require owner/admin; topic/purpose allow any member
            let has_privileged_tag = event.tags.iter().any(|t| {
                let k = t.kind().to_string();
                k == "name" || k == "about" || k == "archived"
            });
            if has_privileged_tag {
                let members = state.db.get_members(channel_id).await?;
                let actor_member = members.iter().find(|m| m.pubkey == actor_bytes);
                match actor_member {
                    Some(m) if m.role == "owner" || m.role == "admin" => Ok(()),
                    _ => Err(anyhow::anyhow!(
                        "actor not authorized for name/about/archived changes"
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

            // Verify the target event exists and belongs to the h-tag channel
            // BEFORE storage. Fail closed: missing target → reject.
            let target_event = state
                .db
                .get_event_by_id(&target_id)
                .await
                .map_err(|e| anyhow::anyhow!("db error looking up target: {e}"))?
                .ok_or_else(|| anyhow::anyhow!("target event not found"))?;

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
            let author =
                effective_message_author(&target_event.event, &state.relay_keypair.public_key());
            if author == actor_bytes {
                return Ok(()); // Author can always delete their own messages
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

    if let Err(e) = state.db.insert_event(&event, Some(channel_id)).await {
        warn!(channel = %channel_id, error = %e, "system message insert failed");
    }

    // Fan out to subscribers
    if let Err(e) = state.pubsub.publish_event(channel_id, &event).await {
        warn!("System message fan-out failed: {e}");
    }

    Ok(())
}

/// Emit a relay-signed membership notification event stored globally (channel_id = None).
///
/// kind:44100 = member added, kind:44101 = member removed.
/// The p tag addresses the target pubkey; the h tag carries the channel UUID as metadata.
/// Stored with channel_id = None so global subscribers receive it via slow-path fan-out.
pub async fn emit_membership_notification(
    state: &Arc<AppState>,
    channel_id: Uuid,
    target_pubkey: &[u8],
    actor_pubkey: &[u8],
    notification_kind: u32,
) -> anyhow::Result<()> {
    let target_hex = hex::encode(target_pubkey);
    let actor_hex = hex::encode(actor_pubkey);
    let channel_id_str = channel_id.to_string();

    let p_tag = Tag::parse(&["p", &target_hex])
        .map_err(|e| anyhow::anyhow!("failed to build p tag: {e}"))?;
    let h_tag = Tag::parse(&["h", &channel_id_str])
        .map_err(|e| anyhow::anyhow!("failed to build h tag: {e}"))?;

    let event_type = match notification_kind {
        KIND_MEMBER_ADDED_NOTIFICATION => "member_added",
        KIND_MEMBER_REMOVED_NOTIFICATION => "member_removed",
        _ => {
            return Err(anyhow::anyhow!(
                "invalid notification kind: {notification_kind}"
            ))
        }
    };

    let content = serde_json::json!({
        "type": event_type,
        "channel_id": channel_id_str,
        "actor": actor_hex,
    })
    .to_string();

    let event = EventBuilder::new(
        Kind::Custom(notification_kind as u16),
        content,
        [p_tag, h_tag],
    )
    .sign_with_keys(&state.relay_keypair)
    .map_err(|e| anyhow::anyhow!("failed to sign membership notification: {e}"))?;

    // Store with channel_id = None → globally scoped, reachable by global subscribers.
    let (stored, was_inserted) = state.db.insert_event(&event, None).await?;
    if !was_inserted {
        return Ok(());
    }

    // Fan-out only — skip search indexing and workflow evaluation.
    let matches = state.sub_registry.fan_out(&stored);
    if !matches.is_empty() {
        let event_json = match serde_json::to_string(&stored.event) {
            Ok(json) => json,
            Err(e) => {
                warn!("failed to serialize membership notification for fan-out: {e}");
                return Ok(());
            }
        };
        for (target_conn_id, sub_id) in &matches {
            let msg = format!(r#"["EVENT","{}",{}]"#, sub_id, event_json);
            state.conn_manager.send_to(*target_conn_id, msg);
        }
    }

    info!(
        channel = %channel_id,
        target = %target_hex,
        kind = notification_kind,
        "membership notification emitted"
    );
    Ok(())
}

/// Sign, store (replacing previous), and fan-out a single addressable discovery event.
async fn emit_addressable_discovery_event(
    state: &Arc<AppState>,
    channel_id: Uuid,
    kind: u32,
    tags: Vec<Tag>,
    relay_pubkey_hex: &str,
) -> anyhow::Result<()> {
    let event = EventBuilder::new(Kind::Custom(kind as u16), "", tags)
        .sign_with_keys(&state.relay_keypair)
        .map_err(|e| anyhow::anyhow!("failed to sign kind:{kind}: {e}"))?;

    let (stored, _) = state
        .db
        .replace_addressable_event(&event, Some(channel_id))
        .await?;
    let kind_u32 = event_kind_u32(&stored.event);
    dispatch_persistent_event(state, &stored, kind_u32, relay_pubkey_hex).await;
    Ok(())
}

/// Emit NIP-29 group discovery events (39000, 39001, 39002) signed by the relay keypair.
/// Called after group creation, metadata changes, or membership changes.
/// Events are stored channel-scoped (`channel_id = Some(...)`) so that existing
/// access control applies — private channel member lists are only visible to members.
///
/// NOTE: Channel-scoped storage means live global subscriptions (e.g. `{kinds:[39000]}`)
/// won't receive these events via fan-out. Clients discover groups via historical REQ
/// queries. Live push for open-channel discovery is a future enhancement.
pub async fn emit_group_discovery_events(
    state: &Arc<AppState>,
    channel_id: Uuid,
) -> anyhow::Result<()> {
    let channel = state.db.get_channel(channel_id).await?;
    let members = state.db.get_members(channel_id).await?;

    let relay_pubkey_hex = hex::encode(state.relay_keypair.public_key().serialize());
    let group_id = channel_id.to_string();

    // ── kind:39000 group metadata ────────────────────────────────────────────
    {
        let mut tags: Vec<Tag> = vec![Tag::parse(&["d", &group_id])?];
        tags.push(Tag::parse(&["name", &channel.name])?);
        if let Some(ref desc) = channel.description {
            if !desc.is_empty() {
                tags.push(Tag::parse(&["about", desc])?);
            }
        }
        if channel.visibility == "private" {
            tags.push(Tag::parse(&["private"])?);
        }
        // NIP-29 hidden tag: hint to clients not to show DMs in public group lists.
        // Not a security boundary — access control is handled by channel-scoped storage.
        if channel.channel_type == "dm" {
            tags.push(Tag::parse(&["hidden"])?);
        }
        // Sprout channels always require explicit membership
        tags.push(Tag::parse(&["closed"])?);
        emit_addressable_discovery_event(
            state,
            channel_id,
            KIND_NIP29_GROUP_METADATA,
            tags,
            &relay_pubkey_hex,
        )
        .await?;
    }

    // ── kind:39001 group admins ──────────────────────────────────────────────
    {
        let mut tags: Vec<Tag> = vec![Tag::parse(&["d", &group_id])?];
        for m in members
            .iter()
            .filter(|m| m.role == "owner" || m.role == "admin")
        {
            let pubkey_hex = hex::encode(&m.pubkey);
            tags.push(Tag::parse(&["p", &pubkey_hex, &m.role])?);
        }
        emit_addressable_discovery_event(
            state,
            channel_id,
            KIND_NIP29_GROUP_ADMINS,
            tags,
            &relay_pubkey_hex,
        )
        .await?;
    }

    // ── kind:39002 group members ─────────────────────────────────────────────
    {
        let mut tags: Vec<Tag> = vec![Tag::parse(&["d", &group_id])?];
        for m in &members {
            let pubkey_hex = hex::encode(&m.pubkey);
            tags.push(Tag::parse(&["p", &pubkey_hex])?);
        }
        emit_addressable_discovery_event(
            state,
            channel_id,
            KIND_NIP29_GROUP_MEMBERS,
            tags,
            &relay_pubkey_hex,
        )
        .await?;
    }

    Ok(())
}

// ── NIP-01 Kind:0 Handler ────────────────────────────────────────────────────

/// Kind:0 (NIP-01 profile metadata) side effect — sync profile fields to users table.
async fn handle_kind0_profile(event: &Event, state: &Arc<AppState>) -> anyhow::Result<()> {
    let content: serde_json::Value = serde_json::from_str(&event.content)
        .map_err(|e| anyhow::anyhow!("kind:0 content parse error: {e}"))?;

    // Kind:0 is absolute state (NIP-01 replaceable event). Fields present in the
    // event are set; fields absent are cleared. We use Some("") to clear absent
    // fields, since update_user_profile only writes Some values.
    let display_name = content
        .get("display_name")
        .or_else(|| content.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let avatar_url = content
        .get("picture")
        .or_else(|| content.get("image"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let about = content.get("about").and_then(|v| v.as_str()).unwrap_or("");

    // Validate NIP-05 handle: must be user@domain where domain matches this relay.
    // Invalid or off-domain handles are silently cleared (treated as absent) rather
    // than stored, since the event is already persisted and can't be rejected.
    let nip05_owned = content
        .get("nip05")
        .and_then(|v| v.as_str())
        .and_then(|raw| crate::api::nip05::canonicalize_nip05(raw, &state.config.relay_url).ok());
    let nip05_handle = nip05_owned.as_deref().unwrap_or("");

    let pubkey_bytes = event.pubkey.serialize().to_vec();

    state.db.ensure_user(&pubkey_bytes).await?;

    // Pass all fields as Some — empty string clears the field in the DB.
    // This ensures kind:0 is treated as absolute state, not a partial update.
    // If the NIP-05 handle collides with another user's UNIQUE constraint, retry
    // without it so display_name/about/avatar_url are still written.
    let result = state
        .db
        .update_user_profile(
            &pubkey_bytes,
            Some(display_name),
            Some(avatar_url),
            Some(about),
            Some(nip05_handle),
        )
        .await;

    if let Err(ref e) = result {
        let msg = format!("{e}");
        if msg.contains("duplicate key value") || msg.contains("23505") {
            warn!(pubkey = %nostr::util::hex::encode(&pubkey_bytes),
                "kind:0 NIP-05 handle contested, syncing profile without it");
            state
                .db
                .update_user_profile(
                    &pubkey_bytes,
                    Some(display_name),
                    Some(avatar_url),
                    Some(about),
                    None, // skip contested NIP-05
                )
                .await?;
        } else {
            result?;
        }
    }

    info!(pubkey = %nostr::util::hex::encode(&pubkey_bytes), "kind:0 profile synced to users table");
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

    if let Err(e) = emit_group_discovery_events(state, channel_id).await {
        warn!(channel = %channel_id, error = %e, "NIP-29 group discovery emission failed");
    }

    if let Err(e) = emit_membership_notification(
        state,
        channel_id,
        &target_pubkey,
        &actor_bytes,
        KIND_MEMBER_ADDED_NOTIFICATION,
    )
    .await
    {
        warn!(channel = %channel_id, error = %e, "membership notification emission failed");
    }

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

    if let Err(e) = emit_group_discovery_events(state, channel_id).await {
        warn!(channel = %channel_id, error = %e, "NIP-29 group discovery emission failed");
    }

    if let Err(e) = emit_membership_notification(
        state,
        channel_id,
        &target_pubkey,
        &actor_bytes,
        KIND_MEMBER_REMOVED_NOTIFICATION,
    )
    .await
    {
        warn!(channel = %channel_id, error = %e, "membership notification emission failed");
    }

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
                "archived" => {
                    match val {
                        "true" => {
                            state.db.archive_channel(channel_id).await?;
                            emit_system_message(
                                state,
                                channel_id,
                                serde_json::json!({
                                    "type": "channel_archived", "actor": actor_hex
                                }),
                            )
                            .await?;
                        }
                        "false" => {
                            state.db.unarchive_channel(channel_id).await?;
                            emit_system_message(
                                state,
                                channel_id,
                                serde_json::json!({
                                    "type": "channel_unarchived", "actor": actor_hex
                                }),
                            )
                            .await?;
                        }
                        _ => {} // ignore invalid values
                    }
                }
                _ => {}
            }
        }
    }

    if let Err(e) = emit_group_discovery_events(state, channel_id).await {
        warn!(channel = %channel_id, error = %e, "NIP-29 group discovery emission failed");
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
        .map_err(|_| anyhow::anyhow!("invalid visibility: {visibility_str}"))?;
    let channel_type: sprout_db::channel::ChannelType = channel_type_str
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid channel_type: {channel_type_str}"))?;

    let actor_bytes = event.pubkey.serialize().to_vec();
    let description = extract_tag_value(event, "about");

    // If the event has an h-tag UUID, ingest_event() already created the channel
    // via create_channel_with_id(). Fetch it rather than creating a duplicate.
    // If no h-tag, fall back to the original auto-UUID creation path.
    let channel = if let Some(client_uuid) = extract_h_tag_channel(event) {
        match state.db.get_channel(client_uuid).await {
            Ok(ch) => ch,
            Err(_) => {
                // Channel not found — shouldn't happen (ingest_event pre-created it),
                // but fall back to creation to stay resilient.
                state
                    .db
                    .create_channel(
                        &name,
                        channel_type,
                        visibility,
                        description.as_deref(),
                        &actor_bytes,
                    )
                    .await?
            }
        }
    } else {
        state
            .db
            .create_channel(
                &name,
                channel_type,
                visibility,
                description.as_deref(),
                &actor_bytes,
            )
            .await?
    };

    let actor_hex = nostr::util::hex::encode(&actor_bytes);
    emit_system_message(
        state,
        channel.id,
        serde_json::json!({
            "type": "channel_created", "actor": actor_hex
        }),
    )
    .await?;

    if let Err(e) = emit_group_discovery_events(state, channel.id).await {
        warn!(channel = %channel.id, error = %e, "NIP-29 group discovery emission failed");
    }

    if let Err(e) = emit_membership_notification(
        state,
        channel.id,
        &actor_bytes,
        &actor_bytes, // creator is both actor and target
        KIND_MEMBER_ADDED_NOTIFICATION,
    )
    .await
    {
        warn!(channel = %channel.id, error = %e, "membership notification emission failed");
    }

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

    // Clean up NIP-29 discovery events for the deleted group.
    if let Err(e) = state
        .db
        .soft_delete_discovery_events(channel_id, &state.relay_keypair.public_key().serialize())
        .await
    {
        warn!(channel = %channel_id, error = %e, "failed to clean up NIP-29 discovery events");
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

async fn handle_join_request(event: &Event, state: &Arc<AppState>) -> anyhow::Result<()> {
    let channel_id =
        extract_h_tag_channel(event).ok_or_else(|| anyhow::anyhow!("missing h tag"))?;
    let actor_bytes = event.pubkey.serialize().to_vec();

    // Only open channels allow self-join via kind:9021.
    let channel = state
        .db
        .get_channel(channel_id)
        .await
        .map_err(|_| anyhow::anyhow!("channel not found"))?;
    if channel.visibility != "open" {
        return Err(anyhow::anyhow!(
            "channel is private — request an invitation"
        ));
    }

    // Skip if already an active member — prevents duplicate join notifications.
    // Fail closed on DB errors rather than falling through to add_member.
    if state.db.is_member(channel_id, &actor_bytes).await? {
        info!(channel = %channel_id, "kind:9021 join — already a member, skipping");
        return Ok(());
    }

    // Add as member (idempotent — add_member handles duplicates).
    state
        .db
        .add_member(
            channel_id,
            &actor_bytes,
            sprout_db::channel::MemberRole::Member,
            None,
        )
        .await?;

    let actor_hex = nostr::util::hex::encode(&actor_bytes);
    emit_system_message(
        state,
        channel_id,
        serde_json::json!({
            "type": "member_joined",
            "actor": actor_hex,
            "target": actor_hex,
        }),
    )
    .await?;

    if let Err(e) = emit_group_discovery_events(state, channel_id).await {
        warn!(channel = %channel_id, error = %e, "NIP-29 group discovery emission failed");
    }

    if let Err(e) = emit_membership_notification(
        state,
        channel_id,
        &actor_bytes,
        &actor_bytes,
        sprout_core::kind::KIND_MEMBER_ADDED_NOTIFICATION,
    )
    .await
    {
        warn!("membership notification for join failed: {e}");
    }

    info!(channel = %channel_id, "kind:9021 join processed");
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

    if let Err(e) = emit_group_discovery_events(state, channel_id).await {
        warn!(channel = %channel_id, error = %e, "NIP-29 group discovery emission failed");
    }

    if let Err(e) = emit_membership_notification(
        state,
        channel_id,
        &actor_bytes,
        &actor_bytes, // self-leave: actor == target
        KIND_MEMBER_REMOVED_NOTIFICATION,
    )
    .await
    {
        warn!(channel = %channel_id, error = %e, "membership notification emission failed");
    }

    Ok(())
}

// handle_reaction() removed — kind:7 reaction dedup and DB writes are now
// handled inline in ingest_event() before storage (see ingest.rs step 20a).

async fn handle_standard_deletion_event(
    event: &Event,
    state: &Arc<AppState>,
) -> anyhow::Result<()> {
    let target_ids = extract_target_event_ids(event);
    if target_ids.is_empty() {
        return Err(anyhow::anyhow!("missing e tag for target event"));
    }

    for target_id in target_ids {
        let target_event = match state
            .db
            .get_event_by_id_including_deleted(&target_id)
            .await?
        {
            Some(target) => target,
            None => continue,
        };

        let meta = state.db.get_thread_metadata_by_event(&target_id).await?;
        let parent_id = meta.as_ref().and_then(|m| m.parent_event_id.clone());
        let root_id = meta.as_ref().and_then(|m| m.root_event_id.clone());

        let deleted = state
            .db
            .soft_delete_event_and_update_thread(
                &target_id,
                parent_id.as_deref(),
                root_id.as_deref(),
            )
            .await?;

        if !deleted {
            continue;
        }

        if u32::from(target_event.event.kind.as_u16()) == KIND_REACTION {
            let _ = state
                .db
                .remove_reaction_by_source_event_id(&target_id)
                .await?;
        }
    }

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
        if let Some(actor_hex) = extract_tag_value(event, "actor") {
            if let Ok(bytes) = hex::decode(actor_hex) {
                if bytes.len() == 32 {
                    return bytes;
                }
            }
        }
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

fn extract_target_event_ids(event: &Event) -> Vec<Vec<u8>> {
    event
        .tags
        .iter()
        .filter_map(|tag| {
            if tag.kind().to_string() != "e" {
                return None;
            }

            tag.content().and_then(|value| {
                if value.len() == 64 && value.chars().all(|c| c.is_ascii_hexdigit()) {
                    hex::decode(value).ok().filter(|bytes| bytes.len() == 32)
                } else {
                    None
                }
            })
        })
        .collect()
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
