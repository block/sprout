#![deny(unsafe_code)]
#![warn(missing_docs)]
//! sprout-db — MySQL event store for Sprout.
//!
//! ## Design invariants
//! - AUTH events (kind 22242) are never stored — they carry bearer tokens.
//! - Ephemeral events (20000–29999) are never stored — Redis pub/sub only.
//! - Events table is partitioned by month on `created_at`.
//! - No FK references to partitioned tables.
//! - Uses `sqlx::query()` (runtime) not `sqlx::query!()` (compile-time).

/// API token storage and lookup.
pub mod api_token;
/// Channel and membership persistence.
pub mod channel;
/// Direct message channel persistence.
pub mod dm;
/// Database error types.
pub mod error;
/// Event storage and retrieval.
pub mod event;
/// Home feed queries.
pub mod feed;
/// Monthly table partition management.
pub mod partition;
/// Reaction persistence.
pub mod reaction;
/// Thread metadata persistence.
pub mod thread;
/// User profile persistence.
pub mod user;
/// Workflow, run, and approval persistence.
pub mod workflow;

pub use error::{DbError, Result};
pub use event::EventQuery;

use chrono::{DateTime, Utc};
use sqlx::mysql::MySqlPoolOptions;
use sqlx::{MySqlPool, Row};
use std::time::Duration;
use uuid::Uuid;

use sprout_core::StoredEvent;

use crate::event::uuid_from_bytes;

/// Extract p-tag mentions from an event and insert into the `event_mentions` table.
///
/// Called after event insertion. Failures are logged but do not block event storage.
/// Uses `INSERT IGNORE` so duplicate inserts (e.g. on retry) are silently skipped.
pub async fn insert_mentions(
    pool: &MySqlPool,
    event: &nostr::Event,
    channel_id: Option<Uuid>,
) -> Result<()> {
    let p_tags: Vec<&str> = event
        .tags
        .iter()
        .filter_map(|tag| {
            let tag_vec = tag.as_slice();
            if tag_vec.len() >= 2 && tag_vec[0] == "p" {
                Some(tag_vec[1].as_str())
            } else {
                None
            }
        })
        .collect();

    if p_tags.is_empty() {
        return Ok(());
    }

    let event_id_bytes = event.id.as_bytes();
    let created_at_secs = event.created_at.as_u64() as i64;
    let created_at = DateTime::from_timestamp(created_at_secs, 0)
        .ok_or(crate::error::DbError::InvalidTimestamp(created_at_secs))?;
    let channel_id_bytes = channel_id.map(|id| id.as_bytes().to_vec());
    let kind = event.kind.as_u16() as u32;

    for pubkey_hex in p_tags {
        // Validate: must be exactly 64 hex characters (32-byte pubkey)
        if pubkey_hex.len() != 64 || !pubkey_hex.chars().all(|c| c.is_ascii_hexdigit()) {
            tracing::debug!(
                event_id = %event.id,
                invalid_ptag = pubkey_hex,
                "skipping malformed p-tag in mentions insert"
            );
            continue;
        }
        // Normalize to lowercase — queries use hex::encode which produces lowercase.
        let pubkey_lower = pubkey_hex.to_ascii_lowercase();
        sqlx::query(
            "INSERT IGNORE INTO event_mentions \
             (pubkey_hex, event_id, event_created_at, channel_id, event_kind) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&pubkey_lower)
        .bind(event_id_bytes.as_slice())
        .bind(created_at)
        .bind(channel_id_bytes.as_deref())
        .bind(kind)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Database handle. Clone is cheap (Arc-backed pool).
#[derive(Clone, Debug)]
pub struct Db {
    pub(crate) pool: MySqlPool,
}

/// Configuration for the MySQL connection pool.
#[derive(Debug, Clone)]
pub struct DbConfig {
    /// MySQL connection URL (e.g. `mysql://user:pass@host/db`).
    pub database_url: String,
    /// Maximum number of connections in the pool.
    pub max_connections: u32,
    /// Minimum number of idle connections to maintain.
    pub min_connections: u32,
    /// Seconds to wait when acquiring a connection before timing out.
    pub acquire_timeout_secs: u64,
    /// Maximum connection lifetime in seconds before recycling.
    pub max_lifetime_secs: u64,
    /// Seconds a connection may sit idle before being closed.
    pub idle_timeout_secs: u64,
}

impl Default for DbConfig {
    fn default() -> Self {
        Self {
            database_url: "mysql://sprout:sprout_dev@localhost:3306/sprout".to_string(),
            max_connections: 50,
            min_connections: 5,
            acquire_timeout_secs: 3,
            max_lifetime_secs: 1800,
            idle_timeout_secs: 600,
        }
    }
}

/// Token summary returned by [`Db::list_active_tokens`].
#[derive(Debug, Clone)]
pub struct TokenSummary {
    /// Unique token identifier.
    pub id: Uuid,
    /// Human-readable token name.
    pub name: String,
    /// Compressed public key bytes of the token owner.
    pub owner_pubkey: Vec<u8>,
    /// Permission scopes granted to this token.
    pub scopes: Vec<String>,
    /// When the token was created.
    pub created_at: DateTime<Utc>,
    /// Optional expiry timestamp; `None` means no expiry.
    pub expires_at: Option<DateTime<Utc>>,
}

impl Db {
    /// Creates a new `Db` by connecting a MySQL pool with the given config.
    pub async fn new(config: &DbConfig) -> Result<Self> {
        let pool = MySqlPoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .acquire_timeout(Duration::from_secs(config.acquire_timeout_secs))
            .max_lifetime(Duration::from_secs(config.max_lifetime_secs))
            .idle_timeout(Duration::from_secs(config.idle_timeout_secs))
            .connect(&config.database_url)
            .await?;
        Ok(Self { pool })
    }

    /// Creates a `Db` from an existing `MySqlPool` (useful in tests).
    pub fn from_pool(pool: MySqlPool) -> Self {
        Self { pool }
    }

    /// Runs all pending SQLx migrations against the database.
    pub async fn migrate(&self) -> Result<()> {
        sqlx::migrate!("../../migrations").run(&self.pool).await?;
        Ok(())
    }

    /// Returns `true` if the database is reachable (used by readiness probes).
    pub async fn ping(&self) -> bool {
        sqlx::query("SELECT 1").execute(&self.pool).await.is_ok()
    }

    // ── Events ───────────────────────────────────────────────────────────────

    /// Inserts an event. Returns `(StoredEvent, was_inserted)` — `false` on duplicate.
    pub async fn insert_event(
        &self,
        event: &nostr::Event,
        channel_id: Option<Uuid>,
    ) -> Result<(StoredEvent, bool)> {
        let result = event::insert_event(&self.pool, event, channel_id).await?;
        if result.1 {
            if let Err(e) = insert_mentions(&self.pool, event, channel_id).await {
                tracing::warn!(event_id = %event.id, "Failed to insert mentions: {e}");
            }
        }
        Ok(result)
    }

    /// Queries events matching the given filter parameters.
    pub async fn query_events(&self, q: &EventQuery) -> Result<Vec<StoredEvent>> {
        event::query_events(&self.pool, q).await
    }

    /// Fetches a single non-deleted event by its raw ID bytes.
    ///
    /// Returns `None` if the event does not exist or has been soft-deleted.
    pub async fn get_event_by_id(&self, id_bytes: &[u8]) -> Result<Option<StoredEvent>> {
        event::get_event_by_id(&self.pool, id_bytes).await
    }

    /// Fetches a single event by ID, **including soft-deleted rows**.
    ///
    /// Most callers should use [`get_event_by_id`] instead.
    pub async fn get_event_by_id_including_deleted(
        &self,
        id_bytes: &[u8],
    ) -> Result<Option<StoredEvent>> {
        event::get_event_by_id_including_deleted(&self.pool, id_bytes).await
    }

    /// Batch-fetch non-deleted events by their raw ID bytes.
    ///
    /// Returns events in arbitrary order — callers reorder as needed.
    pub async fn get_events_by_ids(&self, ids: &[&[u8]]) -> Result<Vec<StoredEvent>> {
        event::get_events_by_ids(&self.pool, ids).await
    }

    /// Atomically insert an event and its thread metadata in one transaction.
    ///
    /// Prevents the race where a concurrent delete between separate insert calls
    /// could leave reply counters permanently inflated.
    pub async fn insert_event_with_thread_metadata(
        &self,
        ev: &nostr::Event,
        channel_id: Option<Uuid>,
        thread_meta: Option<event::ThreadMetadataParams<'_>>,
    ) -> Result<(StoredEvent, bool)> {
        let result =
            event::insert_event_with_thread_metadata(&self.pool, ev, channel_id, thread_meta)
                .await?;
        if result.1 {
            if let Err(e) = insert_mentions(&self.pool, ev, channel_id).await {
                tracing::warn!(event_id = %ev.id, "Failed to insert mentions: {e}");
            }
        }
        Ok(result)
    }

    /// Soft-delete an event. Returns `true` if the event was deleted.
    pub async fn soft_delete_event(&self, event_id: &[u8]) -> Result<bool> {
        event::soft_delete_event(&self.pool, event_id).await
    }

    /// Atomically soft-delete an event and decrement thread counters in one transaction.
    pub async fn soft_delete_event_and_update_thread(
        &self,
        event_id: &[u8],
        parent_event_id: Option<&[u8]>,
        root_event_id: Option<&[u8]>,
    ) -> Result<bool> {
        event::soft_delete_event_and_update_thread(
            &self.pool,
            event_id,
            parent_event_id,
            root_event_id,
        )
        .await
    }

    /// Returns the timestamp of the most recent non-deleted event in a channel.
    pub async fn get_last_message_at(&self, channel_id: Uuid) -> Result<Option<DateTime<Utc>>> {
        event::get_last_message_at(&self.pool, channel_id).await
    }

    /// Bulk-fetch last message timestamps for multiple channels in one query.
    pub async fn get_last_message_at_bulk(
        &self,
        channel_ids: &[Uuid],
    ) -> Result<std::collections::HashMap<Uuid, DateTime<Utc>>> {
        event::get_last_message_at_bulk(&self.pool, channel_ids).await
    }

    // ── Feed ─────────────────────────────────────────────────────────────────

    /// Returns events that mention `pubkey` in the given channels.
    pub async fn query_feed_mentions(
        &self,
        pubkey: &[u8],
        channel_ids: &[Uuid],
        since: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<StoredEvent>> {
        feed::query_mentions(&self.pool, pubkey, channel_ids, since, limit).await
    }

    /// Returns events that require action from `pubkey` (approvals, reactions, etc.).
    pub async fn query_feed_needs_action(
        &self,
        pubkey: &[u8],
        channel_ids: &[Uuid],
        since: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<StoredEvent>> {
        feed::query_needs_action(&self.pool, pubkey, channel_ids, since, limit).await
    }

    /// Returns recent activity across the given channels.
    pub async fn query_feed_activity(
        &self,
        channel_ids: &[Uuid],
        since: Option<DateTime<Utc>>,
        limit: i64,
    ) -> Result<Vec<StoredEvent>> {
        feed::query_activity(&self.pool, channel_ids, since, limit).await
    }

    // ── Channels ─────────────────────────────────────────────────────────────

    /// Creates a new channel, bootstraps the creator as owner, and returns the record.
    pub async fn create_channel(
        &self,
        name: &str,
        channel_type: channel::ChannelType,
        visibility: channel::ChannelVisibility,
        description: Option<&str>,
        created_by: &[u8],
    ) -> Result<channel::ChannelRecord> {
        channel::create_channel(
            &self.pool,
            name,
            channel_type,
            visibility,
            description,
            created_by,
        )
        .await
    }

    /// Fetches a channel record by ID.
    pub async fn get_channel(&self, channel_id: Uuid) -> Result<channel::ChannelRecord> {
        channel::get_channel(&self.pool, channel_id).await
    }

    /// Adds a member to a channel with the given role.
    pub async fn add_member(
        &self,
        channel_id: Uuid,
        pubkey: &[u8],
        role: channel::MemberRole,
        invited_by: Option<&[u8]>,
    ) -> Result<channel::MemberRecord> {
        channel::add_member(&self.pool, channel_id, pubkey, role, invited_by).await
    }

    /// Remove a member. `actor_pubkey` must be an owner/admin or the member themselves.
    pub async fn remove_member(
        &self,
        channel_id: Uuid,
        pubkey: &[u8],
        actor_pubkey: &[u8],
    ) -> Result<()> {
        channel::remove_member(&self.pool, channel_id, pubkey, actor_pubkey).await
    }

    /// Returns `true` if the given pubkey is an active member of the channel.
    pub async fn is_member(&self, channel_id: Uuid, pubkey: &[u8]) -> Result<bool> {
        channel::is_member(&self.pool, channel_id, pubkey).await
    }

    /// Returns all active members of the given channel.
    pub async fn get_members(&self, channel_id: Uuid) -> Result<Vec<channel::MemberRecord>> {
        channel::get_members(&self.pool, channel_id).await
    }

    /// Returns IDs of all channels accessible to the given pubkey.
    pub async fn get_accessible_channel_ids(&self, pubkey: &[u8]) -> Result<Vec<Uuid>> {
        channel::get_accessible_channel_ids(&self.pool, pubkey).await
    }

    /// Returns the canvas content for a channel, if any.
    pub async fn get_canvas(&self, channel_id: Uuid) -> Result<Option<String>> {
        channel::get_canvas(&self.pool, channel_id).await
    }

    /// Sets or clears the canvas content for a channel.
    pub async fn set_canvas(&self, channel_id: Uuid, canvas: Option<&str>) -> Result<()> {
        channel::set_canvas(&self.pool, channel_id, canvas).await
    }

    /// Lists channels, optionally filtered by visibility (`"open"`, `"private"`, etc.).
    pub async fn list_channels(
        &self,
        visibility: Option<&str>,
    ) -> Result<Vec<channel::ChannelRecord>> {
        channel::list_channels(&self.pool, visibility).await
    }

    /// Returns full channel records for all channels accessible to `pubkey`:
    /// open channels plus channels where the user is an active member.
    ///
    /// If `visibility_filter` is `Some("open")` or `Some("private")`, only channels
    /// with that visibility are returned.
    /// If `member_only` is `Some(true)`, only channels the user is a member of are returned.
    pub async fn get_accessible_channels(
        &self,
        pubkey: &[u8],
        visibility_filter: Option<&str>,
        member_only: Option<bool>,
    ) -> Result<Vec<channel::AccessibleChannel>> {
        channel::get_accessible_channels(&self.pool, pubkey, visibility_filter, member_only).await
    }

    /// Returns all bot-role members with aggregated channel names.
    pub async fn get_bot_members(&self) -> Result<Vec<channel::BotMemberRecord>> {
        channel::get_bot_members(&self.pool).await
    }

    /// Bulk-fetch user records by pubkey. Returns empty vec for empty input.
    pub async fn get_users_bulk(&self, pubkeys: &[Vec<u8>]) -> Result<Vec<channel::UserRecord>> {
        channel::get_users_bulk(&self.pool, pubkeys).await
    }

    // ── Channel Metadata ─────────────────────────────────────────────────────

    /// Updates a channel's name and/or description.
    pub async fn update_channel(
        &self,
        channel_id: Uuid,
        updates: channel::ChannelUpdate,
    ) -> Result<channel::ChannelRecord> {
        channel::update_channel(&self.pool, channel_id, updates).await
    }

    /// Sets the topic for a channel.
    pub async fn set_topic(&self, channel_id: Uuid, topic: &str, set_by: &[u8]) -> Result<()> {
        channel::set_topic(&self.pool, channel_id, topic, set_by).await
    }

    /// Sets the purpose for a channel.
    pub async fn set_purpose(&self, channel_id: Uuid, purpose: &str, set_by: &[u8]) -> Result<()> {
        channel::set_purpose(&self.pool, channel_id, purpose, set_by).await
    }

    /// Archives a channel.
    pub async fn archive_channel(&self, channel_id: Uuid) -> Result<()> {
        channel::archive_channel(&self.pool, channel_id).await
    }

    /// Unarchives a channel.
    pub async fn unarchive_channel(&self, channel_id: Uuid) -> Result<()> {
        channel::unarchive_channel(&self.pool, channel_id).await
    }

    /// Soft-delete a channel. Returns `true` if the channel was deleted.
    pub async fn soft_delete_channel(&self, channel_id: Uuid) -> Result<bool> {
        channel::soft_delete_channel(&self.pool, channel_id).await
    }

    /// Returns the count of active members in a channel.
    pub async fn get_member_count(&self, channel_id: Uuid) -> Result<i64> {
        channel::get_member_count(&self.pool, channel_id).await
    }

    /// Bulk-fetch member counts for multiple channels in one query.
    pub async fn get_member_counts_bulk(
        &self,
        channel_ids: &[Uuid],
    ) -> Result<std::collections::HashMap<Uuid, i64>> {
        channel::get_member_counts_bulk(&self.pool, channel_ids).await
    }

    /// Returns the active role of a pubkey in a channel.
    pub async fn get_member_role(&self, channel_id: Uuid, pubkey: &[u8]) -> Result<Option<String>> {
        channel::get_member_role(&self.pool, channel_id, pubkey).await
    }

    // ── Threads ───────────────────────────────────────────────────────────────

    /// Insert a row into `thread_metadata`.
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_thread_metadata(
        &self,
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
        thread::insert_thread_metadata(
            &self.pool,
            event_id,
            event_created_at,
            channel_id,
            parent_event_id,
            parent_event_created_at,
            root_event_id,
            root_event_created_at,
            depth,
            broadcast,
        )
        .await
    }

    /// Fetch replies within a thread, optionally limited by depth.
    pub async fn get_thread_replies(
        &self,
        root_event_id: &[u8],
        depth_limit: Option<u32>,
        limit: u32,
        cursor: Option<&[u8]>,
    ) -> Result<Vec<thread::ThreadReply>> {
        thread::get_thread_replies(&self.pool, root_event_id, depth_limit, limit, cursor).await
    }

    /// Get aggregated thread statistics for a root message.
    pub async fn get_thread_summary(
        &self,
        event_id: &[u8],
    ) -> Result<Option<thread::ThreadSummary>> {
        thread::get_thread_summary(&self.pool, event_id).await
    }

    /// Get top-level channel messages with optional thread summaries.
    pub async fn get_channel_messages_top_level(
        &self,
        channel_id: Uuid,
        limit: u32,
        before: Option<DateTime<Utc>>,
    ) -> Result<Vec<thread::TopLevelMessage>> {
        thread::get_channel_messages_top_level(&self.pool, channel_id, limit, before).await
    }

    /// Decrement reply counts when a thread reply is deleted.
    ///
    /// Decrements `reply_count` on the parent and `descendant_count` on the root
    /// (floor at 0). Mirrors [`thread::increment_reply_count`] exactly.
    pub async fn decrement_reply_count(
        &self,
        parent_event_id: &[u8],
        root_event_id: Option<&[u8]>,
    ) -> Result<()> {
        thread::decrement_reply_count(&self.pool, parent_event_id, root_event_id).await
    }

    /// Fetch a raw thread_metadata row by event ID.
    pub async fn get_thread_metadata_by_event(
        &self,
        event_id: &[u8],
    ) -> Result<Option<thread::ThreadMetadataRecord>> {
        thread::get_thread_metadata_by_event(&self.pool, event_id).await
    }

    // ── DMs ───────────────────────────────────────────────────────────────────

    /// Open (or find existing) a DM channel for the given set of pubkeys.
    pub async fn open_dm(
        &self,
        pubkeys: &[&[u8]],
        created_by: &[u8],
    ) -> Result<(channel::ChannelRecord, bool)> {
        dm::open_dm(&self.pool, pubkeys, created_by).await
    }

    /// List all DM conversations for a given user.
    pub async fn list_dms_for_user(
        &self,
        pubkey: &[u8],
        limit: u32,
        cursor: Option<Uuid>,
    ) -> Result<Vec<dm::DmRecord>> {
        dm::list_dms_for_user(&self.pool, pubkey, limit, cursor).await
    }

    /// Find an existing DM by its participant hash.
    pub async fn find_dm_by_participants(
        &self,
        participant_hash: &[u8],
    ) -> Result<Option<channel::ChannelRecord>> {
        dm::find_dm_by_participants(&self.pool, participant_hash).await
    }

    // ── Reactions ─────────────────────────────────────────────────────────────

    /// Add (or re-activate) a reaction.
    pub async fn add_reaction(
        &self,
        event_id: &[u8],
        event_created_at: DateTime<Utc>,
        pubkey: &[u8],
        emoji: &str,
        reaction_event_id: Option<&[u8]>,
    ) -> Result<bool> {
        reaction::add_reaction(
            &self.pool,
            event_id,
            event_created_at,
            pubkey,
            emoji,
            reaction_event_id,
        )
        .await
    }

    /// Soft-delete a reaction.
    pub async fn remove_reaction(
        &self,
        event_id: &[u8],
        event_created_at: DateTime<Utc>,
        pubkey: &[u8],
        emoji: &str,
    ) -> Result<bool> {
        reaction::remove_reaction(&self.pool, event_id, event_created_at, pubkey, emoji).await
    }

    /// Soft-delete a reaction by the reaction event's own event ID.
    pub async fn remove_reaction_by_source_event_id(
        &self,
        reaction_event_id: &[u8],
    ) -> Result<bool> {
        reaction::remove_reaction_by_source_event_id(&self.pool, reaction_event_id).await
    }

    /// Backfill the source event ID on an active reaction row.
    pub async fn set_reaction_event_id(
        &self,
        event_id: &[u8],
        event_created_at: DateTime<Utc>,
        pubkey: &[u8],
        emoji: &str,
        reaction_event_id: &[u8],
    ) -> Result<bool> {
        reaction::set_reaction_event_id(
            &self.pool,
            event_id,
            event_created_at,
            pubkey,
            emoji,
            reaction_event_id,
        )
        .await
    }

    /// Look up the active reaction row for one actor + emoji + target tuple.
    pub async fn get_active_reaction_record(
        &self,
        event_id: &[u8],
        event_created_at: DateTime<Utc>,
        pubkey: &[u8],
        emoji: &str,
    ) -> Result<Option<reaction::ActiveReactionRecord>> {
        reaction::get_active_reaction_record(&self.pool, event_id, event_created_at, pubkey, emoji)
            .await
    }

    /// Get all active reactions for an event, grouped by emoji.
    pub async fn get_reactions(
        &self,
        event_id: &[u8],
        event_created_at: DateTime<Utc>,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<Vec<reaction::ReactionGroup>> {
        reaction::get_reactions(&self.pool, event_id, event_created_at, limit, cursor).await
    }

    /// Batch-fetch emoji counts for a set of (event_id, event_created_at) pairs.
    pub async fn get_reactions_bulk(
        &self,
        event_ids: &[(&[u8], DateTime<Utc>)],
    ) -> Result<Vec<reaction::BulkReactionEntry>> {
        reaction::get_reactions_bulk(&self.pool, event_ids).await
    }

    // ── Users ────────────────────────────────────────────────────────────────

    /// Ensures a user row exists for the given pubkey (upsert).
    pub async fn ensure_user(&self, pubkey: &[u8]) -> Result<()> {
        user::ensure_user(&self.pool, pubkey).await
    }

    /// Fetch a user profile by pubkey.
    pub async fn get_user(&self, pubkey: &[u8]) -> Result<Option<user::UserProfile>> {
        user::get_user(&self.pool, pubkey).await
    }

    /// Search users by display name, NIP-05 handle, or pubkey prefix.
    pub async fn search_users(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<user::UserSearchProfile>> {
        user::search_users(&self.pool, query, limit).await
    }

    /// Update a user's display_name, avatar_url, about, and/or nip05_handle.
    pub async fn update_user_profile(
        &self,
        pubkey: &[u8],
        display_name: Option<&str>,
        avatar_url: Option<&str>,
        about: Option<&str>,
        nip05_handle: Option<&str>,
    ) -> Result<()> {
        user::update_user_profile(
            &self.pool,
            pubkey,
            display_name,
            avatar_url,
            about,
            nip05_handle,
        )
        .await
    }

    /// Look up a user by their full NIP-05 handle (exact match).
    pub async fn get_user_by_nip05(
        &self,
        local_part: &str,
        domain: &str,
    ) -> Result<Option<user::UserProfile>> {
        user::get_user_by_nip05(&self.pool, local_part, domain).await
    }

    /// Set the owner pubkey for an agent user.
    pub async fn set_agent_owner(&self, agent_pubkey: &[u8], owner_pubkey: &[u8]) -> Result<()> {
        user::set_agent_owner(&self.pool, agent_pubkey, owner_pubkey).await
    }

    /// Get the channel_add_policy and agent_owner_pubkey for a user.
    pub async fn get_agent_channel_policy(
        &self,
        pubkey: &[u8],
    ) -> Result<Option<(String, Option<Vec<u8>>)>> {
        user::get_agent_channel_policy(&self.pool, pubkey).await
    }

    /// Set the channel_add_policy for a user.
    pub async fn set_channel_add_policy(&self, pubkey: &[u8], policy: &str) -> Result<()> {
        user::set_channel_add_policy(&self.pool, pubkey, policy).await
    }

    // ── API Tokens ───────────────────────────────────────────────────────────

    /// Looks up a non-revoked API token by its SHA-256 hash.
    pub async fn get_api_token_by_hash(&self, hash: &[u8]) -> Result<ApiTokenRecord> {
        let row = sqlx::query(
            r#"
            SELECT id, token_hash, owner_pubkey, name, scopes, channel_ids,
                   created_at, expires_at, last_used_at, revoked_at, revoked_by
            FROM api_tokens
            WHERE token_hash = ? AND revoked_at IS NULL
            "#,
        )
        .bind(hash)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(DbError::InvalidData(
            "token not found or revoked".to_string(),
        ))?;

        let id_bytes: Vec<u8> = row.try_get("id")?;
        let id = uuid_from_bytes(&id_bytes)?;

        let scopes_json: serde_json::Value = row.try_get("scopes")?;
        let scopes: Vec<String> = serde_json::from_value(scopes_json)
            .map_err(|e| DbError::InvalidData(format!("scopes JSON: {e}")))?;

        let channel_ids: Option<Vec<Uuid>> = {
            let raw: Option<serde_json::Value> = row.try_get("channel_ids")?;
            match raw {
                None => None,
                Some(v) => {
                    let strings: Vec<String> = serde_json::from_value(v)
                        .map_err(|e| DbError::InvalidData(format!("channel_ids JSON: {e}")))?;
                    let uuids: std::result::Result<Vec<Uuid>, _> =
                        strings.iter().map(|s| s.parse::<Uuid>()).collect();
                    Some(
                        uuids
                            .map_err(|e| DbError::InvalidData(format!("channel_ids UUID: {e}")))?,
                    )
                }
            }
        };

        Ok(ApiTokenRecord {
            id,
            token_hash: row.try_get("token_hash")?,
            owner_pubkey: row.try_get("owner_pubkey")?,
            name: row.try_get("name")?,
            scopes,
            channel_ids,
            created_at: row.try_get("created_at")?,
            expires_at: row.try_get("expires_at")?,
            last_used_at: row.try_get("last_used_at")?,
            revoked_at: row.try_get("revoked_at")?,
        })
    }

    /// Updates the `last_used_at` timestamp for the token with the given hash.
    pub async fn update_token_last_used(&self, hash: &[u8]) -> Result<()> {
        sqlx::query("UPDATE api_tokens SET last_used_at = NOW() WHERE token_hash = ?")
            .bind(hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Creates a new API token record and returns its UUID.
    pub async fn create_api_token(
        &self,
        token_hash: &[u8],
        owner_pubkey: &[u8],
        name: &str,
        scopes: &[String],
        channel_ids: Option<&[Uuid]>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<Uuid> {
        api_token::create_api_token(
            &self.pool,
            token_hash,
            owner_pubkey,
            name,
            scopes,
            channel_ids,
            expires_at,
        )
        .await
    }

    /// Atomic conditional INSERT: create a self-minted token only if the owner has
    /// fewer than 10 active (non-revoked, non-expired) tokens.
    ///
    /// Returns `Ok(Some(uuid))` on success, `Ok(None)` if the limit is exceeded.
    pub async fn create_api_token_if_under_limit(
        &self,
        token_hash: &[u8],
        owner_pubkey: &[u8],
        name: &str,
        scopes: &[String],
        channel_ids: Option<&[Uuid]>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<Option<Uuid>> {
        api_token::create_api_token_if_under_limit(
            &self.pool,
            token_hash,
            owner_pubkey,
            name,
            scopes,
            channel_ids,
            expires_at,
        )
        .await
    }

    /// Look up an API token by its SHA-256 hash, **including revoked tokens**.
    ///
    /// Unlike [`Db::get_api_token_by_hash`], this does not filter on `revoked_at IS NULL`.
    /// Used by the relay to return distinct `token_revoked` vs `invalid_token` errors.
    pub async fn get_api_token_by_hash_including_revoked(
        &self,
        hash: &[u8],
    ) -> Result<Option<ApiTokenRecord>> {
        api_token::get_api_token_by_hash_including_revoked(&self.pool, hash).await
    }

    /// List all tokens (including revoked) for a pubkey, ordered by `created_at DESC`.
    ///
    /// Does not return `token_hash`. Used by `GET /api/tokens`.
    pub async fn list_tokens_by_owner(&self, pubkey: &[u8]) -> Result<Vec<ApiTokenRecord>> {
        api_token::list_tokens_by_owner(&self.pool, pubkey).await
    }

    /// Revoke a single token by ID, scoped to the owner.
    ///
    /// Returns `true` if revoked, `false` if not found, not owned by caller, or already revoked.
    pub async fn revoke_token(
        &self,
        id: Uuid,
        owner_pubkey: &[u8],
        revoked_by: &[u8],
    ) -> Result<bool> {
        api_token::revoke_token(&self.pool, id, owner_pubkey, revoked_by).await
    }

    /// Revoke all active tokens for a pubkey. Skips already-revoked tokens (idempotent).
    ///
    /// Returns the count of newly revoked tokens (0 if all already revoked).
    pub async fn revoke_all_tokens(&self, owner_pubkey: &[u8], revoked_by: &[u8]) -> Result<u64> {
        api_token::revoke_all_tokens(&self.pool, owner_pubkey, revoked_by).await
    }

    /// List all non-revoked, non-expired API tokens.
    ///
    /// Returns a summary view — does not expose raw token hashes.
    pub async fn list_active_tokens(&self) -> Result<Vec<TokenSummary>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, owner_pubkey, scopes, created_at, expires_at
            FROM api_tokens
            WHERE revoked_at IS NULL
              AND (expires_at IS NULL OR expires_at > NOW())
            ORDER BY created_at DESC
            LIMIT 1000
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let id_bytes: Vec<u8> = row.try_get("id")?;
            let id = uuid_from_bytes(&id_bytes)?;

            let scopes_json: serde_json::Value = row.try_get("scopes")?;
            let scopes: Vec<String> = serde_json::from_value(scopes_json)
                .map_err(|e| DbError::InvalidData(format!("scopes JSON: {e}")))?;

            out.push(TokenSummary {
                id,
                name: row.try_get("name")?,
                owner_pubkey: row.try_get("owner_pubkey")?,
                scopes,
                created_at: row.try_get("created_at")?,
                expires_at: row.try_get("expires_at")?,
            });
        }
        Ok(out)
    }

    // ── Pubkey Allowlist ─────────────────────────────────────────────────────

    /// Returns `true` if the given pubkey is in the allowlist.
    pub async fn is_pubkey_allowed(&self, pubkey: &[u8]) -> Result<bool> {
        let row = sqlx::query("SELECT 1 FROM pubkey_allowlist WHERE pubkey = ?")
            .bind(pubkey)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.is_some())
    }

    // ── Addressable Event Replacement ────────────────────────────────────────

    /// Atomically replace the active addressable event for a (kind, pubkey, channel)
    /// tuple. Insert-first ordering guarantees at least one active row at all times:
    ///
    /// 1. INSERT the new event (IGNORE on duplicate ID — a no-op, not an error).
    /// 2. Soft-delete all OTHER active events for the same address, excluding the
    ///    just-inserted row by ID.
    ///
    /// If the INSERT was a no-op (duplicate event ID), the delete step is skipped
    /// entirely — the existing row is already the latest state.
    ///
    /// Used for NIP-29 group discovery events (39000/39001/39002) which should
    /// only have one active version per group.
    ///
    /// Returns `(StoredEvent, was_inserted)` — `was_inserted` is `false` on duplicate.
    pub async fn replace_addressable_event(
        &self,
        event: &nostr::Event,
        channel_id: Uuid,
    ) -> Result<(StoredEvent, bool)> {
        use sprout_core::kind::event_kind_i32;

        let id_bytes = event.id.as_bytes();
        let pubkey_bytes = event.pubkey.to_bytes();
        let sig_bytes = event.sig.serialize();
        let tags_json = serde_json::to_value(&event.tags)?;
        let kind_i32 = event_kind_i32(event);
        let created_at_secs = event.created_at.as_u64() as i64;
        let created_at = DateTime::from_timestamp(created_at_secs, 0)
            .ok_or(crate::error::DbError::InvalidTimestamp(created_at_secs))?;
        let received_at = Utc::now();
        let channel_id_bytes: [u8; 16] = *channel_id.as_bytes();

        let mut tx = self.pool.begin().await?;

        // Step 0: Acquire an exclusive next-key lock on the logical address
        // (kind, pubkey, channel_id) via idx_events_addressable. This serializes
        // concurrent replacements — a second transaction blocks here until the
        // first commits. The index is required so InnoDB can take a gap lock
        // even on a cold address (no existing rows), preventing two concurrent
        // first-time emissions from both inserting.
        sqlx::query(
            "SELECT id FROM events \
             WHERE kind = ? AND pubkey = ? AND channel_id = ? AND deleted_at IS NULL \
             FOR UPDATE",
        )
        .bind(kind_i32)
        .bind(pubkey_bytes.as_slice())
        .bind(channel_id_bytes.as_slice())
        .fetch_all(&mut *tx)
        .await?;

        // Step 1: Insert the new event first — guarantees at least one active row.
        let result = sqlx::query(
            "INSERT IGNORE INTO events \
             (id, pubkey, created_at, kind, tags, content, sig, received_at, channel_id) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id_bytes.as_slice())
        .bind(pubkey_bytes.as_slice())
        .bind(created_at)
        .bind(kind_i32)
        .bind(&tags_json)
        .bind(&event.content)
        .bind(sig_bytes.as_slice())
        .bind(received_at)
        .bind(channel_id_bytes.as_slice())
        .execute(&mut *tx)
        .await?;

        let was_inserted = result.rows_affected() > 0;

        // Step 2: Soft-delete previous active events for this address, excluding
        // the row we just inserted. Skipped on duplicate (was_inserted == false)
        // because the existing row is already the current state.
        if was_inserted {
            sqlx::query(
                "UPDATE events SET deleted_at = NOW(6) \
                 WHERE kind = ? AND pubkey = ? AND channel_id = ? \
                 AND deleted_at IS NULL AND id != ?",
            )
            .bind(kind_i32)
            .bind(pubkey_bytes.as_slice())
            .bind(channel_id_bytes.as_slice())
            .bind(id_bytes.as_slice())
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        if was_inserted {
            if let Err(e) = insert_mentions(&self.pool, event, Some(channel_id)).await {
                tracing::warn!(event_id = %event.id, "Failed to insert mentions: {e}");
            }
        }

        Ok((
            StoredEvent::with_received_at(event.clone(), received_at, Some(channel_id), true),
            was_inserted,
        ))
    }

    /// Soft-delete all NIP-29 discovery events (39000/39001/39002) for a channel.
    ///
    /// Used during group deletion to clean up addressable discovery events.
    pub async fn soft_delete_discovery_events(
        &self,
        channel_id: Uuid,
        relay_pubkey: &[u8],
    ) -> Result<u64> {
        let channel_id_bytes = channel_id.as_bytes().as_slice().to_vec();
        let result = sqlx::query(
            "UPDATE events SET deleted_at = NOW(6) \
             WHERE kind IN (39000, 39001, 39002) AND pubkey = ? AND channel_id = ? \
             AND deleted_at IS NULL",
        )
        .bind(relay_pubkey)
        .bind(&channel_id_bytes)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    // ── Partitions ───────────────────────────────────────────────────────────

    /// Ensures monthly partition tables exist for the next `months_ahead` months.
    pub async fn ensure_future_partitions(&self, months_ahead: u32) -> Result<()> {
        partition::ensure_future_partitions(&self.pool, months_ahead).await
    }

    // ── Workflows ─────────────────────────────────────────────────────────────

    /// Creates a new workflow definition and returns its UUID.
    pub async fn create_workflow(
        &self,
        channel_id: Option<Uuid>,
        owner_pubkey: &[u8],
        name: &str,
        definition_json: &str,
        definition_hash: &[u8],
    ) -> Result<Uuid> {
        workflow::create_workflow(
            &self.pool,
            channel_id,
            owner_pubkey,
            name,
            definition_json,
            definition_hash,
        )
        .await
    }

    /// Fetches a workflow definition by ID.
    pub async fn get_workflow(&self, id: Uuid) -> Result<workflow::WorkflowRecord> {
        workflow::get_workflow(&self.pool, id).await
    }

    /// Lists all workflows for a channel (enabled and disabled).
    pub async fn list_channel_workflows(
        &self,
        channel_id: Uuid,
    ) -> Result<Vec<workflow::WorkflowRecord>> {
        workflow::list_channel_workflows(&self.pool, channel_id, None, None).await
    }

    /// Lists only enabled workflows for a channel.
    pub async fn list_enabled_channel_workflows(
        &self,
        channel_id: Uuid,
    ) -> Result<Vec<workflow::WorkflowRecord>> {
        workflow::list_enabled_channel_workflows(&self.pool, channel_id).await
    }

    /// Lists all active, enabled workflows across all channels.
    ///
    /// Used by the cron scheduler.
    pub async fn list_all_enabled_workflows(&self) -> Result<Vec<workflow::WorkflowRecord>> {
        workflow::list_all_enabled_workflows(&self.pool).await
    }

    /// Updates a workflow's name and definition.
    pub async fn update_workflow(
        &self,
        id: Uuid,
        name: &str,
        definition_json: &str,
        definition_hash: &[u8],
    ) -> Result<()> {
        workflow::update_workflow(&self.pool, id, name, definition_json, definition_hash).await
    }

    /// Deletes a workflow definition by ID.
    pub async fn delete_workflow(&self, id: Uuid) -> Result<()> {
        workflow::delete_workflow(&self.pool, id).await
    }

    /// Creates a new workflow run record and returns its UUID.
    pub async fn create_workflow_run(
        &self,
        workflow_id: Uuid,
        trigger_event_id: Option<&[u8]>,
        trigger_context: Option<&serde_json::Value>,
    ) -> Result<Uuid> {
        workflow::create_workflow_run(&self.pool, workflow_id, trigger_event_id, trigger_context)
            .await
    }

    /// Fetches a workflow run record by ID.
    pub async fn get_workflow_run(&self, id: Uuid) -> Result<workflow::WorkflowRunRecord> {
        workflow::get_workflow_run(&self.pool, id).await
    }

    /// Lists the most recent runs for a workflow, up to `limit`.
    pub async fn list_workflow_runs(
        &self,
        workflow_id: Uuid,
        limit: i64,
    ) -> Result<Vec<workflow::WorkflowRunRecord>> {
        workflow::list_workflow_runs(&self.pool, workflow_id, limit).await
    }

    /// Updates the enabled/disabled status of a workflow definition.
    pub async fn update_workflow_status(
        &self,
        id: Uuid,
        status: workflow::WorkflowStatus,
    ) -> Result<()> {
        workflow::update_workflow_status(&self.pool, id, status).await
    }

    /// Enables or disables a workflow.
    pub async fn set_workflow_enabled(&self, id: Uuid, enabled: bool) -> Result<()> {
        workflow::set_workflow_enabled(&self.pool, id, enabled).await
    }

    /// Updates a workflow run's status, current step index, execution trace, and error.
    pub async fn update_workflow_run(
        &self,
        id: Uuid,
        status: workflow::RunStatus,
        current_step: i32,
        trace: &serde_json::Value,
        error: Option<&str>,
    ) -> Result<()> {
        workflow::update_workflow_run(&self.pool, id, status, current_step, trace, error).await
    }

    /// Creates a pending approval record for a workflow step.
    pub async fn create_approval(&self, params: workflow::CreateApprovalParams<'_>) -> Result<()> {
        workflow::create_approval(&self.pool, params).await
    }

    /// Fetches an approval record by its token string.
    pub async fn get_approval(&self, token: &str) -> Result<workflow::ApprovalRecord> {
        workflow::get_approval(&self.pool, token).await
    }

    /// Updates an approval's status. Returns `true` if the row was updated.
    pub async fn update_approval(
        &self,
        token: &str,
        status: workflow::ApprovalStatus,
        approver_pubkey: Option<&[u8]>,
        note: Option<&str>,
    ) -> Result<bool> {
        workflow::update_approval(&self.pool, token, status, approver_pubkey, note).await
    }
}

/// Full API token record (for auth middleware use).
#[derive(Debug, Clone)]
pub struct ApiTokenRecord {
    /// Unique token identifier.
    pub id: Uuid,
    /// SHA-256 hash of the raw token bytes.
    pub token_hash: Vec<u8>,
    /// Compressed public key bytes of the token owner.
    pub owner_pubkey: Vec<u8>,
    /// Human-readable token name.
    pub name: String,
    /// Permission scopes granted to this token.
    pub scopes: Vec<String>,
    /// Optional channel restriction; `None` means all channels.
    pub channel_ids: Option<Vec<Uuid>>,
    /// When the token was created.
    pub created_at: DateTime<Utc>,
    /// Optional expiry timestamp; `None` means no expiry.
    pub expires_at: Option<DateTime<Utc>>,
    /// Last time this token was used for authentication.
    pub last_used_at: Option<DateTime<Utc>>,
    /// When the token was revoked, if applicable.
    pub revoked_at: Option<DateTime<Utc>>,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind};

    const TEST_DB_URL: &str = "mysql://sprout:sprout_dev@localhost:3306/sprout";

    async fn setup_db() -> Db {
        let pool = MySqlPool::connect(TEST_DB_URL)
            .await
            .expect("connect to test DB");
        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("migrate");
        Db::from_pool(pool)
    }

    fn make_event(kind: Kind) -> nostr::Event {
        let keys = Keys::generate();
        EventBuilder::new(kind, "test content", [])
            .sign_with_keys(&keys)
            .expect("sign")
    }

    async fn cleanup_channel(db: &Db, channel_id: Uuid) {
        let id = channel_id.as_bytes().to_vec();
        sqlx::query("DELETE FROM events WHERE channel_id = ?")
            .bind(&id)
            .execute(&db.pool)
            .await
            .ok();
        sqlx::query("DELETE FROM channel_members WHERE channel_id = ?")
            .bind(&id)
            .execute(&db.pool)
            .await
            .ok();
        sqlx::query("DELETE FROM channels WHERE id = ?")
            .bind(&id)
            .execute(&db.pool)
            .await
            .ok();
    }

    async fn cleanup_event(db: &Db, event_id: &[u8]) {
        sqlx::query("DELETE FROM events WHERE id = ?")
            .bind(event_id)
            .execute(&db.pool)
            .await
            .ok();
    }

    #[tokio::test]
    #[ignore = "requires MySQL"]
    async fn insert_and_retrieve_event() {
        let db = setup_db().await;
        let event = make_event(Kind::TextNote);
        let event_id = event.id.as_bytes().to_vec();

        let (stored, was_inserted) = db.insert_event(&event, None).await.expect("insert");
        assert_eq!(stored.event.id, event.id);
        assert!(stored.is_verified());
        assert!(was_inserted);

        let retrieved = db
            .get_event_by_id(&event_id)
            .await
            .expect("get")
            .expect("exists");
        assert_eq!(retrieved.event.id, event.id);

        cleanup_event(&db, &event_id).await;
    }

    #[tokio::test]
    #[ignore = "requires MySQL"]
    async fn duplicate_insert_is_noop() {
        let db = setup_db().await;
        let event = make_event(Kind::TextNote);
        let event_id = event.id.as_bytes().to_vec();

        let (_, first) = db.insert_event(&event, None).await.expect("first insert");
        assert!(first);
        let (_, second) = db.insert_event(&event, None).await.expect("second insert");
        assert!(!second);

        let cnt: i64 = sqlx::query("SELECT COUNT(*) as cnt FROM events WHERE id = ?")
            .bind(&event_id)
            .fetch_one(&db.pool)
            .await
            .expect("count")
            .try_get("cnt")
            .unwrap();
        assert_eq!(cnt, 1);

        cleanup_event(&db, &event_id).await;
    }

    #[tokio::test]
    #[ignore = "requires MySQL"]
    async fn auth_event_rejected() {
        let db = setup_db().await;
        let event = make_event(Kind::from(22242u16));
        let result = db.insert_event(&event, None).await;
        assert!(matches!(result, Err(DbError::AuthEventRejected)));
    }

    #[tokio::test]
    #[ignore = "requires MySQL"]
    async fn query_events_by_channel_and_kind() {
        let db = setup_db().await;
        let keys = Keys::generate();
        let pubkey = keys.public_key().serialize().to_vec();

        let channel = db
            .create_channel(
                "test-query",
                channel::ChannelType::Stream,
                channel::ChannelVisibility::Open,
                None,
                &pubkey,
            )
            .await
            .expect("create channel");

        let ev1 = make_event(Kind::TextNote);
        let ev2 = make_event(Kind::TextNote);
        let ev3 = make_event(Kind::Metadata);
        let ev3_id = ev3.id.as_bytes().to_vec();

        db.insert_event(&ev1, Some(channel.id)).await.expect("ev1");
        db.insert_event(&ev2, Some(channel.id)).await.expect("ev2");
        db.insert_event(&ev3, None).await.expect("ev3");

        let by_channel = db
            .query_events(&EventQuery {
                channel_id: Some(channel.id),
                ..Default::default()
            })
            .await
            .expect("query");
        assert_eq!(by_channel.len(), 2);

        let by_kind = db
            .query_events(&EventQuery {
                kinds: Some(vec![1i32]),
                ..Default::default()
            })
            .await
            .expect("query by kind");
        assert!(by_kind.iter().all(|e| e.event.kind.as_u16() == 1));

        cleanup_channel(&db, channel.id).await;
        cleanup_event(&db, &ev3_id).await;
    }

    #[tokio::test]
    #[ignore = "requires MySQL"]
    async fn query_events_pagination() {
        let db = setup_db().await;
        let keys = Keys::generate();
        let pubkey = keys.public_key().serialize().to_vec();
        let channel = db
            .create_channel(
                "test-pagination",
                channel::ChannelType::Stream,
                channel::ChannelVisibility::Open,
                None,
                &pubkey,
            )
            .await
            .expect("create channel");

        for i in 0..5 {
            let ev = EventBuilder::new(Kind::TextNote, format!("msg {i}"), [])
                .sign_with_keys(&keys)
                .expect("sign");
            db.insert_event(&ev, Some(channel.id))
                .await
                .expect("insert");
        }

        let page1 = db
            .query_events(&EventQuery {
                channel_id: Some(channel.id),
                limit: Some(2),
                offset: Some(0),
                ..Default::default()
            })
            .await
            .expect("page1");
        let page2 = db
            .query_events(&EventQuery {
                channel_id: Some(channel.id),
                limit: Some(2),
                offset: Some(2),
                ..Default::default()
            })
            .await
            .expect("page2");
        assert_eq!(page1.len(), 2);
        assert_eq!(page2.len(), 2);
        let p1_ids: Vec<_> = page1.iter().map(|e| e.event.id).collect();
        for e in &page2 {
            assert!(!p1_ids.contains(&e.event.id));
        }

        cleanup_channel(&db, channel.id).await;
    }

    #[tokio::test]
    #[ignore = "requires MySQL"]
    async fn channel_create_get_membership() {
        let db = setup_db().await;
        let owner_keys = Keys::generate();
        let owner = owner_keys.public_key().serialize().to_vec();
        let member_keys = Keys::generate();
        let member = member_keys.public_key().serialize().to_vec();

        let channel = db
            .create_channel(
                "test-membership",
                channel::ChannelType::Stream,
                channel::ChannelVisibility::Private,
                Some("desc"),
                &owner,
            )
            .await
            .expect("create");
        assert_eq!(channel.name, "test-membership");
        assert_eq!(channel.description, Some("desc".to_string()));
        assert!(db.is_member(channel.id, &owner).await.unwrap());

        db.add_member(
            channel.id,
            &member,
            channel::MemberRole::Member,
            Some(&owner),
        )
        .await
        .expect("add member");
        assert!(db.is_member(channel.id, &member).await.unwrap());

        let members = db.get_members(channel.id).await.expect("get members");
        assert_eq!(members.len(), 2);

        db.remove_member(channel.id, &member, &owner)
            .await
            .expect("remove");
        assert!(!db.is_member(channel.id, &member).await.unwrap());

        cleanup_channel(&db, channel.id).await;
    }

    #[tokio::test]
    #[ignore = "requires MySQL"]
    async fn open_channel_join_no_invite() {
        let db = setup_db().await;
        let creator = Keys::generate().public_key().serialize().to_vec();
        let joiner = Keys::generate().public_key().serialize().to_vec();

        let channel = db
            .create_channel(
                "test-open",
                channel::ChannelType::Stream,
                channel::ChannelVisibility::Open,
                None,
                &creator,
            )
            .await
            .expect("create");

        db.add_member(channel.id, &joiner, channel::MemberRole::Member, None)
            .await
            .expect("join open");
        assert!(db.is_member(channel.id, &joiner).await.unwrap());

        cleanup_channel(&db, channel.id).await;
    }

    #[tokio::test]
    #[ignore = "requires MySQL"]
    async fn private_channel_requires_invite() {
        let db = setup_db().await;
        let creator = Keys::generate().public_key().serialize().to_vec();
        let outsider = Keys::generate().public_key().serialize().to_vec();

        let channel = db
            .create_channel(
                "test-private",
                channel::ChannelType::Stream,
                channel::ChannelVisibility::Private,
                None,
                &creator,
            )
            .await
            .expect("create");

        let result = db
            .add_member(channel.id, &outsider, channel::MemberRole::Member, None)
            .await;
        assert!(matches!(result, Err(DbError::AccessDenied(_))));
        assert!(!db.is_member(channel.id, &outsider).await.unwrap());

        cleanup_channel(&db, channel.id).await;
    }

    #[tokio::test]
    #[ignore = "requires MySQL"]
    async fn remove_member_requires_authorization() {
        let db = setup_db().await;
        let owner = Keys::generate().public_key().serialize().to_vec();
        let member = Keys::generate().public_key().serialize().to_vec();
        let rando = Keys::generate().public_key().serialize().to_vec();

        let channel = db
            .create_channel(
                "test-remove-auth",
                channel::ChannelType::Stream,
                channel::ChannelVisibility::Private,
                None,
                &owner,
            )
            .await
            .expect("create");

        db.add_member(channel.id, &owner, channel::MemberRole::Owner, Some(&owner))
            .await
            .expect("add owner");
        db.add_member(
            channel.id,
            &member,
            channel::MemberRole::Member,
            Some(&owner),
        )
        .await
        .expect("add member");
        db.add_member(
            channel.id,
            &rando,
            channel::MemberRole::Member,
            Some(&owner),
        )
        .await
        .expect("add rando");

        let result = db.remove_member(channel.id, &member, &rando).await;
        assert!(matches!(result, Err(DbError::AccessDenied(_))));

        db.remove_member(channel.id, &member, &owner)
            .await
            .expect("owner removes");
        assert!(!db.is_member(channel.id, &member).await.unwrap());

        db.remove_member(channel.id, &rando, &rando)
            .await
            .expect("self-remove");
        assert!(!db.is_member(channel.id, &rando).await.unwrap());

        cleanup_channel(&db, channel.id).await;
    }
}
