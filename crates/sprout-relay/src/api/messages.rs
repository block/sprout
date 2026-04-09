//! Channel messages and thread REST API.
//!
//! Endpoints:
//!   GET  /api/channels/:channel_id/messages          — list top-level messages
//!   GET  /api/channels/:channel_id/threads/:event_id — full thread tree

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use chrono::Utc;
use nostr::util::hex as nostr_hex;
use serde::Deserialize;

use crate::state::AppState;

use super::{
    api_error, check_channel_access, check_token_channel_access, extract_auth_context,
    internal_error, not_found,
};

/// Validate imeta tags for correctness and safety.
///
/// Shared between REST (send_message) and WebSocket (handle_event) paths.
/// Returns Ok(()) if all tags are valid, or a human-readable error string.
pub fn validate_imeta_tags(tags: &[Vec<String>], media_base_url: &str) -> Result<(), String> {
    const ALLOWED_IMETA_KEYS: &[&str] = &[
        "url", "m", "x", "size", "dim", "blurhash", "alt", "thumb", "fallback", "duration",
        "bitrate", "image",
    ];
    const SINGLETON_KEYS: &[&str] = &[
        "url", "m", "x", "size", "dim", "blurhash", "thumb", "alt", "duration", "bitrate", "image",
    ];
    const ALLOWED_MIME: &[&str] = &[
        "image/jpeg",
        "image/png",
        "image/gif",
        "image/webp",
        "video/mp4",
    ];

    for tag in tags {
        if tag.first().map(|s| s.as_str()) != Some("imeta") {
            return Err("only imeta tags allowed in media_tags".into());
        }

        let mut has_url = false;
        let mut has_m = false;
        let mut has_x = false;
        let mut has_size = false;
        let mut seen_keys = std::collections::HashSet::new();
        let mut url_value = String::new();
        let mut x_value = String::new();
        let mut m_value = String::new();
        let mut thumb_value = String::new();

        for part in tag.iter().skip(1) {
            let mut parts = part.splitn(2, ' ');
            let key = parts.next().unwrap_or("");
            let value = parts.next().unwrap_or("");

            if !ALLOWED_IMETA_KEYS.contains(&key) {
                return Err(format!("disallowed imeta key: {key}"));
            }
            if SINGLETON_KEYS.contains(&key) && !seen_keys.insert(key.to_string()) {
                return Err(format!("duplicate imeta key: {key}"));
            }

            match key {
                "url" => {
                    if !is_local_media_url(value, media_base_url) {
                        return Err("imeta url must be a local /media/ path".into());
                    }
                    if value.contains(".thumb.") {
                        return Err(
                            "imeta url must not be a thumbnail path; use thumb field".into()
                        );
                    }
                    url_value = value.to_string();
                    has_url = true;
                }
                "m" => {
                    if !ALLOWED_MIME.contains(&value) {
                        return Err(
                            "imeta m must be a supported MIME type (image/jpeg, image/png, image/gif, image/webp, video/mp4)"
                                .into(),
                        );
                    }
                    m_value = value.to_string();
                    has_m = true;
                }
                "x" => {
                    if value.len() != 64
                        || !value.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
                    {
                        return Err("imeta x must be a 64-char lowercase hex SHA-256".into());
                    }
                    x_value = value.to_string();
                    has_x = true;
                }
                "size" => {
                    match value.parse::<u64>() {
                        Ok(0) | Err(_) => {
                            return Err("imeta size must be a positive integer".into())
                        }
                        Ok(_) => {}
                    }
                    has_size = true;
                }
                "thumb" => {
                    if !is_local_media_url(value, media_base_url) || !value.ends_with(".thumb.jpg")
                    {
                        return Err("imeta thumb must be a local .thumb.jpg path".into());
                    }
                    thumb_value = value.to_string();
                }
                "duration" => {
                    // NIP-71 standard field: seconds as float, strictly positive.
                    // Zero-duration videos are semantically invalid; server-side
                    // validate_video_file() also catches this via mvhd timescale.
                    if let Ok(d) = value.parse::<f64>() {
                        if d <= 0.0 || d.is_nan() || d.is_infinite() {
                            return Err("imeta duration must be a positive finite number".into());
                        }
                    } else {
                        return Err("imeta duration must be a valid float".into());
                    }
                }
                "bitrate" => {
                    // NIP-71 standard field: bits/sec as integer, positive
                    if value.parse::<u64>().map_or(true, |b| b == 0) {
                        return Err("imeta bitrate must be a positive integer".into());
                    }
                }
                "image" => {
                    // NIP-71 poster frame — must be a local media URL with an image extension.
                    // Poster frames are independent blobs, NOT thumbnails.
                    // Video URLs (e.g. .mp4) and thumbnail URLs (.thumb.jpg) are rejected.
                    const IMAGE_EXTS: &[&str] = &["jpg", "png", "gif", "webp"];
                    if !is_local_media_url(value, media_base_url) {
                        return Err("imeta image must be a local /media/ path".into());
                    }
                    if value.contains(".thumb.") {
                        return Err(
                            "imeta image must reference a standalone poster frame, not a thumbnail"
                                .into(),
                        );
                    }
                    let ext = value.rsplit('.').next().unwrap_or("");
                    if !IMAGE_EXTS.contains(&ext) {
                        return Err(
                            "imeta image must reference an image file (jpg, png, gif, webp), not video"
                                .into(),
                        );
                    }
                }
                _ => {}
            }
        }

        if !has_url || !has_m || !has_x || !has_size {
            return Err("imeta tag must include url, m, x, and size".into());
        }

        // Cross-check internal consistency: url hash must match x, url ext must match m.
        if let Some(hash_in_url) = extract_hash_from_media_url(&url_value) {
            if hash_in_url != x_value {
                return Err("imeta url hash does not match x".into());
            }
        }
        if let Some(ext_in_url) = extract_ext_from_media_url(&url_value) {
            let expected_ext = mime_to_canonical_ext(&m_value);
            if ext_in_url != expected_ext {
                return Err("imeta url extension does not match m".into());
            }
        }
        // Thumb URL hash segment must match x — thumbnails are keyed by their
        // parent blob's hash (e.g. {video_hash}.thumb.jpg), not by their own
        // content hash. This checks URL consistency, not content identity.
        if !thumb_value.is_empty() {
            if let Some(thumb_hash) = extract_hash_from_media_url(&thumb_value) {
                if thumb_hash != x_value {
                    return Err("imeta thumb hash does not match x".into());
                }
            }
        }
        // NIP-71 poster frame (`image`) is an independent blob with its own
        // content hash — it cannot match the video's `x` hash. Validated as a
        // local media URL with an image extension only (no hash cross-check).
    }
    Ok(())
}

/// Verify that every imeta tag references a blob that actually exists in storage
/// and that the claimed metadata (size, MIME) matches the sidecar.
///
/// Called after syntactic validation. Returns Ok(()) if all blobs exist and match,
/// or a human-readable error string. This prevents clients from referencing
/// nonexistent blobs or lying about size/MIME in imeta tags.
pub async fn verify_imeta_blobs(
    tags: &[Vec<String>],
    storage: &sprout_media::MediaStorage,
) -> Result<(), String> {
    for tag in tags {
        let mut x_value = String::new();
        let mut m_value = String::new();
        let mut size_value: u64 = 0;
        let mut thumb_value = String::new();
        let mut image_value = String::new();
        let mut duration_value: f64 = 0.0;

        for part in tag.iter().skip(1) {
            let mut parts = part.splitn(2, ' ');
            let key = parts.next().unwrap_or("");
            let value = parts.next().unwrap_or("");
            match key {
                "x" => x_value = value.to_string(),
                "m" => m_value = value.to_string(),
                "size" => size_value = value.parse().unwrap_or(0),
                "thumb" => thumb_value = value.to_string(),
                "image" => image_value = value.to_string(),
                "duration" => duration_value = value.parse().unwrap_or(0.0),
                _ => {}
            }
        }

        if x_value.is_empty() {
            continue; // syntactic validation already caught this
        }

        // 1. Sidecar must exist — proves the upload pipeline completed.
        let sidecar = storage
            .get_sidecar(&x_value)
            .await
            .map_err(|_| format!("imeta references nonexistent blob: {x_value}"))?;

        // 2. HEAD the actual blob object — sidecar alone is not proof of blob existence.
        let blob_key = format!("{x_value}.{}", sidecar.ext);
        let blob_exists = storage
            .head(&blob_key)
            .await
            .map_err(|e| format!("storage error checking blob {x_value}: {e}"))?;
        if !blob_exists {
            return Err(format!("imeta blob object missing in storage: {x_value}"));
        }

        // 3. Cross-check claimed metadata against sidecar.
        if !m_value.is_empty() && sidecar.mime_type != m_value {
            return Err(format!(
                "imeta m ({m_value}) does not match stored MIME ({})",
                sidecar.mime_type
            ));
        }
        if size_value > 0 && sidecar.size != size_value {
            return Err(format!(
                "imeta size ({size_value}) does not match stored size ({})",
                sidecar.size
            ));
        }
        // Duration cross-check: if sidecar has duration and client claims one,
        // they must agree within 0.1s tolerance (float rounding from mvhd).
        if let Some(stored_dur) = sidecar.duration_secs {
            if duration_value > 0.0 && (duration_value - stored_dur).abs() > 0.1 {
                return Err(format!(
                    "imeta duration ({duration_value}) does not match stored duration ({stored_dur})"
                ));
            }
        }

        // 4. If thumb is claimed, HEAD the thumbnail object too.
        if !thumb_value.is_empty() {
            let thumb_key = format!("{x_value}.thumb.jpg");
            let thumb_exists = storage
                .head(&thumb_key)
                .await
                .map_err(|e| format!("storage error checking thumbnail: {e}"))?;
            if !thumb_exists {
                return Err(format!(
                    "imeta thumb references missing thumbnail: {x_value}"
                ));
            }
        }

        // 5. If image (poster frame) is claimed, verify sidecar + blob.
        //    Poster frames are independent blobs — extract hash from the image
        //    URL itself, not from x_value. Sidecar must exist (serving is gated
        //    on it) and MIME must be an image type.
        if !image_value.is_empty() {
            if let Some(img_hash) = extract_hash_from_media_url(&image_value) {
                let img_sidecar = storage
                    .get_sidecar(img_hash)
                    .await
                    .map_err(|_| format!("imeta image references nonexistent poster: {img_hash}"))?;

                // Poster frame must be an image, not video or other type.
                const IMAGE_MIMES: &[&str] = &["image/jpeg", "image/png", "image/gif", "image/webp"];
                if !IMAGE_MIMES.contains(&img_sidecar.mime_type.as_str()) {
                    return Err(format!(
                        "imeta image poster MIME must be image type, got {}",
                        img_sidecar.mime_type
                    ));
                }

                // URL extension must match sidecar's canonical extension.
                // Mismatch means the URL would 404 on serve (GET resolves via sidecar).
                if let Some(url_ext) = extract_ext_from_media_url(&image_value) {
                    if url_ext != img_sidecar.ext {
                        return Err(format!(
                            "imeta image extension ({url_ext}) does not match stored extension ({})",
                            img_sidecar.ext
                        ));
                    }
                }

                let img_key = format!("{img_hash}.{}", img_sidecar.ext);
                let img_exists = storage
                    .head(&img_key)
                    .await
                    .map_err(|e| format!("storage error checking poster image: {e}"))?;
                if !img_exists {
                    return Err(format!(
                        "imeta image references missing poster frame: {img_hash}"
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Extract the 64-char hex hash from a `/media/{hash}.{ext}` or `/media/{hash}.thumb.jpg` URL.
fn extract_hash_from_media_url(url: &str) -> Option<&str> {
    let after = url.rsplit("/media/").next()?;
    let hash = after.split('.').next()?;
    if hash.len() == 64 && hash.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')) {
        Some(hash)
    } else {
        None
    }
}

/// Extract the primary extension from a `/media/{hash}.{ext}` URL (not thumb).
fn extract_ext_from_media_url(url: &str) -> Option<&str> {
    let after = url.rsplit("/media/").next()?;
    let segments: Vec<&str> = after.split('.').collect();
    if segments.len() == 2 {
        Some(segments[1])
    } else {
        None // bare hash or thumb — no primary ext to check
    }
}

/// Map MIME to canonical extension (must match sprout-media's mime_to_ext).
fn mime_to_canonical_ext(mime: &str) -> &str {
    match mime {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "video/mp4" => "mp4",
        _ => "bin",
    }
}

/// Validate that a URL references a valid local media blob path.
///
/// Accepts:
///   - Relative: `/media/<sha256>.<ext>` or `/media/<sha256>.thumb.jpg`
///   - Absolute: `<media_base_url>/<sha256>.<ext>` or `<media_base_url>/<sha256>.thumb.jpg`
///
///     Where sha256 is exactly 64 lowercase hex chars and ext is an allowed image extension.
///     Thumbnails are always JPEG — only `.thumb.jpg` is accepted.
///     Rejects percent-encoded traversal, query strings, fragments, and external origins.
fn is_local_media_url(url: &str, media_base_url: &str) -> bool {
    const ALLOWED_EXTS: &[&str] = &["jpg", "png", "gif", "webp", "mp4"];

    // Extract the path portion after /media/
    let path_after_media = if let Some(rest) = url.strip_prefix("/media/") {
        rest
    } else {
        let base = media_base_url.trim_end_matches('/');
        let prefix = format!("{}/", base);
        if let Some(rest) = url.strip_prefix(&prefix) {
            rest
        } else {
            return false;
        }
    };

    // Reject query strings and fragments
    if path_after_media.contains('?') || path_after_media.contains('#') {
        return false;
    }

    // Reject percent-encoding (no legitimate blob path needs it)
    if path_after_media.contains('%') {
        return false;
    }

    // Parse segments: must be {sha256}.{ext} or {sha256}.thumb.{ext}
    let segments: Vec<&str> = path_after_media.split('.').collect();
    match segments.len() {
        2 => {
            // {sha256}.{ext}
            let hash = segments[0];
            let ext = segments[1];
            hash.len() == 64
                && hash.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
                && ALLOWED_EXTS.contains(&ext)
        }
        3 => {
            // {sha256}.thumb.jpg — thumbnails are always JPEG
            let hash = segments[0];
            hash.len() == 64
                && hash.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
                && segments[1] == "thumb"
                && segments[2] == "jpg"
        }
        _ => false,
    }
}

/// Extract the effective message author from a stored event.
///
/// REST-created messages are signed by the relay keypair and attribute the real
/// sender via a `p` tag. For user-signed events (WebSocket), `event.pubkey` is
/// the author. This helper returns the correct author bytes in both cases.
fn effective_author(event: &nostr::Event, relay_pubkey: &nostr::PublicKey) -> Vec<u8> {
    if event.pubkey == *relay_pubkey {
        // Relay-signed: real author is in the first p tag.
        for tag in event.tags.iter() {
            if tag.kind().to_string() == "p" {
                if let Some(hex) = tag.content() {
                    if let Ok(bytes) = nostr_hex::decode(hex) {
                        if bytes.len() == 32 {
                            return bytes;
                        }
                    }
                }
            }
        }
    }
    // User-signed or no p tag found: pubkey is the author.
    event.pubkey.serialize().to_vec()
}

/// Resolve the effective author pubkey from stored (non-Event) data.
///
/// REST-created messages are signed by the relay keypair and carry the real
/// sender in the first `p` tag. This helper mirrors `effective_author` but
/// works with raw bytes + stored tags JSON rather than a `nostr::Event`.
fn effective_author_bytes(
    msg_pubkey: &[u8],
    tags: &serde_json::Value,
    relay_pubkey_bytes: &[u8],
) -> Vec<u8> {
    if msg_pubkey == relay_pubkey_bytes {
        // Relay-signed: real author is in the first p tag.
        if let Some(tags_arr) = tags.as_array() {
            for tag in tags_arr {
                if let Some(arr) = tag.as_array() {
                    if arr.len() >= 2 && arr[0].as_str() == Some("p") {
                        if let Some(hex) = arr[1].as_str() {
                            if let Ok(bytes) = nostr_hex::decode(hex) {
                                if bytes.len() == 32 {
                                    return bytes;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    msg_pubkey.to_vec()
}

/// Serialize a slice of reaction summaries to JSON.
fn reactions_to_json(reactions: &[sprout_db::reaction::ReactionSummary]) -> serde_json::Value {
    serde_json::json!(reactions
        .iter()
        .map(|r| serde_json::json!({
            "emoji": r.emoji,
            "count": r.count,
        }))
        .collect::<Vec<_>>())
}

// ── GET /api/channels/:channel_id/messages ────────────────────────────────────

/// Query parameters for listing top-level channel messages.
#[derive(Debug, Deserialize)]
pub struct ListMessagesParams {
    /// Maximum messages to return. Default: 50, max: 200.
    pub limit: Option<u32>,
    /// Pagination cursor — Unix timestamp (seconds). Returns messages created
    /// strictly before this time.
    pub before: Option<i64>,
    /// Pagination cursor — Unix timestamp (seconds). Returns messages created
    /// strictly after this time. Results are ordered oldest-first when `since`
    /// is provided without `before`.
    pub since: Option<i64>,
    /// Legacy parameter (thread summaries are now always included). Kept for backward compatibility.
    #[serde(default)]
    pub with_threads: bool,
    /// Comma-separated event kind numbers to filter by (e.g. "45001" or "9,45001").
    #[serde(default)]
    pub kinds: Option<String>,
}

/// List top-level messages in a channel.
///
/// Default ordering is newest-first (DESC). When `since` is provided without
/// `before`, ordering flips to oldest-first (ASC) for chronological polling.
///
/// Returns root messages and broadcast replies. Thread summaries (reply counts,
/// participant pubkeys) are always included. Thread replies themselves are excluded —
/// use `get_thread` to fetch the full reply tree for a specific message.
pub async fn list_messages(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel_id_str): Path<String>,
    Query(params): Query<ListMessagesParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::MessagesRead)
        .map_err(super::scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();

    let channel_id = uuid::Uuid::parse_str(&channel_id_str)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid channel UUID"))?;

    check_token_channel_access(&ctx, &channel_id)?;
    check_channel_access(&state, channel_id, &pubkey_bytes).await?;

    let limit = params.limit.unwrap_or(50).min(200);

    let before_cursor: Option<chrono::DateTime<Utc>> = params
        .before
        .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0));

    let since_cursor: Option<chrono::DateTime<Utc>> = params
        .since
        .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0));

    let kind_filter: Option<Vec<u32>> = params
        .kinds
        .as_deref()
        .map(|s| {
            s.split(',')
                .map(|k| k.trim().parse::<u32>())
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()
        .map_err(|_| {
            api_error(
                StatusCode::BAD_REQUEST,
                "Invalid 'kinds' parameter — expected comma-separated integers (e.g. '45001' or '9,45001')",
            )
        })?;

    let mut messages = state
        .db
        .get_channel_messages_top_level(
            channel_id,
            limit,
            before_cursor,
            since_cursor,
            kind_filter.as_deref(),
        )
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    // Always enrich with thread summaries for messages that have replies.
    // The `with_threads` param is kept for backward compatibility but summaries
    // are now included by default.
    for msg in &mut messages {
        if let Ok(summary) = state.db.get_thread_summary(&msg.event_id).await {
            msg.thread_summary = summary;
        }
    }

    // Bulk-fetch reaction counts for all messages in this page.
    let event_pairs: Vec<(&[u8], chrono::DateTime<Utc>)> = messages
        .iter()
        .map(|m| (m.event_id.as_slice(), m.created_at))
        .collect();
    let bulk_reactions = state
        .db
        .get_reactions_bulk(&event_pairs)
        .await
        .unwrap_or_default();

    // Index reactions by event_id for O(1) lookup during serialization.
    let reaction_map: std::collections::HashMap<Vec<u8>, &[sprout_db::reaction::ReactionSummary]> =
        bulk_reactions
            .iter()
            .map(|entry| (entry.event_id.clone(), entry.reactions.as_slice()))
            .collect();

    // Determine next_cursor from the oldest message in this page.
    let next_cursor = messages.last().map(|m| m.created_at.timestamp());

    // Compute relay pubkey bytes once for effective-author resolution.
    let relay_pk_bytes = state.relay_keypair.public_key().serialize().to_vec();

    let result: Vec<serde_json::Value> = messages
        .iter()
        .map(|m| {
            let author = effective_author_bytes(&m.pubkey, &m.tags, &relay_pk_bytes);
            let mut obj = serde_json::json!({
                "event_id":   nostr_hex::encode(&m.event_id),
                "pubkey":     nostr_hex::encode(&author),
                "content":    m.content,
                "kind":       m.kind,
                "created_at": m.created_at.timestamp(),
                "channel_id": m.channel_id.to_string(),
                "tags":       m.tags,
            });

            if let Some(ref ts) = m.thread_summary {
                obj["thread_summary"] = serde_json::json!({
                    "reply_count":      ts.reply_count,
                    "descendant_count": ts.descendant_count,
                    "last_reply_at":    ts.last_reply_at.map(|t| t.timestamp()),
                    "participants":     ts.participants.iter()
                        .map(nostr_hex::encode)
                        .collect::<Vec<_>>(),
                });
            }

            // Embed reaction counts if any exist for this message.
            if let Some(reactions) = reaction_map.get(&m.event_id) {
                obj["reactions"] = reactions_to_json(reactions);
            }

            obj
        })
        .collect();

    Ok(Json(serde_json::json!({
        "messages":    result,
        "next_cursor": next_cursor,
    })))
}

// ── GET /api/channels/:channel_id/threads/:event_id ──────────────────────────

/// Query parameters for fetching a thread tree.
#[derive(Debug, Deserialize)]
pub struct GetThreadParams {
    /// Maximum reply depth to include. Omit for unlimited.
    pub depth_limit: Option<u32>,
    /// Maximum replies to return. Default: 100, max: 500.
    pub limit: Option<u32>,
    /// Keyset pagination cursor — hex-encoded event_id of the last seen reply.
    pub cursor: Option<String>,
}

/// Fetch the full reply tree for a thread rooted at `event_id`.
///
/// Returns the root event details, all replies (optionally depth-limited),
/// and pagination info.
pub async fn get_thread(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((channel_id_str, event_id_hex)): Path<(String, String)>,
    Query(params): Query<GetThreadParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::MessagesRead)
        .map_err(super::scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();

    let channel_id = uuid::Uuid::parse_str(&channel_id_str)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid channel UUID"))?;

    check_token_channel_access(&ctx, &channel_id)?;
    check_channel_access(&state, channel_id, &pubkey_bytes).await?;

    let root_id_bytes = nostr_hex::decode(&event_id_hex)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid event_id hex"))?;

    // Fetch the root event.
    let root_event = state
        .db
        .get_event_by_id(&root_id_bytes)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?
        .ok_or_else(|| not_found("event not found"))?;

    // Verify the root event belongs to the requested channel.
    if let Some(root_channel) = root_event.channel_id {
        if root_channel != channel_id {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "event belongs to a different channel",
            ));
        }
    }

    // Fetch thread summary for the root.
    let summary = state
        .db
        .get_thread_summary(&root_id_bytes)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    let limit = params.limit.unwrap_or(100).min(500);

    // Decode optional cursor.
    // The cursor is a hex-encoded 8-byte big-endian i64 Unix timestamp (seconds),
    // matching the encoding produced when building next_cursor below (F8).
    let cursor_bytes: Option<Vec<u8>> = match params.cursor {
        Some(ref hex) => {
            let bytes = nostr_hex::decode(hex)
                .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid cursor hex"))?;
            if bytes.len() != 8 {
                return Err(api_error(
                    StatusCode::BAD_REQUEST,
                    "cursor must be 8 bytes (timestamp)",
                ));
            }
            Some(bytes)
        }
        None => None,
    };

    let replies = state
        .db
        .get_thread_replies(
            &root_id_bytes,
            params.depth_limit,
            limit,
            cursor_bytes.as_deref(),
        )
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    // Encode next_cursor as hex of the last reply's created_at timestamp (8-byte big-endian i64).
    // Using created_at (not event_id) because the ORDER BY is on event_created_at and binary
    // event IDs do not correlate with chronological order (F8).
    let next_cursor = replies.last().map(|r| {
        let secs: i64 = r.created_at.timestamp();
        nostr_hex::encode(secs.to_be_bytes())
    });

    let total_replies = summary.as_ref().map(|s| s.descendant_count).unwrap_or(0);

    // Serialize root event.
    let root_created_at = root_event.event.created_at.as_u64() as i64;
    let relay_pk = state.relay_keypair.public_key();
    let relay_pk_bytes = relay_pk.serialize().to_vec();
    let root_author = effective_author(&root_event.event, &relay_pk);
    let root_tags =
        serde_json::to_value(&root_event.event.tags).unwrap_or(serde_json::Value::Array(vec![]));
    let mut root_obj = serde_json::json!({
        "event_id":   root_event.event.id.to_hex(),
        "pubkey":     nostr_hex::encode(&root_author),
        "content":    root_event.event.content,
        "kind":       root_event.event.kind.as_u16(),
        "tags":       root_tags,
        "created_at": root_created_at,
        "channel_id": channel_id.to_string(),
        "thread_summary": summary.as_ref().map(|s| serde_json::json!({
            "reply_count":      s.reply_count,
            "descendant_count": s.descendant_count,
            "last_reply_at":    s.last_reply_at.map(|t| t.timestamp()),
            "participants":     s.participants.iter()
                .map(nostr_hex::encode)
                .collect::<Vec<_>>(),
        })),
    });

    // Bulk-fetch reaction counts for root + all replies.
    let root_created_at_dt =
        chrono::DateTime::from_timestamp(root_created_at, 0).unwrap_or_else(Utc::now);
    let mut thread_event_pairs: Vec<(&[u8], chrono::DateTime<Utc>)> =
        vec![(root_id_bytes.as_slice(), root_created_at_dt)];
    for r in &replies {
        thread_event_pairs.push((r.event_id.as_slice(), r.created_at));
    }
    let thread_bulk_reactions = state
        .db
        .get_reactions_bulk(&thread_event_pairs)
        .await
        .unwrap_or_default();
    let thread_reaction_map: std::collections::HashMap<
        Vec<u8>,
        &[sprout_db::reaction::ReactionSummary],
    > = thread_bulk_reactions
        .iter()
        .map(|entry| (entry.event_id.clone(), entry.reactions.as_slice()))
        .collect();

    // Attach reactions to root event.
    if let Some(reactions) = thread_reaction_map.get(&root_id_bytes) {
        root_obj["reactions"] = reactions_to_json(reactions);
    }

    // Serialize replies.
    let reply_objs: Vec<serde_json::Value> = replies
        .iter()
        .map(|r| {
            let reply_author = effective_author_bytes(&r.pubkey, &r.tags, &relay_pk_bytes);
            let mut obj = serde_json::json!({
                "event_id":        nostr_hex::encode(&r.event_id),
                "parent_event_id": r.parent_event_id.as_ref().map(nostr_hex::encode),
                "root_event_id":   r.root_event_id.as_ref().map(nostr_hex::encode),
                "channel_id":      r.channel_id.to_string(),
                "pubkey":          nostr_hex::encode(&reply_author),
                "content":         r.content,
                "kind":            r.kind,
                "depth":           r.depth,
                "created_at":      r.created_at.timestamp(),
                "broadcast":       r.broadcast,
                "tags":            r.tags,
            });

            if let Some(reactions) = thread_reaction_map.get(&r.event_id) {
                obj["reactions"] = reactions_to_json(reactions);
            }

            obj
        })
        .collect();

    Ok(Json(serde_json::json!({
        "root":          root_obj,
        "replies":       reply_objs,
        "total_replies": total_replies,
        "next_cursor":   next_cursor,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── local media URL tests ───────────────────────────────────────────────

    const HASH: &str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
    const BASE: &str = "https://relay.example.com/media";

    #[test]
    fn test_local_media_url_relative() {
        assert!(is_local_media_url(&format!("/media/{HASH}.jpg"), BASE));
        assert!(is_local_media_url(&format!("/media/{HASH}.png"), BASE));
        assert!(is_local_media_url(&format!("/media/{HASH}.gif"), BASE));
        assert!(is_local_media_url(&format!("/media/{HASH}.webp"), BASE));
    }

    #[test]
    fn test_local_media_url_absolute() {
        assert!(is_local_media_url(&format!("{BASE}/{HASH}.jpg"), BASE));
    }

    #[test]
    fn test_local_media_url_thumb_jpg_only() {
        assert!(is_local_media_url(
            &format!("/media/{HASH}.thumb.jpg"),
            BASE
        ));
        // Other thumb extensions rejected
        assert!(!is_local_media_url(
            &format!("/media/{HASH}.thumb.png"),
            BASE
        ));
        assert!(!is_local_media_url(
            &format!("/media/{HASH}.thumb.webp"),
            BASE
        ));
    }

    #[test]
    fn test_local_media_url_rejects_external() {
        assert!(!is_local_media_url(
            &format!("https://evil.com/media/{HASH}.jpg"),
            BASE
        ));
    }

    #[test]
    fn test_local_media_url_rejects_query_string() {
        assert!(!is_local_media_url(
            &format!("/media/{HASH}.jpg?foo=bar"),
            BASE
        ));
    }

    #[test]
    fn test_local_media_url_rejects_fragment() {
        assert!(!is_local_media_url(
            &format!("/media/{HASH}.jpg#frag"),
            BASE
        ));
    }

    #[test]
    fn test_local_media_url_rejects_percent_encoding() {
        assert!(!is_local_media_url(&format!("/media/{HASH}%2e.jpg"), BASE));
        assert!(!is_local_media_url("/media/%2e%2e/etc/passwd", BASE));
    }

    #[test]
    fn test_local_media_url_rejects_uppercase_hash() {
        let upper = "ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789";
        assert!(!is_local_media_url(&format!("/media/{upper}.jpg"), BASE));
    }

    #[test]
    fn test_local_media_url_rejects_short_hash() {
        assert!(!is_local_media_url("/media/abc123.jpg", BASE));
    }

    #[test]
    fn test_local_media_url_rejects_bad_ext() {
        assert!(!is_local_media_url(&format!("/media/{HASH}.svg"), BASE));
        assert!(!is_local_media_url(&format!("/media/{HASH}.exe"), BASE));
        // .jpeg is not canonical — uploads produce .jpg only
        assert!(!is_local_media_url(&format!("/media/{HASH}.jpeg"), BASE));
    }

    /// Thumb validation requires BOTH is_local_media_url AND .thumb.jpg suffix.
    /// A full-size blob URL must not be accepted as a thumbnail.
    #[test]
    fn test_thumb_must_be_thumb_jpg() {
        let thumb = format!("/media/{HASH}.thumb.jpg");
        let blob = format!("/media/{HASH}.jpg");
        assert!(is_local_media_url(&thumb, BASE) && thumb.ends_with(".thumb.jpg"));
        assert!(is_local_media_url(&blob, BASE));
        assert!(!blob.ends_with(".thumb.jpg"));
    }

    // ── imeta consistency cross-checks ──────────────────────────────────────

    #[test]
    fn test_imeta_url_hash_must_match_x() {
        let other = "b".repeat(64);
        let tag = vec![
            "imeta".into(),
            format!("url /media/{HASH}.jpg"),
            "m image/jpeg".into(),
            format!("x {other}"),
            "size 100".into(),
        ];
        let err = validate_imeta_tags(&[tag], BASE).unwrap_err();
        assert!(err.contains("url hash does not match x"), "{err}");
    }

    #[test]
    fn test_imeta_url_ext_must_match_m() {
        let tag = vec![
            "imeta".into(),
            format!("url /media/{HASH}.png"),
            "m image/jpeg".into(),
            format!("x {HASH}"),
            "size 100".into(),
        ];
        let err = validate_imeta_tags(&[tag], BASE).unwrap_err();
        assert!(err.contains("url extension does not match m"), "{err}");
    }

    #[test]
    fn test_imeta_thumb_hash_must_match_x() {
        let other = "c".repeat(64);
        let tag = vec![
            "imeta".into(),
            format!("url /media/{HASH}.jpg"),
            "m image/jpeg".into(),
            format!("x {HASH}"),
            "size 100".into(),
            format!("thumb /media/{other}.thumb.jpg"),
        ];
        let err = validate_imeta_tags(&[tag], BASE).unwrap_err();
        assert!(err.contains("thumb hash does not match x"), "{err}");
    }

    #[test]
    fn test_imeta_consistent_tags_pass() {
        let tag = vec![
            "imeta".into(),
            format!("url /media/{HASH}.jpg"),
            "m image/jpeg".into(),
            format!("x {HASH}"),
            "size 100".into(),
            format!("thumb /media/{HASH}.thumb.jpg"),
        ];
        assert!(validate_imeta_tags(&[tag], BASE).is_ok());
    }

    // ── kinds filter parsing ────────────────────────────────────────────────

    /// Helper: simulate the kinds-parsing logic from `list_messages`.
    fn parse_kinds(input: Option<&str>) -> Result<Option<Vec<u32>>, ()> {
        input
            .map(|s| {
                s.split(',')
                    .map(|k| k.trim().parse::<u32>())
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()
            .map_err(|_| ())
    }

    #[test]
    fn kinds_none_returns_none() {
        assert_eq!(parse_kinds(None), Ok(None));
    }

    #[test]
    fn kinds_single_value() {
        assert_eq!(parse_kinds(Some("45001")), Ok(Some(vec![45001])));
    }

    #[test]
    fn kinds_multiple_values() {
        assert_eq!(
            parse_kinds(Some("9,45001,45002")),
            Ok(Some(vec![9, 45001, 45002]))
        );
    }

    #[test]
    fn kinds_with_whitespace() {
        assert_eq!(
            parse_kinds(Some("45001 , 45002")),
            Ok(Some(vec![45001, 45002]))
        );
    }

    #[test]
    fn kinds_empty_string_is_error() {
        assert!(parse_kinds(Some("")).is_err());
    }

    #[test]
    fn kinds_non_numeric_is_error() {
        assert!(parse_kinds(Some("abc")).is_err());
    }

    #[test]
    fn kinds_mixed_valid_invalid_is_error() {
        assert!(parse_kinds(Some("45001,abc")).is_err());
    }

    #[test]
    fn kinds_negative_is_error() {
        assert!(parse_kinds(Some("-1")).is_err());
    }

    // ── video / NIP-71 imeta tests ──────────────────────────────────────────

    #[test]
    fn test_imeta_video_mp4_accepted() {
        let tag = vec![
            "imeta".into(),
            format!("url /media/{HASH}.mp4"),
            "m video/mp4".into(),
            format!("x {HASH}"),
            "size 5000000".into(),
        ];
        assert!(validate_imeta_tags(&[tag], BASE).is_ok());
    }

    #[test]
    fn test_imeta_duration_valid() {
        let tag = vec![
            "imeta".into(),
            format!("url /media/{HASH}.mp4"),
            "m video/mp4".into(),
            format!("x {HASH}"),
            "size 5000000".into(),
            "duration 29.5".into(),
        ];
        assert!(validate_imeta_tags(&[tag], BASE).is_ok());
    }

    #[test]
    fn test_imeta_duration_negative_rejected() {
        let tag = vec![
            "imeta".into(),
            format!("url /media/{HASH}.mp4"),
            "m video/mp4".into(),
            format!("x {HASH}"),
            "size 5000000".into(),
            "duration -5".into(),
        ];
        let err = validate_imeta_tags(&[tag], BASE).unwrap_err();
        assert!(err.contains("positive finite number"), "{err}");
    }

    #[test]
    fn test_imeta_duration_zero_rejected() {
        let tag = vec![
            "imeta".into(),
            format!("url /media/{HASH}.mp4"),
            "m video/mp4".into(),
            format!("x {HASH}"),
            "size 5000000".into(),
            "duration 0".into(),
        ];
        let err = validate_imeta_tags(&[tag], BASE).unwrap_err();
        assert!(err.contains("positive finite number"), "{err}");
    }

    #[test]
    fn test_imeta_duration_non_float_rejected() {
        let tag = vec![
            "imeta".into(),
            format!("url /media/{HASH}.mp4"),
            "m video/mp4".into(),
            format!("x {HASH}"),
            "size 5000000".into(),
            "duration abc".into(),
        ];
        let err = validate_imeta_tags(&[tag], BASE).unwrap_err();
        assert!(err.contains("valid float"), "{err}");
    }

    #[test]
    fn test_imeta_bitrate_valid() {
        let tag = vec![
            "imeta".into(),
            format!("url /media/{HASH}.mp4"),
            "m video/mp4".into(),
            format!("x {HASH}"),
            "size 5000000".into(),
            "bitrate 1500000".into(),
        ];
        assert!(validate_imeta_tags(&[tag], BASE).is_ok());
    }

    #[test]
    fn test_imeta_bitrate_zero_rejected() {
        let tag = vec![
            "imeta".into(),
            format!("url /media/{HASH}.mp4"),
            "m video/mp4".into(),
            format!("x {HASH}"),
            "size 5000000".into(),
            "bitrate 0".into(),
        ];
        let err = validate_imeta_tags(&[tag], BASE).unwrap_err();
        assert!(err.contains("positive integer"), "{err}");
    }

    #[test]
    fn test_imeta_image_poster_frame_accepted() {
        // Poster frame is an independent blob with its own hash — different from
        // the video's x hash. This must be accepted (no hash cross-check).
        let poster_hash = "b".repeat(64);
        let tag = vec![
            "imeta".into(),
            format!("url /media/{HASH}.mp4"),
            "m video/mp4".into(),
            format!("x {HASH}"),
            "size 5000000".into(),
            format!("image /media/{poster_hash}.jpg"),
        ];
        assert!(validate_imeta_tags(&[tag], BASE).is_ok());
    }

    #[test]
    fn test_imeta_image_video_url_rejected() {
        // NIP-71 image field is a still poster frame — .mp4 must be rejected
        let tag = vec![
            "imeta".into(),
            format!("url /media/{HASH}.mp4"),
            "m video/mp4".into(),
            format!("x {HASH}"),
            "size 5000000".into(),
            format!("image /media/{HASH}.mp4"),
        ];
        let err = validate_imeta_tags(&[tag], BASE).unwrap_err();
        assert!(
            err.contains("image file") && err.contains("not video"),
            "{err}"
        );
    }

    #[test]
    fn test_imeta_video_quicktime_rejected() {
        let tag = vec![
            "imeta".into(),
            format!("url /media/{HASH}.mp4"),
            "m video/quicktime".into(),
            format!("x {HASH}"),
            "size 5000000".into(),
        ];
        let err = validate_imeta_tags(&[tag], BASE).unwrap_err();
        assert!(err.contains("supported MIME type"), "{err}");
    }

    #[test]
    fn test_local_media_url_mp4_accepted() {
        assert!(is_local_media_url(&format!("/media/{HASH}.mp4"), BASE));
    }
}
