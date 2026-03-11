//! User CRUD operations.

use crate::error::Result;
use sqlx::MySqlPool;

/// A user's profile fields.
#[derive(Debug, Clone)]
pub struct UserProfile {
    /// Raw 32-byte compressed public key.
    pub pubkey: Vec<u8>,
    /// Human-readable display name chosen by the user.
    pub display_name: Option<String>,
    /// URL of the user's avatar image.
    pub avatar_url: Option<String>,
    /// Short bio or description provided by the user.
    pub about: Option<String>,
    /// NIP-05 identifier (user@domain).
    pub nip05_handle: Option<String>,
}

/// Ensure a user record exists for the given pubkey (upsert).
/// Creates with minimal fields if not present; no-op if already exists.
pub async fn ensure_user(pool: &MySqlPool, pubkey: &[u8]) -> Result<()> {
    sqlx::query(
        r#"
        INSERT IGNORE INTO users (pubkey)
        VALUES (?)
        "#,
    )
    .bind(pubkey)
    .execute(pool)
    .await?;
    Ok(())
}

/// Get a single user record by pubkey.
pub async fn get_user(pool: &MySqlPool, pubkey: &[u8]) -> Result<Option<UserProfile>> {
    let row = sqlx::query_as::<
        _,
        (
            Vec<u8>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    >(
        r#"
        SELECT pubkey, display_name, avatar_url, about, nip05_handle
        FROM users
        WHERE pubkey = ?
        "#,
    )
    .bind(pubkey)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(
        |(pubkey, display_name, avatar_url, about, nip05_handle)| UserProfile {
            pubkey,
            display_name,
            avatar_url,
            about,
            nip05_handle,
        },
    ))
}

/// Update a user's profile fields (display_name, avatar_url, about, nip05_handle).
/// Only updates fields that are Some — None fields are left unchanged.
/// At least one field must be Some, otherwise returns Ok(()) without touching the DB.
///
/// Empty strings are treated as "clear to NULL" — this is important for kind:0
/// absolute-state semantics where absent fields must be cleared, and for the
/// `nip05_handle` column which has a UNIQUE constraint (multiple NULLs are allowed,
/// but multiple empty strings would violate uniqueness).
pub async fn update_user_profile(
    pool: &MySqlPool,
    pubkey: &[u8],
    display_name: Option<&str>,
    avatar_url: Option<&str>,
    about: Option<&str>,
    nip05_handle: Option<&str>,
) -> Result<()> {
    let mut set_parts: Vec<&str> = Vec::new();
    if display_name.is_some() { set_parts.push("display_name = ?"); }
    if avatar_url.is_some() { set_parts.push("avatar_url = ?"); }
    if about.is_some() { set_parts.push("about = ?"); }
    if nip05_handle.is_some() { set_parts.push("nip05_handle = ?"); }

    if set_parts.is_empty() {
        return Ok(());
    }

    // Helper: convert empty string to None (NULL in DB). This ensures UNIQUE
    // columns like nip05_handle don't collide on empty strings, and keeps
    // semantics clean: absent profile data is NULL, not "".
    fn empty_to_none(val: Option<&str>) -> Option<&str> {
        val.filter(|s| !s.is_empty())
    }

    let sql = format!("UPDATE users SET {} WHERE pubkey = ?", set_parts.join(", "));
    let mut query = sqlx::query(&sql);
    if display_name.is_some() { query = query.bind(empty_to_none(display_name)); }
    if avatar_url.is_some() { query = query.bind(empty_to_none(avatar_url)); }
    if about.is_some() { query = query.bind(empty_to_none(about)); }
    if nip05_handle.is_some() { query = query.bind(empty_to_none(nip05_handle)); }
    query = query.bind(pubkey);
    query.execute(pool).await?;
    Ok(())
}

/// Look up a user by their full NIP-05 handle (exact match, case-insensitive).
/// Both `local_part` and `domain` must already be lowercased by the caller.
pub async fn get_user_by_nip05(
    pool: &MySqlPool,
    local_part: &str,
    domain: &str,
) -> Result<Option<UserProfile>> {
    let handle = format!("{}@{}", local_part, domain);
    let row = sqlx::query_as::<
        _,
        (
            Vec<u8>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    >(
        r#"
        SELECT pubkey, display_name, avatar_url, about, nip05_handle
        FROM users
        WHERE LOWER(nip05_handle) = LOWER(?)
        LIMIT 1
        "#,
    )
    .bind(&handle)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(
        |(pubkey, display_name, avatar_url, about, nip05_handle)| UserProfile {
            pubkey,
            display_name,
            avatar_url,
            about,
            nip05_handle,
        },
    ))
}
