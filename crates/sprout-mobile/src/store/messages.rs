//! SQLite cache queries for message data.
//! Insert, list by channel, and lookup messages with thread metadata.
#![allow(dead_code)]

use rusqlite::params;

use crate::error::SproutError;
use crate::types::Message;

use super::Store;

impl Store {
    /// Insert a message into the cache.
    pub fn insert_message(&self, msg: &Message, raw_json: &str) -> Result<(), SproutError> {
        self.lock()?
            .execute(
                "INSERT OR REPLACE INTO messages
                    (event_id, channel_id, author_pubkey, content, kind, created_at, reply_to, thread_root, reply_count, raw_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    msg.event_id,
                    msg.channel_id,
                    msg.author_pubkey,
                    msg.content,
                    msg.kind,
                    msg.created_at,
                    msg.reply_to,
                    msg.thread_root,
                    msg.reply_count,
                    raw_json,
                ],
            )
            .map_err(sqlite_err)?;
        Ok(())
    }

    /// List cached messages for a channel, ordered by created_at DESC.
    /// Optionally fetch only messages before a given timestamp.
    pub fn list_messages(
        &self,
        channel_id: &str,
        before: Option<i64>,
        limit: u32,
    ) -> Result<Vec<Message>, SproutError> {
        let (sql, params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = if let Some(ts) = before {
            (
                "SELECT event_id, channel_id, author_pubkey, content, kind, created_at, reply_to, thread_root, reply_count
                 FROM messages WHERE channel_id = ?1 AND created_at < ?2
                 ORDER BY created_at DESC LIMIT ?3",
                vec![
                    Box::new(channel_id.to_string()),
                    Box::new(ts),
                    Box::new(limit),
                ],
            )
        } else {
            (
                "SELECT event_id, channel_id, author_pubkey, content, kind, created_at, reply_to, thread_root, reply_count
                 FROM messages WHERE channel_id = ?1
                 ORDER BY created_at DESC LIMIT ?2",
                vec![Box::new(channel_id.to_string()), Box::new(limit)],
            )
        };

        let conn = self.lock()?;
        let mut stmt = conn.prepare_cached(sql).map_err(sqlite_err)?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt
            .query_map(param_refs.as_slice(), row_to_message)
            .map_err(sqlite_err)?;

        let mut messages = Vec::new();
        for row in rows {
            messages.push(row.map_err(sqlite_err)?);
        }
        Ok(messages)
    }

    /// Get a single message by event ID.
    pub fn get_message(&self, event_id: &str) -> Result<Option<Message>, SproutError> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare_cached(
                "SELECT event_id, channel_id, author_pubkey, content, kind, created_at, reply_to, thread_root, reply_count
                 FROM messages WHERE event_id = ?1",
            )
            .map_err(sqlite_err)?;

        let result = stmt.query_row([event_id], row_to_message);

        match result {
            Ok(m) => Ok(Some(m)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(sqlite_err(e)),
        }
    }
}

fn row_to_message(row: &rusqlite::Row<'_>) -> Result<Message, rusqlite::Error> {
    Ok(Message {
        event_id: row.get(0)?,
        channel_id: row.get(1)?,
        author_pubkey: row.get(2)?,
        content: row.get(3)?,
        kind: row.get(4)?,
        created_at: row.get(5)?,
        reply_to: row.get(6)?,
        thread_root: row.get(7)?,
        reply_count: row.get(8)?,
        // These fields are populated from API responses, not the cache.
        reactions: Vec::new(),
        media: Vec::new(),
        author_profile: None,
    })
}

fn sqlite_err(e: rusqlite::Error) -> SproutError {
    SproutError::StorageError {
        message: e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_message(event_id: &str, channel_id: &str, created_at: i64) -> Message {
        Message {
            event_id: event_id.to_string(),
            channel_id: channel_id.to_string(),
            author_pubkey: "deadbeef".to_string(),
            content: "hello".to_string(),
            kind: 9,
            created_at,
            reply_to: None,
            thread_root: None,
            reactions: Vec::new(),
            media: Vec::new(),
            reply_count: 0,
            author_profile: None,
        }
    }

    #[test]
    fn insert_and_get() {
        let store = Store::open(":memory:").unwrap();
        let msg = test_message("evt-1", "ch-1", 1000);
        store.insert_message(&msg, "{}").unwrap();

        let loaded = store.get_message("evt-1").unwrap().unwrap();
        assert_eq!(loaded.content, "hello");
        assert_eq!(loaded.kind, 9);
    }

    #[test]
    fn list_by_channel() {
        let store = Store::open(":memory:").unwrap();
        store
            .insert_message(&test_message("e1", "ch-1", 1000), "{}")
            .unwrap();
        store
            .insert_message(&test_message("e2", "ch-1", 2000), "{}")
            .unwrap();
        store
            .insert_message(&test_message("e3", "ch-2", 3000), "{}")
            .unwrap();

        let msgs = store.list_messages("ch-1", None, 50).unwrap();
        assert_eq!(msgs.len(), 2);
        // Should be ordered DESC.
        assert_eq!(msgs[0].event_id, "e2");
        assert_eq!(msgs[1].event_id, "e1");
    }

    #[test]
    fn list_with_before_cursor() {
        let store = Store::open(":memory:").unwrap();
        store
            .insert_message(&test_message("e1", "ch-1", 1000), "{}")
            .unwrap();
        store
            .insert_message(&test_message("e2", "ch-1", 2000), "{}")
            .unwrap();
        store
            .insert_message(&test_message("e3", "ch-1", 3000), "{}")
            .unwrap();

        let msgs = store.list_messages("ch-1", Some(2500), 50).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].event_id, "e2");
    }
}
