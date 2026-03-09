//! User CRUD operations.

use crate::error::Result;
use sqlx::MySqlPool;

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
