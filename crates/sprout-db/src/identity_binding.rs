//! Identity binding persistence for proxy identity mode.
//!
//! Maps (corporate_uid, device_cn) pairs to Nostr pubkeys. Each device
//! gets its own binding, enabling multi-device support under one corporate
//! identity.

use crate::error::Result;
use sqlx::PgPool;

/// Result of attempting to bind a pubkey to a (uid, device_cn) pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingResult {
    /// No prior binding existed; a new one was created.
    Created,
    /// A binding already existed and the pubkey matches.
    Matched,
    /// A binding already existed but for a different pubkey.
    Mismatch {
        /// The pubkey that is already bound to this (uid, device_cn).
        existing_pubkey: Vec<u8>,
    },
}

/// A stored identity binding record.
#[derive(Debug, Clone)]
pub struct IdentityBinding {
    /// Corporate user identifier.
    pub uid: String,
    /// Device common name from client certificate.
    pub device_cn: String,
    /// Bound Nostr public key (32 bytes).
    pub pubkey: Vec<u8>,
    /// Cached username from the identity JWT.
    pub username: Option<String>,
}

/// Look up a binding by (uid, device_cn).
pub async fn get_identity_binding(
    pool: &PgPool,
    uid: &str,
    device_cn: &str,
) -> Result<Option<IdentityBinding>> {
    let row = sqlx::query_as::<_, (String, String, Vec<u8>, Option<String>)>(
        r#"
        SELECT uid, device_cn, pubkey, username
        FROM identity_bindings
        WHERE uid = $1 AND device_cn = $2
        "#,
    )
    .bind(uid)
    .bind(device_cn)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(uid, device_cn, pubkey, username)| IdentityBinding {
        uid,
        device_cn,
        pubkey,
        username,
    }))
}

/// Bind a pubkey to (uid, device_cn), or validate an existing binding.
///
/// Uses `SELECT ... FOR UPDATE` inside a transaction to prevent race conditions
/// on first bind.
///
/// Returns:
/// - `Created` if no prior binding existed and a new one was inserted.
/// - `Matched` if the existing binding's pubkey matches.
/// - `Mismatch` if the existing binding has a different pubkey.
pub async fn bind_or_validate_identity(
    pool: &PgPool,
    uid: &str,
    device_cn: &str,
    pubkey: &[u8],
    username: &str,
) -> Result<BindingResult> {
    let mut tx = pool.begin().await?;

    let existing = sqlx::query_as::<_, (Vec<u8>,)>(
        r#"
        SELECT pubkey
        FROM identity_bindings
        WHERE uid = $1 AND device_cn = $2
        FOR UPDATE
        "#,
    )
    .bind(uid)
    .bind(device_cn)
    .fetch_optional(&mut *tx)
    .await?;

    let result = match existing {
        Some((existing_pubkey,)) => {
            if existing_pubkey == pubkey {
                // Update last_seen_at and username on successful match.
                sqlx::query(
                    r#"
                    UPDATE identity_bindings
                    SET last_seen_at = NOW(), username = NULLIF($3, '')
                    WHERE uid = $1 AND device_cn = $2
                    "#,
                )
                .bind(uid)
                .bind(device_cn)
                .bind(username)
                .execute(&mut *tx)
                .await?;
                BindingResult::Matched
            } else {
                BindingResult::Mismatch { existing_pubkey }
            }
        }
        None => {
            sqlx::query(
                r#"
                INSERT INTO identity_bindings (uid, device_cn, pubkey, username)
                VALUES ($1, $2, $3, NULLIF($4, ''))
                "#,
            )
            .bind(uid)
            .bind(device_cn)
            .bind(pubkey)
            .bind(username)
            .execute(&mut *tx)
            .await?;
            BindingResult::Created
        }
    };

    tx.commit().await?;
    Ok(result)
}

/// Get all bindings for a given uid (all devices).
pub async fn get_bindings_for_uid(
    pool: &PgPool,
    uid: &str,
) -> Result<Vec<IdentityBinding>> {
    let rows = sqlx::query_as::<_, (String, String, Vec<u8>, Option<String>)>(
        r#"
        SELECT uid, device_cn, pubkey, username
        FROM identity_bindings
        WHERE uid = $1
        ORDER BY created_at
        "#,
    )
    .bind(uid)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(uid, device_cn, pubkey, username)| IdentityBinding {
            uid,
            device_cn,
            pubkey,
            username,
        })
        .collect())
}
