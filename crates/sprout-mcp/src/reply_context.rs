use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct ReplyContextEntry {
    parent_event_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// The ACP-provided reply context for the channel's current reply scope.
pub struct ActiveReplyContext {
    /// The forced parent event ID for this scope. `None` means send top-level.
    pub parent_event_id: Option<String>,
}

#[derive(Debug, Clone)]
/// Reads the ACP-managed reply-context sidecar file for deterministic thread placement.
pub struct ReplyContextStore {
    path: Option<PathBuf>,
}

impl ReplyContextStore {
    /// Build a store from the optional `SPROUT_REPLY_CONTEXT_FILE` environment variable.
    pub fn from_env() -> Self {
        let path = std::env::var("SPROUT_REPLY_CONTEXT_FILE")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from);
        Self { path }
    }

    /// Return the currently-active reply context for `channel_id`, if ACP published one.
    pub fn active_context_for_channel(&self, channel_id: &str) -> Option<ActiveReplyContext> {
        let path = self.path.as_ref()?;
        let raw = fs::read_to_string(path).ok()?;
        let entries: HashMap<String, ReplyContextEntry> = serde_json::from_str(&raw).ok()?;
        let entry = entries.get(channel_id)?;
        if let Some(parent_event_id) = entry.parent_event_id.as_deref() {
            validate_hex64(parent_event_id)?;
        }
        Some(ActiveReplyContext {
            parent_event_id: entry.parent_event_id.clone(),
        })
    }
}

fn validate_hex64(value: &str) -> Option<()> {
    if value.len() == 64 && value.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(())
    } else {
        None
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
            "sprout-mcp-{name}-{}-{nanos}.json",
            std::process::id()
        ))
    }

    #[test]
    fn active_context_for_channel_reads_nested_parent() {
        let path = temp_path("reply-context");
        fs::write(
            &path,
            format!(
                "{{\"channel-1\":{{\"parent_event_id\":\"{}\"}}}}",
                "a".repeat(64)
            ),
        )
        .expect("write should succeed");

        let store = ReplyContextStore {
            path: Some(path.clone()),
        };
        assert_eq!(
            store.active_context_for_channel("channel-1"),
            Some(ActiveReplyContext {
                parent_event_id: Some("a".repeat(64)),
            })
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn active_context_for_channel_preserves_explicit_top_level() {
        let path = temp_path("reply-context-top-level");
        fs::write(&path, "{\"channel-1\":{\"parent_event_id\":null}}")
            .expect("write should succeed");

        let store = ReplyContextStore {
            path: Some(path.clone()),
        };
        assert_eq!(
            store.active_context_for_channel("channel-1"),
            Some(ActiveReplyContext {
                parent_event_id: None,
            })
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn active_context_for_channel_ignores_invalid_parent() {
        let path = temp_path("reply-context-invalid");
        fs::write(&path, "{\"channel-1\":{\"parent_event_id\":\"bad\"}}")
            .expect("write should succeed");

        let store = ReplyContextStore {
            path: Some(path.clone()),
        };
        assert_eq!(store.active_context_for_channel("channel-1"), None);

        let _ = fs::remove_file(path);
    }
}
