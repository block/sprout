//! Worktree data sync and on-launch reconciliation for the Sprout desktop app.
//!
//! **Worktree sync** (`sync_shared_agent_data`): Per-launch symlink creation
//! from the current worktree data directory to the canonical dev data
//! directory (`xyz.block.sprout.app.dev`). Only runs when
//! `SPROUT_SHARE_IDENTITY=1` and `SPROUT_PRIVATE_KEY` is set. All dev
//! instances share the same physical files — edits in any worktree are
//! immediately visible to all others.
//!
//! **Provider reconciliation** (`reconcile_provider_mcp_commands`): Per-launch
//! fix-up of `mcp_command` values in `managed-agents.json` against the
//! discovery table. Ensures known providers always have their canonical
//! `mcp_command`; unknown/custom agents are left untouched.

use std::path::{Path, PathBuf};
use tauri::Manager;

const CANONICAL_DEV_IDENTIFIER: &str = "xyz.block.sprout.app.dev";

/// JSON files symlinked from worktree data directories to the canonical
/// dev data directory. Only data files — never `agent-pids/` or `logs/`.
/// `identity.key` is deliberately excluded because worktree instances
/// receive their identity via the `SPROUT_PRIVATE_KEY` env var.
///
/// NOTE: `agents/packs/` is intentionally excluded — recursive directory
/// symlink is out of scope. Pack personas will appear in the worktree but
/// agents with `persona_pack_path` may fail if the ACP reads pack files
/// at runtime. Install packs in the worktree separately if needed.
const SHARED_AGENT_FILES: &[&str] = &[
    "agents/managed-agents.json",
    "agents/personas.json",
    "agents/teams.json",
];

fn canonical_dev_data_dir(current: &Path) -> Option<PathBuf> {
    current.parent().map(|p| p.join(CANONICAL_DEV_IDENTIFIER))
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

/// Create symlinks for shared agent data files from the current (worktree)
/// data directory to the canonical dev data directory.
///
/// Guards:
/// - `SPROUT_SHARE_IDENTITY` must be `"1"`
/// - `SPROUT_PRIVATE_KEY` must parse as valid `nostr::Keys`
/// - The canonical dir must differ from the current dir (skip if we ARE canonical)
/// - The canonical dir must exist
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

        if let Some(parent) = dst.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                eprintln!(
                    "sprout-desktop: shared-agent-sync: failed to create {}: {e}",
                    parent.display()
                );
                continue;
            }
        }

        // Already a correct symlink — nothing to do.
        if dst.is_symlink() {
            if let Ok(target) = std::fs::read_link(&dst) {
                if target == src {
                    continue;
                }
            }
        }

        // Remove whatever's at dst (regular file, wrong symlink, broken symlink).
        if dst.exists() || dst.is_symlink() {
            let _ = std::fs::remove_file(&dst);
        }

        match std::os::unix::fs::symlink(&src, &dst) {
            Ok(_) => synced += 1,
            Err(e) => {
                eprintln!("sprout-desktop: shared-agent-sync: failed to symlink {rel}: {e}");
            }
        }
    }

    if synced > 0 {
        eprintln!(
            "sprout-desktop: shared-agent-sync: {synced} file(s) linked to {}",
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
    /// Mirrors the symlink loop of `sync_shared_agent_data` but takes explicit
    /// paths. `sync_shared_agent_data` requires a live Tauri AppHandle and
    /// cannot be unit-tested directly.
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
            if dst.is_symlink() {
                if let Ok(target) = std::fs::read_link(&dst) {
                    if target == src {
                        continue;
                    }
                }
            }
            if dst.exists() || dst.is_symlink() {
                let _ = std::fs::remove_file(&dst);
            }
            std::os::unix::fs::symlink(&src, &dst).unwrap();
            synced += 1;
        }
        synced
    }

    #[test]
    fn sync_creates_symlinks_to_fresh_worktree() {
        let (_parent, canonical, worktree) = setup_sync_layout();
        let synced = sync_files(&canonical, &worktree);
        assert_eq!(synced, 3);
        for rel in SHARED_AGENT_FILES {
            let dst = worktree.join(rel);
            assert!(dst.is_symlink(), "{rel} should be a symlink");
            assert_eq!(std::fs::read_link(&dst).unwrap(), canonical.join(rel));
        }
        // Content is readable through symlinks.
        assert_eq!(
            std::fs::read_to_string(worktree.join("agents/managed-agents.json")).unwrap(),
            r#"[{"id":"agent-1"}]"#,
        );
    }

    #[test]
    fn sync_replaces_existing_files_with_symlinks() {
        let (_parent, canonical, worktree) = setup_sync_layout();
        std::fs::create_dir_all(worktree.join("agents")).unwrap();
        std::fs::write(worktree.join("agents/managed-agents.json"), "[]").unwrap();
        std::fs::write(worktree.join("agents/personas.json"), "[]").unwrap();
        std::fs::write(worktree.join("agents/teams.json"), "[]").unwrap();

        let synced = sync_files(&canonical, &worktree);

        assert_eq!(synced, 3);
        for rel in SHARED_AGENT_FILES {
            let dst = worktree.join(rel);
            assert!(
                dst.is_symlink(),
                "{rel} should be a symlink after replacing regular file"
            );
            assert_eq!(std::fs::read_link(&dst).unwrap(), canonical.join(rel));
        }
        assert_eq!(
            std::fs::read_to_string(worktree.join("agents/managed-agents.json")).unwrap(),
            r#"[{"id":"agent-1"}]"#,
        );
    }

    #[test]
    fn sync_preserves_correct_symlinks() {
        let (_parent, canonical, worktree) = setup_sync_layout();
        // First sync creates symlinks.
        assert_eq!(sync_files(&canonical, &worktree), 3);
        // Second sync should be a no-op.
        assert_eq!(sync_files(&canonical, &worktree), 0);
        for rel in SHARED_AGENT_FILES {
            let dst = worktree.join(rel);
            assert!(dst.is_symlink());
            assert_eq!(std::fs::read_link(&dst).unwrap(), canonical.join(rel));
        }
    }

    #[test]
    fn sync_replaces_wrong_symlinks() {
        let (_parent, canonical, worktree) = setup_sync_layout();
        let wrong_target = PathBuf::from("/nonexistent/wrong-target.json");
        std::fs::create_dir_all(worktree.join("agents")).unwrap();
        for rel in SHARED_AGENT_FILES {
            std::os::unix::fs::symlink(&wrong_target, worktree.join(rel)).unwrap();
        }
        let synced = sync_files(&canonical, &worktree);
        assert_eq!(synced, 3);
        for rel in SHARED_AGENT_FILES {
            assert_eq!(
                std::fs::read_link(worktree.join(rel)).unwrap(),
                canonical.join(rel)
            );
        }
    }

    #[test]
    fn sync_handles_broken_symlinks() {
        let (_parent, canonical, worktree) = setup_sync_layout();
        std::fs::create_dir_all(worktree.join("agents")).unwrap();
        let broken_target = PathBuf::from("/this/does/not/exist.json");
        for rel in SHARED_AGENT_FILES {
            std::os::unix::fs::symlink(&broken_target, worktree.join(rel)).unwrap();
        }
        let synced = sync_files(&canonical, &worktree);
        assert_eq!(synced, 3);
        for rel in SHARED_AGENT_FILES {
            let dst = worktree.join(rel);
            assert!(dst.is_symlink());
            assert_eq!(std::fs::read_link(&dst).unwrap(), canonical.join(rel));
            // Content should be readable through the fixed symlink.
            assert!(std::fs::read_to_string(&dst).is_ok());
        }
    }

    #[test]
    fn writes_through_symlink_reach_canonical() {
        let (_parent, canonical, worktree) = setup_sync_layout();
        sync_files(&canonical, &worktree);

        let worktree_path = worktree.join("agents/personas.json");
        let canonical_path = canonical.join("agents/personas.json");

        // Write through the symlink using the same pattern as atomic_write_json.
        let new_content = r#"[{"id":"builtin:solo","updated":true}]"#;
        let resolved = std::fs::canonicalize(&worktree_path).unwrap();
        let tmp = resolved.with_extension("json.tmp");
        std::fs::write(&tmp, new_content.as_bytes()).unwrap();
        std::fs::rename(&tmp, &resolved).unwrap();

        // The canonical file should have the new content.
        assert_eq!(
            std::fs::read_to_string(&canonical_path).unwrap(),
            new_content
        );
        // The worktree path should still be a symlink.
        assert!(worktree_path.is_symlink());
        // Reading through the symlink should return the new content.
        assert_eq!(
            std::fs::read_to_string(&worktree_path).unwrap(),
            new_content
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
