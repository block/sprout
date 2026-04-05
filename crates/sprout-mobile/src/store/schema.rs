// ABOUTME: SQLite schema definitions and migration logic.
// ABOUTME: Creates tables for channels, messages, profiles, reactions, auth tokens, and key-value.

use rusqlite::Connection;

use crate::error::SproutError;

/// Current schema version. Bump when adding migrations.
const SCHEMA_VERSION: u32 = 1;

/// Apply all schema migrations up to the current version.
pub(crate) fn migrate(conn: &Connection) -> Result<(), SproutError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY
        );",
    )
    .map_err(sqlite_err)?;

    let current: u32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .map_err(sqlite_err)?;

    if current < 1 {
        apply_v1(conn)?;
    }

    if current < SCHEMA_VERSION {
        conn.execute(
            "INSERT OR REPLACE INTO schema_version (version) VALUES (?1)",
            [SCHEMA_VERSION],
        )
        .map_err(sqlite_err)?;
    }

    Ok(())
}

/// V1: initial schema.
fn apply_v1(conn: &Connection) -> Result<(), SproutError> {
    conn.execute_batch(
        "
        -- Generic key-value store for tokens, read state, preferences.
        CREATE TABLE IF NOT EXISTS kv (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        -- Channel cache.
        CREATE TABLE IF NOT EXISTS channels (
            id              TEXT PRIMARY KEY,
            name            TEXT NOT NULL,
            about           TEXT,
            topic           TEXT,
            channel_type    TEXT NOT NULL DEFAULT 'stream',
            visibility      TEXT NOT NULL DEFAULT 'open',
            member_count    INTEGER NOT NULL DEFAULT 0,
            is_member       INTEGER NOT NULL DEFAULT 0,
            last_message_at INTEGER,
            updated_at      INTEGER NOT NULL
        );

        -- Message cache.
        CREATE TABLE IF NOT EXISTS messages (
            event_id      TEXT PRIMARY KEY,
            channel_id    TEXT NOT NULL,
            author_pubkey TEXT NOT NULL,
            content       TEXT NOT NULL,
            kind          INTEGER NOT NULL,
            created_at    INTEGER NOT NULL,
            reply_to      TEXT,
            thread_root   TEXT,
            reply_count   INTEGER NOT NULL DEFAULT 0,
            raw_json      TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_messages_channel_time
            ON messages(channel_id, created_at DESC);

        -- User profile cache.
        CREATE TABLE IF NOT EXISTS profiles (
            pubkey       TEXT PRIMARY KEY,
            display_name TEXT,
            picture      TEXT,
            about        TEXT,
            nip05        TEXT,
            updated_at   INTEGER NOT NULL
        );

        -- Reaction cache.
        CREATE TABLE IF NOT EXISTS reactions (
            event_id        TEXT PRIMARY KEY,
            target_event_id TEXT NOT NULL,
            author_pubkey   TEXT NOT NULL,
            emoji           TEXT NOT NULL,
            created_at      INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_reactions_target
            ON reactions(target_event_id);

        -- Cached API tokens.
        CREATE TABLE IF NOT EXISTS auth_tokens (
            id          TEXT PRIMARY KEY,
            token_value TEXT NOT NULL,
            expires_at  INTEGER,
            created_at  INTEGER NOT NULL
        );
        ",
    )
    .map_err(sqlite_err)?;
    Ok(())
}

fn sqlite_err(e: rusqlite::Error) -> SproutError {
    SproutError::StorageError {
        message: e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_creates_tables() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();

        // Verify all tables exist by querying them.
        conn.execute("SELECT * FROM kv LIMIT 1", []).unwrap();
        conn.execute("SELECT * FROM channels LIMIT 1", []).unwrap();
        conn.execute("SELECT * FROM messages LIMIT 1", []).unwrap();
        conn.execute("SELECT * FROM profiles LIMIT 1", []).unwrap();
        conn.execute("SELECT * FROM reactions LIMIT 1", []).unwrap();
        conn.execute("SELECT * FROM auth_tokens LIMIT 1", [])
            .unwrap();
    }

    #[test]
    fn migrate_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap(); // second call should be a no-op
    }

    #[test]
    fn schema_version_is_recorded() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let version: u32 = conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
    }
}
