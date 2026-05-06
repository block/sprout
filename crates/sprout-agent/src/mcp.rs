//! MCP registry: spawn stdio servers, list tools, call tools.
//!
//! Tool names are qualified `{server}__{tool}`. We validate against OpenAI's
//! function-name constraints (a-zA-Z0-9_-, ≤64) and reject duplicates so the
//! LLM sees a flat, well-formed namespace.

use std::collections::HashMap;
use std::sync::Arc;

use rmcp::model::CallToolRequestParams;
use rmcp::service::{RoleClient, RunningService};
use rmcp::transport::TokioChildProcess;
use rmcp::ServiceExt;
use serde_json::{Map, Value};
use tokio::process::Command;

use crate::types::{clamp, AgentError, McpServerStdio, ToolDef, ToolResult};

const SEP: &str = "__";
const MAX_NAME_LEN: usize = 64;

type Client = RunningService<RoleClient, ()>;

struct Entry {
    tool: String,
    client: Arc<Client>,
}

pub struct McpRegistry {
    by_qname: HashMap<String, Entry>,
    defs: Vec<ToolDef>,
    _clients: Vec<Arc<Client>>, // kept alive for session lifetime
}

impl McpRegistry {
    /// Spawn all servers, list their tools. All-or-nothing: any failure aborts.
    pub async fn spawn_all(servers: &[McpServerStdio]) -> Result<Self, AgentError> {
        let mut reg = Self {
            by_qname: HashMap::new(),
            defs: Vec::new(),
            _clients: Vec::new(),
        };

        for s in servers {
            if !valid_name(&s.name) {
                return Err(AgentError::Mcp(format!("invalid server name: {}", s.name)));
            }
            let mut cmd = Command::new(&s.command);
            cmd.args(&s.args);
            for kv in &s.env {
                cmd.env(&kv.name, &kv.value);
            }
            cmd.stderr(std::process::Stdio::inherit());

            let transport = TokioChildProcess::new(cmd)
                .map_err(|e| AgentError::Mcp(format!("spawn {}: {e}", s.name)))?;
            let client: Client = ()
                .serve(transport)
                .await
                .map_err(|e| AgentError::Mcp(format!("init {}: {e}", s.name)))?;
            let client = Arc::new(client);

            let tools = client
                .peer()
                .list_all_tools()
                .await
                .map_err(|e| AgentError::Mcp(format!("list_tools {}: {e}", s.name)))?;

            for t in tools {
                let bare = t.name.to_string();
                let qname = format!("{}{SEP}{}", s.name, bare);
                if !valid_name(&qname) {
                    return Err(AgentError::Mcp(format!("invalid tool name: {qname}")));
                }
                if reg.by_qname.contains_key(&qname) {
                    return Err(AgentError::Mcp(format!("duplicate tool: {qname}")));
                }
                reg.defs.push(ToolDef {
                    name: qname.clone(),
                    description: t.description.as_deref().unwrap_or("").to_owned(),
                    input_schema: Value::Object((*t.input_schema).clone()),
                });
                reg.by_qname.insert(
                    qname,
                    Entry {
                        tool: bare,
                        client: client.clone(),
                    },
                );
            }
            reg._clients.push(client);
        }
        Ok(reg)
    }

    pub fn tools(&self) -> &[ToolDef] {
        &self.defs
    }

    pub fn has(&self, qname: &str) -> bool {
        self.by_qname.contains_key(qname)
    }

    /// Call a tool. Returns a flat `ToolResult` with text bounded to `max_bytes`.
    pub async fn call(
        &self,
        qname: &str,
        provider_id: &str,
        arguments: &Value,
        max_bytes: usize,
    ) -> Result<ToolResult, AgentError> {
        let entry = self
            .by_qname
            .get(qname)
            .ok_or_else(|| AgentError::Mcp(format!("unknown tool {qname}")))?;

        let arg_obj: Option<Map<String, Value>> = match arguments {
            Value::Object(m) => Some(m.clone()),
            Value::Null => None,
            _ => {
                return Err(AgentError::Mcp(format!(
                    "tool {qname} arguments must be a JSON object"
                )))
            }
        };

        let mut params = CallToolRequestParams::default();
        params.name = entry.tool.clone().into();
        params.arguments = arg_obj;

        let res = entry
            .client
            .peer()
            .call_tool(params)
            .await
            .map_err(|e| AgentError::Mcp(format!("call {qname}: {e}")))?;

        let text = collapse_content(&res.content);
        Ok(ToolResult {
            provider_id: provider_id.to_owned(),
            text: clamp(text, max_bytes),
            is_error: res.is_error.unwrap_or(false),
        })
    }
}

/// OpenAI function-name constraints: ^[a-zA-Z0-9_-]{1,64}$
fn valid_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_NAME_LEN
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Flatten MCP content blocks into a single text blob. Binary content is elided
/// with a marker so the model knows it existed.
fn collapse_content(blocks: &[rmcp::model::Content]) -> String {
    use rmcp::model::RawContent;
    let mut out = String::new();
    for c in blocks {
        if !out.is_empty() {
            out.push('\n');
        }
        match &c.raw {
            RawContent::Text(t) => out.push_str(&t.text),
            RawContent::Image(i) => {
                out.push_str(&format!(
                    "[image elided: {}, {} bytes]",
                    i.mime_type,
                    i.data.len()
                ));
            }
            RawContent::Audio(a) => {
                out.push_str(&format!(
                    "[audio elided: {}, {} bytes]",
                    a.mime_type,
                    a.data.len()
                ));
            }
            RawContent::ResourceLink(r) => out.push_str(&format!("[resource: {}]", r.uri)),
            RawContent::Resource(r) => {
                out.push_str(&serde_json::to_string(r).unwrap_or_default());
            }
        }
    }
    out
}
