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
    let row = sqlx::query_as::<_, (Vec<u8>, Option<String>, Option<String>, Option<String>)>(
        r#"
        SELECT pubkey, display_name, avatar_url, nip05_handle
        FROM users
        WHERE pubkey = ?
        "#,
    )
    .bind(pubkey)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(
        |(pubkey, display_name, avatar_url, nip05_handle)| UserProfile {
            pubkey,
            display_name,
            avatar_url,
            nip05_handle,
        },
    ))
}

/// Update a user's profile fields (display_name, avatar_url).
/// Only updates fields that are Some — None fields are left unchanged.
/// At least one field must be Some, otherwise returns Ok(()) without touching the DB.
pub async fn update_user_profile(
    pool: &MySqlPool,
    pubkey: &[u8],
    display_name: Option<&str>,
    avatar_url: Option<&str>,
) -> Result<()> {
    match (display_name, avatar_url) {
        (Some(name), Some(url)) => {
            sqlx::query(r#"UPDATE users SET display_name = ?, avatar_url = ? WHERE pubkey = ?"#)
                .bind(name)
                .bind(url)
                .bind(pubkey)
                .execute(pool)
                .await?;
        }
        (Some(name), None) => {
            sqlx::query(r#"UPDATE users SET display_name = ? WHERE pubkey = ?"#)
                .bind(name)
                .bind(pubkey)
                .execute(pool)
                .await?;
        }
        (None, Some(url)) => {
            sqlx::query(r#"UPDATE users SET avatar_url = ? WHERE pubkey = ?"#)
                .bind(url)
                .bind(pubkey)
                .execute(pool)
                .await?;
        }
        (None, None) => {
            // Nothing to update — caller should have validated at least one field.
        }
    }
    Ok(())
}
