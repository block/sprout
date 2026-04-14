use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ReplyContextEntry {
    parent_event_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ReplyContextStore {
    path: PathBuf,
    write_lock: Arc<Mutex<()>>,
}

impl ReplyContextStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            write_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn set_channel_parent(
        &self,
        channel_id: Uuid,
        parent_event_id: Option<&str>,
    ) -> io::Result<()> {
        if let Some(parent) = parent_event_id {
            validate_hex64(parent, "parent_event_id")?;
        }

        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| io::Error::other("reply context store lock poisoned"))?;
        let mut contexts = self.read_all()?;
        contexts.insert(
            channel_id.to_string(),
            ReplyContextEntry {
                parent_event_id: parent_event_id.map(str::to_owned),
            },
        );
        self.write_all(&contexts)
    }

    #[cfg(test)]
    pub fn clear_channel(&self, channel_id: Uuid) -> io::Result<()> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| io::Error::other("reply context store lock poisoned"))?;
        let mut contexts = self.read_all()?;
        contexts.remove(&channel_id.to_string());
        self.write_all(&contexts)
    }

    fn read_all(&self) -> io::Result<HashMap<String, ReplyContextEntry>> {
        match fs::read_to_string(&self.path) {
            Ok(raw) => serde_json::from_str(&raw)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(HashMap::new()),
            Err(err) => Err(err),
        }
    }

    fn write_all(&self, contexts: &HashMap<String, ReplyContextEntry>) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let serialized = serde_json::to_vec(contexts)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let tmp_path = self
            .path
            .with_extension(format!("tmp-{}", std::process::id()));
        fs::write(&tmp_path, serialized)?;
        fs::rename(&tmp_path, &self.path)
    }
}

fn validate_hex64(value: &str, label: &str) -> io::Result<()> {
    if value.len() == 64 && value.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{label} must be exactly 64 hex characters"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be monotonic enough for tests")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "sprout-acp-{name}-{}-{nanos}.json",
            std::process::id()
        ))
    }

    #[test]
    fn set_and_clear_channel_parent_round_trips() {
        let path = temp_path("reply-context");
        let store = ReplyContextStore::new(path.clone());
        let channel_id = Uuid::new_v4();
        let parent = "a".repeat(64);

        store
            .set_channel_parent(channel_id, Some(&parent))
            .expect("set should succeed");
        let raw = fs::read_to_string(&path).expect("file should exist");
        let parsed: HashMap<String, ReplyContextEntry> =
            serde_json::from_str(&raw).expect("json should parse");
        assert_eq!(
            parsed.get(&channel_id.to_string()),
            Some(&ReplyContextEntry {
                parent_event_id: Some(parent.clone()),
            })
        );

        store
            .clear_channel(channel_id)
            .expect("clear should succeed");
        let raw = fs::read_to_string(&path).expect("file should still exist");
        let parsed: HashMap<String, ReplyContextEntry> =
            serde_json::from_str(&raw).expect("json should parse");
        assert!(parsed.is_empty());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn set_channel_parent_rejects_invalid_hex() {
        let store = ReplyContextStore::new(temp_path("reply-context-invalid"));
        let err = store
            .set_channel_parent(Uuid::new_v4(), Some("not-hex"))
            .expect_err("invalid parent should fail");
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }
}
