use crate::paths::resolve_within;
use crate::shell::SharedState;
use rmcp::ErrorData;
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::PathBuf;

const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
const DEFAULT_LIMIT: usize = 2000;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadFileParams {
    /// File path (absolute or relative to workdir).
    pub path: String,
    /// 0-based line offset to start reading from. Defaults to 0.
    #[serde(default)]
    pub offset: Option<usize>,
    /// Maximum number of lines to return. Defaults to 2000.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Workspace root for relative path resolution. Defaults to server cwd.
    #[serde(default)]
    pub workdir: Option<String>,
}

pub fn run(state: &SharedState, p: ReadFileParams) -> Result<String, ErrorData> {
    let workspace_root: PathBuf = match p.workdir.as_deref() {
        Some(w) => PathBuf::from(w),
        None => state.cwd.clone(),
    };
    let target = match resolve_within(&workspace_root, &p.path) {
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

    let all_lines: Vec<&str> = content.lines().collect();
    let total = all_lines.len();

    if total == 0 {
        return Ok(format!("{} is empty (0 lines)", p.path));
    }

    let offset = p.offset.unwrap_or(0);
    let limit = p.limit.unwrap_or(DEFAULT_LIMIT);

    let slice = &all_lines[offset.min(total)..];
    let slice = &slice[..slice.len().min(limit)];

    // 1-based line numbers in the output.
    let start_line = offset + 1;
    let end_line = offset + slice.len();

    let mut out = format!(
        "{} (lines {}-{} of {})\n",
        p.path, start_line, end_line, total
    );
    for (i, line) in slice.iter().enumerate() {
        let line_number = offset + i + 1;
        out.push_str(&format!("{line_number}\t{line}\n"));
    }
    // Remove the trailing newline added by the last iteration so the result
    // has exactly one trailing newline (the format! above already ends with \n
    // for the header, and each line pushes its own \n).
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn make_state(cwd: &std::path::Path) -> SharedState {
        let shim = crate::shim::Shim::install().expect("shim install");
        SharedState::new(cwd.to_path_buf(), shim).expect("state new")
    }

    #[test]
    fn read_basic() {
        let dir = tempdir().expect("tempdir");
        let f = dir.path().join("basic.txt");
        fs::write(&f, "line1\nline2\nline3\nline4\nline5\n").expect("write");
        let state = make_state(dir.path());
        let p = ReadFileParams {
            path: "basic.txt".into(),
            offset: None,
            limit: None,
            workdir: Some(dir.path().display().to_string()),
        };
        let out = run(&state, p).expect("ok");
        assert!(out.contains("lines 1-5 of 5"), "out: {out}");
        assert!(out.contains("1\tline1"), "out: {out}");
        assert!(out.contains("2\tline2"), "out: {out}");
        assert!(out.contains("3\tline3"), "out: {out}");
        assert!(out.contains("4\tline4"), "out: {out}");
        assert!(out.contains("5\tline5"), "out: {out}");
    }

    #[test]
    fn read_offset_limit() {
        let dir = tempdir().expect("tempdir");
        let f = dir.path().join("ten.txt");
        let contents: String = (1..=10).map(|i| format!("line{i}\n")).collect();
        fs::write(&f, &contents).expect("write");
        let state = make_state(dir.path());
        let p = ReadFileParams {
            path: "ten.txt".into(),
            offset: Some(3),
            limit: Some(2),
            workdir: Some(dir.path().display().to_string()),
        };
        let out = run(&state, p).expect("ok");
        assert!(out.contains("lines 4-5 of 10"), "out: {out}");
        let line_count = out.lines().skip(1).count(); // skip header
        assert_eq!(line_count, 2, "expected 2 data lines, got: {out}");
        assert!(out.contains("4\tline4"), "out: {out}");
        assert!(out.contains("5\tline5"), "out: {out}");
    }

    #[test]
    fn read_empty_file() {
        let dir = tempdir().expect("tempdir");
        let f = dir.path().join("empty.txt");
        fs::write(&f, b"").expect("write");
        let state = make_state(dir.path());
        let p = ReadFileParams {
            path: "empty.txt".into(),
            offset: None,
            limit: None,
            workdir: Some(dir.path().display().to_string()),
        };
        let out = run(&state, p).expect("ok");
        assert!(out.contains("is empty (0 lines)"), "out: {out}");
    }

    #[test]
    fn read_rejects_path_escape() {
        let dir = tempdir().expect("tempdir");
        let state = make_state(dir.path());
        let p = ReadFileParams {
            path: "/etc/hosts".into(),
            offset: None,
            limit: None,
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
    fn read_rejects_too_large() {
        let dir = tempdir().expect("tempdir");
        let f = dir.path().join("big.bin");
        let big = vec![b'a'; (MAX_FILE_BYTES as usize) + 1024];
        fs::write(&f, &big).expect("write");
        let state = make_state(dir.path());
        let p = ReadFileParams {
            path: "big.bin".into(),
            offset: None,
            limit: None,
            workdir: Some(dir.path().display().to_string()),
        };
        let err = run(&state, p).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("too large"), "msg: {msg}");
    }
}
