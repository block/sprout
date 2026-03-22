//! Channel CRUD and membership management.
//!
//! Channels have two visibility modes:
//! - `open`: searchable, anyone can join
//! - `private`: hidden, invite-only

use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::error::{DbError, Result};

/// Whether a channel is publicly visible or invite-only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelVisibility {
    /// Searchable; anyone can join without an invite.
    Open,
    /// Hidden; requires an invite to join.
    Private,
}

impl ChannelVisibility {
    /// Returns the canonical string representation stored in the database.
    pub fn as_str(&self) -> &'static str {
        match self {
            ChannelVisibility::Open => "open",
            ChannelVisibility::Private => "private",
        }
    }
}

impl std::str::FromStr for ChannelVisibility {
    type Err = crate::error::DbError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "open" => Ok(ChannelVisibility::Open),
            "private" => Ok(ChannelVisibility::Private),
            other => Err(crate::error::DbError::InvalidData(format!(
                "unknown channel visibility: {other:?}"
            ))),
        }
    }
}

/// The functional type of a channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelType {
    /// Linear message stream (the default channel type).
    Stream,
    /// Threaded forum-style discussion.
    Forum,
    /// Direct message conversation.
    Dm,
    /// Internal workflow execution channel.
    Workflow,
}

impl ChannelType {
    /// Returns the canonical string representation stored in the database.
    pub fn as_str(&self) -> &'static str {
        match self {
            ChannelType::Stream => "stream",
            ChannelType::Forum => "forum",
            ChannelType::Dm => "dm",
            ChannelType::Workflow => "workflow",
        }
    }
}

impl std::str::FromStr for ChannelType {
    type Err = crate::error::DbError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "stream" => Ok(ChannelType::Stream),
            "forum" => Ok(ChannelType::Forum),
            "dm" => Ok(ChannelType::Dm),
            "workflow" => Ok(ChannelType::Workflow),
            other => Err(crate::error::DbError::InvalidData(format!(
                "unknown channel type: {other:?}"
            ))),
        }
    }
}

/// A member's role within a channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemberRole {
    /// Full control — can manage members and delete the channel.
    Owner,
    /// Can manage members and channel settings.
    Admin,
    /// Standard participant.
    Member,
    /// Read-only external participant.
    Guest,
    /// Automated agent or integration.
    Bot,
}

impl MemberRole {
    /// Returns the canonical string representation stored in the database.
    pub fn as_str(&self) -> &'static str {
        match self {
            MemberRole::Owner => "owner",
            MemberRole::Admin => "admin",
            MemberRole::Member => "member",
            MemberRole::Guest => "guest",
            MemberRole::Bot => "bot",
        }
    }

    /// Elevated roles that only existing owners/admins may grant.
    fn is_elevated(&self) -> bool {
        matches!(self, MemberRole::Owner | MemberRole::Admin)
    }
}

impl std::str::FromStr for MemberRole {
    type Err = crate::error::DbError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "owner" => Ok(MemberRole::Owner),
            "admin" => Ok(MemberRole::Admin),
            "member" => Ok(MemberRole::Member),
            "guest" => Ok(MemberRole::Guest),
            "bot" => Ok(MemberRole::Bot),
            other => Err(crate::error::DbError::InvalidData(format!(
                "unknown member role: {other:?}"
            ))),
        }
    }
}

/// A channel row as returned from the database.
#[derive(Debug, Clone)]
pub struct ChannelRecord {
    /// Unique channel identifier.
    pub id: Uuid,
    /// Human-readable channel name.
    pub name: String,
    /// Channel type string (e.g. `"stream"`, `"forum"`, `"dm"`).
    pub channel_type: String,
    /// Visibility string (`"open"` or `"private"`).
    pub visibility: String,
    /// Optional channel description.
    pub description: Option<String>,
    /// Optional canvas (rich document) content.
    pub canvas: Option<String>,
    /// Compressed public key bytes of the channel creator.
    pub created_by: Vec<u8>,
    /// When the channel was created.
    pub created_at: DateTime<Utc>,
    /// When the channel was last updated.
    pub updated_at: DateTime<Utc>,
    /// When the channel was archived, if applicable.
    pub archived_at: Option<DateTime<Utc>>,
    /// When the channel was soft-deleted, if applicable.
    pub deleted_at: Option<DateTime<Utc>>,
    /// NIP-29 group ID for external Nostr clients.
    pub nip29_group_id: Option<String>,
    /// Whether posts must be associated with a topic.
    pub topic_required: bool,
    /// Optional cap on the number of members.
    pub max_members: Option<i32>,
    /// Current channel topic (short, visible in header).
    pub topic: Option<String>,
    /// Compressed public key bytes of the user who last set the topic.
    pub topic_set_by: Option<Vec<u8>>,
    /// When the topic was last set.
    pub topic_set_at: Option<DateTime<Utc>>,
    /// Channel purpose / description of intent.
    pub purpose: Option<String>,
    /// Compressed public key bytes of the user who last set the purpose.
    pub purpose_set_by: Option<Vec<u8>>,
    /// When the purpose was last set.
    pub purpose_set_at: Option<DateTime<Utc>>,
}

/// A channel membership row as returned from the database.
#[derive(Debug, Clone)]
pub struct MemberRecord {
    /// The channel this membership belongs to.
    pub channel_id: Uuid,
    /// Compressed public key bytes of the member.
    pub pubkey: Vec<u8>,
    /// Role string (e.g. `"owner"`, `"member"`, `"bot"`).
    pub role: String,
    /// When the member joined.
    pub joined_at: DateTime<Utc>,
    /// Who invited this member, if applicable.
    pub invited_by: Option<Vec<u8>>,
    /// When the member was removed, if applicable.
    pub removed_at: Option<DateTime<Utc>>,
}

/// Creates a new channel, bootstraps the creator as owner, and returns the record.
pub async fn create_channel(
    pool: &PgPool,
    name: &str,
    channel_type: ChannelType,
    visibility: ChannelVisibility,
    description: Option<&str>,
    created_by: &[u8],
) -> Result<ChannelRecord> {
    if created_by.len() != 32 {
        return Err(DbError::InvalidData(format!(
            "pubkey must be 32 bytes, got {}",
            created_by.len()
        )));
    }

    let id = Uuid::new_v4();

    let mut tx = pool.begin().await?;

    sqlx::query(
        r#"
        INSERT INTO channels (id, name, channel_type, visibility, description, created_by)
        VALUES ($1, $2, $3::channel_type, $4::channel_visibility, $5, $6)
        "#,
    )
    .bind(id)
    .bind(name)
    .bind(channel_type.as_str())
    .bind(visibility.as_str())
    .bind(description)
    .bind(created_by)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO channel_members (channel_id, pubkey, role, invited_by)
        VALUES ($1, $2, 'owner', $3)
        ON CONFLICT (channel_id, pubkey) DO UPDATE SET
            removed_at = NULL,
            removed_by = NULL,
            role = EXCLUDED.role
        "#,
    )
    .bind(id)
    .bind(created_by)
    .bind(created_by)
    .execute(&mut *tx)
    .await?;

    let row = sqlx::query(
        r#"
        SELECT id, name, channel_type::text AS channel_type, visibility::text AS visibility,
               description, canvas,
               created_by, created_at, updated_at, archived_at, deleted_at,
               nip29_group_id, topic_required, max_members,
               topic, topic_set_by, topic_set_at,
               purpose, purpose_set_by, purpose_set_at
        FROM channels WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;

    let record = row_to_channel_record(row)?;
    tx.commit().await?;
    Ok(record)
}

/// Fetches a channel record by ID. Returns `ChannelNotFound` if missing or deleted.
pub async fn get_channel(pool: &PgPool, channel_id: Uuid) -> Result<ChannelRecord> {
    let row = sqlx::query(
        r#"
        SELECT id, name, channel_type::text AS channel_type, visibility::text AS visibility,
               description, canvas,
               created_by, created_at, updated_at, archived_at, deleted_at,
               nip29_group_id, topic_required, max_members,
               topic, topic_set_by, topic_set_at,
               purpose, purpose_set_by, purpose_set_at
        FROM channels WHERE id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(channel_id)
    .fetch_optional(pool)
    .await?
    .ok_or(DbError::ChannelNotFound(channel_id))?;

    row_to_channel_record(row)
}

/// Returns the canvas content for a channel, if any.
pub async fn get_canvas(pool: &PgPool, channel_id: Uuid) -> Result<Option<String>> {
    let row = sqlx::query("SELECT canvas FROM channels WHERE id = $1 AND deleted_at IS NULL")
        .bind(channel_id)
        .fetch_optional(pool)
        .await?
        .ok_or(DbError::ChannelNotFound(channel_id))?;
    Ok(row.try_get("canvas")?)
}

/// Sets or clears the canvas content for a channel.
pub async fn set_canvas(pool: &PgPool, channel_id: Uuid, canvas: Option<&str>) -> Result<()> {
    let rows = sqlx::query("UPDATE channels SET canvas = $1 WHERE id = $2 AND deleted_at IS NULL")
        .bind(canvas)
        .bind(channel_id)
        .execute(pool)
        .await?;
    if rows.rows_affected() == 0 {
        return Err(DbError::ChannelNotFound(channel_id));
    }
    Ok(())
}

/// Add a member to a channel.
///
/// Role enforcement:
/// - Open channels: `invited_by` is optional; role is forced to `Member` regardless of
///   what the caller passes — callers cannot self-assign elevated roles.
/// - Private channels: requires an `invited_by` who is an active owner/admin.
/// - Elevated roles (`Owner`, `Admin`) may only be granted by an existing owner/admin,
///   even on open channels.
///
/// The entire check-then-insert sequence runs inside a transaction to prevent TOCTOU
/// races (e.g. the inviter being removed between the role check and the INSERT).
pub async fn add_member(
    pool: &PgPool,
    channel_id: Uuid,
    pubkey: &[u8],
    role: MemberRole,
    invited_by: Option<&[u8]>,
) -> Result<MemberRecord> {
    if pubkey.len() != 32 {
        return Err(DbError::InvalidData(format!(
            "pubkey must be 32 bytes, got {}",
            pubkey.len()
        )));
    }

    let mut tx = pool.begin().await?;

    let channel = get_channel_tx(&mut tx, channel_id).await?;

    let effective_role = if channel.visibility == "private" {
        let inviter = invited_by.ok_or_else(|| {
            DbError::AccessDenied("private channel requires an invite".to_string())
        })?;

        // Bootstrap: channel creator may add themselves as the first member.
        let is_creator_bootstrap = inviter == pubkey && inviter == channel.created_by.as_slice();

        if !is_creator_bootstrap {
            let inviter_role_str = get_active_role_tx(&mut tx, channel_id, inviter)
                .await?
                .ok_or_else(|| {
                    DbError::AccessDenied("inviter is not an active member".to_string())
                })?;

            let inviter_role: MemberRole = inviter_role_str.parse().map_err(|_| {
                DbError::InvalidData(format!("invalid role in database: {inviter_role_str}"))
            })?;

            if !inviter_role.is_elevated() {
                return Err(DbError::AccessDenied(
                    "inviter must be owner or admin".to_string(),
                ));
            }

            // Only owners/admins may grant elevated roles (already verified above — kept for clarity).
            if role.is_elevated() && !inviter_role.is_elevated() {
                return Err(DbError::AccessDenied(
                    "only owners/admins may grant elevated roles".to_string(),
                ));
            }
        }

        role
    } else {
        // Open channel: anyone may join, but only existing owners/admins may grant
        // elevated roles. Self-join always gets Member.
        if role.is_elevated() {
            let granter_role = match invited_by {
                Some(inv) => get_active_role_tx(&mut tx, channel_id, inv).await?,
                None => None,
            };
            match granter_role.as_deref() {
                Some("owner") | Some("admin") => role,
                _ => {
                    return Err(DbError::AccessDenied(
                        "only owners/admins may grant elevated roles".to_string(),
                    ))
                }
            }
        } else {
            role
        }
    };

    sqlx::query(
        r#"
        INSERT INTO channel_members (channel_id, pubkey, role, invited_by)
        VALUES ($1, $2, $3::member_role, $4)
        ON CONFLICT (channel_id, pubkey) DO UPDATE SET
            removed_at = NULL,
            removed_by = NULL,
            role = EXCLUDED.role
        "#,
    )
    .bind(channel_id)
    .bind(pubkey)
    .bind(effective_role.as_str())
    .bind(invited_by)
    .execute(&mut *tx)
    .await?;

    let row = sqlx::query(
        r#"
        SELECT channel_id, pubkey, role::text AS role, joined_at, invited_by, removed_at
        FROM channel_members WHERE channel_id = $1 AND pubkey = $2
        "#,
    )
    .bind(channel_id)
    .bind(pubkey)
    .fetch_one(&mut *tx)
    .await?;

    let record = row_to_member_record(row)?;
    tx.commit().await?;
    Ok(record)
}

/// Remove a member from a channel (soft delete).
///
/// `actor_pubkey` must be an active owner/admin, or the member removing themselves.
///
/// Returns `Err(DbError::MemberNotFound)` if the target is not an active member.
/// The authorization check and the UPDATE run inside a transaction to prevent a
/// TOCTOU race where the actor's role changes between the check and the update.
pub async fn remove_member(
    pool: &PgPool,
    channel_id: Uuid,
    pubkey: &[u8],
    actor_pubkey: &[u8],
) -> Result<()> {
    let mut tx = pool.begin().await?;

    let is_self_remove = pubkey == actor_pubkey;
    if !is_self_remove {
        let actor_role_str = get_active_role_tx(&mut tx, channel_id, actor_pubkey)
            .await?
            .ok_or_else(|| DbError::AccessDenied("actor is not an active member".to_string()))?;
        let actor_role: MemberRole = actor_role_str.parse().map_err(|_| {
            DbError::InvalidData(format!("invalid role in database: {actor_role_str}"))
        })?;
        if !actor_role.is_elevated() {
            return Err(DbError::AccessDenied(
                "only owners/admins may remove other members".to_string(),
            ));
        }
    }

    // Defense-in-depth: prevent removing the last owner regardless of caller.
    // Callers (REST handlers, NIP-29 handlers) also check this, but the DB
    // layer enforces it as the final safety net.
    let target_role = get_active_role_tx(&mut tx, channel_id, pubkey).await?;
    if target_role.as_deref() == Some("owner") {
        let row = sqlx::query(
            "SELECT COUNT(*) as cnt FROM channel_members \
             WHERE channel_id = $1 AND role = 'owner' AND removed_at IS NULL",
        )
        .bind(channel_id)
        .fetch_one(&mut *tx)
        .await?;
        let owner_count: i64 = row.try_get("cnt")?;
        if owner_count <= 1 {
            return Err(DbError::AccessDenied(
                "cannot remove the last owner — transfer ownership first".to_string(),
            ));
        }
    }

    let result = sqlx::query(
        r#"
        UPDATE channel_members
        SET removed_at = NOW(), removed_by = $1
        WHERE channel_id = $2 AND pubkey = $3 AND removed_at IS NULL
        "#,
    )
    .bind(actor_pubkey)
    .bind(channel_id)
    .bind(pubkey)
    .execute(&mut *tx)
    .await?;

    if result.rows_affected() == 0 {
        return Err(DbError::MemberNotFound(channel_id));
    }

    tx.commit().await?;
    Ok(())
}

/// Returns `true` if the given pubkey is an active member of the channel.
pub async fn is_member(pool: &PgPool, channel_id: Uuid, pubkey: &[u8]) -> Result<bool> {
    let row = sqlx::query(
        "SELECT COUNT(*) as cnt FROM channel_members cm \
         JOIN channels c ON cm.channel_id = c.id AND c.deleted_at IS NULL \
         WHERE cm.channel_id = $1 AND cm.pubkey = $2 AND cm.removed_at IS NULL",
    )
    .bind(channel_id)
    .bind(pubkey)
    .fetch_one(pool)
    .await?;
    let cnt: i64 = row.try_get("cnt")?;
    Ok(cnt > 0)
}

/// Returns all active members of the given channel.
///
/// Returns an empty list if the channel has been soft-deleted.
pub async fn get_members(pool: &PgPool, channel_id: Uuid) -> Result<Vec<MemberRecord>> {
    let rows = sqlx::query(
        r#"
        SELECT cm.channel_id, cm.pubkey, cm.role::text AS role, cm.joined_at, cm.invited_by, cm.removed_at
        FROM channel_members cm
        JOIN channels c ON cm.channel_id = c.id AND c.deleted_at IS NULL
        WHERE cm.channel_id = $1 AND cm.removed_at IS NULL
        ORDER BY cm.joined_at ASC
        LIMIT 1000
        "#,
    )
    .bind(channel_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(row_to_member_record).collect()
}

/// Get all channel IDs accessible to a pubkey.
///
/// Includes channels where the pubkey is an active member AND all open channels.
/// Open channels must be included in REQ filter resolution.
pub async fn get_accessible_channel_ids(pool: &PgPool, pubkey: &[u8]) -> Result<Vec<Uuid>> {
    let rows = sqlx::query(
        r#"
        SELECT cm.channel_id
        FROM channel_members cm
        JOIN channels c ON cm.channel_id = c.id AND c.deleted_at IS NULL
        WHERE cm.pubkey = $1 AND cm.removed_at IS NULL
        UNION
        SELECT id AS channel_id
        FROM channels
        WHERE visibility = 'open' AND deleted_at IS NULL
        LIMIT 1000
        "#,
    )
    .bind(pubkey)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            let id: Uuid = r.try_get("channel_id")?;
            Ok(id)
        })
        .collect()
}

/// Lists channels, optionally filtered by visibility string.
pub async fn list_channels(pool: &PgPool, visibility: Option<&str>) -> Result<Vec<ChannelRecord>> {
    let rows = if let Some(vis) = visibility {
        sqlx::query(
            r#"
            SELECT id, name, channel_type::text AS channel_type, visibility::text AS visibility,
                   description, canvas,
                   created_by, created_at, updated_at, archived_at, deleted_at,
                   nip29_group_id, topic_required, max_members,
                   topic, topic_set_by, topic_set_at,
                   purpose, purpose_set_by, purpose_set_at
            FROM channels
            WHERE deleted_at IS NULL AND visibility::text = $1
            ORDER BY created_at DESC
            LIMIT 1000
            "#,
        )
        .bind(vis)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            r#"
            SELECT id, name, channel_type::text AS channel_type, visibility::text AS visibility,
                   description, canvas,
                   created_by, created_at, updated_at, archived_at, deleted_at,
                   nip29_group_id, topic_required, max_members,
                   topic, topic_set_by, topic_set_at,
                   purpose, purpose_set_by, purpose_set_at
            FROM channels
            WHERE deleted_at IS NULL
            ORDER BY created_at DESC
            LIMIT 1000
            "#,
        )
        .fetch_all(pool)
        .await?
    };

    rows.into_iter().map(row_to_channel_record).collect()
}

/// Transaction-aware variant of [`get_active_role_tx`].
async fn get_active_role_tx(
    tx: &mut Transaction<'_, Postgres>,
    channel_id: Uuid,
    pubkey: &[u8],
) -> Result<Option<String>> {
    let row = sqlx::query(
        "SELECT role::text AS role FROM channel_members \
         WHERE channel_id = $1 AND pubkey = $2 AND removed_at IS NULL",
    )
    .bind(channel_id)
    .bind(pubkey)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(row.map(|r| r.try_get("role")).transpose()?)
}

/// Transaction-aware variant of [`get_channel`].
async fn get_channel_tx(
    tx: &mut Transaction<'_, Postgres>,
    channel_id: Uuid,
) -> Result<ChannelRecord> {
    let row = sqlx::query(
        r#"
        SELECT id, name, channel_type::text AS channel_type, visibility::text AS visibility,
               description, canvas,
               created_by, created_at, updated_at, archived_at, deleted_at,
               nip29_group_id, topic_required, max_members,
               topic, topic_set_by, topic_set_at,
               purpose, purpose_set_by, purpose_set_at
        FROM channels WHERE id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(channel_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(DbError::ChannelNotFound(channel_id))?;
    row_to_channel_record(row)
}

/// A channel entry returned as part of a bot member record.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BotChannelEntry {
    /// Channel display name.
    pub name: String,
    /// Channel UUID (as string from the DB).
    pub id: String,
}

/// Bot member record — a user with role=bot, with their channel memberships aggregated.
#[derive(Debug, Clone)]
pub struct BotMemberRecord {
    /// Compressed public key bytes of the bot user.
    pub pubkey: Vec<u8>,
    /// Optional display name for the bot.
    pub display_name: Option<String>,
    /// Optional agent type identifier.
    pub agent_type: Option<String>,
    /// Optional JSON capabilities descriptor.
    pub capabilities: Option<serde_json::Value>,
    /// Channel entries with both name and UUID, from json_agg.
    pub channels: Vec<BotChannelEntry>,
}

/// User record for bulk lookup.
#[derive(Debug, Clone)]
pub struct UserRecord {
    /// Compressed public key bytes of the user.
    pub pubkey: Vec<u8>,
    /// Optional display name.
    pub display_name: Option<String>,
    /// Optional avatar image URL.
    pub avatar_url: Option<String>,
    /// Optional NIP-05 identifier (e.g. `user@example.com`).
    pub nip05_handle: Option<String>,
}

/// A channel record paired with whether the querying user is an active member.
#[derive(Debug, Clone)]
pub struct AccessibleChannel {
    /// The channel record.
    pub channel: ChannelRecord,
    /// Whether the querying user is an active member of this channel.
    pub is_member: bool,
}

/// Returns full channel records for all channels a user can access:
/// open channels (visible to everyone) plus channels where the user is an active member.
///
/// Uses a LEFT JOIN on channel_members (PK: channel_id + pubkey) which produces at
/// most one row per channel. Results are ordered stream -> forum -> dm, then by name.
///
/// If `visibility_filter` is `Some("open")` or `Some("private")`, only channels with
/// that visibility value are returned. `None` returns all accessible channels.
pub async fn get_accessible_channels(
    pool: &PgPool,
    pubkey: &[u8],
    visibility_filter: Option<&str>,
    member_only: Option<bool>,
) -> Result<Vec<AccessibleChannel>> {
    // When `member_only` is `Some(true)`, restrict to channels where the user
    // has an active membership (cm.channel_id IS NOT NULL). This is a strict
    // subset of the default result set and is pushed into SQL so the LIMIT 1000
    // applies to the filtered set, not the pre-filter set.
    let membership_clause = if member_only == Some(true) {
        "AND cm.channel_id IS NOT NULL"
    } else {
        "AND (c.visibility = 'open' OR cm.channel_id IS NOT NULL)"
    };

    let base = format!(
        r#"
        SELECT c.id, c.name, c.channel_type::text AS channel_type,
               c.visibility::text AS visibility, c.description, c.canvas,
               c.created_by, c.created_at, c.updated_at, c.archived_at, c.deleted_at,
               c.nip29_group_id, c.topic_required, c.max_members,
               c.topic, c.topic_set_by, c.topic_set_at,
               c.purpose, c.purpose_set_by, c.purpose_set_at,
               (cm.channel_id IS NOT NULL) AS is_member
        FROM channels c
        LEFT JOIN channel_members cm
            ON c.id = cm.channel_id AND cm.pubkey = $1 AND cm.removed_at IS NULL
        WHERE c.deleted_at IS NULL
          {membership_clause}
          AND (c.channel_type != 'dm' OR cm.hidden_at IS NULL)
    "#
    );

    let sql = if visibility_filter.is_some() {
        format!("{base}  AND c.visibility::text = $2\n        ORDER BY array_position(ARRAY['stream','forum','dm']::text[], c.channel_type::text), c.name\n        LIMIT 1000")
    } else {
        format!("{base}        ORDER BY array_position(ARRAY['stream','forum','dm']::text[], c.channel_type::text), c.name\n        LIMIT 1000")
    };

    let query = sqlx::query(&sql).bind(pubkey);
    let query = if let Some(vis) = visibility_filter {
        query.bind(vis)
    } else {
        query
    };

    let rows = query.fetch_all(pool).await?;
    rows.into_iter()
        .map(|row| {
            let is_member: bool = row.try_get("is_member").unwrap_or(false);
            let channel = row_to_channel_record(row)?;
            Ok(AccessibleChannel { channel, is_member })
        })
        .collect()
}

/// Returns all bot-role members with their channel memberships.
///
/// Channels are returned as a JSON array of `{name, id}` objects via `json_agg`,
/// preserving the 1:1 name↔UUID pairing. No separate string_agg ordering issues.
/// Members with no active channel memberships are excluded (INNER JOIN on channels).
pub async fn get_bot_members(pool: &PgPool) -> Result<Vec<BotMemberRecord>> {
    let rows = sqlx::query(
        r#"
        SELECT cm.pubkey, u.display_name, u.agent_type, u.capabilities,
               COALESCE(json_agg(DISTINCT jsonb_build_object('name', c.name, 'id', c.id::text)), '[]') AS channels_json
        FROM channel_members cm
        LEFT JOIN users u ON cm.pubkey = u.pubkey
        JOIN channels c ON cm.channel_id = c.id AND c.deleted_at IS NULL
        WHERE cm.role = 'bot' AND cm.removed_at IS NULL
        GROUP BY cm.pubkey, u.display_name, u.agent_type, u.capabilities
        LIMIT 1000
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let capabilities: Option<serde_json::Value> = row.try_get("capabilities")?;
        let channels_json: serde_json::Value = row
            .try_get::<serde_json::Value, _>("channels_json")
            .unwrap_or(serde_json::Value::Array(vec![]));
        let channels: Vec<BotChannelEntry> =
            serde_json::from_value(channels_json).unwrap_or_default();
        out.push(BotMemberRecord {
            pubkey: row.try_get("pubkey")?,
            display_name: row.try_get("display_name")?,
            agent_type: row.try_get("agent_type")?,
            capabilities,
            channels,
        });
    }
    Ok(out)
}

/// Bulk-fetch user records by pubkey.
///
/// Returns only users that exist in the `users` table. Ordering matches input order
/// is NOT guaranteed — callers should index by pubkey if order matters.
/// Returns an empty vec immediately when `pubkeys` is empty (no query issued).
pub async fn get_users_bulk(pool: &PgPool, pubkeys: &[Vec<u8>]) -> Result<Vec<UserRecord>> {
    if pubkeys.is_empty() {
        return Ok(Vec::new());
    }

    // Build a parameterised IN clause: ($1, $2, ...)
    let placeholders = (1..=pubkeys.len())
        .map(|i| format!("${i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql =
        format!("SELECT pubkey, display_name, avatar_url, nip05_handle FROM users WHERE pubkey IN ({placeholders})");

    let mut q = sqlx::query(&sql);
    for pk in pubkeys {
        q = q.bind(pk);
    }

    let rows = q.fetch_all(pool).await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(UserRecord {
            pubkey: row.try_get("pubkey")?,
            display_name: row.try_get("display_name")?,
            avatar_url: row.try_get("avatar_url")?,
            nip05_handle: row.try_get("nip05_handle")?,
        });
    }
    Ok(out)
}

fn row_to_channel_record(row: sqlx::postgres::PgRow) -> Result<ChannelRecord> {
    let id: Uuid = row.try_get("id")?;
    let topic_required: bool = row.try_get("topic_required")?;

    // topic/purpose fields are new — use try_get and fall back to None if the
    // column is absent (e.g. queries that don't SELECT these columns yet).
    let topic: Option<String> = row.try_get("topic").unwrap_or(None);
    let topic_set_by: Option<Vec<u8>> = row.try_get("topic_set_by").unwrap_or(None);
    let topic_set_at: Option<DateTime<Utc>> = row.try_get("topic_set_at").unwrap_or(None);
    let purpose: Option<String> = row.try_get("purpose").unwrap_or(None);
    let purpose_set_by: Option<Vec<u8>> = row.try_get("purpose_set_by").unwrap_or(None);
    let purpose_set_at: Option<DateTime<Utc>> = row.try_get("purpose_set_at").unwrap_or(None);

    Ok(ChannelRecord {
        id,
        name: row.try_get("name")?,
        channel_type: row.try_get("channel_type")?,
        visibility: row.try_get("visibility")?,
        description: row.try_get("description")?,
        canvas: row.try_get("canvas")?,
        created_by: row.try_get("created_by")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        archived_at: row.try_get("archived_at")?,
        deleted_at: row.try_get("deleted_at")?,
        nip29_group_id: row.try_get("nip29_group_id")?,
        topic_required,
        max_members: row.try_get("max_members")?,
        topic,
        topic_set_by,
        topic_set_at,
        purpose,
        purpose_set_by,
        purpose_set_at,
    })
}

fn row_to_member_record(row: sqlx::postgres::PgRow) -> Result<MemberRecord> {
    let channel_id: Uuid = row.try_get("channel_id")?;

    Ok(MemberRecord {
        channel_id,
        pubkey: row.try_get("pubkey")?,
        role: row.try_get("role")?,
        joined_at: row.try_get("joined_at")?,
        invited_by: row.try_get("invited_by")?,
        removed_at: row.try_get("removed_at")?,
    })
}

// ── Phase 2: Channel Metadata ─────────────────────────────────────────────────

/// Partial update for channel name/description.
pub struct ChannelUpdate {
    /// New channel name, or `None` to leave unchanged.
    pub name: Option<String>,
    /// New channel description, or `None` to leave unchanged.
    pub description: Option<String>,
}

/// Updates channel name and/or description dynamically.
///
/// At least one field must be `Some`; returns `InvalidData` otherwise.
/// Returns the updated `ChannelRecord` on success.
pub async fn update_channel(
    pool: &PgPool,
    channel_id: Uuid,
    updates: ChannelUpdate,
) -> Result<ChannelRecord> {
    if updates.name.is_none() && updates.description.is_none() {
        return Err(DbError::InvalidData(
            "at least one field must be provided for update".to_string(),
        ));
    }

    // Build SET clause dynamically — only include fields that are Some.
    // Track parameter index for positional placeholders.
    let mut set_parts: Vec<String> = Vec::new();
    let mut param_idx: usize = 1;
    if updates.name.is_some() {
        set_parts.push(format!("name = ${param_idx}"));
        param_idx += 1;
    }
    if updates.description.is_some() {
        set_parts.push(format!("description = ${param_idx}"));
        param_idx += 1;
    }
    let sql = format!(
        "UPDATE channels SET {}, updated_at = NOW() WHERE id = ${param_idx} AND deleted_at IS NULL",
        set_parts.join(", ")
    );

    let mut q = sqlx::query(&sql);
    if let Some(ref name) = updates.name {
        q = q.bind(name);
    }
    if let Some(ref desc) = updates.description {
        q = q.bind(desc);
    }
    q = q.bind(channel_id);

    let result = q.execute(pool).await?;
    if result.rows_affected() == 0 {
        return Err(DbError::ChannelNotFound(channel_id));
    }

    get_channel(pool, channel_id).await
}

/// Sets the topic for a channel, recording who set it and when.
pub async fn set_topic(pool: &PgPool, channel_id: Uuid, topic: &str, set_by: &[u8]) -> Result<()> {
    let result = sqlx::query(
        "UPDATE channels SET topic = $1, topic_set_by = $2, topic_set_at = NOW() \
         WHERE id = $3 AND deleted_at IS NULL",
    )
    .bind(topic)
    .bind(set_by)
    .bind(channel_id)
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(DbError::ChannelNotFound(channel_id));
    }
    Ok(())
}

/// Sets the purpose for a channel, recording who set it and when.
pub async fn set_purpose(
    pool: &PgPool,
    channel_id: Uuid,
    purpose: &str,
    set_by: &[u8],
) -> Result<()> {
    let result = sqlx::query(
        "UPDATE channels SET purpose = $1, purpose_set_by = $2, purpose_set_at = NOW() \
         WHERE id = $3 AND deleted_at IS NULL",
    )
    .bind(purpose)
    .bind(set_by)
    .bind(channel_id)
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(DbError::ChannelNotFound(channel_id));
    }
    Ok(())
}

/// Archives a channel.
///
/// Returns `AccessDenied` if the channel is already archived.
/// Returns `ChannelNotFound` if the channel does not exist or is deleted.
pub async fn archive_channel(pool: &PgPool, channel_id: Uuid) -> Result<()> {
    // First check: does the channel exist and what is its state?
    let row = sqlx::query("SELECT archived_at FROM channels WHERE id = $1 AND deleted_at IS NULL")
        .bind(channel_id)
        .fetch_optional(pool)
        .await?;

    match row {
        None => return Err(DbError::ChannelNotFound(channel_id)),
        Some(r) => {
            let archived_at: Option<DateTime<Utc>> = r.try_get("archived_at")?;
            if archived_at.is_some() {
                return Err(DbError::AccessDenied(
                    "channel is already archived".to_string(),
                ));
            }
        }
    }

    sqlx::query(
        "UPDATE channels SET archived_at = NOW() \
         WHERE id = $1 AND deleted_at IS NULL AND archived_at IS NULL",
    )
    .bind(channel_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Unarchives a channel.
///
/// Returns `AccessDenied` if the channel is not currently archived.
/// Returns `ChannelNotFound` if the channel does not exist or is deleted.
pub async fn unarchive_channel(pool: &PgPool, channel_id: Uuid) -> Result<()> {
    // First check: does the channel exist and what is its state?
    let row = sqlx::query("SELECT archived_at FROM channels WHERE id = $1 AND deleted_at IS NULL")
        .bind(channel_id)
        .fetch_optional(pool)
        .await?;

    match row {
        None => return Err(DbError::ChannelNotFound(channel_id)),
        Some(r) => {
            let archived_at: Option<DateTime<Utc>> = r.try_get("archived_at")?;
            if archived_at.is_none() {
                return Err(DbError::AccessDenied("channel is not archived".to_string()));
            }
        }
    }

    sqlx::query(
        "UPDATE channels SET archived_at = NULL \
         WHERE id = $1 AND deleted_at IS NULL AND archived_at IS NOT NULL",
    )
    .bind(channel_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Soft-delete a channel by setting `deleted_at = NOW()`.
///
/// Returns `Ok(true)` if the channel was deleted, `Ok(false)` if already
/// deleted or not found.
pub async fn soft_delete_channel(pool: &PgPool, channel_id: Uuid) -> Result<bool> {
    let result =
        sqlx::query("UPDATE channels SET deleted_at = NOW() WHERE id = $1 AND deleted_at IS NULL")
            .bind(channel_id)
            .execute(pool)
            .await?;

    Ok(result.rows_affected() > 0)
}

/// Returns the count of active (non-removed) members in a channel.
pub async fn get_member_count(pool: &PgPool, channel_id: Uuid) -> Result<i64> {
    let row = sqlx::query(
        "SELECT COUNT(*) as cnt FROM channel_members WHERE channel_id = $1 AND removed_at IS NULL",
    )
    .bind(channel_id)
    .fetch_one(pool)
    .await?;
    Ok(row.try_get("cnt")?)
}

/// Bulk-fetch member counts for a set of channel IDs.
///
/// Returns a map of `channel_id -> count`. Channels with zero members are omitted.
/// Single query regardless of input size.
pub async fn get_member_counts_bulk(
    pool: &PgPool,
    channel_ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, i64>> {
    if channel_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    let mut qb: sqlx::QueryBuilder<sqlx::Postgres> = sqlx::QueryBuilder::new(
        "SELECT channel_id, COUNT(*) as cnt FROM channel_members \
         WHERE removed_at IS NULL AND channel_id IN (",
    );
    let mut sep = qb.separated(", ");
    for id in channel_ids {
        sep.push_bind(*id);
    }
    qb.push(") GROUP BY channel_id");

    let rows = qb.build().fetch_all(pool).await?;

    let mut map = std::collections::HashMap::with_capacity(rows.len());
    for row in rows {
        let id: Uuid = row.try_get("channel_id")?;
        let cnt: i64 = row.try_get("cnt")?;
        map.insert(id, cnt);
    }
    Ok(map)
}

/// Get the active role of a pubkey in a channel.
///
/// Returns `None` if the pubkey is not an active member.
pub async fn get_member_role(
    pool: &PgPool,
    channel_id: Uuid,
    pubkey: &[u8],
) -> Result<Option<String>> {
    let row = sqlx::query(
        "SELECT cm.role::text AS role FROM channel_members cm \
         JOIN channels c ON cm.channel_id = c.id AND c.deleted_at IS NULL \
         WHERE cm.channel_id = $1 AND cm.pubkey = $2 AND cm.removed_at IS NULL",
    )
    .bind(channel_id)
    .bind(pubkey)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.try_get("role")).transpose()?)
}
