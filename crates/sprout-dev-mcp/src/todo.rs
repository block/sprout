use crate::shell::SharedState;
use rmcp::ErrorData;
use schemars::JsonSchema;
use serde::Deserialize;

const EMPTY_HINT: &str = "(TODO is empty — call todo with `content` to set it)";

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TodoParams {
    #[serde(default)]
    pub content: Option<String>,
}

pub fn run(state: &SharedState, p: TodoParams) -> Result<String, ErrorData> {
    if let Some(new_content) = p.content {
        if let Err(e) = std::fs::write(&state.todo_path, &new_content) {
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
