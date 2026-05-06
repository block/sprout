use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rmcp::model::CallToolRequestParams;
use rmcp::service::{RoleClient, RunningService};
use rmcp::transport::TokioChildProcess;
use rmcp::ServiceExt;
use serde_json::{Map, Value};
use tokio::process::Command;

use crate::config::Config;
use crate::types::{clamp, AgentError, McpServerStdio, ToolDef, ToolResult};

const SEP: &str = "__";
const MAX_NAME_LEN: usize = 64;
const MAX_TOOLS_PER_SESSION: usize = 128;
const MAX_DESCRIPTION_BYTES: usize = 1024;
const MAX_SCHEMA_BYTES: usize = 4096;
const MARKER_FIELD_MAX: usize = 256;
pub const MAX_MCP_SERVERS: usize = 16;

const PASSTHROUGH_ENV: &[&str] = &["PATH", "HOME", "TERM", "LANG", "LC_ALL", "TMPDIR"];

type Client = RunningService<RoleClient, ()>;

struct Entry {
    server: String,
    tool: String,
    client: Arc<Client>,
}

struct Server {
    name: String,
    pgid: Mutex<Option<u32>>,
}

impl Server {
    fn kill_group(&self, stage: &str) {
        if let Some(p) = self.pgid.lock().unwrap_or_else(|e| e.into_inner()).take() {
            killpg(p, &self.name, stage);
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(p) = self.pgid.get_mut().unwrap_or_else(|e| e.into_inner()).take() {
            killpg(p, &self.name, "drop");
        }
    }
}

pub struct McpRegistry {
    by_qname: HashMap<String, Entry>,
    defs: Vec<ToolDef>,
    servers: Vec<Server>,
    poisoned: Mutex<HashSet<String>>,
}

impl McpRegistry {
    pub async fn spawn_all(
        cfg: &Config,
        servers: &[McpServerStdio],
        cwd: &str,
    ) -> Result<Self, AgentError> {
        if servers.len() > MAX_MCP_SERVERS {
            return Err(AgentError::Mcp(format!(
                "too many MCP servers: {} > {MAX_MCP_SERVERS}",
                servers.len()
            )));
        }
        let mut reg = Self {
            by_qname: HashMap::new(),
            defs: Vec::new(),
            servers: Vec::new(),
            poisoned: Mutex::new(HashSet::new()),
        };

        let init_timeout = cfg.mcp_init_timeout;
        let mut seen_names = HashSet::new();
        for s in servers {
            if !valid_name(&s.name) {
                return Err(AgentError::Mcp(format!("invalid server name: {}", s.name)));
            }
            if !seen_names.insert(s.name.clone()) {
                return Err(AgentError::Mcp(format!("duplicate server name: {}", s.name)));
            }
            let mut cmd = Command::new(&s.command);
            cmd.args(&s.args);
            cmd.env_clear();
            for k in PASSTHROUGH_ENV {
                if let Ok(v) = std::env::var(k) {
                    cmd.env(k, v);
                }
            }
            for kv in &s.env {
                cmd.env(&kv.name, &kv.value);
            }
            cmd.current_dir(cwd);
            cmd.stderr(std::process::Stdio::inherit());

            #[cfg(unix)]
            cmd.process_group(0);

            let transport = TokioChildProcess::new(cmd)
                .map_err(|e| AgentError::Mcp(format!("spawn {}: {e}", s.name)))?;
            let pgid = transport.id();
            let kill_on_init_fail = || {
                if let Some(p) = pgid {
                    killpg(p, &s.name, "init");
                }
            };

            let client: Client = match tokio::time::timeout(init_timeout, ().serve(transport)).await
            {
                Ok(Ok(c)) => c,
                Ok(Err(e)) => {
                    kill_on_init_fail();
                    return Err(AgentError::Mcp(format!("init {}: {e}", s.name)));
                }
                Err(_) => {
                    kill_on_init_fail();
                    return Err(AgentError::Mcp(timeout_msg("init", &s.name, init_timeout)));
                }
            };
            let client = Arc::new(client);

            reg.servers.push(Server {
                name: s.name.clone(),
                pgid: Mutex::new(pgid),
            });

            let tools = match tokio::time::timeout(init_timeout, client.peer().list_all_tools())
                .await
            {
                Ok(Ok(t)) => t,
                Ok(Err(e)) => return Err(AgentError::Mcp(format!("list_tools {}: {e}", s.name))),
                Err(_) => {
                    return Err(AgentError::Mcp(timeout_msg(
                        "list_tools",
                        &s.name,
                        init_timeout,
                    )))
                }
            };

            for t in tools {
                if reg.defs.len() >= MAX_TOOLS_PER_SESSION {
                    return Err(AgentError::Mcp(format!(
                        "too many tools (>{MAX_TOOLS_PER_SESSION})"
                    )));
                }
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
                    description: clamp(
                        t.description.as_deref().unwrap_or("").to_owned(),
                        MAX_DESCRIPTION_BYTES,
                    ),
                    input_schema: cap_schema(&qname, Value::Object((*t.input_schema).clone())),
                });
                reg.by_qname.insert(
                    qname,
                    Entry {
                        server: s.name.clone(),
                        tool: bare,
                        client: client.clone(),
                    },
                );
            }
        }
        Ok(reg)
    }

    pub fn poison(&self, server_name: &str, reason: &str) {
        let newly = self
            .poisoned
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(server_name.to_owned());
        if !newly {
            return;
        }
        if let Some(server) = self.servers.iter().find(|s| s.name == server_name) {
            server.kill_group(reason);
        }
        eprintln!("sprout-agent: MCP server '{server_name}' killed after {reason}");
    }

    fn is_poisoned(&self, server_name: &str) -> bool {
        self.poisoned
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(server_name)
    }

    pub fn server_of(&self, qname: &str) -> Option<&str> {
        self.by_qname.get(qname).map(|e| e.server.as_str())
    }

    pub fn tools(&self) -> &[ToolDef] {
        &self.defs
    }

    pub fn has(&self, qname: &str) -> bool {
        self.by_qname.contains_key(qname)
    }

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
        if self.is_poisoned(&entry.server) {
            return Err(AgentError::Mcp(format!(
                "server unavailable after timeout: {}",
                entry.server
            )));
        }
        let arg_obj = match arguments {
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
        let text = collapse_content(&res.content, max_bytes);
        Ok(ToolResult {
            provider_id: provider_id.to_owned(),
            text: clamp(text, max_bytes),
            is_error: res.is_error.unwrap_or(false),
        })
    }
}

fn timeout_msg(stage: &str, name: &str, t: Duration) -> String {
    format!("{stage} {name}: timeout after {}s", t.as_secs())
}

fn cap_schema(qname: &str, schema: Value) -> Value {
    let size = serde_json::to_vec(&schema).map(|b| b.len()).unwrap_or(0);
    if size <= MAX_SCHEMA_BYTES {
        return schema;
    }
    eprintln!(
        "sprout-agent: tool {qname} schema is {size} bytes (>{MAX_SCHEMA_BYTES}); replacing with empty object"
    );
    Value::Object(Map::new())
}

#[cfg(unix)]
fn killpg(pgid: u32, name: &str, stage: &str) {
    use nix::sys::signal::{killpg as nix_killpg, Signal};
    use nix::unistd::Pid;
    let result = nix_killpg(Pid::from_raw(pgid as i32), Signal::SIGKILL);
    eprintln!("sprout-agent: killpg MCP {name} ({stage}) pgid={pgid} ok={}", result.is_ok());
}
#[cfg(not(unix))]
fn killpg(_pgid: u32, name: &str, stage: &str) {
    eprintln!("sprout-agent: relying on Drop to kill MCP {name} ({stage})");
}

fn valid_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_NAME_LEN
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn truncate_at_boundary(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut cut = max;
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    &s[..cut]
}

fn push_bounded(out: &mut String, s: &str, max: usize) {
    let remaining = max.saturating_sub(out.len());
    if remaining > 0 {
        out.push_str(truncate_at_boundary(s, remaining));
    }
}

fn collapse_content(blocks: &[rmcp::model::Content], max_bytes: usize) -> String {
    use rmcp::model::RawContent;
    let mut out = String::new();
    let short = |s: &str| truncate_at_boundary(s, MARKER_FIELD_MAX).to_owned();
    for c in blocks {
        if out.len() >= max_bytes {
            break;
        }
        if !out.is_empty() {
            push_bounded(&mut out, "\n", max_bytes);
        }
        let chunk: String = match &c.raw {
            RawContent::Text(t) => t.text.clone(),
            RawContent::Image(i) => {
                format!(
                    "[image elided: {}, {} bytes]",
                    short(&i.mime_type),
                    i.data.len()
                )
            }
            RawContent::Audio(a) => {
                format!(
                    "[audio elided: {}, {} bytes]",
                    short(&a.mime_type),
                    a.data.len()
                )
            }
            RawContent::ResourceLink(r) => format!("[resource: {}]", short(&r.uri)),
            RawContent::Resource(_) => "[resource elided]".into(),
        };
        push_bounded(&mut out, &chunk, max_bytes);
    }
    out
}
