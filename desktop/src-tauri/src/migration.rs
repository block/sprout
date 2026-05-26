//! Data migrations and worktree sync for the Sprout desktop app.
//!
//! **Legacy migration** (`migrate_legacy_data_dir`): One-time, idempotent
//! per-file copy from the legacy `com.wesb.sprout` app data directory to
//! the current directory. Runs on every launch; each file's existence at
//! the destination is its own guard.
//!
//! **Worktree sync** (`sync_shared_agent_data`): Per-launch copy-with-
//! overwrite of agent data files from the canonical dev data directory
//! (`xyz.block.sprout.app.dev`) to the current worktree data directory.
//! Only runs when `SPROUT_SHARE_IDENTITY=1` and `SPROUT_PRIVATE_KEY` is
//! set. Overwrites on every launch so worktree data stays current.

use std::path::{Path, PathBuf};
use tauri::Manager;

const LEGACY_DATA_DIR_NAME: &str = "com.wesb.sprout";

const CANONICAL_DEV_IDENTIFIER: &str = "xyz.block.sprout.app.dev";

/// JSON files synced from the canonical dev data directory to worktree
/// data directories. This is the agent-data subset of [`LEGACY_FILES`];
/// `identity.key` is deliberately excluded because worktree instances
/// receive their identity via the `SPROUT_PRIVATE_KEY` env var.
/// Only data files — never `agent-pids/` or `logs/`.
///
/// NOTE: `agents/packs/` is intentionally excluded — recursive directory
/// sync is out of scope. Pack personas will appear in the worktree but
/// agents with `persona_pack_path` may fail if the ACP reads pack files
/// at runtime. Install packs in the worktree separately if needed.
const SHARED_AGENT_FILES: &[&str] = &[
    "agents/managed-agents.json",
    "agents/personas.json",
    "agents/teams.json",
];

/// Known files to migrate from the legacy data directory.
///
/// Agent logs and `.window-state.json` are excluded — logs are ephemeral and
/// the window-state plugin recreates its file automatically.
const LEGACY_FILES: &[&str] = &[
    "identity.key",
    "agents/managed-agents.json",
    "agents/personas.json",
    "agents/teams.json",
];

fn sibling_data_dir(current: &Path, name: &str) -> Option<PathBuf> {
    current.parent().map(|p| p.join(name))
}

fn legacy_data_dir(current: &Path) -> Option<PathBuf> {
    sibling_data_dir(current, LEGACY_DATA_DIR_NAME)
}

fn canonical_dev_data_dir(current: &Path) -> Option<PathBuf> {
    sibling_data_dir(current, CANONICAL_DEV_IDENTIFIER)
}

/// Copy a single file from `old_dir/rel` to `new_dir/rel`, creating parent
/// directories as needed. Skips the file if it already exists at the
/// destination.
///
/// Returns `true` if the file was copied, `false` if skipped or missing.
fn migrate_file(old_dir: &Path, new_dir: &Path, rel: &str) -> bool {
    let src = old_dir.join(rel);
    let dst = new_dir.join(rel);

    if dst.exists() {
        return false; // Already present — nothing to do.
    }

    if !src.exists() {
        return false; // Nothing to migrate.
    }

    // Ensure parent directories exist (e.g. `agents/`).
    if let Some(parent) = dst.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!(
                "sprout-desktop: migration: failed to create {}: {e}",
                parent.display()
            );
            return false;
        }
    }

    match std::fs::copy(&src, &dst) {
        Ok(_) => {
            eprintln!("sprout-desktop: migration: copied {rel}");
            true
        }
        Err(e) => {
            eprintln!("sprout-desktop: migration: failed to copy {rel}: {e}");
            false
        }
    }
}

/// Migrate known files from the legacy `com.wesb.sprout` app data directory
/// to the current directory.
///
/// Called in `setup()` **before** `resolve_persisted_identity` so the persisted
/// key is available at the new path on first launch after the identifier change.
pub fn migrate_legacy_data_dir(app: &tauri::AppHandle) {
    let current_dir = match app.path().app_data_dir() {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("sprout-desktop: migration: cannot resolve app data dir: {e}");
            return;
        }
    };

    let old_dir = match legacy_data_dir(&current_dir) {
        Some(dir) => dir,
        None => {
            eprintln!("sprout-desktop: migration: cannot compute legacy data dir (no parent)");
            return;
        }
    };

    if !old_dir.exists() {
        return; // Nothing to migrate.
    }

    let mut copied = 0u32;
    for rel in LEGACY_FILES {
        if migrate_file(&old_dir, &current_dir, rel) {
            copied += 1;
        }
    }

    if copied > 0 {
        eprintln!("sprout-desktop: migration: {copied} file(s) migrated from legacy data dir");
    }
}

/// Read a JSON array of objects from `path`, apply `f` to each object,
/// and write back if any mutation returned `true`.
fn patch_json_records(
    path: &Path,
    mut f: impl FnMut(&mut serde_json::Map<String, serde_json::Value>) -> bool,
) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    let Ok(mut records) = serde_json::from_str::<Vec<serde_json::Value>>(&content) else {
        eprintln!(
            "sprout-desktop: patch-json-records: failed to parse {}",
            path.display()
        );
        return;
    };
    let mut changed = false;
    for record in &mut records {
        if let Some(obj) = record.as_object_mut() {
            changed |= f(obj);
        }
    }
    if changed {
        if let Ok(bytes) = serde_json::to_vec_pretty(&records) {
            let _ = std::fs::write(path, bytes);
        }
    }
}

/// After copying managed-agents.json from the canonical dir, remove
/// process-local runtime state that would cause the worktree instance
/// to kill the canonical instance's running agents.
fn scrub_managed_agents_runtime_state(path: &Path) {
    patch_json_records(path, |obj| {
        const RUNTIME_FIELDS: &[&str] = &[
            "runtime_pid",
            "last_error",
            "last_exit_code",
            "last_stopped_at",
            "last_started_at",
            "backend_agent_id",
        ];
        let mut scrubbed = false;
        for field in RUNTIME_FIELDS {
            if obj.remove(*field).is_some() {
                scrubbed = true;
            }
        }
        scrubbed
    });
}

/// Copy shared agent data files from the canonical dev data directory to
/// the current (worktree) data directory, overwriting any existing files.
///
/// Guards:
/// - `SPROUT_SHARE_IDENTITY` must be `"1"`
/// - `SPROUT_PRIVATE_KEY` must parse as valid `nostr::Keys`
/// - The canonical dir must differ from the current dir (skip if we ARE canonical)
/// - The canonical dir must exist
///
/// Unlike `migrate_file()`, this always overwrites — pre-existing worktree
/// data directories already have empty/default files that must be replaced.
pub fn sync_shared_agent_data(app: &tauri::AppHandle) {
    // Guard: only runs when sharing identity with a worktree.
    let is_shared = std::env::var("SPROUT_SHARE_IDENTITY")
        .map(|v| v == "1")
        .unwrap_or(false);
    if !is_shared {
        return;
    }

    // Guard: SPROUT_PRIVATE_KEY must be a valid nostr key.
    let has_valid_key = std::env::var("SPROUT_PRIVATE_KEY")
        .ok()
        .and_then(|k| k.parse::<nostr::Keys>().ok())
        .is_some();
    if !has_valid_key {
        eprintln!(
            "sprout-desktop: shared-agent-sync: SPROUT_PRIVATE_KEY missing or invalid, skipping"
        );
        return;
    }

    let current_dir = match app.path().app_data_dir() {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("sprout-desktop: shared-agent-sync: cannot resolve app data dir: {e}");
            return;
        }
    };

    let canonical_dir = match canonical_dev_data_dir(&current_dir) {
        Some(dir) => dir,
        None => {
            eprintln!(
                "sprout-desktop: shared-agent-sync: cannot compute canonical dir (no parent)"
            );
            return;
        }
    };

    // Guard: skip if we ARE the canonical instance.
    // Use canonicalize to handle case-insensitive FS and symlinks.
    let current_canonical =
        std::fs::canonicalize(&current_dir).unwrap_or_else(|_| current_dir.clone());
    let source_canonical =
        std::fs::canonicalize(&canonical_dir).unwrap_or_else(|_| canonical_dir.clone());
    if current_canonical == source_canonical {
        return;
    }

    // Guard: skip if canonical dir doesn't exist.
    if !canonical_dir.exists() {
        eprintln!(
            "sprout-desktop: shared-agent-sync: canonical dir does not exist: {}",
            canonical_dir.display()
        );
        return;
    }

    let mut synced = 0u32;
    for rel in SHARED_AGENT_FILES {
        let src = canonical_dir.join(rel);
        let dst = current_dir.join(rel);

        if !src.exists() {
            continue;
        }

        // Ensure parent directories exist (e.g. `agents/`).
        if let Some(parent) = dst.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                eprintln!(
                    "sprout-desktop: shared-agent-sync: failed to create {}: {e}",
                    parent.display()
                );
                continue;
            }
        }

        match std::fs::copy(&src, &dst) {
            Ok(_) => synced += 1,
            Err(e) => {
                eprintln!("sprout-desktop: shared-agent-sync: failed to copy {rel}: {e}");
            }
        }
    }

    // Scrub runtime-local state from the copied managed-agents.json to
    // prevent the worktree from killing the canonical instance's running agents.
    let managed_agents_dst = current_dir.join("agents/managed-agents.json");
    if managed_agents_dst.exists() {
        scrub_managed_agents_runtime_state(&managed_agents_dst);
    }

    if synced > 0 {
        eprintln!(
            "sprout-desktop: shared-agent-sync: {synced} file(s) synced from {}",
            canonical_dir.display()
        );
    }
}

fn reconcile_mcp_commands_in_file(path: &Path) {
    patch_json_records(path, |obj| {
        let agent_command = match obj.get("agent_command").and_then(|v| v.as_str()) {
            Some(cmd) => cmd.to_string(),
            None => return false,
        };
        let Some(provider) = crate::managed_agents::known_acp_provider(&agent_command) else {
            return false;
        };
        let expected = provider.mcp_command.unwrap_or("");
        let current = obj
            .get("mcp_command")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if current != expected {
            eprintln!(
                "sprout-desktop: provider-reconcile: {:?} ({:?}): mcp_command {:?} → {:?}",
                obj.get("name").and_then(|v| v.as_str()).unwrap_or("?"),
                agent_command,
                current,
                expected,
            );
            obj.insert(
                "mcp_command".to_string(),
                serde_json::Value::String(expected.to_string()),
            );
            true
        } else {
            false
        }
    });
}

/// Reconcile `mcp_command` values in managed-agents.json against the
/// discovery table. Known providers get their canonical mcp_command;
/// unknown/custom agents are left untouched.
pub fn reconcile_provider_mcp_commands(app: &tauri::AppHandle) {
    let Ok(dir) = app.path().app_data_dir() else {
        return;
    };
    let path = dir.join("agents/managed-agents.json");
    if !path.exists() {
        return;
    }
    reconcile_mcp_commands_in_file(&path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_data_dir_replaces_last_component() {
        let current = PathBuf::from("/Users/me/Library/Application Support/xyz.block.sprout.app");
        let legacy = legacy_data_dir(&current).unwrap();
        assert_eq!(
            legacy,
            PathBuf::from("/Users/me/Library/Application Support/com.wesb.sprout")
        );
    }

    #[test]
    fn legacy_data_dir_returns_none_for_root() {
        let current = PathBuf::from("/");
        let _ = legacy_data_dir(&current);
    }

    /// Helper: create a temp dir structure mimicking the old `com.wesb.sprout`
    /// layout and return `(parent_dir, old_dir, new_dir)`.
    fn setup_legacy_layout() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let parent = tempfile::tempdir().unwrap();
        let old_dir = parent.path().join(LEGACY_DATA_DIR_NAME);
        let new_dir = parent.path().join("xyz.block.sprout.app");

        std::fs::create_dir_all(old_dir.join("agents")).unwrap();
        std::fs::write(old_dir.join("identity.key"), "nsec1-fake-key-data").unwrap();
        std::fs::write(
            old_dir.join("agents/managed-agents.json"),
            r#"[{"id":"a1"}]"#,
        )
        .unwrap();
        std::fs::write(old_dir.join("agents/personas.json"), "[]").unwrap();
        std::fs::write(old_dir.join("agents/teams.json"), "[]").unwrap();

        (parent, old_dir, new_dir)
    }

    #[test]
    fn migrate_file_copies_when_missing_at_dest() {
        let (_parent, old_dir, new_dir) = setup_legacy_layout();

        assert!(migrate_file(&old_dir, &new_dir, "identity.key"));
        assert_eq!(
            std::fs::read_to_string(new_dir.join("identity.key")).unwrap(),
            "nsec1-fake-key-data"
        );
    }

    #[test]
    fn migrate_file_creates_parent_dirs() {
        let (_parent, old_dir, new_dir) = setup_legacy_layout();

        // agents/ doesn't exist in new_dir yet.
        assert!(migrate_file(&old_dir, &new_dir, "agents/teams.json"));
        assert_eq!(
            std::fs::read_to_string(new_dir.join("agents/teams.json")).unwrap(),
            "[]"
        );
    }

    #[test]
    fn migrate_file_skips_when_dest_exists() {
        let (_parent, old_dir, new_dir) = setup_legacy_layout();

        // Pre-create the file at the new location with different content.
        std::fs::create_dir_all(&new_dir).unwrap();
        std::fs::write(new_dir.join("identity.key"), "nsec1-new-key").unwrap();

        // Should skip — returns false.
        assert!(!migrate_file(&old_dir, &new_dir, "identity.key"));

        // Original content preserved.
        assert_eq!(
            std::fs::read_to_string(new_dir.join("identity.key")).unwrap(),
            "nsec1-new-key"
        );
    }

    #[test]
    fn migrate_file_skips_when_source_missing() {
        let (_parent, old_dir, new_dir) = setup_legacy_layout();

        // File that doesn't exist in old dir.
        assert!(!migrate_file(&old_dir, &new_dir, "nonexistent.json"));
        assert!(!new_dir.join("nonexistent.json").exists());
    }

    #[test]
    fn migrate_all_known_files() {
        let (_parent, old_dir, new_dir) = setup_legacy_layout();

        let mut copied = 0u32;
        for rel in LEGACY_FILES {
            if migrate_file(&old_dir, &new_dir, rel) {
                copied += 1;
            }
        }

        assert_eq!(copied, 4);
        assert_eq!(
            std::fs::read_to_string(new_dir.join("identity.key")).unwrap(),
            "nsec1-fake-key-data"
        );
        assert_eq!(
            std::fs::read_to_string(new_dir.join("agents/managed-agents.json")).unwrap(),
            r#"[{"id":"a1"}]"#
        );
        assert_eq!(
            std::fs::read_to_string(new_dir.join("agents/personas.json")).unwrap(),
            "[]"
        );
        assert_eq!(
            std::fs::read_to_string(new_dir.join("agents/teams.json")).unwrap(),
            "[]"
        );

        // Old directory must still exist.
        assert!(old_dir.exists());
    }

    #[test]
    fn migrate_is_idempotent() {
        let (_parent, old_dir, new_dir) = setup_legacy_layout();

        // First pass: copies everything.
        let first_pass: u32 = LEGACY_FILES
            .iter()
            .map(|rel| u32::from(migrate_file(&old_dir, &new_dir, rel)))
            .sum();
        assert_eq!(first_pass, 4);

        // Second pass: skips everything (all files already exist).
        let second_pass: u32 = LEGACY_FILES
            .iter()
            .map(|rel| u32::from(migrate_file(&old_dir, &new_dir, rel)))
            .sum();
        assert_eq!(second_pass, 0);
    }

    #[test]
    fn migrate_partial_only_copies_missing() {
        let (_parent, old_dir, new_dir) = setup_legacy_layout();

        // Pre-create identity.key in new dir — only the other 3 should copy.
        std::fs::create_dir_all(&new_dir).unwrap();
        std::fs::write(new_dir.join("identity.key"), "nsec1-already-here").unwrap();

        let copied: u32 = LEGACY_FILES
            .iter()
            .map(|rel| u32::from(migrate_file(&old_dir, &new_dir, rel)))
            .sum();
        assert_eq!(copied, 3);

        // identity.key should be untouched.
        assert_eq!(
            std::fs::read_to_string(new_dir.join("identity.key")).unwrap(),
            "nsec1-already-here"
        );
    }

    #[test]
    fn canonical_dev_data_dir_replaces_last_component() {
        let current = PathBuf::from(
            "/Users/me/Library/Application Support/xyz.block.sprout.app.dev.my-branch",
        );
        let canonical = canonical_dev_data_dir(&current).unwrap();
        assert_eq!(
            canonical,
            PathBuf::from("/Users/me/Library/Application Support/xyz.block.sprout.app.dev")
        );
    }

    #[test]
    fn canonical_dev_data_dir_returns_none_for_root() {
        // A root path has no parent — should return None.
        assert!(canonical_dev_data_dir(Path::new("/")).is_none());
    }

    /// Helper: create a temp dir structure mimicking canonical + worktree layout.
    /// Returns `(parent_dir_handle, canonical_dir, worktree_dir)`.
    fn setup_sync_layout() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let parent = tempfile::tempdir().unwrap();
        let canonical = parent.path().join(CANONICAL_DEV_IDENTIFIER);
        let worktree = parent.path().join("xyz.block.sprout.app.dev.my-branch");

        // Populate canonical with agent data.
        std::fs::create_dir_all(canonical.join("agents")).unwrap();
        std::fs::write(
            canonical.join("agents/managed-agents.json"),
            r#"[{"id":"agent-1"}]"#,
        )
        .unwrap();
        std::fs::write(
            canonical.join("agents/personas.json"),
            r#"[{"id":"builtin:solo"}]"#,
        )
        .unwrap();
        std::fs::write(canonical.join("agents/teams.json"), r#"[{"id":"team-1"}]"#).unwrap();

        (parent, canonical, worktree)
    }

    /// Helper: sync files directly (without a Tauri AppHandle) for unit testing.
    /// Mirrors the core copy loop of `sync_shared_agent_data` but takes explicit
    /// paths. Does NOT include the post-copy `scrub_managed_agents_runtime_state`
    /// call — tests that need scrubbing must call it explicitly after `sync_files`.
    /// This split exists because `sync_shared_agent_data` requires a live Tauri
    /// AppHandle and cannot be unit-tested directly.
    fn sync_files(canonical: &Path, worktree: &Path) -> u32 {
        let mut synced = 0u32;
        for rel in SHARED_AGENT_FILES {
            let src = canonical.join(rel);
            let dst = worktree.join(rel);
            if !src.exists() {
                continue;
            }
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::copy(&src, &dst).unwrap();
            synced += 1;
        }
        synced
    }

    #[test]
    fn sync_copies_files_to_fresh_worktree() {
        let (_parent, canonical, worktree) = setup_sync_layout();

        let synced = sync_files(&canonical, &worktree);

        assert_eq!(synced, 3);
        assert_eq!(
            std::fs::read_to_string(worktree.join("agents/managed-agents.json")).unwrap(),
            r#"[{"id":"agent-1"}]"#,
        );
        assert_eq!(
            std::fs::read_to_string(worktree.join("agents/personas.json")).unwrap(),
            r#"[{"id":"builtin:solo"}]"#,
        );
        assert_eq!(
            std::fs::read_to_string(worktree.join("agents/teams.json")).unwrap(),
            r#"[{"id":"team-1"}]"#,
        );
    }

    #[test]
    fn sync_overwrites_existing_files() {
        let (_parent, canonical, worktree) = setup_sync_layout();

        // Pre-create worktree with stale/empty data (the real-world scenario).
        std::fs::create_dir_all(worktree.join("agents")).unwrap();
        std::fs::write(worktree.join("agents/managed-agents.json"), "[]").unwrap();
        std::fs::write(worktree.join("agents/personas.json"), "[]").unwrap();
        std::fs::write(worktree.join("agents/teams.json"), "[]").unwrap();

        let synced = sync_files(&canonical, &worktree);

        assert_eq!(synced, 3);
        // Must contain canonical data, NOT the empty arrays.
        assert_eq!(
            std::fs::read_to_string(worktree.join("agents/managed-agents.json")).unwrap(),
            r#"[{"id":"agent-1"}]"#,
        );
    }

    #[test]
    fn canonical_dev_data_dir_returns_self_for_canonical_instance() {
        // When the current app data dir IS the canonical dev identifier,
        // canonical_dev_data_dir returns the exact same path — the caller
        // (sync_shared_agent_data) uses this equality to skip the sync.
        // The env-var guards (SPROUT_SHARE_IDENTITY, SPROUT_PRIVATE_KEY)
        // require a live Tauri AppHandle and are covered by integration
        // testing only.
        let current =
            PathBuf::from("/Users/me/Library/Application Support/xyz.block.sprout.app.dev");
        assert_eq!(canonical_dev_data_dir(&current).unwrap(), current);

        // Also verify with a temp dir on the real filesystem.
        let parent = tempfile::tempdir().unwrap();
        let canonical = parent.path().join(CANONICAL_DEV_IDENTIFIER);
        assert_eq!(canonical_dev_data_dir(&canonical).unwrap(), canonical);
    }

    #[test]
    fn sync_scrubs_runtime_pid_from_managed_agents() {
        let (_parent, canonical, worktree) = setup_sync_layout();

        // Write canonical managed-agents.json with runtime state fields.
        std::fs::write(
            canonical.join("agents/managed-agents.json"),
            serde_json::to_string_pretty(&serde_json::json!([{
                "id": "agent-1",
                "name": "Test Agent",
                "runtime_pid": 12345,
                "last_error": "some error",
                "last_exit_code": 1,
                "last_stopped_at": "2026-01-01T00:00:00Z",
                "last_started_at": "2026-01-01T00:00:00Z",
                "backend_agent_id": "ba-123"
            }]))
            .unwrap(),
        )
        .unwrap();

        sync_files(&canonical, &worktree);
        scrub_managed_agents_runtime_state(&worktree.join("agents/managed-agents.json"));

        let content = std::fs::read_to_string(worktree.join("agents/managed-agents.json")).unwrap();
        let records: Vec<serde_json::Value> = serde_json::from_str(&content).unwrap();
        let agent = &records[0];

        assert_eq!(agent["id"], "agent-1");
        assert_eq!(agent["name"], "Test Agent");
        assert!(
            agent.get("runtime_pid").is_none(),
            "runtime_pid should be scrubbed"
        );
        assert!(
            agent.get("last_error").is_none(),
            "last_error should be scrubbed"
        );
        assert!(
            agent.get("last_exit_code").is_none(),
            "last_exit_code should be scrubbed"
        );
        assert!(
            agent.get("last_stopped_at").is_none(),
            "last_stopped_at should be scrubbed"
        );
        assert!(
            agent.get("last_started_at").is_none(),
            "last_started_at should be scrubbed"
        );
        assert!(
            agent.get("backend_agent_id").is_none(),
            "backend_agent_id should be scrubbed"
        );
    }

    fn write_agents_json(dir: &Path, records: &serde_json::Value) {
        std::fs::create_dir_all(dir.join("agents")).unwrap();
        std::fs::write(
            dir.join("agents/managed-agents.json"),
            serde_json::to_vec_pretty(records).unwrap(),
        )
        .unwrap();
    }

    fn read_agents_json(dir: &Path) -> Vec<serde_json::Value> {
        let content = std::fs::read_to_string(dir.join("agents/managed-agents.json")).unwrap();
        serde_json::from_str(&content).unwrap()
    }

    #[test]
    fn reconcile_clears_mcp_command_for_goose() {
        let dir = tempfile::tempdir().unwrap();
        write_agents_json(
            dir.path(),
            &serde_json::json!([{
                "name": "Scout",
                "agent_command": "goose",
                "mcp_command": "sprout-mcp-server"
            }]),
        );
        reconcile_mcp_commands_in_file(&dir.path().join("agents/managed-agents.json"));
        let records = read_agents_json(dir.path());
        assert_eq!(records[0]["mcp_command"], "");
    }

    #[test]
    fn reconcile_clears_mcp_command_for_claude() {
        let dir = tempfile::tempdir().unwrap();
        write_agents_json(
            dir.path(),
            &serde_json::json!([{
                "name": "Claude Agent",
                "agent_command": "claude-agent-acp",
                "mcp_command": "sprout-mcp-server"
            }]),
        );
        reconcile_mcp_commands_in_file(&dir.path().join("agents/managed-agents.json"));
        let records = read_agents_json(dir.path());
        assert_eq!(records[0]["mcp_command"], "");
    }

    #[test]
    fn reconcile_preserves_sprout_dev_mcp() {
        let dir = tempfile::tempdir().unwrap();
        write_agents_json(
            dir.path(),
            &serde_json::json!([{
                "name": "Solo",
                "agent_command": "sprout-agent",
                "mcp_command": "sprout-dev-mcp"
            }]),
        );
        let before =
            std::fs::read_to_string(dir.path().join("agents/managed-agents.json")).unwrap();
        reconcile_mcp_commands_in_file(&dir.path().join("agents/managed-agents.json"));
        let after = std::fs::read_to_string(dir.path().join("agents/managed-agents.json")).unwrap();
        assert_eq!(
            before, after,
            "file should not be rewritten when already correct"
        );
    }

    #[test]
    fn reconcile_fixes_sprout_agent_if_stale() {
        let dir = tempfile::tempdir().unwrap();
        write_agents_json(
            dir.path(),
            &serde_json::json!([{
                "name": "Solo",
                "agent_command": "sprout-agent",
                "mcp_command": "sprout-mcp-server"
            }]),
        );
        reconcile_mcp_commands_in_file(&dir.path().join("agents/managed-agents.json"));
        let records = read_agents_json(dir.path());
        assert_eq!(records[0]["mcp_command"], "sprout-dev-mcp");
    }

    #[test]
    fn reconcile_leaves_unknown_agent_untouched() {
        let dir = tempfile::tempdir().unwrap();
        write_agents_json(
            dir.path(),
            &serde_json::json!([{
                "name": "Custom Bot",
                "agent_command": "my-custom-agent",
                "mcp_command": "my-custom-mcp"
            }]),
        );
        reconcile_mcp_commands_in_file(&dir.path().join("agents/managed-agents.json"));
        let records = read_agents_json(dir.path());
        assert_eq!(records[0]["mcp_command"], "my-custom-mcp");
    }

    #[test]
    fn reconcile_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        write_agents_json(
            dir.path(),
            &serde_json::json!([{
                "name": "Scout",
                "agent_command": "goose",
                "mcp_command": "sprout-mcp-server"
            }]),
        );
        let path = dir.path().join("agents/managed-agents.json");
        reconcile_mcp_commands_in_file(&path);
        let after_first = std::fs::read_to_string(&path).unwrap();
        reconcile_mcp_commands_in_file(&path);
        let after_second = std::fs::read_to_string(&path).unwrap();
        assert_eq!(after_first, after_second);
    }

    #[test]
    fn reconcile_handles_mixed_records() {
        let dir = tempfile::tempdir().unwrap();
        write_agents_json(
            dir.path(),
            &serde_json::json!([
                {"name": "Scout", "agent_command": "goose", "mcp_command": "sprout-mcp-server"},
                {"name": "Claude", "agent_command": "claude-agent-acp", "mcp_command": "sprout-mcp-server"},
                {"name": "Solo", "agent_command": "sprout-agent", "mcp_command": "sprout-dev-mcp"},
                {"name": "Custom", "agent_command": "my-bot", "mcp_command": "my-mcp"},
                {"name": "Codex", "agent_command": "codex-acp", "mcp_command": "sprout-mcp-server"}
            ]),
        );
        reconcile_mcp_commands_in_file(&dir.path().join("agents/managed-agents.json"));
        let records = read_agents_json(dir.path());
        assert_eq!(records[0]["mcp_command"], "", "goose should be cleared");
        assert_eq!(records[1]["mcp_command"], "", "claude should be cleared");
        assert_eq!(
            records[2]["mcp_command"], "sprout-dev-mcp",
            "sprout-agent preserved"
        );
        assert_eq!(
            records[3]["mcp_command"], "my-mcp",
            "custom agent untouched"
        );
        assert_eq!(records[4]["mcp_command"], "", "codex should be cleared");
    }

    #[test]
    fn reconcile_adds_mcp_command_when_key_absent() {
        let dir = tempfile::tempdir().unwrap();
        write_agents_json(
            dir.path(),
            &serde_json::json!([{
                "name": "Solo",
                "agent_command": "sprout-agent"
            }]),
        );
        reconcile_mcp_commands_in_file(&dir.path().join("agents/managed-agents.json"));
        let records = read_agents_json(dir.path());
        assert_eq!(records[0]["mcp_command"], "sprout-dev-mcp");
    }

    #[test]
    fn reconcile_treats_null_mcp_command_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        write_agents_json(
            dir.path(),
            &serde_json::json!([{
                "name": "Solo",
                "agent_command": "sprout-agent",
                "mcp_command": null
            }]),
        );
        reconcile_mcp_commands_in_file(&dir.path().join("agents/managed-agents.json"));
        let records = read_agents_json(dir.path());
        assert_eq!(records[0]["mcp_command"], "sprout-dev-mcp");
    }
}
