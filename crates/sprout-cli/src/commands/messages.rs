use crate::client::SproutClient;
use crate::error::CliError;
use crate::validate::{
    infer_language, normalize_mention_pubkeys, percent_encode, read_or_stdin, truncate_diff,
    validate_content_size, validate_hex64, validate_uuid, MAX_DIFF_BYTES,
};

pub async fn cmd_get_messages(
    client: &SproutClient,
    channel_id: &str,
    limit: Option<u32>,
    before: Option<i64>,
    kinds: Option<&str>,
) -> Result<(), CliError> {
    validate_uuid(channel_id)?;
    let limit = limit.unwrap_or(50).min(200);
    let mut path = format!("/api/channels/{}/messages?limit={}", channel_id, limit);
    if let Some(b) = before {
        path.push_str(&format!("&before={b}"));
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

pub async fn cmd_send_message(
    client: &SproutClient,
    channel_id: &str,
    content: &str,
    kind: Option<u16>,
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

    // Normalize mentions: lowercase, deduplicate, remove sender (unknown in v1)
    let normalized_mentions = normalize_mention_pubkeys(mentions, "");

    let mut body = serde_json::json!({
        "content": content,
        "broadcast_to_channel": broadcast,
    });
    // kind: u16 flag → u32 in JSON body (all valid Nostr kinds fit in u16)
    if let Some(k) = kind {
        body["kind"] = (k as u32).into();
    }
    if let Some(r) = reply_to {
        body["parent_event_id"] = r.into();
    }
    if !normalized_mentions.is_empty() {
        body["mention_pubkeys"] = normalized_mentions.into();
    }

    client
        .run_post(&format!("/api/channels/{}/messages", channel_id), &body)
        .await
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

    let mut body = serde_json::json!({
        "content": diff,
        "kind": 40008,
        "broadcast_to_channel": false,
        "diff_repo_url": p.repo_url,
        "diff_commit_sha": p.commit_sha,
        "diff_alt": alt,
    });
    if truncated {
        body["diff_truncated"] = true.into();
    }
    if let Some(fp) = &p.file_path {
        body["diff_file_path"] = fp.clone().into();
    }
    if let Some(pc) = &p.parent_commit_sha {
        body["diff_parent_commit_sha"] = pc.clone().into();
    }
    if let Some(sb) = &p.source_branch {
        body["diff_source_branch"] = sb.clone().into();
    }
    if let Some(tb) = &p.target_branch {
        body["diff_target_branch"] = tb.clone().into();
    }
    if let Some(pr) = p.pr_number {
        body["diff_pr_number"] = pr.into();
    }
    if let Some(lg) = language {
        body["diff_language"] = lg.into();
    }
    if let Some(ds) = &p.description {
        body["diff_description"] = ds.clone().into();
    }
    if let Some(re) = &p.reply_to {
        body["parent_event_id"] = re.clone().into();
    }

    client
        .run_post(&format!("/api/channels/{}/messages", p.channel_id), &body)
        .await
}

pub async fn cmd_delete_message(client: &SproutClient, event_id: &str) -> Result<(), CliError> {
    validate_hex64(event_id)?;
    client
        .run_delete(&format!("/api/messages/{}", event_id))
        .await
}

/// Edit a message you previously sent.
pub async fn cmd_edit_message(
    client: &SproutClient,
    event_id: &str,
    content: &str,
) -> Result<(), CliError> {
    validate_hex64(event_id)?;
    validate_content_size(content)?;
    client
        .run_put(
            &format!("/api/messages/{}", event_id),
            &serde_json::json!({ "content": content }),
        )
        .await
}

/// Vote on a forum post or comment.
pub async fn cmd_vote_on_post(
    client: &SproutClient,
    event_id: &str,
    direction: &str,
) -> Result<(), CliError> {
    validate_hex64(event_id)?;
    match direction {
        "up" | "down" => {}
        _ => {
            return Err(CliError::Usage(format!(
                "--direction must be 'up' or 'down' (got: {direction})"
            )))
        }
    }
    client
        .run_post(
            &format!("/api/messages/{}/votes", event_id),
            &serde_json::json!({ "direction": direction }),
        )
        .await
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
