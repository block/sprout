//! Relay HTTP helpers for huddle operations.
//!
//! Thin wrappers around the relay REST API for LiveKit token requests,
//! channel membership queries, and human participant counting.

use reqwest::Method;
use serde::Deserialize;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::relay::{api_path, build_authed_request, send_json_request};

use super::state::LiveKitTokenResponse;

/// Maximum number of agents that can be invited to a single huddle.
pub(crate) const MAX_HUDDLE_AGENTS: usize = 20;

/// Validate that a string looks like a Nostr pubkey hex (64 hex chars).
pub(crate) fn validate_pubkey_hex(pubkey: &str) -> Result<(), String> {
    if pubkey.len() != 64 || !pubkey.chars().all(|c| c.is_ascii_hexdigit()) {
        let preview: String = pubkey.chars().take(16).collect();
        return Err(format!("invalid pubkey hex: {preview}"));
    }
    Ok(())
}

pub(crate) fn parse_channel_uuid(channel_id: &str) -> Result<Uuid, String> {
    Uuid::parse_str(channel_id).map_err(|_| format!("invalid channel UUID: {channel_id}"))
}

/// Fetch a LiveKit token from the relay for the given channel.
///
/// When `parent_channel_id` is `Some`, appends `?parent_channel_id={id}` to
/// the URL so the relay can auto-add the caller as a member of the ephemeral
/// channel (used by joiners — creators are already owners and pass `None`).
pub(crate) async fn fetch_livekit_token(
    channel_id: &str,
    parent_channel_id: Option<&str>,
    state: &AppState,
) -> Result<LiveKitTokenResponse, String> {
    let base = api_path(&["huddles", channel_id, "token"]);
    let path = match parent_channel_id {
        Some(pid) => format!("{base}?parent_channel_id={pid}"),
        None => base,
    };
    let request = build_authed_request(&state.http_client, Method::POST, &path, state)?;
    send_json_request(request).await
}

/// Fetch channel members with their roles from the relay.
/// Returns (pubkey, role) tuples — the authoritative source for both
/// `fetch_channel_members` (filtered by role) and `count_human_members`.
pub(crate) async fn fetch_channel_members_with_roles(
    channel_id: &str,
    state: &AppState,
) -> Result<Vec<(String, Option<String>)>, String> {
    #[derive(Deserialize)]
    struct Member {
        pubkey: String,
        role: Option<String>,
    }
    #[derive(Deserialize)]
    struct MembersResponse {
        members: Vec<Member>,
    }

    let path = api_path(&["channels", channel_id, "members"]);
    let request = build_authed_request(&state.http_client, Method::GET, &path, state)?;
    let resp: MembersResponse = send_json_request(request).await.map_err(|e| {
        eprintln!("sprout-desktop: fetch channel members failed: {e}");
        e
    })?;

    Ok(resp
        .members
        .into_iter()
        .map(|m| (m.pubkey, m.role))
        .collect())
}

/// Fetch channel members from the relay. If `role_filter` is Some, only return
/// members with that role (e.g., "bot" for agents). Returns all members if None.
pub(crate) async fn fetch_channel_members(
    channel_id: &str,
    role_filter: Option<&str>,
    state: &AppState,
) -> Result<Vec<String>, String> {
    let all = fetch_channel_members_with_roles(channel_id, state).await?;
    Ok(all
        .into_iter()
        .filter(|(_, role)| role_filter.map_or(true, |r| role.as_deref() == Some(r)))
        .map(|(pubkey, _)| pubkey)
        .collect())
}

/// Count human (non-bot) members remaining in a channel.
/// Built on `fetch_channel_members_with_roles` — fetches all members then counts non-bots.
pub(crate) async fn count_human_members(
    channel_id: &str,
    state: &AppState,
) -> Result<usize, String> {
    let all = fetch_channel_members_with_roles(channel_id, state).await?;
    Ok(all
        .iter()
        .filter(|(_, role)| role.as_deref() != Some("bot"))
        .count())
}
