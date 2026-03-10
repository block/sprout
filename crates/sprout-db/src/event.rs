//! Event storage and retrieval.
//!
//! AUTH events (kind 22242) are never stored — they carry bearer tokens.
//! Ephemeral events (kinds 20000–29999) are never stored — Redis pub/sub only.
//! Deduplication is application-layer: INSERT IGNORE.

use chrono::{DateTime, Utc};
use nostr::Event;
use sqlx::{MySqlPool, QueryBuilder, Row};
use uuid::Uuid;

use sprout_core::kind::{event_kind_i32, is_ephemeral, KIND_AUTH};
use sprout_core::StoredEvent;

use crate::error::{DbError, Result};

/// Optional filters for [`query_events`].
#[derive(Debug, Default, Clone)]
pub struct EventQuery {
    /// Restrict results to this channel.
    pub channel_id: Option<Uuid>,
    /// Restrict results to these kind values (stored as `i32` in MySQL).
    pub kinds: Option<Vec<i32>>,
    /// Restrict results to events from this pubkey.
    pub pubkey: Option<Vec<u8>>,
    /// Return events created at or after this time.
    pub since: Option<DateTime<Utc>>,
    /// Return events created at or before this time.
    pub until: Option<DateTime<Utc>>,
    /// Maximum number of events to return.
    pub limit: Option<i64>,
    /// Number of events to skip (for pagination).
    pub offset: Option<i64>,
}

/// Insert a Nostr event. Rejects AUTH and ephemeral kinds.
///
/// Returns `(StoredEvent, was_inserted)` — `was_inserted` is `false` on duplicate.
pub async fn insert_event(
    pool: &MySqlPool,
    event: &Event,
    channel_id: Option<Uuid>,
) -> Result<(StoredEvent, bool)> {
    let kind_u16 = event.kind.as_u16();
    let kind_u32 = u32::from(kind_u16);

    if kind_u32 == KIND_AUTH {
        return Err(DbError::AuthEventRejected);
    }
    if is_ephemeral(kind_u32) {
        return Err(DbError::EphemeralEventRejected(kind_u16));
    }

    let id_bytes = event.id.as_bytes();
    let pubkey_bytes = event.pubkey.to_bytes();
    let sig_bytes = event.sig.serialize();
    let tags_json = serde_json::to_value(&event.tags)?;
    // Cast chain: nostr Kind (u16) → i32 (MySQL INT column). Safe: all Sprout kinds fit in i32.
    let kind_i32 = event_kind_i32(event);
    let created_at_secs = event.created_at.as_u64() as i64;
    let created_at = DateTime::from_timestamp(created_at_secs, 0)
        .ok_or(DbError::InvalidTimestamp(created_at_secs))?;
    let received_at = Utc::now();
    let channel_id_bytes: Option<[u8; 16]> = channel_id.map(|u| *u.as_bytes());

    let result = sqlx::query(
        r#"
        INSERT IGNORE INTO events (id, pubkey, created_at, kind, tags, content, sig, received_at, channel_id)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(id_bytes.as_slice())
    .bind(pubkey_bytes.as_slice())
    .bind(created_at)
    .bind(kind_i32)
    .bind(&tags_json)
    .bind(&event.content)
    .bind(sig_bytes.as_slice())
    .bind(received_at)
    .bind(channel_id_bytes.as_ref().map(|b| b.as_slice()))
    .execute(pool)
    .await?;

    let was_inserted = result.rows_affected() > 0;

    Ok((
        StoredEvent::with_received_at(event.clone(), received_at, channel_id, true),
        was_inserted,
    ))
}

/// Query events with optional filters. Results ordered by `created_at DESC`.
///
/// Uses `QueryBuilder` for dynamic filter composition — avoids string concatenation
/// while keeping all user values in bind parameters.
pub async fn query_events(pool: &MySqlPool, q: &EventQuery) -> Result<Vec<StoredEvent>> {
    let limit_val = q.limit.unwrap_or(100).min(1000);
    let offset_val = q.offset.unwrap_or(0);

    let mut qb: QueryBuilder<sqlx::MySql> = QueryBuilder::new(
        "SELECT id, pubkey, created_at, kind, tags, content, sig, received_at, channel_id \
         FROM events WHERE deleted_at IS NULL",
    );

    if let Some(ch) = q.channel_id {
        qb.push(" AND channel_id = ")
            .push_bind(ch.as_bytes().to_vec());
    }

    if let Some(ks) = q.kinds.as_deref().filter(|k| !k.is_empty()) {
        qb.push(" AND kind IN (");
        let mut sep = qb.separated(", ");
        for k in ks {
            sep.push_bind(*k);
        }
        qb.push(")");
    }

    if let Some(ref pk) = q.pubkey {
        qb.push(" AND pubkey = ").push_bind(pk.clone());
    }
    if let Some(s) = q.since {
        qb.push(" AND created_at >= ").push_bind(s);
    }
    if let Some(u) = q.until {
        qb.push(" AND created_at <= ").push_bind(u);
    }

    qb.push(" ORDER BY created_at DESC LIMIT ")
        .push_bind(limit_val);
    qb.push(" OFFSET ").push_bind(offset_val);

    let rows = qb.build().fetch_all(pool).await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        if let Some(ev) = row_to_stored_event(row)? {
            out.push(ev);
        }
    }
    Ok(out)
}

pub(crate) fn row_to_stored_event(row: sqlx::mysql::MySqlRow) -> Result<Option<StoredEvent>> {
    let id_bytes: Vec<u8> = row.try_get("id")?;
    let pubkey_bytes: Vec<u8> = row.try_get("pubkey")?;
    let created_at: DateTime<Utc> = row.try_get("created_at")?;
    let kind_i32: i32 = row.try_get("kind")?;
    let tags_json: serde_json::Value = row.try_get("tags")?;
    let content: String = row.try_get("content")?;
    let sig_bytes: Vec<u8> = row.try_get("sig")?;
    let received_at: DateTime<Utc> = row.try_get("received_at")?;

    let channel_id_bytes: Option<Vec<u8>> = row.try_get("channel_id")?;
    let channel_id: Option<Uuid> = channel_id_bytes.map(|b| uuid_from_bytes(&b)).transpose()?;

    // kind is stored as i32 (MySQL INT) but Nostr uses u16. Values > 65535 are corrupt.
    let kind_u16 = u16::try_from(kind_i32)
        .map_err(|_| DbError::InvalidData(format!("kind out of u16 range: {kind_i32}")))?;

    let event_json = serde_json::json!({
        "id": hex::encode(&id_bytes),
        "pubkey": hex::encode(&pubkey_bytes),
        "created_at": created_at.timestamp(),
        "kind": kind_u16,
        "tags": tags_json,
        "content": content,
        "sig": hex::encode(&sig_bytes),
    });

    // Avoid the Value → String → parse round-trip: deserialize directly from the Value.
    let event: nostr::Event = match serde_json::from_value(event_json) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("failed to reconstruct event from DB row: {e}");
            return Ok(None);
        }
    };

    Ok(Some(StoredEvent::with_received_at(
        event,
        received_at,
        channel_id,
        true,
    )))
}

/// Soft-delete an event by setting `deleted_at = NOW(6)`.
///
/// Returns `Ok(true)` if the event was deleted, `Ok(false)` if already deleted
/// or not found. Callers are responsible for decrementing thread reply counts
/// when the deleted event is a thread reply.
pub async fn soft_delete_event(pool: &MySqlPool, event_id: &[u8]) -> Result<bool> {
    let result =
        sqlx::query("UPDATE events SET deleted_at = NOW(6) WHERE id = ? AND deleted_at IS NULL")
            .bind(event_id)
            .execute(pool)
            .await?;

    Ok(result.rows_affected() > 0)
}

/// Atomically soft-delete an event and decrement thread reply counters.
///
/// Wraps the delete + counter update in a single transaction so a crash between
/// them cannot leave counters permanently inflated. Returns `Ok(true)` if the
/// event was deleted this call.
pub async fn soft_delete_event_and_update_thread(
    pool: &MySqlPool,
    event_id: &[u8],
    parent_event_id: Option<&[u8]>,
    root_event_id: Option<&[u8]>,
) -> Result<bool> {
    let mut tx = pool.begin().await?;

    let result =
        sqlx::query("UPDATE events SET deleted_at = NOW(6) WHERE id = ? AND deleted_at IS NULL")
            .bind(event_id)
            .execute(&mut *tx)
            .await?;

    let deleted = result.rows_affected() > 0;

    if deleted {
        if let Some(pid) = parent_event_id {
            sqlx::query(
                "UPDATE thread_metadata \
                 SET reply_count = GREATEST(reply_count - 1, 0) \
                 WHERE event_id = ?",
            )
            .bind(pid)
            .execute(&mut *tx)
            .await?;

            if let Some(root_id) = root_event_id {
                sqlx::query(
                    "UPDATE thread_metadata \
                     SET descendant_count = GREATEST(descendant_count - 1, 0) \
                     WHERE event_id = ?",
                )
                .bind(root_id)
                .execute(&mut *tx)
                .await?;
            }
        }
    }

    tx.commit().await?;
    Ok(deleted)
}

/// Returns the `created_at` timestamp of the most recent non-deleted event in a channel.
pub async fn get_last_message_at(
    pool: &MySqlPool,
    channel_id: uuid::Uuid,
) -> Result<Option<DateTime<Utc>>> {
    let id_bytes = channel_id.as_bytes().as_slice().to_vec();
    let row = sqlx::query(
        "SELECT created_at FROM events \
         WHERE channel_id = ? AND deleted_at IS NULL \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(&id_bytes)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(r) => Ok(Some(r.try_get("created_at")?)),
        None => Ok(None),
    }
}

/// Bulk-fetch the most recent `created_at` for a set of channel IDs.
///
/// Returns a map of `channel_id → last_message_at`. Channels with no events are omitted.
/// Single query regardless of input size.
pub async fn get_last_message_at_bulk(
    pool: &MySqlPool,
    channel_ids: &[uuid::Uuid],
) -> Result<std::collections::HashMap<uuid::Uuid, DateTime<Utc>>> {
    if channel_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    let mut qb: QueryBuilder<sqlx::MySql> = QueryBuilder::new(
        "SELECT channel_id, MAX(created_at) as last_at FROM events \
         WHERE deleted_at IS NULL AND channel_id IN (",
    );
    let mut sep = qb.separated(", ");
    for id in channel_ids {
        sep.push_bind(id.as_bytes().to_vec());
    }
    qb.push(") GROUP BY channel_id");

    let rows = qb.build().fetch_all(pool).await?;

    let mut map = std::collections::HashMap::with_capacity(rows.len());
    for row in rows {
        let id_bytes: Vec<u8> = row.try_get("channel_id")?;
        let id = uuid_from_bytes(&id_bytes)?;
        let last_at: DateTime<Utc> = row.try_get("last_at")?;
        map.insert(id, last_at);
    }
    Ok(map)
}

/// Fetches a single non-deleted event by its raw 32-byte ID.
///
/// Returns `None` if the event does not exist or has been soft-deleted.
/// Use [`get_event_by_id_including_deleted`] when you need to inspect
/// tombstoned rows (e.g. audit, undelete).
pub async fn get_event_by_id(pool: &MySqlPool, id_bytes: &[u8]) -> Result<Option<StoredEvent>> {
    let row = sqlx::query(
        "SELECT id, pubkey, created_at, kind, tags, content, sig, received_at, channel_id \
         FROM events WHERE id = ? AND deleted_at IS NULL ORDER BY created_at DESC LIMIT 1",
    )
    .bind(id_bytes)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(r) => row_to_stored_event(r),
        None => Ok(None),
    }
}

/// Fetches a single event by its raw 32-byte ID, **including soft-deleted rows**.
///
/// Most callers should use [`get_event_by_id`] instead. This variant is needed
/// when the caller must distinguish "never existed" from "was deleted" (e.g.
/// audit trails, compliance queries).
pub async fn get_event_by_id_including_deleted(
    pool: &MySqlPool,
    id_bytes: &[u8],
) -> Result<Option<StoredEvent>> {
    let row = sqlx::query(
        "SELECT id, pubkey, created_at, kind, tags, content, sig, received_at, channel_id \
         FROM events WHERE id = ? ORDER BY created_at DESC LIMIT 1",
    )
    .bind(id_bytes)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(r) => row_to_stored_event(r),
        None => Ok(None),
    }
}

/// Parameters for [`insert_event_with_thread_metadata`].
#[derive(Debug)]
pub struct ThreadMetadataParams<'a> {
    /// The Nostr event ID of this message.
    pub event_id: &'a [u8],
    /// When the event was created.
    pub event_created_at: DateTime<Utc>,
    /// The channel this event belongs to.
    pub channel_id: Uuid,
    /// Event ID of the direct parent, if this is a reply.
    pub parent_event_id: Option<&'a [u8]>,
    /// When the parent event was created.
    pub parent_event_created_at: Option<DateTime<Utc>>,
    /// Event ID of the thread root, if this is a nested reply.
    pub root_event_id: Option<&'a [u8]>,
    /// When the root event was created.
    pub root_event_created_at: Option<DateTime<Utc>>,
    /// Nesting depth (root = 0).
    pub depth: i32,
    /// Whether this reply is broadcast to the channel timeline.
    pub broadcast: bool,
}

/// Atomically insert an event AND its thread metadata in a single transaction.
///
/// This prevents the race condition where a concurrent delete between separate
/// `insert_event` and `insert_thread_metadata` calls could leave reply counters
/// permanently inflated (the metadata insert increments counters for an event
/// that was already soft-deleted).
///
/// Returns `(StoredEvent, was_inserted)`.
pub async fn insert_event_with_thread_metadata(
    pool: &MySqlPool,
    event: &Event,
    channel_id: Option<Uuid>,
    thread_meta: Option<ThreadMetadataParams<'_>>,
) -> Result<(StoredEvent, bool)> {
    let kind_u16 = event.kind.as_u16();
    let kind_u32 = u32::from(kind_u16);

    if kind_u32 == KIND_AUTH {
        return Err(DbError::AuthEventRejected);
    }
    if is_ephemeral(kind_u32) {
        return Err(DbError::EphemeralEventRejected(kind_u16));
    }

    let id_bytes = event.id.as_bytes();
    let pubkey_bytes = event.pubkey.to_bytes();
    let sig_bytes = event.sig.serialize();
    let tags_json = serde_json::to_value(&event.tags)?;
    let kind_i32 = event_kind_i32(event);
    let created_at_secs = event.created_at.as_u64() as i64;
    let created_at = DateTime::from_timestamp(created_at_secs, 0)
        .ok_or(DbError::InvalidTimestamp(created_at_secs))?;
    let received_at = Utc::now();
    let channel_id_bytes: Option<[u8; 16]> = channel_id.map(|u| *u.as_bytes());

    let mut tx = pool.begin().await?;

    // ── Insert event ──────────────────────────────────────────────────────────
    let result = sqlx::query(
        r#"
        INSERT IGNORE INTO events (id, pubkey, created_at, kind, tags, content, sig, received_at, channel_id)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(id_bytes.as_slice())
    .bind(pubkey_bytes.as_slice())
    .bind(created_at)
    .bind(kind_i32)
    .bind(&tags_json)
    .bind(&event.content)
    .bind(sig_bytes.as_slice())
    .bind(received_at)
    .bind(channel_id_bytes.as_ref().map(|b| b.as_slice()))
    .execute(&mut *tx)
    .await?;

    let was_inserted = result.rows_affected() > 0;

    // ── Insert thread metadata (if provided and event was actually inserted) ──
    if was_inserted {
        if let Some(ref meta) = thread_meta {
            let ch_bytes = meta.channel_id.as_bytes().as_slice().to_vec();
            let broadcast_val: i8 = if meta.broadcast { 1 } else { 0 };

            let tm_result = sqlx::query(
                r#"
                INSERT IGNORE INTO thread_metadata
                    (event_created_at, event_id, channel_id,
                     parent_event_id, parent_event_created_at,
                     root_event_id, root_event_created_at,
                     depth, broadcast)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(meta.event_created_at)
            .bind(meta.event_id)
            .bind(ch_bytes.as_slice())
            .bind(meta.parent_event_id)
            .bind(meta.parent_event_created_at)
            .bind(meta.root_event_id)
            .bind(meta.root_event_created_at)
            .bind(meta.depth)
            .bind(broadcast_val)
            .execute(&mut *tx)
            .await?;

            // Only bump reply counts if the metadata row was actually inserted.
            if tm_result.rows_affected() > 0 {
                if let Some(pid) = meta.parent_event_id {
                    // Ensure the parent has a thread_metadata row so the UPDATE
                    // below has something to hit. Root (depth=0) messages don't
                    // get a row on first insert, so we create a stub here.
                    let parent_ts = meta.parent_event_created_at.unwrap_or(meta.event_created_at);
                    sqlx::query(
                        r#"
                        INSERT IGNORE INTO thread_metadata
                            (event_created_at, event_id, channel_id,
                             parent_event_id, parent_event_created_at,
                             root_event_id, root_event_created_at,
                             depth, broadcast)
                        VALUES (?, ?, ?, NULL, NULL, NULL, NULL, 0, 0)
                        "#,
                    )
                    .bind(parent_ts)
                    .bind(pid)
                    .bind(ch_bytes.as_slice())
                    .execute(&mut *tx)
                    .await?;

                    // Ensure the root also has a row (may differ from parent for nested replies).
                    if let Some(root_id) = meta.root_event_id {
                        if root_id != pid {
                            let root_ts = meta.root_event_created_at.unwrap_or(meta.event_created_at);
                            sqlx::query(
                                r#"
                                INSERT IGNORE INTO thread_metadata
                                    (event_created_at, event_id, channel_id,
                                     parent_event_id, parent_event_created_at,
                                     root_event_id, root_event_created_at,
                                     depth, broadcast)
                                VALUES (?, ?, ?, NULL, NULL, NULL, NULL, 0, 0)
                                "#,
                            )
                            .bind(root_ts)
                            .bind(root_id)
                            .bind(ch_bytes.as_slice())
                            .execute(&mut *tx)
                            .await?;
                        }
                    }

                    sqlx::query(
                        r#"
                        UPDATE thread_metadata
                        SET reply_count = reply_count + 1, last_reply_at = NOW(6)
                        WHERE event_id = ?
                        "#,
                    )
                    .bind(pid)
                    .execute(&mut *tx)
                    .await?;

                    if let Some(root_id) = meta.root_event_id {
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
        }
    }

    tx.commit().await?;

    Ok((
        StoredEvent::with_received_at(event.clone(), received_at, channel_id, true),
        was_inserted,
    ))
}

/// Convert raw BINARY(16) bytes to a [`Uuid`].
pub(crate) fn uuid_from_bytes(bytes: &[u8]) -> Result<Uuid> {
    Uuid::from_slice(bytes).map_err(|e| DbError::InvalidData(format!("invalid UUID: {e}")))
}
