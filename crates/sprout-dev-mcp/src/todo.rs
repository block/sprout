use crate::shell::SharedState;
use rmcp::ErrorData;
use schemars::JsonSchema;
use serde::Deserialize;
use std::io::Write;
use std::path::Path;

const EMPTY_HINT: &str = "(TODO is empty — call todo with `content` to set it)";
pub(crate) const MAX_TODO_BYTES: usize = 1024 * 1024; // 1MB

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TodoParams {
    #[serde(default)]
    pub content: Option<String>,
}

pub fn run(state: &SharedState, p: TodoParams) -> Result<String, ErrorData> {
    if let Some(new_content) = p.content {
        if new_content.len() > MAX_TODO_BYTES {
            return Err(ErrorData::invalid_params(
                format!(
                    "TODO content too large: {} bytes (limit {} bytes)",
                    new_content.len(),
                    MAX_TODO_BYTES
                ),
                None,
            ));
        }
        if let Err(e) = atomic_write(&state.todo_path, new_content.as_bytes()) {
            return Err(ErrorData::internal_error(
                format!("writing TODO at {}: {e}", state.todo_path.display()),
                None,
            ));
        }
        return Ok(format!(
            "TODO updated ({} bytes) at {}\n\n{}",
            new_content.len(),
            state.todo_path.display(),
            new_content
        ));
    }
    match std::fs::read_to_string(&state.todo_path) {
        Ok(s) if s.is_empty() => Ok(EMPTY_HINT.into()),
        Ok(s) => Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(EMPTY_HINT.into()),
        Err(e) => Err(ErrorData::internal_error(
            format!("reading TODO at {}: {e}", state.todo_path.display()),
            None,
        )),
    }
}

/// Atomic write via temp-file-in-same-dir + rename.
pub(crate) fn atomic_write(target: &Path, content: &[u8]) -> std::io::Result<()> {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(content)?;
    tmp.flush()?;
    tmp.persist(target).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::SharedState;
    use crate::shim::Shim;
    use tempfile::tempdir;

    fn make_state(cwd: &std::path::Path) -> SharedState {
        let shim = Shim::install().expect("shim install");
        SharedState::new(cwd.to_path_buf(), shim).expect("state new")
    }

    #[test]
    fn read_then_write_then_read() {
        let dir = tempdir().expect("tempdir");
        let state = make_state(dir.path());
        // First read: empty hint.
        let r0 = run(&state, TodoParams { content: None }).expect("ok");
        assert!(r0.contains("TODO is empty"), "r0: {r0}");
        // Write content.
        let r1 = run(
            &state,
            TodoParams {
                content: Some("- buy milk\n".into()),
            },
        )
        .expect("ok");
        assert!(r1.contains("TODO updated"), "r1: {r1}");
        // Read it back.
        let r2 = run(&state, TodoParams { content: None }).expect("ok");
        assert!(r2.contains("buy milk"), "r2: {r2}");
    }

    #[test]
    fn rejects_oversize_content() {
        let dir = tempdir().expect("tempdir");
        let state = make_state(dir.path());
        let big = "x".repeat(MAX_TODO_BYTES + 1);
        let err = run(&state, TodoParams { content: Some(big) }).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("too large"), "msg: {msg}");
    }
}
