//! MCP client registry — spawns stdio MCP servers via rmcp, lists their
//! tools, calls them, normalizes results.
//!
//! Tool names are qualified `{server}__{tool}` so the LLM sees a flat
//! namespace and we don't accidentally cross-talk between servers.

use std::collections::HashMap;
use std::sync::Arc;

use rmcp::model::CallToolRequestParams;
use rmcp::service::{RoleClient, RunningService};
use rmcp::transport::TokioChildProcess;
use rmcp::ServiceExt;
use serde_json::{Map, Value};
use tokio::process::Command;

use crate::types::{AgentError, McpContent, McpServerStdio, ToolDef};

const QUALIFIED_SEP: &str = "__";

type Client = RunningService<RoleClient, ()>;

struct Entry {
    server: String,
    tool: String,
    client: Arc<Client>,
}

pub struct McpRegistry {
    /// Qualified `{server}__{tool}` → entry.
    by_qualified: HashMap<String, Entry>,
    /// Tool definitions in stable order for LLM advertisement.
    tool_defs: Vec<ToolDef>,
    /// Held to keep the children alive for the session lifetime.
    _clients: Vec<Arc<Client>>,
}

impl McpRegistry {
    pub fn empty() -> Self {
        Self {
            by_qualified: HashMap::new(),
            tool_defs: Vec::new(),
            _clients: Vec::new(),
        }
    }

    /// Spawn all MCP servers, list their tools, build the registry.
    /// All-or-nothing: any failure aborts and the children are dropped.
    pub async fn spawn_all(servers: &[McpServerStdio]) -> Result<Self, AgentError> {
        let mut reg = Self::empty();

        for s in servers {
            let mut cmd = Command::new(&s.command);
            cmd.args(&s.args);
            for kv in &s.env {
                cmd.env(&kv.name, &kv.value);
            }
            // rmcp will set stdin/stdout to piped; we inherit stderr so the
            // harness sees server diagnostics.
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
                let qname = format!("{}{QUALIFIED_SEP}{}", s.name, t.name);
                reg.tool_defs.push(ToolDef {
                    name: qname.clone(),
                    description: t.description.as_ref().map(|c| c.to_string()),
                    input_schema: Value::Object((*t.input_schema).clone()),
                });
                reg.by_qualified.insert(
                    qname,
                    Entry {
                        server: s.name.clone(),
                        tool: t.name.to_string(),
                        client: client.clone(),
                    },
                );
            }
            reg._clients.push(client);
        }

        Ok(reg)
    }

    pub fn tools(&self) -> &[ToolDef] {
        &self.tool_defs
    }

    pub fn has(&self, qualified: &str) -> bool {
        self.by_qualified.contains_key(qualified)
    }

    /// Call a tool by qualified name. Returns content + isError. Caller
    /// applies truncation and emits ACP updates.
    pub async fn call(
        &self,
        qualified: &str,
        arguments: &Value,
    ) -> Result<McpCallResult, AgentError> {
        let entry = self
            .by_qualified
            .get(qualified)
            .ok_or_else(|| AgentError::Mcp(format!("unknown tool {qualified}")))?;

        let arg_obj: Option<Map<String, Value>> = match arguments {
            Value::Object(m) => Some(m.clone()),
            Value::Null => None,
            _ => {
                return Err(AgentError::Mcp(format!(
                    "tool {qualified} arguments must be a JSON object"
                )));
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
            .map_err(|e| AgentError::Mcp(format!("call {qualified}: {e}")))?;

        let content = res.content.into_iter().map(normalize_content).collect();

        Ok(McpCallResult {
            content,
            is_error: res.is_error.unwrap_or(false),
            _server: entry.server.clone(),
        })
    }
}

pub struct McpCallResult {
    pub content: Vec<McpContent>,
    pub is_error: bool,
    pub _server: String,
}

fn normalize_content(c: rmcp::model::Content) -> McpContent {
    use rmcp::model::RawContent;
    match c.raw {
        RawContent::Text(t) => McpContent::Text { text: t.text },
        RawContent::Image(i) => McpContent::Image {
            data: i.data,
            mime_type: i.mime_type,
        },
        RawContent::Audio(a) => McpContent::Audio {
            data: a.data,
            mime_type: a.mime_type,
        },
        RawContent::ResourceLink(r) => McpContent::ResourceLink { uri: r.uri },
        RawContent::Resource(r) => {
            McpContent::Other(serde_json::to_value(&r).unwrap_or(Value::Null))
        }
    }
}

/// Truncate the largest `Text` block until total serialized size fits.
/// Returns `(truncated, content)`.
pub fn truncate_for_context(
    mut content: Vec<McpContent>,
    max_bytes: usize,
) -> (bool, Vec<McpContent>) {
    let total: usize = content
        .iter()
        .map(|c| match c {
            McpContent::Text { text } => text.len(),
            McpContent::Image { data, .. } | McpContent::Audio { data, .. } => data.len(),
            McpContent::ResourceLink { uri } => uri.len(),
            McpContent::Other(v) => v.to_string().len(),
        })
        .sum();
    if total <= max_bytes {
        return (false, content);
    }

    // Truncate the largest text block.
    let mut largest: Option<(usize, usize)> = None;
    for (i, c) in content.iter().enumerate() {
        if let McpContent::Text { text } = c {
            match largest {
                None => largest = Some((i, text.len())),
                Some((_, n)) if text.len() > n => largest = Some((i, text.len())),
                _ => {}
            }
        }
    }
    if let Some((idx, _)) = largest {
        if let McpContent::Text { text } = &mut content[idx] {
            let elide = total - max_bytes + 64;
            let cut = text.len().saturating_sub(elide);
            // Be careful with utf-8 char boundaries.
            let mut cut = cut;
            while cut > 0 && !text.is_char_boundary(cut) {
                cut -= 1;
            }
            let elided = text.len() - cut;
            text.truncate(cut);
            text.push_str(&format!("\n… [TRUNCATED: {elided} bytes elided]"));
        }
    }
    (true, content)
}
