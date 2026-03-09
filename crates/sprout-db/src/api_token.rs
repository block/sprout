//! API token CRUD operations.

use chrono::{DateTime, Utc};
use sqlx::MySqlPool;
use uuid::Uuid;

use crate::error::{DbError, Result};

/// Create a new API token record. The caller is responsible for generating
/// the raw token and computing its SHA-256 hash.
pub async fn create_api_token(
    pool: &MySqlPool,
    token_hash: &[u8],
    owner_pubkey: &[u8],
    name: &str,
    scopes: &[String],
    channel_ids: Option<&[Uuid]>,
    expires_at: Option<DateTime<Utc>>,
) -> Result<Uuid> {
    let id = Uuid::new_v4();
    let id_bytes = id.as_bytes().as_slice();

    let scopes_json =
        serde_json::to_value(scopes).map_err(|e| DbError::InvalidData(e.to_string()))?;

    // Serialize channel_ids; propagate errors rather than silently dropping to NULL.
    let channel_ids_json: Option<serde_json::Value> = channel_ids
        .map(|ids| {
            serde_json::to_value(ids.iter().map(|id| id.to_string()).collect::<Vec<_>>())
                .map_err(|e| DbError::InvalidData(format!("channel_ids serialization: {e}")))
        })
        .transpose()?;

    sqlx::query(
        r#"
        INSERT INTO api_tokens (id, token_hash, owner_pubkey, name, scopes, channel_ids, expires_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(id_bytes)
    .bind(token_hash)
    .bind(owner_pubkey)
    .bind(name)
    .bind(&scopes_json)
    .bind(&channel_ids_json)
    .bind(expires_at)
    .execute(pool)
    .await?;

    Ok(id)
}
