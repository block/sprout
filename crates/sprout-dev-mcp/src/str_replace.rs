use crate::shell::SharedState;
use rmcp::ErrorData;
use schemars::JsonSchema;
use serde::Deserialize;
use similar::{DiffTag, TextDiff};
use std::io::Write;
use std::path::{Path, PathBuf};

const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024; // 10MB
const HINT_SCAN_LINE_LIMIT: usize = 200;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StrReplaceParams {
    pub path: String,
    pub old_str: String,
    pub new_str: String,
    #[serde(default)]
    pub workdir: Option<String>,
}

pub fn run(state: &SharedState, p: StrReplaceParams) -> Result<String, ErrorData> {
    if p.old_str.is_empty() {
        return Err(ErrorData::invalid_params(
            "old_str must not be empty".to_string(),
            None,
        ));
    }

    let workspace_root = match p.workdir.as_deref() {
        Some(w) => PathBuf::from(w),
        None => state.cwd.clone(),
    };
    let target = match resolve_within(&workspace_root, &p.path) {
        Ok(t) => t,
        Err(e) => return Err(ErrorData::invalid_params(e, None)),
    };

    match std::fs::metadata(&target) {
        Ok(m) if m.len() > MAX_FILE_BYTES => {
            return Err(ErrorData::invalid_params(
                format!(
                    "file too large: {} is {} bytes (limit {} bytes)",
                    target.display(),
                    m.len(),
                    MAX_FILE_BYTES
                ),
                None,
            ));
        }
        Ok(_) => {}
        Err(e) => {
            return Err(ErrorData::internal_error(
                format!("cannot stat {}: {e}", target.display()),
                None,
            ));
        }
    }

    let content = match std::fs::read_to_string(&target) {
        Ok(c) => c,
        Err(e) => {
            return Err(ErrorData::internal_error(
                format!("cannot read {}: {e}", target.display()),
                None,
            ));
        }
    };

    let count = count_occurrences_capped(&content, &p.old_str);
    match count {
        0 => {
            let hint = nearest_line_hint(&content, &p.old_str)
                .map(|h| format!("\n{h}"))
                .unwrap_or_default();
            Err(ErrorData::invalid_params(
                format!(
                    "old_str not found in {}.\nold_str (truncated): {:?}{hint}",
                    target.display(),
                    truncate(&p.old_str, 80)
                ),
                None,
            ))
        }
        1 => {
            let new_content = content.replacen(&p.old_str, &p.new_str, 1);
            if let Err(e) = atomic_write(&target, &new_content) {
                return Err(ErrorData::internal_error(
                    format!("failed to write {}: {e}", target.display()),
                    None,
                ));
            }
            let diff = unified_diff(&content, &new_content, &target);
            Ok(format!(
                "Replaced 1 occurrence in {}.\n\n{diff}",
                target.display()
            ))
        }
        _ => Err(ErrorData::invalid_params(
            format!(
                "old_str matched multiple locations in {}; provide more surrounding context to make the match unique.",
                target.display()
            ),
            None,
        )),
    }
}

/// Resolve `path` against `root` and ensure the result is contained within `root`.
///
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

/// Count occurrences of `pattern` in `text`, capped at 2.
///
/// We only need to know: 0 (not found), 1 (unique), or 2+ (ambiguous).
/// Stopping early avoids scanning huge files for nothing.
pub(crate) fn count_occurrences_capped(text: &str, pattern: &str) -> usize {
    if pattern.is_empty() {
        return 0;
    }
    let bytes = text.as_bytes();
    let pat = pattern.as_bytes();
    let mut count = 0;
    let mut i = 0;
    while i + pat.len() <= bytes.len() {
        if &bytes[i..i + pat.len()] == pat {
            count += 1;
            if count >= 2 {
                return count;
            }
            i += pat.len();
        } else {
            i += 1;
        }
    }
    count
}

fn atomic_write(target: &Path, content: &str) -> std::io::Result<()> {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    // Capture original permissions (if the file exists) so the atomic rename
    // doesn't drop the original file's mode.
    let original_perms = std::fs::metadata(target).ok().map(|m| m.permissions());

    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(content.as_bytes())?;
    tmp.flush()?;
    tmp.persist(target).map_err(|e| e.error)?;

    if let Some(perms) = original_perms {
        // Best-effort: if restoring perms fails, the file is still written.
        let _ = std::fs::set_permissions(target, perms);
    }
    Ok(())
}

fn unified_diff(old: &str, new: &str, path: &Path) -> String {
    let diff = TextDiff::from_lines(old, new);
    let display = path.display();
    let mut out = format!("--- a/{display}\n+++ b/{display}\n");
    for hunk in diff.unified_diff().context_radius(3).iter_hunks() {
        out.push_str(&hunk.to_string());
    }
    out
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let head: String = s.chars().take(max_chars).collect();
        format!("{head}…")
    }
}

fn similarity(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let matched: usize = TextDiff::from_chars(a, b)
        .ops()
        .iter()
        .filter(|op| matches!(op.tag(), DiffTag::Equal))
        .map(|op| op.new_range().len())
        .sum();
    matched as f64 / a.len().max(b.len()) as f64
}

fn nearest_line_hint(content: &str, pattern: &str) -> Option<String> {
    let first = pattern.lines().next()?.trim();
    if first.is_empty() {
        return None;
    }
    let best = content
        .lines()
        .take(HINT_SCAN_LINE_LIMIT)
        .enumerate()
        .map(|(i, line)| (i, similarity(line.trim(), first), line))
        .filter(|(_, s, _)| *s > 0.6)
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))?;
    Some(format!(
        "Hint: nearest match around line {} (similarity {:.2}):\n  found:    {:?}\n  expected: {:?}",
        best.0 + 1,
        best.1,
        best.2.trim(),
        first
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn count_occurrences_capped_returns_0_1_2() {
        assert_eq!(count_occurrences_capped("hello world", "x"), 0);
        assert_eq!(count_occurrences_capped("hello world", "hello"), 1);
        assert_eq!(count_occurrences_capped("a a a a a", "a"), 2); // capped at 2
        assert_eq!(count_occurrences_capped("abc", ""), 0);
    }

    #[test]
    fn resolve_within_rejects_escape() {
        let dir = tempdir().expect("tempdir");
        let inside = dir.path().join("file.txt");
        fs::write(&inside, b"x").expect("write");
        // Symlink targeting outside the dir should be rejected.
        #[cfg(unix)]
        {
            let outside = std::env::temp_dir().join("sprout-mcp-escape-target");
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

    fn make_state(cwd: &std::path::Path) -> SharedState {
        let shim = crate::shim::Shim::install().expect("shim install");
        SharedState::new(cwd.to_path_buf(), shim).expect("state new")
    }

    #[test]
    fn run_basic_replace_emits_diff() {
        let dir = tempdir().expect("tempdir");
        let f = dir.path().join("a.txt");
        fs::write(&f, "alpha\nbeta\ngamma\n").expect("write");
        let state = make_state(dir.path());
        let p = StrReplaceParams {
            path: "a.txt".into(),
            old_str: "beta".into(),
            new_str: "BETA".into(),
            workdir: Some(dir.path().display().to_string()),
        };
        let out = run(&state, p).expect("ok");
        assert!(out.contains("Replaced 1 occurrence"), "out: {out}");
        assert!(out.contains("-beta"), "out: {out}");
        assert!(out.contains("+BETA"), "out: {out}");
        let contents = fs::read_to_string(&f).expect("read");
        assert_eq!(contents, "alpha\nBETA\ngamma\n");
    }

    #[test]
    fn run_rejects_path_outside_workspace() {
        let dir = tempdir().expect("tempdir");
        let state = make_state(dir.path());
        let p = StrReplaceParams {
            path: "/etc/hosts".into(),
            old_str: "x".into(),
            new_str: "y".into(),
            workdir: Some(dir.path().display().to_string()),
        };
        let err = run(&state, p).unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("escapes workspace") || msg.contains("not accessible"),
            "msg: {msg}"
        );
    }

    #[test]
    fn run_rejects_file_too_large() {
        let dir = tempdir().expect("tempdir");
        let f = dir.path().join("big.bin");
        // 11MB of zeros
        let big = vec![b'a'; (MAX_FILE_BYTES as usize) + 1024];
        fs::write(&f, &big).expect("write");
        let state = make_state(dir.path());
        let p = StrReplaceParams {
            path: "big.bin".into(),
            old_str: "a".into(),
            new_str: "b".into(),
            workdir: Some(dir.path().display().to_string()),
        };
        let err = run(&state, p).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("too large"), "msg: {msg}");
    }
}
