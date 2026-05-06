//! LLM client. Two providers (Anthropic, OpenAI-compat). Non-streaming.
//! One HTTP POST per round.

use reqwest::Client;
use serde_json::{json, Value};

use crate::types::{
    nonempty, AgentError, Config, HistoryItem, LlmResponse, Provider, ProviderStop, ToolCall,
    ToolDef,
};

pub struct Llm {
    http: Client,
}

impl Llm {
    pub fn new(cfg: &Config) -> Result<Self, AgentError> {
        let http = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(cfg.llm_timeout)
            .build()
            .map_err(|e| AgentError::Llm(format!("http: {e}")))?;
        Ok(Self { http })
    }

    pub async fn complete(
        &self,
        cfg: &Config,
        history: &[HistoryItem],
        tools: &[ToolDef],
    ) -> Result<LlmResponse, AgentError> {
        match cfg.provider {
            Provider::Anthropic => {
                let body = anthropic_body(cfg, history, tools);
                let url = format!("{}/v1/messages", cfg.base_url.trim_end_matches('/'));
                let v = post(&self.http, &url, &body, |r| {
                    r.header("x-api-key", &cfg.api_key)
                        .header("anthropic-version", &cfg.anthropic_api_version)
                        .header("content-type", "application/json")
                })
                .await?;
                parse_anthropic(v)
            }
            Provider::OpenAi => {
                let body = openai_body(cfg, history, tools);
                let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
                let v = post(&self.http, &url, &body, |r| {
                    r.bearer_auth(&cfg.api_key)
                        .header("content-type", "application/json")
                })
                .await?;
                parse_openai(v)
            }
        }
    }
}

// ─── Anthropic ──────────────────────────────────────────────────────────────

fn anthropic_body(cfg: &Config, history: &[HistoryItem], tools: &[ToolDef]) -> Value {
    let mut messages: Vec<Value> = Vec::new();
    let mut pending: Vec<Value> = Vec::new();
    let flush = |out: &mut Vec<Value>, p: &mut Vec<Value>| {
        if !p.is_empty() {
            out.push(json!({ "role": "user", "content": std::mem::take(p) }));
        }
    };
    for item in history {
        match item {
            HistoryItem::User(text) => {
                flush(&mut messages, &mut pending);
                messages.push(json!({
                    "role": "user",
                    "content": [{ "type": "text", "text": text }],
                }));
            }
            HistoryItem::Assistant { text, tool_calls } => {
                flush(&mut messages, &mut pending);
                let mut content: Vec<Value> = Vec::new();
                if !text.is_empty() {
                    content.push(json!({ "type": "text", "text": text }));
                }
                for c in tool_calls {
                    content.push(json!({
                        "type": "tool_use", "id": c.provider_id,
                        "name": c.name, "input": c.arguments,
                    }));
                }
                // Anthropic rejects empty `content` arrays. If the model
                // returned nothing (no text, no tool calls), emit a single
                // empty text block so history stays valid.
                if content.is_empty() {
                    content.push(json!({ "type": "text", "text": "" }));
                }
                messages.push(json!({ "role": "assistant", "content": content }));
            }
            HistoryItem::ToolResult(r) => pending.push(json!({
                "type": "tool_result",
                "tool_use_id": r.provider_id,
                "content": [{ "type": "text", "text": r.text }],
                "is_error": r.is_error,
            })),
        }
    }
    flush(&mut messages, &mut pending);

    let tools_json: Vec<Value> = tools
        .iter()
        .map(|t| {
            json!({
                "name": t.name, "description": t.description, "input_schema": t.input_schema,
            })
        })
        .collect();

    json!({
        "model": cfg.model,
        "max_tokens": cfg.max_output_tokens,
        "system": cfg.system_prompt,
        "tools": tools_json,
        "messages": messages,
    })
}

fn parse_anthropic(v: Value) -> Result<LlmResponse, AgentError> {
    let stop = match v.get("stop_reason").and_then(Value::as_str) {
        Some("end_turn") => ProviderStop::EndTurn,
        Some("tool_use") => ProviderStop::ToolUse,
        Some("max_tokens") => ProviderStop::MaxTokens,
        Some("refusal") => ProviderStop::Refusal,
        _ => ProviderStop::Other,
    };
    let mut tool_calls = Vec::new();
    let mut text = String::new();
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
                    let args = b.get("input").cloned().unwrap_or(Value::Null);
                    tool_calls.push(make_tool_call(id, name, args)?);
                }
                _ => {}
            }
        }
    }
    Ok(LlmResponse {
        text,
        tool_calls,
        stop,
    })
}

// ─── OpenAI-compat ──────────────────────────────────────────────────────────

fn openai_body(cfg: &Config, history: &[HistoryItem], tools: &[ToolDef]) -> Value {
    let mut messages: Vec<Value> = vec![json!({ "role": "system", "content": cfg.system_prompt })];
    for item in history {
        match item {
            HistoryItem::User(text) => messages.push(json!({ "role": "user", "content": text })),
            HistoryItem::Assistant { text, tool_calls } => {
                let mut msg = serde_json::Map::new();
                msg.insert("role".into(), json!("assistant"));
                // Always emit `content` as a string. Official OpenAI accepts
                // `null` when `tool_calls` is present, but several
                // OpenAI-compatible providers reject `null` outright. An
                // empty string is valid in both worlds.
                let content = if !text.is_empty() {
                    json!(text)
                } else {
                    json!("")
                };
                msg.insert("content".into(), content);
                if !tool_calls.is_empty() {
                    let calls: Vec<Value> = tool_calls
                        .iter()
                        .map(|c| {
                            json!({
                                "id": c.provider_id, "type": "function",
                                "function": {
                                    "name": c.name,
                                    "arguments": serde_json::to_string(&c.arguments)
                                        .unwrap_or_else(|_| "{}".into()),
                                },
                            })
                        })
                        .collect();
                    msg.insert("tool_calls".into(), Value::Array(calls));
                }
                messages.push(Value::Object(msg));
            }
            HistoryItem::ToolResult(r) => messages.push(json!({
                "role": "tool", "tool_call_id": r.provider_id, "content": r.text,
            })),
        }
    }
    let tools_json: Vec<Value> = tools
        .iter()
        .map(|t| {
            json!({
                "type": "function",
                "function": {
                    "name": t.name, "description": t.description, "parameters": t.input_schema,
                },
            })
        })
        .collect();
    json!({
        "model": cfg.model, "stream": false,
        "max_tokens": cfg.max_output_tokens,
        "messages": messages, "tools": tools_json, "tool_choice": "auto",
    })
}

fn parse_openai(v: Value) -> Result<LlmResponse, AgentError> {
    let choice = v
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .ok_or_else(|| AgentError::Llm("response missing choices".into()))?;
    let stop = match choice.get("finish_reason").and_then(Value::as_str) {
        Some("stop") => ProviderStop::EndTurn,
        Some("tool_calls") => ProviderStop::ToolUse,
        Some("length") => ProviderStop::MaxTokens,
        Some("content_filter") => ProviderStop::Refusal,
        _ => ProviderStop::Other,
    };
    let msg = choice
        .get("message")
        .ok_or_else(|| AgentError::Llm("missing message".into()))?;
    // OpenAI-compat: `content` is a string, null, or (rarely) an array of
    // parts. Treat anything non-string as no text.
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
            let raw = f.get("arguments").and_then(Value::as_str).unwrap_or("{}");
            let args: Value = serde_json::from_str(raw)
                .map_err(|e| AgentError::Llm(format!("tool_call.arguments not valid JSON: {e}")))?;
            tool_calls.push(make_tool_call(id, name, args)?);
        }
    }
    Ok(LlmResponse {
        text,
        tool_calls,
        stop,
    })
}

// ─── Shared helpers ─────────────────────────────────────────────────────────

fn make_tool_call(id: String, name: String, args: Value) -> Result<ToolCall, AgentError> {
    let provider_id = nonempty(id, "tool_call.id")?;
    let name = nonempty(name, "tool_call.name")?;
    let arguments = match args {
        Value::Object(_) => args,
        Value::Null => Value::Object(Default::default()),
        _ => {
            return Err(AgentError::Llm(
                "tool_call arguments must be a JSON object".into(),
            ))
        }
    };
    Ok(ToolCall {
        provider_id,
        name,
        arguments,
    })
}

/// Hard cap on LLM response body size. A buggy or malicious endpoint cannot
/// be allowed to OOM the agent before parsing.
const MAX_LLM_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

async fn post<F>(http: &Client, url: &str, body: &Value, apply: F) -> Result<Value, AgentError>
where
    F: Fn(reqwest::RequestBuilder) -> reqwest::RequestBuilder,
{
    for attempt in 0..2u32 {
        let resp = match apply(http.post(url).json(body)).send().await {
            Ok(r) => r,
            Err(e) => {
                if attempt == 0 && (e.is_timeout() || e.is_connect()) {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    continue;
                }
                return Err(AgentError::Llm(format!("transport: {e}")));
            }
        };
        let status = resp.status();
        if status == 401 || status == 403 {
            return Err(AgentError::LlmAuth(resp.text().await.unwrap_or_default()));
        }
        if (status.is_server_error() || status == 429) && attempt == 0 {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            continue;
        }
        if !status.is_success() {
            return Err(AgentError::Llm(format!(
                "{status}: {}",
                resp.text().await.unwrap_or_default()
            )));
        }
        // Reject up-front if Content-Length advertises an oversized body.
        if let Some(len) = resp.content_length() {
            if len as usize > MAX_LLM_RESPONSE_BYTES {
                return Err(AgentError::Llm(format!(
                    "response too large: {len} > {MAX_LLM_RESPONSE_BYTES}"
                )));
            }
        }
        // Bounded read: stream chunks until the cap, then bail.
        let mut buf: Vec<u8> = Vec::new();
        let mut stream = resp;
        loop {
            match stream.chunk().await {
                Ok(Some(chunk)) => {
                    if buf.len() + chunk.len() > MAX_LLM_RESPONSE_BYTES {
                        return Err(AgentError::Llm(format!(
                            "response exceeded {MAX_LLM_RESPONSE_BYTES} bytes"
                        )));
                    }
                    buf.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                Err(e) => return Err(AgentError::Llm(format!("read: {e}"))),
            }
        }
        return serde_json::from_slice(&buf).map_err(|e| AgentError::Llm(format!("json: {e}")));
    }
    Err(AgentError::Llm("exhausted retries".into()))
}
