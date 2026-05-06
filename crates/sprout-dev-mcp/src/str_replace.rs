use schemars::JsonSchema;
use serde::Deserialize;
use similar::{DiffTag, TextDiff};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StrReplaceParams {
    pub path: String,
    pub old_str: String,
    pub new_str: String,
    #[serde(default)]
    pub workdir: Option<String>,
}

pub fn run(p: StrReplaceParams) -> String {
    let target = resolve_path(&p.path, p.workdir.as_deref());
    let content = match std::fs::read_to_string(&target) {
        Ok(c) => c,
        Err(e) => return format!("Error: cannot read {}: {e}", target.display()),
    };

    if p.old_str.is_empty() {
        return "Error: old_str must not be empty".into();
    }

    let occurrences = find_occurrences(&content, &p.old_str);
    match occurrences.len() {
        0 => {
            let hint = nearest_line_hint(&content, &p.old_str)
                .map(|h| format!("\n{h}"))
                .unwrap_or_default();
            format!(
                "Error: old_str not found in {}.\nold_str (truncated): {:?}{hint}",
                target.display(),
                truncate(&p.old_str, 80)
            )
        }
        1 => {
            let new_content = content.replacen(&p.old_str, &p.new_str, 1);
            if let Err(e) = atomic_write(&target, &new_content) {
                return format!("Error: failed to write {}: {e}", target.display());
            }
            let diff = unified_diff(&content, &new_content, &target);
            format!("Replaced 1 occurrence in {}.\n\n{diff}", target.display())
        }
        n => format!(
            "Error: old_str matched {n} locations in {}; provide more surrounding context to make the match unique.",
            target.display()
        ),
    }
}

fn resolve_path(path: &str, workdir: Option<&str>) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        return p.to_path_buf();
    }
    match workdir {
        Some(w) => Path::new(w).join(p),
        None => p.to_path_buf(),
    }
}

fn find_occurrences(text: &str, pattern: &str) -> Vec<usize> {
    if pattern.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let pat = pattern.as_bytes();
    let mut i = 0;
    while i + pat.len() <= bytes.len() {
        if &bytes[i..i + pat.len()] == pat {
            out.push(i);
            i += pat.len();
        } else {
            i += 1;
        }
    }
    out
}

fn atomic_write(target: &Path, content: &str) -> std::io::Result<()> {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(content.as_bytes())?;
    tmp.flush()?;
    tmp.persist(target).map(|_| ()).map_err(|e| e.error)
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
