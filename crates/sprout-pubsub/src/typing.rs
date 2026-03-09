//! Typing indicators — Redis sorted set with 5-second active window.
//!
//! Each `set_typing` call ZADDs the member, prunes entries older than 5s,
//! and refreshes a key-level TTL of `TYPING_KEY_TTL_SECS` seconds on the
//! sorted set. This prevents orphaned keys from accumulating in Redis when a
//! channel goes quiet: individual members expire via `ZREMRANGEBYSCORE`, but
//! without a key-level TTL the empty sorted set would persist indefinitely.

use deadpool_redis::Pool;
use nostr::PublicKey;
use uuid::Uuid;

use crate::error::PubSubError;

/// Active typing window in seconds. Members with a score older than this are pruned.
pub const TYPING_WINDOW_SECS: f64 = 5.0;

/// Key-level TTL for the typing sorted set. If no `set_typing` call is made
/// for this duration, Redis automatically deletes the key, preventing orphaned
/// empty sets from accumulating when a channel goes permanently quiet.
///
/// Must be longer than `TYPING_WINDOW_SECS` so that a key is never expired
/// while it still contains live members.
pub const TYPING_KEY_TTL_SECS: u64 = 60;

/// Returns the Redis key for the typing sorted set of `channel_id`.
pub fn typing_key(channel_id: Uuid) -> String {
    format!("sprout:typing:{}", channel_id)
}

/// Records that `pubkey` is typing in `channel_id` and prunes stale entries.
pub async fn set_typing(
    pool: &Pool,
    channel_id: Uuid,
    pubkey: &PublicKey,
) -> Result<(), PubSubError> {
    let mut conn = pool.get().await?;
    let key = typing_key(channel_id);
    let now = chrono::Utc::now().timestamp() as f64;

    redis::cmd("ZADD")
        .arg(&key)
        .arg(now)
        .arg(pubkey.to_hex())
        .query_async::<()>(&mut conn)
        .await?;

    redis::cmd("ZREMRANGEBYSCORE")
        .arg(&key)
        .arg("-inf")
        .arg(now - TYPING_WINDOW_SECS)
        .query_async::<()>(&mut conn)
        .await?;

    // Refresh key-level TTL so that orphaned empty sets are eventually
    // reclaimed by Redis even if no further writes arrive for this channel.
    redis::cmd("EXPIRE")
        .arg(&key)
        .arg(TYPING_KEY_TTL_SECS)
        .query_async::<()>(&mut conn)
        .await?;

    Ok(())
}

/// Returns hex pubkeys of users who typed in `channel_id` within the last [`TYPING_WINDOW_SECS`].
pub async fn get_typing(pool: &Pool, channel_id: Uuid) -> Result<Vec<String>, PubSubError> {
    let mut conn = pool.get().await?;
    let key = typing_key(channel_id);
    let now = chrono::Utc::now().timestamp() as f64;

    let members: Vec<String> = redis::cmd("ZRANGEBYSCORE")
        .arg(&key)
        .arg(now - TYPING_WINDOW_SECS)
        .arg("+inf")
        .query_async(&mut conn)
        .await?;

    Ok(members)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::make_test_pool;
    use nostr::Keys;

    #[test]
    fn test_typing_key_format() {
        let channel_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        assert_eq!(
            typing_key(channel_id),
            "sprout:typing:550e8400-e29b-41d4-a716-446655440000"
        );
    }

    #[tokio::test]
    #[ignore = "requires Redis"]
    async fn test_typing_set_and_prune() {
        let pool = make_test_pool();
        let channel_id = Uuid::new_v4();
        let pk1 = Keys::generate().public_key();
        let pk2 = Keys::generate().public_key();

        set_typing(&pool, channel_id, &pk1).await.unwrap();
        set_typing(&pool, channel_id, &pk2).await.unwrap();

        let typing = get_typing(&pool, channel_id).await.unwrap();
        assert_eq!(typing.len(), 2);
        assert!(typing.contains(&pk1.to_hex()));
        assert!(typing.contains(&pk2.to_hex()));

        // Insert a stale entry (score = now - 10s)
        let stale_pk = Keys::generate().public_key();
        {
            let mut conn = pool.get().await.unwrap();
            let key = typing_key(channel_id);
            let stale_score = chrono::Utc::now().timestamp() as f64 - 10.0;
            redis::cmd("ZADD")
                .arg(&key)
                .arg(stale_score)
                .arg(stale_pk.to_hex())
                .query_async::<()>(&mut conn)
                .await
                .unwrap();
        }

        // Prune fires on next set_typing
        set_typing(&pool, channel_id, &pk1).await.unwrap();

        let typing = get_typing(&pool, channel_id).await.unwrap();
        assert!(!typing.contains(&stale_pk.to_hex()));
        assert!(typing.contains(&pk1.to_hex()));
        // pk1 + pk2 should remain (both within 5s window)
        assert!(!typing.is_empty() && typing.len() <= 2);

        let mut conn = pool.get().await.unwrap();
        redis::cmd("DEL")
            .arg(typing_key(channel_id))
            .query_async::<()>(&mut conn)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore = "requires Redis"]
    async fn test_typing_empty_channel() {
        let pool = make_test_pool();
        let channel_id = Uuid::new_v4();
        let typing = get_typing(&pool, channel_id).await.unwrap();
        assert!(typing.is_empty());
    }
}
