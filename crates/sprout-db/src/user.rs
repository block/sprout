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

/// Update a user's profile fields (display_name, avatar_url, about).
/// Only updates fields that are Some — None fields are left unchanged.
/// At least one field must be Some, otherwise returns Ok(()) without touching the DB.
pub async fn update_user_profile(
    pool: &MySqlPool,
    pubkey: &[u8],
    display_name: Option<&str>,
    avatar_url: Option<&str>,
    about: Option<&str>,
) -> Result<()> {
    // Build SET clause dynamically to avoid 2^3 match arms.
    let mut set_parts: Vec<&str> = Vec::new();
    if display_name.is_some() {
        set_parts.push("display_name = ?");
    }
    if avatar_url.is_some() {
        set_parts.push("avatar_url = ?");
    }
    if about.is_some() {
        set_parts.push("about = ?");
    }

    if set_parts.is_empty() {
        // Nothing to update — caller should have validated at least one field.
        return Ok(());
    }

    let sql = format!("UPDATE users SET {} WHERE pubkey = ?", set_parts.join(", "));
    let mut query = sqlx::query(&sql);
    if let Some(name) = display_name {
        query = query.bind(name);
    }
    if let Some(url) = avatar_url {
        query = query.bind(url);
    }
    if let Some(bio) = about {
        query = query.bind(bio);
    }
    query = query.bind(pubkey);
    query.execute(pool).await?;
    Ok(())
}
