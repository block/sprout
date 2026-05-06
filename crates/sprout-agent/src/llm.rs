//! LLM client. Provider enum, two arms. Non-streaming. One HTTP POST per round.
//!
//! ┌──────────────┐                ┌──────────────┐
//! │ Provider enum│ ─ complete() ─►│ HTTP POST    │ ─►  LlmResponse
//! └──────────────┘                └──────────────┘

use reqwest::Client;
use serde_json::{json, Value};

use crate::types::{
    AgentError, Config, HistoryItem, LlmResponse, McpContent, ProviderKind, ProviderStop, ToolCall,
    ToolDef, ToolResult,
};

pub struct Llm {
    http: Client,
}

impl Llm {
    pub fn new(cfg: &Config) -> Result<Self, AgentError> {
        let http = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .pool_idle_timeout(std::time::Duration::from_secs(60))
            .timeout(cfg.llm_timeout)
            .build()
            .map_err(|e| AgentError::Llm(format!("http client: {e}")))?;
        Ok(Self { http })
    }

    pub async fn complete(
        &self,
        cfg: &Config,
        history: &[HistoryItem],
        tools: &[ToolDef],
    ) -> Result<LlmResponse, AgentError> {
        match cfg.provider {
            ProviderKind::Anthropic => anthropic_complete(&self.http, cfg, history, tools).await,
            ProviderKind::OpenAi => openai_complete(&self.http, cfg, history, tools).await,
        }
    }
}

// ─── Anthropic ──────────────────────────────────────────────────────────────

async fn anthropic_complete(
    http: &Client,
    cfg: &Config,
    history: &[HistoryItem],
    tools: &[ToolDef],
) -> Result<LlmResponse, AgentError> {
    let key = cfg
        .anthropic_api_key
        .as_deref()
        .ok_or_else(|| AgentError::LlmAuth("ANTHROPIC_API_KEY missing".into()))?;
    let model = cfg
        .anthropic_model
        .as_deref()
        .ok_or_else(|| AgentError::Llm("ANTHROPIC_MODEL missing".into()))?;

    let messages = anthropic_messages(history);
    let tools_json: Vec<Value> = tools
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "description": t.description.clone().unwrap_or_default(),
                "input_schema": t.input_schema,
            })
        })
        .collect();

    let body = json!({
        "model": model,
        "max_tokens": cfg.max_output_tokens,
        "system": cfg.system_prompt,
        "tools": tools_json,
        "messages": messages,
    });

    let url = format!(
        "{}/v1/messages",
        cfg.anthropic_base_url.trim_end_matches('/')
    );
    let v = post_with_retry(http, &url, &body, |req| {
        req.header("x-api-key", key)
            .header("anthropic-version", &cfg.anthropic_api_version)
            .header("content-type", "application/json")
    })
    .await?;

    parse_anthropic(v)
}

fn anthropic_messages(history: &[HistoryItem]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    let mut pending_tool_results: Vec<Value> = Vec::new();

    let flush_tool_results = |out: &mut Vec<Value>, pending: &mut Vec<Value>| {
        if !pending.is_empty() {
            out.push(json!({
                "role": "user",
                "content": std::mem::take(pending),
            }));
        }
    };

    for item in history {
        match item {
            HistoryItem::User { text } => {
                flush_tool_results(&mut out, &mut pending_tool_results);
                out.push(json!({
                    "role": "user",
                    "content": [{ "type": "text", "text": text }],
                }));
            }
            HistoryItem::Assistant { text, tool_calls } => {
                flush_tool_results(&mut out, &mut pending_tool_results);
                let mut content: Vec<Value> = Vec::new();
                if !text.is_empty() {
                    content.push(json!({ "type": "text", "text": text }));
                }
                for c in tool_calls {
                    content.push(json!({
                        "type": "tool_use",
                        "id": c.provider_id,
                        "name": c.name,
                        "input": c.arguments,
                    }));
                }
                out.push(json!({ "role": "assistant", "content": content }));
            }
            HistoryItem::ToolResult(r) => {
                pending_tool_results.push(json!({
                    "type": "tool_result",
                    "tool_use_id": r.provider_id,
                    "content": anthropic_tool_result_content(r),
                    "is_error": r.is_error,
                }));
            }
        }
    }
    flush_tool_results(&mut out, &mut pending_tool_results);
    out
}

fn anthropic_tool_result_content(r: &ToolResult) -> Vec<Value> {
    r.content
        .iter()
        .map(|c| match c {
            McpContent::Text { text } => json!({ "type": "text", "text": text }),
            McpContent::Image { data, mime_type } => json!({
                "type": "image",
                "source": { "type": "base64", "media_type": mime_type, "data": data },
            }),
            McpContent::Audio { mime_type, data } => json!({
                "type": "text",
                "text": format!("[audio elided: {mime_type}, {} bytes]", data.len()),
            }),
            McpContent::ResourceLink { uri } => json!({
                "type": "text", "text": format!("[resource: {uri}]"),
            }),
            McpContent::Other(v) => json!({
                "type": "text",
                "text": serde_json::to_string(v).unwrap_or_default(),
            }),
        })
        .collect()
}

fn parse_anthropic(v: Value) -> Result<LlmResponse, AgentError> {
    let stop_reason = match v.get("stop_reason").and_then(Value::as_str) {
        Some("end_turn") => ProviderStop::EndTurn,
        Some("tool_use") => ProviderStop::ToolUse,
        Some("max_tokens") => ProviderStop::MaxTokens,
        Some("refusal") => ProviderStop::Refusal,
        _ => ProviderStop::Other,
    };

    let mut text = String::new();
    let mut tool_calls = Vec::new();

    if let Some(blocks) = v.get("content").and_then(Value::as_array) {
        for b in blocks {
            match b.get("type").and_then(Value::as_str) {
                Some("text") => {
                    if let Some(t) = b.get("text").and_then(Value::as_str) {
                        text.push_str(t);
                    }
                }
                Some("tool_use") => {
                    let id = b.get("id").and_then(Value::as_str).unwrap_or("").to_owned();
                    let name = b
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned();
                    let input = b
                        .get("input")
                        .cloned()
                        .unwrap_or(Value::Object(Default::default()));
                    tool_calls.push(ToolCall {
                        provider_id: id,
                        name,
                        arguments: input,
                    });
                }
                _ => {}
            }
        }
    }

    Ok(LlmResponse {
        text,
        tool_calls,
        stop_reason,
    })
}

// ─── OpenAI-compatible ──────────────────────────────────────────────────────

async fn openai_complete(
    http: &Client,
    cfg: &Config,
    history: &[HistoryItem],
    tools: &[ToolDef],
) -> Result<LlmResponse, AgentError> {
    let key = cfg
        .openai_api_key
        .as_deref()
        .ok_or_else(|| AgentError::LlmAuth("OPENAI_COMPAT_API_KEY missing".into()))?;
    let model = cfg
        .openai_model
        .as_deref()
        .ok_or_else(|| AgentError::Llm("OPENAI_COMPAT_MODEL missing".into()))?;

    let mut messages: Vec<Value> = vec![json!({
        "role": "system",
        "content": cfg.system_prompt,
    })];
    for item in history {
        match item {
            HistoryItem::User { text } => messages.push(json!({
                "role": "user", "content": text,
            })),
            HistoryItem::Assistant { text, tool_calls } => {
                let mut msg = serde_json::Map::new();
                msg.insert("role".into(), json!("assistant"));
                msg.insert(
                    "content".into(),
                    if text.is_empty() {
                        Value::Null
                    } else {
                        json!(text)
                    },
                );
                if !tool_calls.is_empty() {
                    let calls: Vec<Value> = tool_calls
                        .iter()
                        .map(|c| {
                            json!({
                                "id": c.provider_id,
                                "type": "function",
                                "function": {
                                    "name": c.name,
                                    "arguments": serde_json::to_string(&c.arguments).unwrap_or("{}".into()),
                                },
                            })
                        })
                        .collect();
                    msg.insert("tool_calls".into(), Value::Array(calls));
                }
                messages.push(Value::Object(msg));
            }
            HistoryItem::ToolResult(r) => {
                let envelope = json!({
                    "content": r.content.iter().map(|c| match c {
                        McpContent::Text { text } => json!({ "type": "text", "text": text }),
                        McpContent::Image { mime_type, data } => json!({
                            "type": "text",
                            "text": format!("[image elided: {mime_type}, {} bytes]", data.len()),
                        }),
                        McpContent::Audio { mime_type, data } => json!({
                            "type": "text",
                            "text": format!("[audio elided: {mime_type}, {} bytes]", data.len()),
                        }),
                        McpContent::ResourceLink { uri } => json!({
                            "type": "text", "text": format!("[resource: {uri}]"),
                        }),
                        McpContent::Other(v) => json!({ "type": "text", "text": serde_json::to_string(v).unwrap_or_default() }),
                    }).collect::<Vec<_>>(),
                    "isError": r.is_error,
                });
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": r.provider_id,
                    "content": serde_json::to_string(&envelope).unwrap_or_default(),
                }));
            }
        }
    }

    let tools_json: Vec<Value> = tools
        .iter()
        .map(|t| {
            json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description.clone().unwrap_or_default(),
                    "parameters": t.input_schema,
                },
            })
        })
        .collect();

    let body = json!({
        "model": model,
        "stream": false,
        "max_tokens": cfg.max_output_tokens,
        "messages": messages,
        "tools": tools_json,
        "tool_choice": "auto",
    });

    let url = format!(
        "{}/chat/completions",
        cfg.openai_base_url.trim_end_matches('/')
    );
    let v = post_with_retry(http, &url, &body, |req| {
        req.bearer_auth(key)
            .header("content-type", "application/json")
    })
    .await?;

    parse_openai(v)
}

fn parse_openai(v: Value) -> Result<LlmResponse, AgentError> {
    let choice = v
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .ok_or_else(|| AgentError::Llm("openai response missing choices".into()))?;

    let stop_reason = match choice.get("finish_reason").and_then(Value::as_str) {
        Some("stop") => ProviderStop::EndTurn,
        Some("tool_calls") => ProviderStop::ToolUse,
        Some("length") => ProviderStop::MaxTokens,
        Some("content_filter") => ProviderStop::Refusal,
        _ => ProviderStop::Other,
    };

    let msg = choice
        .get("message")
        .ok_or_else(|| AgentError::Llm("missing message".into()))?;
    let text = msg
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();

    let mut tool_calls = Vec::new();
    if let Some(arr) = msg.get("tool_calls").and_then(Value::as_array) {
        for tc in arr {
            let id = tc
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let f = tc
                .get("function")
                .ok_or_else(|| AgentError::Llm("tool_call missing function".into()))?;
            let name = f
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let raw_args = f.get("arguments").and_then(Value::as_str).unwrap_or("{}");
            let args: Value =
                serde_json::from_str(raw_args).unwrap_or(Value::Object(Default::default()));
            tool_calls.push(ToolCall {
                provider_id: id,
                name,
                arguments: args,
            });
        }
    }

    Ok(LlmResponse {
        text,
        tool_calls,
        stop_reason,
    })
}

// ─── Shared HTTP ────────────────────────────────────────────────────────────

async fn post_with_retry<F>(
    http: &Client,
    url: &str,
    body: &Value,
    apply_headers: F,
) -> Result<Value, AgentError>
where
    F: Fn(reqwest::RequestBuilder) -> reqwest::RequestBuilder,
{
    for attempt in 0..2u32 {
        let req = apply_headers(http.post(url).json(body));
        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                if attempt == 0 && (e.is_timeout() || e.is_connect()) {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    continue;
                }
                return Err(AgentError::LlmHttp(e.to_string()));
            }
        };
        let status = resp.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            let body = resp.text().await.unwrap_or_default();
            return Err(AgentError::LlmAuth(body));
        }
        if status.is_server_error() || status.as_u16() == 429 {
            if attempt == 0 {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }
            let body = resp.text().await.unwrap_or_default();
            return Err(AgentError::LlmHttp(format!("{status}: {body}")));
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(AgentError::Llm(format!("{status}: {body}")));
        }
        let v: Value = resp
            .json()
            .await
            .map_err(|e| AgentError::Llm(format!("json: {e}")))?;
        return Ok(v);
    }
    Err(AgentError::LlmHttp("exhausted retries".into()))
}
