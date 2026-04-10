use serde::{Deserialize, Serialize};

use crate::managed_agents::resolve_command;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatResponse {
    pub content: String,
}

/// Send messages to an LLM for the persona creator chat.
///
/// Uses goose - the app's primary agent runtime - which resolves
/// provider, model, and credentials from its own config.
#[tauri::command]
pub async fn persona_creator_chat(
    system_prompt: String,
    messages: Vec<ChatMessage>,
) -> Result<ChatResponse, String> {
    let goose_path = resolve_command("goose", None).ok_or_else(|| {
        "No LLM runtime found. Install goose to use the AI persona creator.".to_string()
    })?;

    goose_chat(goose_path, system_prompt, messages).await
}

/// Format the conversation history as a single text prompt for goose.
///
/// For single-turn (one user message), returns the message content directly.
/// For multi-turn, includes prior exchanges as context so the LLM can continue
/// the conversation coherently.
fn format_conversation_prompt(messages: &[ChatMessage]) -> String {
    if messages.len() <= 1 {
        return messages
            .first()
            .map(|m| m.content.clone())
            .unwrap_or_default();
    }

    let mut parts = Vec::with_capacity(messages.len());
    for (i, msg) in messages.iter().enumerate() {
        if i < messages.len() - 1 {
            let label = if msg.role == "assistant" {
                "Assistant"
            } else {
                "User"
            };
            parts.push(format!("{label}: {}", msg.content));
        }
    }

    let history = parts.join("\n\n");
    let last = &messages[messages.len() - 1].content;

    format!(
        "Here is our conversation so far:\n\n{history}\n\n---\n\nNow respond to this message:\n\n{last}"
    )
}

/// Run a one-shot LLM completion through goose.
async fn goose_chat(
    goose_path: std::path::PathBuf,
    system_prompt: String,
    messages: Vec<ChatMessage>,
) -> Result<ChatResponse, String> {
    let prompt_text = format_conversation_prompt(&messages);

    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new(&goose_path)
            .args([
                "run",
                "-t",
                &prompt_text,
                "--system",
                &system_prompt,
                "--no-session",
                "--no-profile",
                "--max-turns",
                "1",
                "-q",
                "--output-format",
                "json",
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .map_err(|e| format!("failed to spawn goose: {e}"))
    })
    .await
    .map_err(|e| format!("goose task failed: {e}"))?
    .map_err(|e: String| e)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "goose exited with {}: {}",
            output.status.code().unwrap_or(-1),
            stderr.chars().take(500).collect::<String>()
        ));
    }

    let response: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("failed to parse goose JSON: {e}"))?;

    // Extract the last assistant message's text content.
    let content = response["messages"]
        .as_array()
        .and_then(|msgs| {
            msgs.iter()
                .rev()
                .find(|m| m["role"].as_str() == Some("assistant"))
        })
        .and_then(|msg| msg["content"].as_array())
        .and_then(|blocks| {
            blocks
                .iter()
                .find(|b| b["type"].as_str() == Some("text"))
                .and_then(|b| b["text"].as_str())
        })
        .unwrap_or("")
        .to_string();

    if content.is_empty() {
        return Err("goose returned no assistant response".to_string());
    }

    Ok(ChatResponse { content })
}
