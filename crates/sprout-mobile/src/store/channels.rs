//! SQLite cache queries for channel data.
//! Insert, update, list, and lookup channels in the local cache.
#![allow(dead_code)]

use rusqlite::params;

use crate::error::SproutError;
use crate::types::{Channel, ChannelType, ChannelVisibility};

use super::Store;

impl Store {
    /// Upsert a channel into the cache.
    pub fn upsert_channel(&self, channel: &Channel) -> Result<(), SproutError> {
        self.lock()?
            .execute(
                "INSERT OR REPLACE INTO channels
                    (id, name, about, topic, channel_type, visibility, member_count, is_member, last_message_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    channel.id,
                    channel.name,
                    channel.about,
                    channel.topic,
                    channel_type_str(&channel.channel_type),
                    visibility_str(&channel.visibility),
                    channel.member_count,
                    channel.is_member,
                    channel.last_message_at,
                    chrono::Utc::now().timestamp(),
                ],
            )
            .map_err(sqlite_err)?;
        Ok(())
    }

    /// Get a channel by ID from the cache.
    pub fn get_channel(&self, id: &str) -> Result<Option<Channel>, SproutError> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare_cached(
                "SELECT id, name, about, topic, channel_type, visibility, member_count, is_member, last_message_at
                 FROM channels WHERE id = ?1",
            )
            .map_err(sqlite_err)?;

        let result = stmt.query_row([id], |row| {
            Ok(Channel {
                id: row.get(0)?,
                name: row.get(1)?,
                about: row.get(2)?,
                topic: row.get(3)?,
                channel_type: parse_channel_type(row.get::<_, String>(4)?.as_str()),
                visibility: parse_visibility(row.get::<_, String>(5)?.as_str()),
                member_count: row.get(6)?,
                last_message_at: row.get(8)?,
                is_member: row.get(7)?,
            })
        });

        match result {
            Ok(ch) => Ok(Some(ch)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(sqlite_err(e)),
        }
    }

    /// List all cached channels, optionally filtered to member-only.
    pub fn list_channels(&self, member_only: bool) -> Result<Vec<Channel>, SproutError> {
        let sql = if member_only {
            "SELECT id, name, about, topic, channel_type, visibility, member_count, is_member, last_message_at
             FROM channels WHERE is_member = 1 ORDER BY last_message_at DESC"
        } else {
            "SELECT id, name, about, topic, channel_type, visibility, member_count, is_member, last_message_at
             FROM channels ORDER BY last_message_at DESC"
        };

        let conn = self.lock()?;
        let mut stmt = conn.prepare_cached(sql).map_err(sqlite_err)?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Channel {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    about: row.get(2)?,
                    topic: row.get(3)?,
                    channel_type: parse_channel_type(row.get::<_, String>(4)?.as_str()),
                    visibility: parse_visibility(row.get::<_, String>(5)?.as_str()),
                    member_count: row.get(6)?,
                    is_member: row.get(7)?,
                    last_message_at: row.get(8)?,
                })
            })
            .map_err(sqlite_err)?;

        let mut channels = Vec::new();
        for row in rows {
            channels.push(row.map_err(sqlite_err)?);
        }
        Ok(channels)
    }
}

fn channel_type_str(ct: &ChannelType) -> &'static str {
    match ct {
        ChannelType::Stream => "stream",
        ChannelType::Forum => "forum",
        ChannelType::Dm => "dm",
    }
}

fn visibility_str(v: &ChannelVisibility) -> &'static str {
    match v {
        ChannelVisibility::Open => "open",
        ChannelVisibility::Private => "private",
    }
}

fn parse_channel_type(s: &str) -> ChannelType {
    match s {
        "forum" => ChannelType::Forum,
        "dm" => ChannelType::Dm,
        _ => ChannelType::Stream,
    }
}

fn parse_visibility(s: &str) -> ChannelVisibility {
    match s {
        "private" => ChannelVisibility::Private,
        _ => ChannelVisibility::Open,
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

    fn test_channel(id: &str, name: &str) -> Channel {
        Channel {
            id: id.to_string(),
            name: name.to_string(),
            about: None,
            topic: None,
            channel_type: ChannelType::Stream,
            visibility: ChannelVisibility::Open,
            member_count: 5,
            last_message_at: Some(1000),
            is_member: true,
        }
    }

    #[test]
    fn upsert_and_get() {
        let store = Store::open(":memory:").unwrap();
        let ch = test_channel("ch-1", "general");
        store.upsert_channel(&ch).unwrap();

        let loaded = store.get_channel("ch-1").unwrap().unwrap();
        assert_eq!(loaded.name, "general");
        assert_eq!(loaded.member_count, 5);
        assert!(loaded.is_member);
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let store = Store::open(":memory:").unwrap();
        assert!(store.get_channel("nope").unwrap().is_none());
    }

    #[test]
    fn list_channels_member_only() {
        let store = Store::open(":memory:").unwrap();
        let mut ch1 = test_channel("ch-1", "general");
        ch1.is_member = true;
        let mut ch2 = test_channel("ch-2", "random");
        ch2.is_member = false;

        store.upsert_channel(&ch1).unwrap();
        store.upsert_channel(&ch2).unwrap();

        let all = store.list_channels(false).unwrap();
        assert_eq!(all.len(), 2);

        let member_only = store.list_channels(true).unwrap();
        assert_eq!(member_only.len(), 1);
        assert_eq!(member_only[0].name, "general");
    }
}
