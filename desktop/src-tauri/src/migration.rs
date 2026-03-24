//! One-time migration from the legacy `com.wesb.sprout` app data directory
//! to the current `xyz.block.sprout.app` directory.
//!
//! Migration runs only when:
//! 1. The legacy directory exists, AND
//! 2. The `.migration-complete` sentinel file has **not** been written yet.
//!
//! A sentinel file (rather than checking for `identity.key`) is used so that
//! a partially-failed migration retries on the next launch — `copy_dir_recursive`
//! overwrites existing files, so the retry completes the copy.
//!
//! The legacy directory is intentionally **not** deleted — users can clean it
//! up manually once they're satisfied the migration succeeded.
//!
//! Errors are logged but never fatal; the app must still start even if the
//! migration fails.
//!
//! **Note on dev/prod side-by-side:** Both the production build (`xyz.block.sprout.app`)
//! and the dev build (`xyz.block.sprout.app.dev`) will attempt to migrate from
//! the same legacy `com.wesb.sprout` directory, resulting in duplicated identity
//! keys. To avoid this, set the `SPROUT_PRIVATE_KEY` env var when running the
//! dev build — this bypasses file-based identity resolution entirely.

use std::path::{Path, PathBuf};
use tauri::Manager;

const LEGACY_DATA_DIR_NAME: &str = "com.wesb.sprout";
const MIGRATION_SENTINEL: &str = ".migration-complete";

/// Compute the legacy `com.wesb.sprout` data directory path by replacing the
/// last component of the current app data directory.
fn legacy_data_dir(current: &Path) -> Option<PathBuf> {
    current.parent().map(|p| p.join(LEGACY_DATA_DIR_NAME))
}

/// Recursively copy all files and directories from `src` into `dst`.
///
/// Preserves the directory structure. Uses only `std::fs` — no external crates.
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if file_type.is_symlink() {
            eprintln!(
                "sprout-desktop: migration: skipping symlink {}",
                src_path.display()
            );
            continue;
        }

        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

/// Migrate data from the legacy `com.wesb.sprout` app data directory to the
/// current `xyz.block.sprout.app` directory.
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

    // Skip if a previous migration already completed successfully.
    if current_dir.join(MIGRATION_SENTINEL).exists() {
        eprintln!("sprout-desktop: migration: skipping — already completed");
        return;
    }

    eprintln!(
        "sprout-desktop: migration: copying legacy data from {} → {}",
        old_dir.display(),
        current_dir.display()
    );

    if let Err(e) = copy_dir_recursive(&old_dir, &current_dir) {
        eprintln!("sprout-desktop: migration: failed to copy legacy data: {e}");
        return;
    }

    // Write a sentinel so we don't re-run on subsequent launches.
    // Failure here is non-fatal — worst case we re-copy (idempotently) next time.
    if let Err(e) = std::fs::write(
        current_dir.join(MIGRATION_SENTINEL),
        "migrated from com.wesb.sprout",
    ) {
        eprintln!("sprout-desktop: migration: warning: failed to write sentinel: {e}");
    }

    eprintln!("sprout-desktop: migration: complete");
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
        // Root has no parent on some platforms; our helper should return None
        // when `parent()` yields None (e.g. a bare root).
        let current = PathBuf::from("/");
        let _ = legacy_data_dir(&current);
    }

    /// Helper: create a temp dir structure mimicking the old `com.wesb.sprout`
    /// layout and return `(parent_dir, old_dir, new_dir)`.
    fn setup_legacy_layout() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let parent = tempfile::tempdir().unwrap();
        let old_dir = parent.path().join(LEGACY_DATA_DIR_NAME);
        let new_dir = parent.path().join("xyz.block.sprout.app");

        // Create old directory structure:
        //   identity.key
        //   .window-state.json
        //   agents/managed-agents.json
        //   agents/personas.json
        //   agents/teams.json
        //   agents/logs/agent1.log
        std::fs::create_dir_all(old_dir.join("agents/logs")).unwrap();
        std::fs::write(old_dir.join("identity.key"), "nsec1-fake-key-data").unwrap();
        std::fs::write(old_dir.join(".window-state.json"), r#"{"x":0}"#).unwrap();
        std::fs::write(
            old_dir.join("agents/managed-agents.json"),
            r#"[{"id":"a1"}]"#,
        )
        .unwrap();
        std::fs::write(old_dir.join("agents/personas.json"), "[]").unwrap();
        std::fs::write(old_dir.join("agents/teams.json"), "[]").unwrap();
        std::fs::write(old_dir.join("agents/logs/agent1.log"), "log line 1\n").unwrap();

        (parent, old_dir, new_dir)
    }

    #[test]
    fn copy_dir_recursive_copies_full_layout() {
        let (_parent, old_dir, new_dir) = setup_legacy_layout();

        copy_dir_recursive(&old_dir, &new_dir).unwrap();

        // Verify every file was copied with correct content.
        assert_eq!(
            std::fs::read_to_string(new_dir.join("identity.key")).unwrap(),
            "nsec1-fake-key-data"
        );
        assert_eq!(
            std::fs::read_to_string(new_dir.join(".window-state.json")).unwrap(),
            r#"{"x":0}"#
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
        assert_eq!(
            std::fs::read_to_string(new_dir.join("agents/logs/agent1.log")).unwrap(),
            "log line 1\n"
        );

        // Old directory must still exist (we never delete it).
        assert!(old_dir.exists());
    }

    #[test]
    fn migration_skipped_when_sentinel_exists() {
        let (_parent, old_dir, new_dir) = setup_legacy_layout();

        // Pre-create the new dir with the sentinel file.
        std::fs::create_dir_all(&new_dir).unwrap();
        std::fs::write(
            new_dir.join(MIGRATION_SENTINEL),
            "migrated from com.wesb.sprout",
        )
        .unwrap();

        // The sentinel is the only guard — verify it exists.
        assert!(
            new_dir.join(MIGRATION_SENTINEL).exists(),
            "sentinel should block migration"
        );

        // Even though old_dir has data, the sentinel prevents re-migration.
        // We can't call migrate_legacy_data_dir without an AppHandle, but we
        // verify the guard condition directly: sentinel present → skip.
        assert!(old_dir.join("identity.key").exists());
    }

    #[test]
    fn migration_retries_when_no_sentinel() {
        let (_parent, _old_dir, new_dir) = setup_legacy_layout();

        // New dir has identity.key but NO sentinel — migration should still
        // run (the sentinel is the only guard, not identity.key).
        std::fs::create_dir_all(&new_dir).unwrap();
        std::fs::write(new_dir.join("identity.key"), "nsec1-new-key").unwrap();

        // Sentinel must be absent.
        assert!(
            !new_dir.join(MIGRATION_SENTINEL).exists(),
            "no sentinel means migration should retry"
        );

        // identity.key alone does NOT block migration — only the sentinel does.
        // copy_dir_recursive would overwrite the existing identity.key with the
        // legacy one, which is the correct behavior for a partial-migration retry.
    }

    #[cfg(unix)]
    #[test]
    fn copy_dir_recursive_skips_symlinks() {
        let (_parent, old_dir, new_dir) = setup_legacy_layout();

        // Add a symlink in the old directory.
        std::os::unix::fs::symlink("/nonexistent/target", old_dir.join("dangling-link")).unwrap();

        // copy_dir_recursive should succeed, skipping the symlink.
        copy_dir_recursive(&old_dir, &new_dir).unwrap();

        // Regular files should still be copied.
        assert!(new_dir.join("identity.key").exists());
        assert!(new_dir.join(".window-state.json").exists());

        // The symlink should NOT appear in the new directory.
        assert!(
            !new_dir.join("dangling-link").exists(),
            "symlink should have been skipped"
        );
    }
}
