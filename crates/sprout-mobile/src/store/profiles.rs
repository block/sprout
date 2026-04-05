//! SQLite cache queries for user profile data.
//! Insert, batch lookup, and search profiles in the local cache.
#![allow(dead_code)]

use rusqlite::params;

use crate::error::SproutError;
use crate::types::UserProfile;

use super::Store;

impl Store {
    /// Upsert a user profile into the cache.
    pub fn upsert_profile(&self, profile: &UserProfile) -> Result<(), SproutError> {
        self.lock()?
            .execute(
                "INSERT OR REPLACE INTO profiles
                    (pubkey, display_name, picture, about, nip05, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    profile.pubkey,
                    profile.display_name,
                    profile.picture,
                    profile.about,
                    profile.nip05,
                    chrono::Utc::now().timestamp(),
                ],
            )
            .map_err(sqlite_err)?;
        Ok(())
    }

    /// Get a profile by pubkey from the cache.
    pub fn get_profile(&self, pubkey: &str) -> Result<Option<UserProfile>, SproutError> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare_cached(
                "SELECT pubkey, display_name, picture, about, nip05
                 FROM profiles WHERE pubkey = ?1",
            )
            .map_err(sqlite_err)?;

        let result = stmt.query_row([pubkey], row_to_profile);

        match result {
            Ok(p) => Ok(Some(p)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(sqlite_err(e)),
        }
    }

    /// Batch-fetch profiles by pubkeys.
    pub fn get_profiles_batch(&self, pubkeys: &[&str]) -> Result<Vec<UserProfile>, SproutError> {
        if pubkeys.is_empty() {
            return Ok(Vec::new());
        }

        // Build a parameterized IN clause.
        let placeholders: Vec<String> = (1..=pubkeys.len()).map(|i| format!("?{i}")).collect();
        let sql = format!(
            "SELECT pubkey, display_name, picture, about, nip05
             FROM profiles WHERE pubkey IN ({})",
            placeholders.join(", ")
        );

        let conn = self.lock()?;
        let mut stmt = conn.prepare(&sql).map_err(sqlite_err)?;
        let params: Vec<&dyn rusqlite::types::ToSql> = pubkeys
            .iter()
            .map(|p| p as &dyn rusqlite::types::ToSql)
            .collect();
        let rows = stmt
            .query_map(params.as_slice(), row_to_profile)
            .map_err(sqlite_err)?;

        let mut profiles = Vec::new();
        for row in rows {
            profiles.push(row.map_err(sqlite_err)?);
        }
        Ok(profiles)
    }
}

fn row_to_profile(row: &rusqlite::Row<'_>) -> Result<UserProfile, rusqlite::Error> {
    Ok(UserProfile {
        pubkey: row.get(0)?,
        display_name: row.get(1)?,
        picture: row.get(2)?,
        about: row.get(3)?,
        nip05: row.get(4)?,
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

    fn test_profile(pubkey: &str, name: &str) -> UserProfile {
        UserProfile {
            pubkey: pubkey.to_string(),
            display_name: Some(name.to_string()),
            picture: None,
            about: None,
            nip05: None,
        }
    }

    #[test]
    fn upsert_and_get() {
        let store = Store::open(":memory:").unwrap();
        let p = test_profile("aabb", "Alice");
        store.upsert_profile(&p).unwrap();

        let loaded = store.get_profile("aabb").unwrap().unwrap();
        assert_eq!(loaded.display_name, Some("Alice".to_string()));
    }

    #[test]
    fn batch_fetch() {
        let store = Store::open(":memory:").unwrap();
        store.upsert_profile(&test_profile("aa", "Alice")).unwrap();
        store.upsert_profile(&test_profile("bb", "Bob")).unwrap();
        store.upsert_profile(&test_profile("cc", "Carol")).unwrap();

        let results = store.get_profiles_batch(&["aa", "cc"]).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn batch_fetch_empty() {
        let store = Store::open(":memory:").unwrap();
        let results = store.get_profiles_batch(&[]).unwrap();
        assert!(results.is_empty());
    }
}
