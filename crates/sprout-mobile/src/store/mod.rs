//! SQLite local cache facade for offline-capable mobile experience.
//! Write-through on all data; stale-while-revalidate reads for messages and profiles.

/// Channel cache queries and helpers.
pub mod channels;
/// Message cache queries and helpers.
pub mod messages;
/// Profile cache queries and helpers.
pub mod profiles;
/// Database schema migrations.
pub mod schema;

use std::path::Path;

use rusqlite::Connection;

use crate::error::SproutError;

/// Local SQLite cache for offline-capable operation.
///
/// All public methods are synchronous (rusqlite is sync). Callers should
/// use `tokio::task::spawn_blocking` when calling from async contexts.
///
/// Note: `Connection` is `Send` but not `Sync`. Access is serialized
/// through an internal `Mutex`.
pub(crate) struct Store {
    conn: std::sync::Mutex<Connection>,
}

impl Store {
    /// Open (or create) the SQLite database at `db_path` and run migrations.
    pub fn open(db_path: &str) -> Result<Self, SproutError> {
        let conn = if db_path == ":memory:" {
            Connection::open_in_memory()
        } else {
            // Ensure parent directory exists.
            if let Some(parent) = Path::new(db_path).parent() {
                std::fs::create_dir_all(parent).map_err(|e| SproutError::StorageError {
                    message: format!("failed to create db directory: {e}"),
                })?;
            }
            Connection::open(db_path)
        }
        .map_err(|e| SproutError::StorageError {
            message: e.to_string(),
        })?;

        // Enable WAL mode for better concurrent read performance.
        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| SproutError::StorageError {
                message: e.to_string(),
            })?;

        schema::migrate(&conn)?;

        Ok(Self {
            conn: std::sync::Mutex::new(conn),
        })
    }

    /// Get a key from the key-value store.
    pub fn kv_get(&self, key: &str) -> Result<Option<String>, SproutError> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare_cached("SELECT value FROM kv WHERE key = ?1")
            .map_err(sqlite_err)?;
        let result = stmt
            .query_row([key], |row| row.get(0))
            .optional()
            .map_err(sqlite_err)?;
        Ok(result)
    }

    /// Set a key in the key-value store.
    pub fn kv_set(&self, key: &str, value: &str) -> Result<(), SproutError> {
        self.lock()?
            .execute(
                "INSERT OR REPLACE INTO kv (key, value) VALUES (?1, ?2)",
                [key, value],
            )
            .map_err(sqlite_err)?;
        Ok(())
    }

    /// Delete a key from the key-value store.
    pub fn kv_delete(&self, key: &str) -> Result<(), SproutError> {
        self.lock()?
            .execute("DELETE FROM kv WHERE key = ?1", [key])
            .map_err(sqlite_err)?;
        Ok(())
    }

    /// Lock the mutex and return the connection guard.
    pub(crate) fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, SproutError> {
        self.conn.lock().map_err(|e| SproutError::StorageError {
            message: format!("lock poisoned: {e}"),
        })
    }
}

/// Extension trait to convert rusqlite's `Option` result pattern.
trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
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
    fn open_in_memory() {
        let store = Store::open(":memory:").unwrap();
        assert!(store.kv_get("nonexistent").unwrap().is_none());
    }

    #[test]
    fn kv_roundtrip() {
        let store = Store::open(":memory:").unwrap();
        store.kv_set("token", "sprout_abc123").unwrap();
        assert_eq!(
            store.kv_get("token").unwrap(),
            Some("sprout_abc123".to_string())
        );
    }

    #[test]
    fn kv_overwrite() {
        let store = Store::open(":memory:").unwrap();
        store.kv_set("key", "v1").unwrap();
        store.kv_set("key", "v2").unwrap();
        assert_eq!(store.kv_get("key").unwrap(), Some("v2".to_string()));
    }

    #[test]
    fn kv_delete() {
        let store = Store::open(":memory:").unwrap();
        store.kv_set("key", "val").unwrap();
        store.kv_delete("key").unwrap();
        assert!(store.kv_get("key").unwrap().is_none());
    }
}
