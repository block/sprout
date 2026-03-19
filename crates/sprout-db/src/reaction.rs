//! Reaction persistence.
//!
//! One reaction per user per emoji per event. Soft-delete via removed_at.

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};

use crate::error::Result;

// -- Public structs -----------------------------------------------------------

/// A grouped set of reactions for a single emoji on an event.
#[derive(Debug, Clone)]
pub struct ReactionGroup {
    /// The emoji character or shortcode used in this reaction group.
    pub emoji: String,
    /// Total number of active reactions with this emoji.
    pub count: i64,
    /// Individual users who reacted with this emoji.
    pub users: Vec<ReactionUser>,
}

/// A single user who reacted with a given emoji.
#[derive(Debug, Clone)]
pub struct ReactionUser {
    /// Compressed 33-byte public key of the reacting user.
    pub pubkey: Vec<u8>,
    /// Optional display name resolved from the users table.
    pub display_name: Option<String>,
}

/// Bulk reaction entry for embedding in message lists.
#[derive(Debug, Clone)]
pub struct BulkReactionEntry {
    /// The event this reaction entry belongs to.
    pub event_id: Vec<u8>,
    /// Partition key timestamp for the event.
    pub event_created_at: DateTime<Utc>,
    /// Emoji + count summaries for this event.
    pub reactions: Vec<ReactionSummary>,
}

/// Emoji + count summary (no user list) for bulk fetches.
#[derive(Debug, Clone)]
pub struct ReactionSummary {
    /// The emoji character or shortcode.
    pub emoji: String,
    /// Number of active reactions with this emoji.
    pub count: i64,
}

/// Active reaction row metadata for a specific actor + emoji + target tuple.
#[derive(Debug, Clone)]
pub struct ActiveReactionRecord {
    /// Nostr event ID of the reaction event, if this row came from a real kind:7 event.
    pub reaction_event_id: Option<Vec<u8>>,
}

// -- Write operations ---------------------------------------------------------

/// Add (or re-activate) a reaction.
///
/// Returns `Ok(true)` if the reaction was added or re-activated, `Ok(false)` if
/// the reaction is already active (duplicate, no change made).
///
/// Uses `INSERT ... ON CONFLICT DO UPDATE` to eliminate the TOCTOU race where
/// two concurrent adds both see no existing row and then race to INSERT.
pub async fn add_reaction(
    pool: &PgPool,
    event_id: &[u8],
    event_created_at: DateTime<Utc>,
    pubkey: &[u8],
    emoji: &str,
    reaction_event_id: Option<&[u8]>,
) -> Result<bool> {
    let result = sqlx::query(
        r#"
        INSERT INTO reactions (event_created_at, event_id, pubkey, emoji, reaction_event_id)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (event_created_at, event_id, pubkey, emoji) DO UPDATE SET
            created_at = CASE WHEN reactions.removed_at IS NOT NULL THEN NOW() ELSE reactions.created_at END,
            removed_at = NULL,
            reaction_event_id = COALESCE(EXCLUDED.reaction_event_id, reactions.reaction_event_id)
        "#,
    )
    .bind(event_created_at)
    .bind(event_id)
    .bind(pubkey)
    .bind(emoji)
    .bind(reaction_event_id)
    .execute(pool)
    .await?;

    // rows_affected = 0 means the row already existed and was already active
    // (removed_at was already NULL, so no values changed).
    Ok(result.rows_affected() != 0)
}

/// Soft-delete a reaction by setting `removed_at`.
///
/// Returns `true` if a row was updated, `false` if not found or already removed.
pub async fn remove_reaction(
    pool: &PgPool,
    event_id: &[u8],
    event_created_at: DateTime<Utc>,
    pubkey: &[u8],
    emoji: &str,
) -> Result<bool> {
    let result = sqlx::query(
        r#"
        UPDATE reactions
        SET removed_at = NOW()
        WHERE event_created_at = $1
          AND event_id = $2
          AND pubkey = $3
          AND emoji = $4
          AND removed_at IS NULL
        "#,
    )
    .bind(event_created_at)
    .bind(event_id)
    .bind(pubkey)
    .bind(emoji)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Soft-delete a reaction by the reaction event's own ID.
///
/// Returns `true` if a row was updated, `false` if not found or already removed.
pub async fn remove_reaction_by_source_event_id(
    pool: &PgPool,
    reaction_event_id: &[u8],
) -> Result<bool> {
    let result = sqlx::query(
        r#"
        UPDATE reactions
        SET removed_at = NOW()
        WHERE reaction_event_id = $1
          AND removed_at IS NULL
        "#,
    )
    .bind(reaction_event_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Look up the active reaction row for one actor + emoji + target tuple.
pub async fn get_active_reaction_record(
    pool: &PgPool,
    event_id: &[u8],
    event_created_at: DateTime<Utc>,
    pubkey: &[u8],
    emoji: &str,
) -> Result<Option<ActiveReactionRecord>> {
    let row = sqlx::query(
        r#"
        SELECT reaction_event_id
        FROM reactions
        WHERE event_id = $1
          AND event_created_at = $2
          AND pubkey = $3
          AND emoji = $4
          AND removed_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(event_id)
    .bind(event_created_at)
    .bind(pubkey)
    .bind(emoji)
    .fetch_optional(pool)
    .await?;

    row.map(|row| -> Result<ActiveReactionRecord> {
        Ok(ActiveReactionRecord {
            reaction_event_id: row.try_get("reaction_event_id")?,
        })
    })
    .transpose()
}

/// Backfill the source event ID on an active reaction row.
///
/// Called after the kind:7 event is created and stored, to link the
/// reaction row to its source event. Returns `true` if the row was updated.
pub async fn set_reaction_event_id(
    pool: &PgPool,
    event_id: &[u8],
    event_created_at: DateTime<Utc>,
    pubkey: &[u8],
    emoji: &str,
    reaction_event_id: &[u8],
) -> Result<bool> {
    let result = sqlx::query(
        r#"
        UPDATE reactions
        SET reaction_event_id = $1
        WHERE event_created_at = $2
          AND event_id = $3
          AND pubkey = $4
          AND emoji = $5
          AND removed_at IS NULL
        "#,
    )
    .bind(reaction_event_id)
    .bind(event_created_at)
    .bind(event_id)
    .bind(pubkey)
    .bind(emoji)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

// -- Read operations ----------------------------------------------------------

/// Get all active reactions for an event, grouped by emoji.
///
/// Returns one [`ReactionGroup`] per emoji, each containing the list of reacting
/// user pubkeys. Display names are NOT resolved here -- callers should enrich via
/// `get_users_bulk` if needed.
///
/// `cursor` is reserved for future keyset pagination (currently unused).
pub async fn get_reactions(
    pool: &PgPool,
    event_id: &[u8],
    event_created_at: DateTime<Utc>,
    limit: u32,
    _cursor: Option<&str>,
) -> Result<Vec<ReactionGroup>> {
    let rows = sqlx::query(
        r#"
        SELECT emoji,
               COUNT(*) AS count,
               string_agg(encode(pubkey, 'hex'), ',' ORDER BY created_at) AS pubkeys_hex
        FROM reactions
        WHERE event_id = $1
          AND event_created_at = $2
          AND removed_at IS NULL
        GROUP BY emoji
        ORDER BY emoji
        LIMIT $3
        "#,
    )
    .bind(event_id)
    .bind(event_created_at)
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;

    let mut groups = Vec::with_capacity(rows.len());

    for row in rows {
        let emoji: String = row.try_get("emoji")?;
        let count: i64 = row.try_get("count")?;
        let pubkeys_hex: Option<String> = row.try_get("pubkeys_hex")?;

        let users = parse_pubkeys_hex(pubkeys_hex.as_deref().unwrap_or(""));

        groups.push(ReactionGroup {
            emoji,
            count,
            users,
        });
    }

    Ok(groups)
}

/// Batch-fetch emoji counts for a set of (event_id, event_created_at) pairs.
///
/// Returns one [`BulkReactionEntry`] per input pair that has at least one
/// active reaction. Pairs with no reactions are omitted.
pub async fn get_reactions_bulk(
    pool: &PgPool,
    event_ids: &[(&[u8], DateTime<Utc>)],
) -> Result<Vec<BulkReactionEntry>> {
    if event_ids.is_empty() {
        return Ok(Vec::new());
    }

    // Run one query per event. For typical message-list sizes (<=100 events)
    // this is acceptable; a single-query approach with dynamic IN clauses over
    // composite keys can be added later if needed.
    let mut entries = Vec::new();

    for (event_id, event_created_at) in event_ids {
        let rows = sqlx::query(
            r#"
            SELECT emoji, COUNT(*) AS count
            FROM reactions
            WHERE event_id = $1
              AND event_created_at = $2
              AND removed_at IS NULL
            GROUP BY emoji
            ORDER BY emoji
            "#,
        )
        .bind(*event_id)
        .bind(event_created_at)
        .fetch_all(pool)
        .await?;

        if rows.is_empty() {
            continue;
        }

        let mut reactions = Vec::with_capacity(rows.len());
        for row in rows {
            let emoji: String = row.try_get("emoji")?;
            let count: i64 = row.try_get("count")?;
            reactions.push(ReactionSummary { emoji, count });
        }

        entries.push(BulkReactionEntry {
            event_id: event_id.to_vec(),
            event_created_at: *event_created_at,
            reactions,
        });
    }

    Ok(entries)
}

// -- Helpers ------------------------------------------------------------------

/// Parse a `string_agg(encode(pubkey, 'hex'))` string into individual pubkeys.
///
/// Postgres `encode(bytea, 'hex')` produces lowercase hex characters, so a
/// 32-byte pubkey becomes a 64-character hex string.
fn parse_pubkeys_hex(hex_str: &str) -> Vec<ReactionUser> {
    if hex_str.is_empty() {
        return Vec::new();
    }
    hex_str
        .split(',')
        .filter_map(|h| hex::decode(h.trim()).ok())
        .filter(|b| b.len() == 32)
        .map(|pubkey| ReactionUser {
            pubkey,
            display_name: None,
        })
        .collect()
}
