use crate::shell::SharedState;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TodoParams {
    #[serde(default)]
    pub content: Option<String>,
}

pub fn run(state: &SharedState, p: TodoParams) -> String {
    if let Some(new_content) = p.content {
        if let Err(e) = std::fs::write(&state.todo_path, &new_content) {
            return format!("Error writing TODO at {}: {e}", state.todo_path.display());
        }
        return format!(
            "TODO updated ({} bytes) at {}\n\n{}",
            new_content.len(),
            state.todo_path.display(),
            new_content
        );
    }
    match std::fs::read_to_string(&state.todo_path) {
        Ok(s) if s.is_empty() => "(TODO is empty — call todo with `content` to set it)".into(),
        Ok(s) => s,
        Err(_) => "(TODO is empty — call todo with `content` to set it)".into(),
    }
}
