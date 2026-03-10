//! Thread metadata persistence.
//!
//! Tracks parent/root relationships, depth, and reply counts for infinitely
//! nested threads. The `thread_metadata` table is populated when events are
//! ingested and updated as replies arrive or are deleted.

use chrono::{DateTime, Utc};
use sqlx::{MySqlPool, Row};
use uuid::Uuid;

use crate::error::Result;
use crate::event::uuid_from_bytes;

// ── Structs ───────────────────────────────────────────────────────────────────

/// A single reply within a thread, joined with event content.
#[derive(Debug, Clone)]
pub struct ThreadReply {
    /// The Nostr event ID of this reply.
    pub event_id: Vec<u8>,
    /// The event ID of the direct parent (one level up), if any.
    pub parent_event_id: Option<Vec<u8>>,
    /// The event ID of the thread root (top-level message), if any.
    pub root_event_id: Option<Vec<u8>>,
    /// The channel this reply belongs to.
    pub channel_id: Uuid,
    /// Compressed public key of the reply author.
    pub pubkey: Vec<u8>,
    /// Text content of the reply.
    pub content: String,
    /// Nostr event kind number.
    pub kind: i32,
    /// Nesting depth within the thread (root = 0, direct reply = 1, etc.).
    pub depth: i32,
    /// When the reply was created.
    pub created_at: DateTime<Utc>,
    /// Whether this reply is also broadcast to the channel timeline.
    pub broadcast: bool,
}

/// Aggregated thread statistics for a root message.
#[derive(Debug, Clone)]
pub struct ThreadSummary {
    /// Number of direct replies to the root message.
    pub reply_count: i32,
    /// Total number of replies at all nesting levels.
    pub descendant_count: i32,
    /// Timestamp of the most recent reply in the thread.
    pub last_reply_at: Option<DateTime<Utc>>,
    /// Compressed public keys of all participants who have replied.
    pub participants: Vec<Vec<u8>>,
}

/// A top-level channel message with optional thread summary.
#[derive(Debug, Clone)]
pub struct TopLevelMessage {
    /// The Nostr event ID of this message.
    pub event_id: Vec<u8>,
    /// Compressed public key of the message author.
    pub pubkey: Vec<u8>,
    /// Text content of the message.
    pub content: String,
    /// Nostr event kind number.
    pub kind: i32,
    /// When the message was created.
    pub created_at: DateTime<Utc>,
    /// The channel this message belongs to.
    pub channel_id: Uuid,
    /// Thread statistics for this message, if it has replies.
    pub thread_summary: Option<ThreadSummary>,
}

/// Raw thread_metadata row — used when processing deletes or computing ancestry.
#[derive(Debug, Clone)]
pub struct ThreadMetadataRecord {
    /// The Nostr event ID this metadata row tracks.
    pub event_id: Vec<u8>,
    /// Partition key timestamp for the event.
    pub event_created_at: DateTime<Utc>,
    /// The channel this event belongs to.
    pub channel_id: Uuid,
    /// Event ID of the direct parent, if this is a reply.
    pub parent_event_id: Option<Vec<u8>>,
    /// Event ID of the thread root, if this is a nested reply.
    pub root_event_id: Option<Vec<u8>>,
    /// Nesting depth (root = 0).
    pub depth: i32,
    /// Number of direct replies to this event.
    pub reply_count: i32,
    /// Total number of descendants at all nesting levels.
    pub descendant_count: i32,
    /// Whether this event is broadcast to the channel timeline.
    pub broadcast: bool,
}

// ── Write operations ──────────────────────────────────────────────────────────

/// Insert a row into `thread_metadata`.
///
/// If `parent_event_id` is `Some`, also increments the parent's reply count
/// and the root's descendant count (always, including when root == parent).
///
/// The INSERT and all counter UPDATEs are wrapped in a single transaction so a
/// crash between them cannot leave reply_count / descendant_count inconsistent
/// with the actual number of reply rows (F9).
#[allow(clippy::too_many_arguments)]
pub async fn insert_thread_metadata(
    pool: &MySqlPool,
    event_id: &[u8],
    event_created_at: DateTime<Utc>,
    channel_id: Uuid,
    parent_event_id: Option<&[u8]>,
    parent_event_created_at: Option<DateTime<Utc>>,
    root_event_id: Option<&[u8]>,
    root_event_created_at: Option<DateTime<Utc>>,
    depth: i32,
    broadcast: bool,
) -> Result<()> {
    let channel_id_bytes = channel_id.as_bytes().as_slice().to_vec();
    let broadcast_val: i8 = if broadcast { 1 } else { 0 };

    let mut tx = pool.begin().await?;

    let result = sqlx::query(
        r#"
        INSERT IGNORE INTO thread_metadata
            (event_created_at, event_id, channel_id,
             parent_event_id, parent_event_created_at,
             root_event_id, root_event_created_at,
             depth, broadcast)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(event_created_at)
    .bind(event_id)
    .bind(channel_id_bytes.as_slice())
    .bind(parent_event_id)
    .bind(parent_event_created_at)
    .bind(root_event_id)
    .bind(root_event_created_at)
    .bind(depth)
    .bind(broadcast_val)
    .execute(&mut *tx)
    .await?;

    // Only bump reply counts if the row was actually inserted (not a duplicate).
    // INSERT IGNORE on a duplicate key returns rows_affected = 0.
    if result.rows_affected() > 0 {
        if let Some(pid) = parent_event_id {
            // Increment parent's direct reply count and last_reply_at.
            sqlx::query(
                r#"
                UPDATE thread_metadata
                SET reply_count   = reply_count + 1,
                    last_reply_at = NOW(6)
                WHERE event_id = ?
                "#,
            )
            .bind(pid)
            .execute(&mut *tx)
            .await?;

            // Increment root's total descendant count.
            if let Some(root_id) = root_event_id {
                sqlx::query(
                    r#"
                    UPDATE thread_metadata
                    SET descendant_count = descendant_count + 1
                    WHERE event_id = ?
                    "#,
                )
                .bind(root_id)
                .execute(&mut *tx)
                .await?;
            }
        }
    }

    tx.commit().await?;

    Ok(())
}

/// Increment `reply_count` (and `last_reply_at`) on the parent event.
/// If `root_event_id` is provided, also increments `descendant_count` on the
/// root — even when root == parent (direct reply to root). This is correct
/// because `reply_count` tracks direct children only, while `descendant_count`
/// tracks ALL descendants at every nesting level.
pub async fn increment_reply_count(
    pool: &MySqlPool,
    parent_event_id: &[u8],
    root_event_id: Option<&[u8]>,
) -> Result<()> {
    // Always bump the parent's direct reply count and last-reply timestamp.
    sqlx::query(
        r#"
        UPDATE thread_metadata
        SET reply_count  = reply_count + 1,
            last_reply_at = NOW(6)
        WHERE event_id = ?
        "#,
    )
    .bind(parent_event_id)
    .execute(pool)
    .await?;

    // Always bump root's descendant_count, regardless of whether root == parent.
    // - Direct reply (root == parent): root row gets reply_count+1 AND descendant_count+1.
    // - Nested reply (root != parent): parent gets reply_count+1, root gets descendant_count+1.
    if let Some(root_id) = root_event_id {
        sqlx::query(
            r#"
            UPDATE thread_metadata
            SET descendant_count = descendant_count + 1
            WHERE event_id = ?
            "#,
        )
        .bind(root_id)
        .execute(pool)
        .await?;
    }

    Ok(())
}

/// Decrement `reply_count` on the parent event (floor at 0).
/// If `root_event_id` is provided, also decrements `descendant_count` on the
/// root — even when root == parent. Mirrors the increment logic exactly.
pub async fn decrement_reply_count(
    pool: &MySqlPool,
    parent_event_id: &[u8],
    root_event_id: Option<&[u8]>,
) -> Result<()> {
    // Always decrement the parent's direct reply count (floor at 0).
    sqlx::query(
        r#"
        UPDATE thread_metadata
        SET reply_count = GREATEST(reply_count - 1, 0)
        WHERE event_id = ?
        "#,
    )
    .bind(parent_event_id)
    .execute(pool)
    .await?;

    // Always decrement root's descendant_count, regardless of whether root == parent.
    if let Some(root_id) = root_event_id {
        sqlx::query(
            r#"
            UPDATE thread_metadata
            SET descendant_count = GREATEST(descendant_count - 1, 0)
            WHERE event_id = ?
            "#,
        )
        .bind(root_id)
        .execute(pool)
        .await?;
    }

    Ok(())
}

// ── Read operations ───────────────────────────────────────────────────────────

/// Fetch all replies under a root event, ordered chronologically.
///
/// - `depth_limit` — if `Some(n)`, only returns replies at depth ≤ n.
/// - `cursor` — if `Some(ts_bytes)`, returns replies with `event_created_at`
///   strictly after the timestamp encoded in `ts_bytes`. The bytes must be an
///   8-byte big-endian i64 Unix timestamp in seconds. The caller (REST handler)
///   encodes the last reply's `created_at` as the next-page cursor.
///   Binary event IDs do NOT correlate with chronological order, so the old
///   `event_id > cursor` condition produced non-deterministic pagination (F8).
/// - `limit` — maximum rows returned (caller should cap this).
pub async fn get_thread_replies(
    pool: &MySqlPool,
    root_event_id: &[u8],
    depth_limit: Option<u32>,
    limit: u32,
    cursor: Option<&[u8]>,
) -> Result<Vec<ThreadReply>> {
    // Decode cursor bytes → DateTime<Utc> for the keyset condition.
    // Bytes are an 8-byte big-endian i64 Unix timestamp (seconds).
    let cursor_ts: Option<DateTime<Utc>> = match cursor {
        Some(bytes) if bytes.len() == 8 => {
            let secs = i64::from_be_bytes(bytes.try_into().expect("length checked"));
            DateTime::from_timestamp(secs, 0)
        }
        _ => None,
    };

    // Build the query dynamically based on optional filters.
    let mut sql = String::from(
        r#"
        SELECT
            tm.event_id,
            tm.parent_event_id,
            tm.root_event_id,
            tm.channel_id,
            e.pubkey,
            e.content,
            e.kind,
            tm.depth,
            tm.event_created_at,
            tm.broadcast
        FROM thread_metadata tm
        JOIN events e
            ON e.created_at = tm.event_created_at
           AND e.id         = tm.event_id
        WHERE tm.root_event_id = ?
          AND e.deleted_at IS NULL
        "#,
    );

    if depth_limit.is_some() {
        sql.push_str(" AND tm.depth <= ?");
    }
    if cursor_ts.is_some() {
        sql.push_str(" AND tm.event_created_at > ?");
    }

    sql.push_str(" ORDER BY tm.event_created_at ASC LIMIT ?");

    let mut q = sqlx::query(&sql).bind(root_event_id);

    if let Some(dl) = depth_limit {
        q = q.bind(dl);
    }
    if let Some(ts) = cursor_ts {
        q = q.bind(ts);
    }
    q = q.bind(limit);

    let rows = q.fetch_all(pool).await?;

    let mut replies = Vec::with_capacity(rows.len());
    for row in rows {
        let event_id: Vec<u8> = row.try_get("event_id")?;
        let parent_event_id: Option<Vec<u8>> = row.try_get("parent_event_id")?;
        let root_event_id_col: Option<Vec<u8>> = row.try_get("root_event_id")?;
        let channel_id_bytes: Vec<u8> = row.try_get("channel_id")?;
        let pubkey: Vec<u8> = row.try_get("pubkey")?;
        let content: String = row.try_get("content")?;
        let kind: i32 = row.try_get("kind")?;
        let depth: i32 = row.try_get("depth")?;
        let created_at: DateTime<Utc> = row.try_get("event_created_at")?;
        let broadcast_val: i8 = row.try_get("broadcast")?;

        let channel_id = uuid_from_bytes(&channel_id_bytes)?;

        replies.push(ThreadReply {
            event_id,
            parent_event_id,
            root_event_id: root_event_id_col,
            channel_id,
            pubkey,
            content,
            kind,
            depth,
            created_at,
            broadcast: broadcast_val != 0,
        });
    }

    Ok(replies)
}

/// Fetch aggregated thread stats for a single event, plus up to 10 participant pubkeys.
pub async fn get_thread_summary(
    pool: &MySqlPool,
    event_id: &[u8],
) -> Result<Option<ThreadSummary>> {
    let row = sqlx::query(
        r#"
        SELECT reply_count, descendant_count, last_reply_at
        FROM thread_metadata
        WHERE event_id = ?
        LIMIT 1
        "#,
    )
    .bind(event_id)
    .fetch_optional(pool)
    .await?;

    let row = match row {
        Some(r) => r,
        None => return Ok(None),
    };

    let reply_count: i32 = row.try_get("reply_count")?;
    let descendant_count: i32 = row.try_get("descendant_count")?;
    let last_reply_at: Option<DateTime<Utc>> = row.try_get("last_reply_at")?;

    // Collect distinct participant pubkeys from the thread, most recent first (M1).
    // Without ORDER BY the result is non-deterministic across MySQL restarts/replicas.
    // Wrapping in a subquery lets us ORDER BY after DISTINCT.
    let participant_rows = sqlx::query(
        r#"
        SELECT pubkey FROM (
            SELECT DISTINCT e.pubkey, MAX(e.created_at) AS last_seen
            FROM thread_metadata tm
            JOIN events e
                ON e.created_at = tm.event_created_at
               AND e.id         = tm.event_id
            WHERE tm.root_event_id = ?
              AND e.deleted_at IS NULL
            GROUP BY e.pubkey
        ) sub
        ORDER BY last_seen DESC
        LIMIT 10
        "#,
    )
    .bind(event_id)
    .fetch_all(pool)
    .await?;

    let participants: Vec<Vec<u8>> = participant_rows
        .into_iter()
        .map(|r| r.try_get::<Vec<u8>, _>("pubkey"))
        .collect::<std::result::Result<_, _>>()?;

    Ok(Some(ThreadSummary {
        reply_count,
        descendant_count,
        last_reply_at,
        participants,
    }))
}

/// Fetch top-level messages for a channel (depth = 0, or broadcast replies).
///
/// Returns events that are either:
/// - Not in thread_metadata at all (no thread context set yet), OR
/// - At depth 0 (root messages), OR
/// - At depth 1 with `broadcast = 1` (replies surfaced to the channel)
///
/// Results are ordered newest-first for a standard channel view.
/// `before_cursor` enables keyset pagination (pass the `created_at` of the
/// last item from the previous page).
pub async fn get_channel_messages_top_level(
    pool: &MySqlPool,
    channel_id: Uuid,
    limit: u32,
    before_cursor: Option<DateTime<Utc>>,
) -> Result<Vec<TopLevelMessage>> {
    let channel_id_bytes = channel_id.as_bytes().as_slice().to_vec();

    let mut sql = String::from(
        r#"
        SELECT
            e.id          AS event_id,
            e.pubkey,
            e.content,
            e.kind,
            e.created_at,
            e.channel_id  AS channel_id_bytes
        FROM events e
        LEFT JOIN thread_metadata tm
            ON tm.event_created_at = e.created_at
           AND tm.event_id         = e.id
        WHERE e.channel_id = ?
          AND e.deleted_at IS NULL
          AND (
                tm.depth IS NULL
             OR tm.depth = 0
             OR (tm.depth = 1 AND tm.broadcast = 1)
          )
        "#,
    );

    if before_cursor.is_some() {
        sql.push_str(" AND e.created_at < ?");
    }

    sql.push_str(" ORDER BY e.created_at DESC LIMIT ?");

    let mut q = sqlx::query(&sql).bind(channel_id_bytes.as_slice());

    if let Some(cursor) = before_cursor {
        q = q.bind(cursor);
    }
    q = q.bind(limit);

    let rows = q.fetch_all(pool).await?;

    let mut messages = Vec::with_capacity(rows.len());
    for row in rows {
        let event_id: Vec<u8> = row.try_get("event_id")?;
        let pubkey: Vec<u8> = row.try_get("pubkey")?;
        let content: String = row.try_get("content")?;
        let kind: i32 = row.try_get("kind")?;
        let created_at: DateTime<Utc> = row.try_get("created_at")?;
        let channel_id_col: Vec<u8> = row.try_get("channel_id_bytes")?;
        let ch_id = uuid_from_bytes(&channel_id_col)?;

        messages.push(TopLevelMessage {
            event_id,
            pubkey,
            content,
            kind,
            created_at,
            channel_id: ch_id,
            thread_summary: None, // Populated by caller if needed
        });
    }

    Ok(messages)
}

/// Look up a single thread_metadata row by event_id.
///
/// Used when processing soft-deletes to find the parent/root so reply counts
/// can be decremented.
pub async fn get_thread_metadata_by_event(
    pool: &MySqlPool,
    event_id: &[u8],
) -> Result<Option<ThreadMetadataRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            event_id,
            event_created_at,
            channel_id,
            parent_event_id,
            root_event_id,
            depth,
            reply_count,
            descendant_count,
            broadcast
        FROM thread_metadata
        WHERE event_id = ?
        LIMIT 1
        "#,
    )
    .bind(event_id)
    .fetch_optional(pool)
    .await?;

    let row = match row {
        Some(r) => r,
        None => return Ok(None),
    };

    let event_id_col: Vec<u8> = row.try_get("event_id")?;
    let event_created_at: DateTime<Utc> = row.try_get("event_created_at")?;
    let channel_id_bytes: Vec<u8> = row.try_get("channel_id")?;
    let parent_event_id: Option<Vec<u8>> = row.try_get("parent_event_id")?;
    let root_event_id: Option<Vec<u8>> = row.try_get("root_event_id")?;
    let depth: i32 = row.try_get("depth")?;
    let reply_count: i32 = row.try_get("reply_count")?;
    let descendant_count: i32 = row.try_get("descendant_count")?;
    let broadcast_val: i8 = row.try_get("broadcast")?;

    let channel_id = uuid_from_bytes(&channel_id_bytes)?;

    Ok(Some(ThreadMetadataRecord {
        event_id: event_id_col,
        event_created_at,
        channel_id,
        parent_event_id,
        root_event_id,
        depth,
        reply_count,
        descendant_count,
        broadcast: broadcast_val != 0,
    }))
}
