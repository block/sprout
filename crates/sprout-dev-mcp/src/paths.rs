//! Path resolution and file I/O shared across dev-mcp tools.
//!
//! `resolve_within` canonicalises a user-supplied path against a workspace
//! root and rejects any result that escapes the root (e.g. via `..`, absolute
//! paths, or symlinks). All tools that touch the filesystem must funnel
//! through this helper so the escape policy stays consistent.
//!
//! `read_text_file` builds on `resolve_within` to provide the full
//! resolve → stat → size-check → read → UTF-8 decode pipeline shared by
//! `read_file` and `str_replace`.

use crate::shell::SharedState;
use rmcp::ErrorData;
use std::path::{Path, PathBuf};

pub(crate) const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;

/// Resolve `path` (absolute or relative) against `root` and require the
/// canonicalised result to live under the canonicalised `root`. Returns an
/// error string suitable for `ErrorData::invalid_params` on rejection.
pub(crate) fn resolve_within(root: &Path, path: &str) -> Result<PathBuf, String> {
    let raw = Path::new(path);
    let candidate: PathBuf = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        root.join(raw)
    };

    let root_canon = std::fs::canonicalize(root)
        .map_err(|e| format!("workdir not accessible: {} ({e})", root.display()))?;

    let resolved = std::fs::canonicalize(&candidate)
        .map_err(|e| format!("path not accessible: {} ({e})", candidate.display()))?;

    if !resolved.starts_with(&root_canon) {
        return Err(format!(
            "path escapes workspace: {} not within {}",
            resolved.display(),
            root_canon.display()
        ));
    }
    Ok(resolved)
}

/// Resolve a user-supplied path within the workspace, read the file, and
/// return `(resolved_path, utf8_content)`. Rejects files that are not
/// regular files, exceed `MAX_FILE_BYTES`, or are not valid UTF-8.
pub(crate) fn read_text_file(
    state: &SharedState,
    path: &str,
    workdir: Option<&str>,
) -> Result<(PathBuf, String), ErrorData> {
    let workspace_root: PathBuf = match workdir {
        Some(w) => PathBuf::from(w),
        None => state.cwd.clone(),
    };
    let target = match resolve_within(&workspace_root, path) {
        Ok(t) => t,
        Err(e) => return Err(ErrorData::invalid_params(e, None)),
    };

    let meta = match std::fs::metadata(&target) {
        Ok(m) => m,
        Err(e) => {
            return Err(ErrorData::internal_error(
                format!("cannot stat {}: {e}", target.display()),
                None,
            ));
        }
    };
    if !meta.is_file() {
        return Err(ErrorData::invalid_params(
            format!("not a regular file: {}", target.display()),
            None,
        ));
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(ErrorData::invalid_params(
            format!(
                "file too large: {} is {} bytes (limit {} bytes)",
                target.display(),
                meta.len(),
                MAX_FILE_BYTES
            ),
            None,
        ));
    }

    let file = match std::fs::File::open(&target) {
        Ok(f) => f,
        Err(e) => {
            return Err(ErrorData::internal_error(
                format!("cannot open {}: {e}", target.display()),
                None,
            ));
        }
    };
    let mut buf = Vec::with_capacity(meta.len() as usize);
    use std::io::Read;
    match file.take(MAX_FILE_BYTES + 1).read_to_end(&mut buf) {
        Ok(n) if n as u64 > MAX_FILE_BYTES => {
            return Err(ErrorData::invalid_params(
                format!("file grew past {} bytes during read", MAX_FILE_BYTES),
                None,
            ));
        }
        Ok(_) => {}
        Err(e) => {
            return Err(ErrorData::internal_error(
                format!("cannot read {}: {e}", target.display()),
                None,
            ));
        }
    }
    let content = match String::from_utf8(buf) {
        Ok(s) => s,
        Err(e) => {
            return Err(ErrorData::internal_error(
                format!("not valid UTF-8: {}: {e}", target.display()),
                None,
            ));
        }
    };

    Ok((target, content))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn resolve_within_rejects_escape() {
        let dir = tempdir().expect("tempdir");
        let inside = dir.path().join("file.txt");
        fs::write(&inside, b"x").expect("write");
        // Symlink targeting outside the dir should be rejected.
        #[cfg(unix)]
        {
            let outside = std::env::temp_dir().join("dev-mcp-paths-escape-target");
            let _ = fs::remove_file(&outside);
            fs::write(&outside, b"y").expect("write outside");
            let link = dir.path().join("link.txt");
            std::os::unix::fs::symlink(&outside, &link).expect("symlink");
            let err = resolve_within(dir.path(), "link.txt").unwrap_err();
            assert!(err.contains("escapes workspace"), "got: {err}");
            let _ = fs::remove_file(&outside);
        }
        // Resolves a normal path inside.
        let p = resolve_within(dir.path(), "file.txt").expect("resolve");
        assert!(p.ends_with("file.txt"));
    }
}
