use nostr::EventId;
use sprout_sdk::{DiffMeta, ThreadRef, VoteDirection};
use uuid::Uuid;

use crate::client::SproutClient;
use crate::error::CliError;
use crate::validate::{
    extract_at_names, infer_language, merge_mentions, normalize_mention_pubkeys, percent_encode,
    read_or_stdin, truncate_diff, validate_content_size, validate_hex64, validate_uuid,
    MAX_DIFF_BYTES,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Require keys on the client — fail fast with a clear error if absent.
macro_rules! require_keys {
    ($client:expr) => {
        $client.keys().ok_or_else(|| {
            CliError::Key(
                "private key required for write operations (set SPROUT_PRIVATE_KEY)".into(),
            )
        })?
    };
}

/// Parse a 64-char hex string into a nostr::EventId.
fn parse_event_id(hex: &str) -> Result<EventId, CliError> {
    EventId::parse(hex).map_err(|e| CliError::Usage(format!("invalid event ID: {e}")))
}

/// Parse a UUID string into uuid::Uuid.
fn parse_uuid(s: &str) -> Result<Uuid, CliError> {
    Uuid::parse_str(s).map_err(|e| CliError::Usage(format!("invalid channel UUID: {e}")))
}

/// Resolve the channel UUID for an event by fetching GET /api/events/{id}.
/// Extracts the `h` tag value from the returned tags array.
async fn resolve_channel_id(client: &SproutClient, event_id: &str) -> Result<Uuid, CliError> {
    let raw = client.get_raw(&format!("/api/events/{}", event_id)).await?;
    let v: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| CliError::Other(format!("failed to parse event response: {e}")))?;
    let tags = v
        .get("tags")
        .and_then(|t| t.as_array())
        .ok_or_else(|| CliError::Other("event response missing 'tags' field".into()))?;
    for tag in tags {
        if let Some(arr) = tag.as_array() {
            if arr.first().and_then(|v| v.as_str()) == Some("h") {
                if let Some(uuid_str) = arr.get(1).and_then(|v| v.as_str()) {
                    return Uuid::parse_str(uuid_str).map_err(|_| {
                        CliError::Other(format!("event h-tag is not a valid UUID: {uuid_str}"))
                    });
                }
            }
        }
    }
    Err(CliError::Other(format!(
        "event {event_id} has no h-tag — cannot determine channel"
    )))
}

/// Resolve @names in content against channel members.
/// Returns matching pubkeys. On any error, returns empty vec — never blocks a send.
async fn resolve_content_mentions(
    client: &SproutClient,
    channel_id: &str,
    content: &str,
) -> Vec<String> {
    let names = extract_at_names(content);
    if names.is_empty() {
        return vec![];
    }
    let body = client
        .get_raw(&format!("/api/channels/{channel_id}/members"))
        .await
        .unwrap_or_default();
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
    let Some(members) = parsed["members"].as_array() else {
        return vec![];
    };
    let mut pubkeys = Vec::new();
    for m in members {
        let Some(dn) = m["display_name"].as_str() else {
            continue;
        };
        if names.iter().any(|n| n.eq_ignore_ascii_case(dn)) {
            if let Some(pk) = m["pubkey"].as_str() {
                pubkeys.push(pk.to_ascii_lowercase());
            }
        }
    }
    pubkeys
}

// ---------------------------------------------------------------------------
// Read commands (unchanged)
// ---------------------------------------------------------------------------

pub async fn cmd_get_messages(
    client: &SproutClient,
    channel_id: &str,
    limit: Option<u32>,
    before: Option<i64>,
    since: Option<i64>,
    kinds: Option<&str>,
) -> Result<(), CliError> {
    validate_uuid(channel_id)?;
    let limit = limit.unwrap_or(50).min(200);
    let mut path = format!("/api/channels/{}/messages?limit={}", channel_id, limit);
    if let Some(b) = before {
        path.push_str(&format!("&before={b}"));
    }
    if let Some(s) = since {
        path.push_str(&format!("&since={s}"));
    }
    if let Some(k) = kinds {
        path.push_str(&format!("&kinds={}", percent_encode(k)));
    }
    client.run_get(&path).await
}

pub async fn cmd_get_thread(
    client: &SproutClient,
    channel_id: &str,
    event_id: &str,
    depth_limit: Option<u32>,
    limit: Option<u32>,
    cursor: Option<&str>,
) -> Result<(), CliError> {
    validate_uuid(channel_id)?;
    validate_hex64(event_id)?;
    let limit = limit.unwrap_or(100).min(500);
    let mut path = format!(
        "/api/channels/{}/threads/{}?limit={}",
        channel_id, event_id, limit
    );
    if let Some(d) = depth_limit {
        path.push_str(&format!("&depth_limit={d}"));
    }
    if let Some(c) = cursor {
        path.push_str(&format!("&cursor={}", percent_encode(c)));
    }
    client.run_get(&path).await
}

pub async fn cmd_search(
    client: &SproutClient,
    query: &str,
    limit: Option<u32>,
) -> Result<(), CliError> {
    let limit = limit.unwrap_or(20).min(100);
    let path = format!("/api/search?q={}&limit={}", percent_encode(query), limit);
    client.run_get(&path).await
}

// ---------------------------------------------------------------------------
// Write commands — signed events
// ---------------------------------------------------------------------------

pub async fn cmd_send_message(
    client: &SproutClient,
    channel_id: &str,
    content: &str,
    _kind: Option<u16>,
    reply_to: Option<&str>,
    broadcast: bool,
    mentions: &[String],
) -> Result<(), CliError> {
    validate_uuid(channel_id)?;
    validate_content_size(content)?;
    if let Some(r) = reply_to {
        validate_hex64(r)?;
    }
    for m in mentions {
        validate_hex64(m)?;
    }

    let keys = require_keys!(client);
    let channel_uuid = parse_uuid(channel_id)?;

    // Build thread ref if replying
    let thread_ref = if let Some(r) = reply_to {
        let eid = parse_event_id(r)?;
        Some(ThreadRef {
            root_event_id: eid,
            parent_event_id: eid,
        })
    } else {
        None
    };

    // Normalize explicit mentions, then merge auto-resolved up to SDK cap of 50.
    let mut merged: Vec<String> = normalize_mention_pubkeys(mentions, "");
    let auto_resolved = resolve_content_mentions(client, channel_id, content).await;
    merge_mentions(&mut merged, &auto_resolved, 50);
    let mention_refs: Vec<&str> = merged.iter().map(|s| s.as_str()).collect();

    let builder = sprout_sdk::build_message(
        channel_uuid,
        content,
        thread_ref.as_ref(),
        &mention_refs,
        broadcast,
        &[],
    )
    .map_err(|e| CliError::Other(format!("build_message failed: {e}")))?;

    let event = builder
        .sign_with_keys(keys)
        .map_err(|e| CliError::Other(format!("signing failed: {e}")))?;

    let resp = client.submit_event(event).await?;
    println!("{resp}");
    Ok(())
}

pub struct SendDiffParams {
    pub channel_id: String,
    pub diff: String,
    pub repo_url: String,
    pub commit_sha: String,
    pub file_path: Option<String>,
    pub parent_commit_sha: Option<String>,
    pub source_branch: Option<String>,
    pub target_branch: Option<String>,
    pub pr_number: Option<u32>,
    pub language: Option<String>,
    pub description: Option<String>,
    pub reply_to: Option<String>,
}

pub async fn cmd_send_diff_message(
    client: &SproutClient,
    p: SendDiffParams,
) -> Result<(), CliError> {
    validate_uuid(&p.channel_id)?;
    if let Some(r) = &p.reply_to {
        validate_hex64(r)?;
    }

    // Branch pairing: both or neither
    match (&p.source_branch, &p.target_branch) {
        (Some(_), None) | (None, Some(_)) => {
            return Err(CliError::Usage(
                "--source-branch and --target-branch must both be provided or both omitted".into(),
            ));
        }
        _ => {}
    }

    let keys = require_keys!(client);
    let channel_uuid = parse_uuid(&p.channel_id)?;

    // Read diff from stdin if "--diff -"
    let diff_content = read_or_stdin(&p.diff)?;

    // Truncate at 60 KiB hunk boundary
    let (diff, truncated) = truncate_diff(&diff_content, MAX_DIFF_BYTES);

    // Language inference: explicit flag wins, then infer from file path
    let language = p
        .language
        .clone()
        .or_else(|| p.file_path.as_deref().and_then(infer_language));

    // NIP-31 alt tag
    let alt = match (&p.file_path, &p.description) {
        (Some(fp), Some(desc)) => format!("Diff: {} — {}", fp, desc),
        (Some(fp), None) => format!("Diff: {}", fp),
        _ => "Diff".to_string(),
    };

    let thread_ref = if let Some(r) = &p.reply_to {
        let eid = parse_event_id(r)?;
        Some(ThreadRef {
            root_event_id: eid,
            parent_event_id: eid,
        })
    } else {
        None
    };

    let branch = match (&p.source_branch, &p.target_branch) {
        (Some(src), Some(tgt)) => Some((src.clone(), tgt.clone())),
        _ => None,
    };

    let diff_meta = DiffMeta {
        repo_url: p.repo_url.clone(),
        commit_sha: p.commit_sha.clone(),
        file_path: p.file_path.clone(),
        parent_commit: p.parent_commit_sha.clone(),
        branch,
        pr_number: p.pr_number,
        language,
        description: p.description.clone(),
        truncated,
        alt_text: Some(alt),
    };

    let builder =
        sprout_sdk::build_diff_message(channel_uuid, &diff, &diff_meta, thread_ref.as_ref())
            .map_err(|e| CliError::Other(format!("build_diff_message failed: {e}")))?;

    let event = builder
        .sign_with_keys(keys)
        .map_err(|e| CliError::Other(format!("signing failed: {e}")))?;

    let resp = client.submit_event(event).await?;
    println!("{resp}");
    Ok(())
}

pub async fn cmd_delete_message(client: &SproutClient, event_id: &str) -> Result<(), CliError> {
    validate_hex64(event_id)?;
    let keys = require_keys!(client);

    // Resolve channel_id from the event's h-tag (needed by build_delete_message)
    let channel_uuid = resolve_channel_id(client, event_id).await?;
    let target_eid = parse_event_id(event_id)?;

    let builder = sprout_sdk::build_delete_message(channel_uuid, target_eid)
        .map_err(|e| CliError::Other(format!("build_delete_message failed: {e}")))?;

    let event = builder
        .sign_with_keys(keys)
        .map_err(|e| CliError::Other(format!("signing failed: {e}")))?;

    let resp = client.submit_event(event).await?;
    println!("{resp}");
    Ok(())
}

/// Edit a message you previously sent.
pub async fn cmd_edit_message(
    client: &SproutClient,
    event_id: &str,
    content: &str,
) -> Result<(), CliError> {
    validate_hex64(event_id)?;
    validate_content_size(content)?;
    let keys = require_keys!(client);

    // Resolve channel_id from the event's h-tag (needed by build_edit)
    let channel_uuid = resolve_channel_id(client, event_id).await?;
    let target_eid = parse_event_id(event_id)?;

    let builder = sprout_sdk::build_edit(channel_uuid, target_eid, content)
        .map_err(|e| CliError::Other(format!("build_edit failed: {e}")))?;

    let event = builder
        .sign_with_keys(keys)
        .map_err(|e| CliError::Other(format!("signing failed: {e}")))?;

    let resp = client.submit_event(event).await?;
    println!("{resp}");
    Ok(())
}

/// Vote on a forum post or comment.
pub async fn cmd_vote_on_post(
    client: &SproutClient,
    event_id: &str,
    direction: &str,
) -> Result<(), CliError> {
    validate_hex64(event_id)?;
    let vote_dir = match direction {
        "up" => VoteDirection::Up,
        "down" => VoteDirection::Down,
        _ => {
            return Err(CliError::Usage(format!(
                "--direction must be 'up' or 'down' (got: {direction})"
            )))
        }
    };

    let keys = require_keys!(client);

    // Resolve channel_id from the event's h-tag (needed by build_vote)
    let channel_uuid = resolve_channel_id(client, event_id).await?;
    let target_eid = parse_event_id(event_id)?;

    let builder = sprout_sdk::build_vote(channel_uuid, target_eid, vote_dir)
        .map_err(|e| CliError::Other(format!("build_vote failed: {e}")))?;

    let event = builder
        .sign_with_keys(keys)
        .map_err(|e| CliError::Other(format!("signing failed: {e}")))?;

    let resp = client.submit_event(event).await?;
    println!("{resp}");
    Ok(())
}
