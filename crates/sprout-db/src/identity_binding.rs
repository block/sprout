//! Identity binding persistence for proxy identity mode.
//!
//! Maps (corporate_uid, device_cn) pairs to Nostr pubkeys. Each device
//! gets its own binding, enabling multi-device support under one corporate
//! identity.
//!
//! # TODO: Self-service key rotation
//!
//! Bindings are currently immutable — rebinding requires admin intervention
//! (`sprout-admin unbind-identity`). Planned work:
//!
//! - Add `POST /api/identity/rotate` endpoint (JWT + device cert + NIP-98 with new key).
//! - Soft-rotate: add `rotated_at` / `replaced_by` columns instead of deleting old rows,
//!   preserving an audit trail and letting the UI resolve old pubkeys to usernames.
//! - Add a UNIQUE constraint on pubkey for active (non-rotated) bindings.
//! - Keep the 409 Conflict on mismatch — rotation must be an explicit action, not implicit.

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

    Ok(
        row.map(|(uid, device_cn, pubkey, username)| IdentityBinding {
            uid,
            device_cn,
            pubkey,
            username,
        }),
    )
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

    sqlx::query("SET LOCAL lock_timeout = '3s'")
        .execute(&mut *tx)
        .await?;

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
pub async fn get_bindings_for_uid(pool: &PgPool, uid: &str) -> Result<Vec<IdentityBinding>> {
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

/// Delete all identity bindings for a given UID (all devices).
/// Used for employee offboarding / GDPR erasure.
pub async fn delete_bindings_for_uid(pool: &PgPool, uid: &str) -> Result<u64> {
    let result = sqlx::query("DELETE FROM identity_bindings WHERE uid = $1")
        .bind(uid)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Check whether a pubkey has any active identity binding.
///
/// Used by the auth layer to enforce "once bound, always require JWT" —
/// a pubkey that was bound to a corporate identity via proxy mode cannot
/// fall through to standard NIP-42 / API token auth in hybrid mode.
pub async fn is_pubkey_identity_bound(pool: &PgPool, pubkey: &[u8]) -> Result<bool> {
    let bound = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM identity_bindings WHERE pubkey = $1)",
    )
    .bind(pubkey)
    .fetch_one(pool)
    .await?;
    Ok(bound)
}

/// Delete a specific identity binding for a (uid, device_cn) pair.
/// Allows re-binding after key loss or device reset.
pub async fn delete_identity_binding(pool: &PgPool, uid: &str, device_cn: &str) -> Result<bool> {
    let result = sqlx::query("DELETE FROM identity_bindings WHERE uid = $1 AND device_cn = $2")
        .bind(uid)
        .bind(device_cn)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::Keys;
    use sqlx::PgPool;

    const TEST_DB_URL: &str = "postgres://sprout:sprout_dev@localhost:5432/sprout";

    async fn setup_pool() -> PgPool {
        PgPool::connect(TEST_DB_URL)
            .await
            .expect("connect to test DB")
    }

    fn random_uid() -> String {
        format!("test-uid-{}", uuid::Uuid::new_v4())
    }

    fn random_pubkey() -> Vec<u8> {
        Keys::generate().public_key().serialize().to_vec()
    }

    #[tokio::test]
    #[ignore = "requires Postgres"]
    async fn bind_creates_new_binding() {
        let pool = setup_pool().await;
        let uid = random_uid();
        let device_cn = "test-laptop";
        let pubkey = random_pubkey();

        let result = bind_or_validate_identity(&pool, &uid, device_cn, &pubkey, "alice")
            .await
            .expect("bind should succeed");
        assert_eq!(result, BindingResult::Created);

        // Verify the binding is readable
        let binding = get_identity_binding(&pool, &uid, device_cn)
            .await
            .expect("get should succeed")
            .expect("binding should exist");
        assert_eq!(binding.pubkey, pubkey);
        assert_eq!(binding.username.as_deref(), Some("alice"));

        // Cleanup
        delete_identity_binding(&pool, &uid, device_cn)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore = "requires Postgres"]
    async fn bind_same_pubkey_returns_matched() {
        let pool = setup_pool().await;
        let uid = random_uid();
        let device_cn = "test-laptop";
        let pubkey = random_pubkey();

        bind_or_validate_identity(&pool, &uid, device_cn, &pubkey, "alice")
            .await
            .expect("first bind");

        let result = bind_or_validate_identity(&pool, &uid, device_cn, &pubkey, "alice")
            .await
            .expect("second bind");
        assert_eq!(result, BindingResult::Matched);

        // Cleanup
        delete_identity_binding(&pool, &uid, device_cn)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore = "requires Postgres"]
    async fn bind_different_pubkey_returns_mismatch() {
        let pool = setup_pool().await;
        let uid = random_uid();
        let device_cn = "test-laptop";
        let pubkey1 = random_pubkey();
        let pubkey2 = random_pubkey();

        bind_or_validate_identity(&pool, &uid, device_cn, &pubkey1, "alice")
            .await
            .expect("first bind");

        let result = bind_or_validate_identity(&pool, &uid, device_cn, &pubkey2, "alice")
            .await
            .expect("second bind with different pubkey");
        assert!(
            matches!(result, BindingResult::Mismatch { .. }),
            "expected Mismatch, got {result:?}"
        );

        // Cleanup
        delete_identity_binding(&pool, &uid, device_cn)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore = "requires Postgres"]
    async fn multi_device_bindings() {
        let pool = setup_pool().await;
        let uid = random_uid();
        let pubkey1 = random_pubkey();
        let pubkey2 = random_pubkey();

        bind_or_validate_identity(&pool, &uid, "laptop", &pubkey1, "alice")
            .await
            .expect("bind laptop");
        bind_or_validate_identity(&pool, &uid, "phone", &pubkey2, "alice")
            .await
            .expect("bind phone");

        let bindings = get_bindings_for_uid(&pool, &uid)
            .await
            .expect("get bindings");
        assert_eq!(bindings.len(), 2);

        // Cleanup
        delete_bindings_for_uid(&pool, &uid).await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires Postgres"]
    async fn delete_binding_allows_rebind() {
        let pool = setup_pool().await;
        let uid = random_uid();
        let device_cn = "test-laptop";
        let pubkey1 = random_pubkey();
        let pubkey2 = random_pubkey();

        // Bind first key
        bind_or_validate_identity(&pool, &uid, device_cn, &pubkey1, "alice")
            .await
            .expect("first bind");

        // Delete the binding
        let deleted = delete_identity_binding(&pool, &uid, device_cn)
            .await
            .expect("delete should succeed");
        assert!(deleted);

        // Rebind with different key should now succeed
        let result = bind_or_validate_identity(&pool, &uid, device_cn, &pubkey2, "alice")
            .await
            .expect("rebind should succeed");
        assert_eq!(result, BindingResult::Created);

        // Cleanup
        delete_identity_binding(&pool, &uid, device_cn)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore = "requires Postgres"]
    async fn delete_bindings_for_uid_removes_all_devices() {
        let pool = setup_pool().await;
        let uid = random_uid();

        bind_or_validate_identity(&pool, &uid, "laptop", &random_pubkey(), "alice")
            .await
            .expect("bind laptop");
        bind_or_validate_identity(&pool, &uid, "phone", &random_pubkey(), "alice")
            .await
            .expect("bind phone");

        let count = delete_bindings_for_uid(&pool, &uid)
            .await
            .expect("delete all");
        assert_eq!(count, 2);

        let bindings = get_bindings_for_uid(&pool, &uid)
            .await
            .expect("get bindings");
        assert!(bindings.is_empty());
    }

    #[tokio::test]
    #[ignore = "requires Postgres"]
    async fn get_nonexistent_binding_returns_none() {
        let pool = setup_pool().await;
        let result = get_identity_binding(&pool, "nonexistent-uid", "nonexistent-device")
            .await
            .expect("query should not error");
        assert!(result.is_none());
    }

    #[tokio::test]
    #[ignore = "requires Postgres"]
    async fn is_pubkey_identity_bound_reflects_binding_lifecycle() {
        let pool = setup_pool().await;
        let uid = random_uid();
        let device_cn = "test-laptop";
        let pubkey = random_pubkey();

        // Not bound before any binding exists.
        assert!(
            !is_pubkey_identity_bound(&pool, &pubkey).await.unwrap(),
            "should not be bound before creation"
        );

        // Bound after creation.
        bind_or_validate_identity(&pool, &uid, device_cn, &pubkey, "alice")
            .await
            .expect("bind should succeed");
        assert!(
            is_pubkey_identity_bound(&pool, &pubkey).await.unwrap(),
            "should be bound after creation"
        );

        // Not bound after deletion.
        delete_identity_binding(&pool, &uid, device_cn)
            .await
            .expect("delete should succeed");
        assert!(
            !is_pubkey_identity_bound(&pool, &pubkey).await.unwrap(),
            "should not be bound after deletion"
        );
    }
}
