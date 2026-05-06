//! MCP registry: spawn stdio servers, list tools, call tools.
//!
//! Tool names are qualified `{server}__{tool}`. We validate against OpenAI's
//! function-name constraints (a-zA-Z0-9_-, ≤64) and reject duplicates so the
//! LLM sees a flat, well-formed namespace.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use rmcp::model::CallToolRequestParams;
use rmcp::service::{RoleClient, RunningService};
use rmcp::transport::TokioChildProcess;
use rmcp::ServiceExt;
use serde_json::{Map, Value};
use tokio::process::Command;

use crate::types::{clamp, AgentError, McpServerStdio, ToolDef, ToolResult};

const SEP: &str = "__";
const MAX_NAME_LEN: usize = 64;
/// Default cap on initialization handshake / tool listing per MCP server.
/// A stuck child must not freeze the whole agent. Tests override via
/// `ACP_SEED_MCP_INIT_TIMEOUT_SECS`.
const MCP_INIT_TIMEOUT_DEFAULT: Duration = Duration::from_secs(30);

fn mcp_init_timeout() -> Duration {
    std::env::var("ACP_SEED_MCP_INIT_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(MCP_INIT_TIMEOUT_DEFAULT)
}
/// Caps on tool metadata sent to the LLM. Protects against malicious or
/// buggy MCP servers that ship enormous descriptions/schemas. All caps are
/// in bytes so they are tight on the wire regardless of UTF-8 width.
const MAX_TOOLS_PER_SESSION: usize = 128;
const MAX_DESCRIPTION_BYTES: usize = 1024;
const MAX_SCHEMA_BYTES: usize = 4096;
/// Cap on number of MCP servers per session. Sixteen is a generous upper
/// bound for any reasonable agent setup; it bounds child spawn pressure.
pub const MAX_MCP_SERVERS: usize = 16;

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

/// Env vars passed through to MCP children unconditionally. Everything else
/// — including LLM API keys — is scrubbed so an untrusted MCP server cannot
/// exfiltrate them.
const PASSTHROUGH_ENV: &[&str] = &["PATH", "HOME", "TERM", "LANG", "LC_ALL", "TMPDIR"];

impl McpRegistry {
    /// Spawn all servers, list their tools. All-or-nothing: any failure aborts.
    /// `cwd` is the session working directory; each child inherits it.
    pub async fn spawn_all(
        servers: &[McpServerStdio],
        cwd: Option<&str>,
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
            _clients: Vec::new(),
        };

        let init_timeout = mcp_init_timeout();
        for s in servers {
            if !valid_name(&s.name) {
                return Err(AgentError::Mcp(format!("invalid server name: {}", s.name)));
            }
            let mut cmd = Command::new(&s.command);
            cmd.args(&s.args);
            // Scrub: no parent env leaks (e.g. ANTHROPIC_API_KEY) to the
            // MCP child. Whitelist a tiny set of essentials, then layer on
            // whatever the caller explicitly listed in `env`.
            cmd.env_clear();
            for k in PASSTHROUGH_ENV {
                if let Ok(v) = std::env::var(k) {
                    cmd.env(k, v);
                }
            }
            for kv in &s.env {
                cmd.env(&kv.name, &kv.value);
            }
            if let Some(dir) = cwd {
                if !dir.is_empty() {
                    cmd.current_dir(dir);
                }
            }
            cmd.stderr(std::process::Stdio::inherit());

            let transport = TokioChildProcess::new(cmd)
                .map_err(|e| AgentError::Mcp(format!("spawn {}: {e}", s.name)))?;
            // Capture the child pid so a stuck server can be force-killed
            // explicitly on timeout, rather than relying on transport-Drop
            // alone. Drop is best-effort; SIGKILL is decisive.
            let child_pid = transport.id();
            let client: Client = match tokio::time::timeout(init_timeout, ().serve(transport)).await
            {
                Ok(Ok(c)) => c,
                Ok(Err(e)) => {
                    force_kill(child_pid, &s.name, "init");
                    return Err(AgentError::Mcp(format!("init {}: {e}", s.name)));
                }
                Err(_) => {
                    force_kill(child_pid, &s.name, "init");
                    return Err(AgentError::Mcp(format!(
                        "init {}: timeout after {}s",
                        s.name,
                        init_timeout.as_secs()
                    )));
                }
            };
            let client = Arc::new(client);

            let tools =
                match tokio::time::timeout(init_timeout, client.peer().list_all_tools()).await {
                    Ok(Ok(t)) => t,
                    Ok(Err(e)) => {
                        force_kill(child_pid, &s.name, "list_tools");
                        return Err(AgentError::Mcp(format!("list_tools {}: {e}", s.name)));
                    }
                    Err(_) => {
                        force_kill(child_pid, &s.name, "list_tools");
                        return Err(AgentError::Mcp(format!(
                            "list_tools {}: timeout after {}s",
                            s.name,
                            init_timeout.as_secs()
                        )));
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
                let description = cap_description(t.description.as_deref().unwrap_or(""));
                let input_schema = cap_schema(&qname, Value::Object((*t.input_schema).clone()));
                reg.defs.push(ToolDef {
                    name: qname.clone(),
                    description,
                    input_schema,
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

        let text = collapse_content(&res.content, max_bytes);
        Ok(ToolResult {
            provider_id: provider_id.to_owned(),
            text: clamp(text, max_bytes),
            is_error: res.is_error.unwrap_or(false),
        })
    }
}

/// Truncate description to `MAX_DESCRIPTION_BYTES` (UTF-8 safe). Output
/// is GUARANTEED to be ≤ `MAX_DESCRIPTION_BYTES` bytes; the marker is
/// included WITHIN the cap (not appended past it). If the marker would not
/// fit, we drop it and just truncate.
fn cap_description(desc: &str) -> String {
    clamp(desc.to_owned(), MAX_DESCRIPTION_BYTES)
}

/// Reject schemas whose serialized form exceeds `MAX_SCHEMA_BYTES`. The LLM
/// gets an empty object instead — a bad schema is preferable to one that
/// blows up the request body.
fn cap_schema(qname: &str, schema: Value) -> Value {
    let size = serde_json::to_vec(&schema).map(|b| b.len()).unwrap_or(0);
    if size <= MAX_SCHEMA_BYTES {
        return schema;
    }
    eprintln!(
        "sprout-agent: tool {qname} schema is {size} bytes (>{MAX_SCHEMA_BYTES}); replacing with empty object",
    );
    Value::Object(Map::new())
}

/// Send SIGKILL to a stuck MCP child by pid. The transport's Drop impl
/// already best-effort kills, but it spawns a tokio task and may race the
/// process exiting. SIGKILL here is decisive and synchronous.
#[cfg(unix)]
fn force_kill(pid: Option<u32>, name: &str, stage: &str) {
    if let Some(p) = pid {
        // SAFETY: kill(2) with SIGKILL on a pid we just spawned. ESRCH on
        // an already-exited child is fine.
        let rc = unsafe { libc::kill(p as libc::pid_t, libc::SIGKILL) };
        eprintln!("sprout-agent: kill MCP {name} ({stage}) pid={p} rc={rc}");
    }
}
#[cfg(not(unix))]
fn force_kill(pid: Option<u32>, name: &str, stage: &str) {
    eprintln!("sprout-agent: relying on Drop to kill MCP {name} ({stage}) pid={pid:?}");
}

/// OpenAI function-name constraints: ^[a-zA-Z0-9_-]{1,64}$
fn valid_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_NAME_LEN
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Append `s` to `out`, but only up to `max - out.len()` bytes (UTF-8 safe).
fn push_bounded(out: &mut String, s: &str, max: usize) {
    let remaining = max.saturating_sub(out.len());
    if remaining == 0 {
        return;
    }
    if s.len() <= remaining {
        out.push_str(s);
    } else {
        let mut cut = remaining;
        while cut > 0 && !s.is_char_boundary(cut) {
            cut -= 1;
        }
        out.push_str(&s[..cut]);
    }
}

/// Pre-truncate UTF-8 strings before formatting them into markers, so a 10MB
/// uri/mime never gets allocated in full only to be clipped by `push_bounded`.
const MARKER_FIELD_MAX: usize = 256;
fn short(s: &str) -> &str {
    if s.len() <= MARKER_FIELD_MAX {
        return s;
    }
    let mut cut = MARKER_FIELD_MAX;
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    &s[..cut]
}

/// Flatten MCP content blocks into a single text blob. Binary content is elided
/// with a marker so the model knows it existed. Every append is bounded by
/// `max_bytes` so a huge resource blob is never serialized in full before being
/// truncated. Final clamping is still done by the caller.
fn collapse_content(blocks: &[rmcp::model::Content], max_bytes: usize) -> String {
    use rmcp::model::RawContent;
    let mut out = String::new();
    for c in blocks {
        if out.len() >= max_bytes {
            break;
        }
        if !out.is_empty() {
            push_bounded(&mut out, "\n", max_bytes);
        }
        match &c.raw {
            RawContent::Text(t) => push_bounded(&mut out, &t.text, max_bytes),
            RawContent::Image(i) => push_bounded(
                &mut out,
                &format!(
                    "[image elided: {}, {} bytes]",
                    short(&i.mime_type),
                    i.data.len()
                ),
                max_bytes,
            ),
            RawContent::Audio(a) => push_bounded(
                &mut out,
                &format!(
                    "[audio elided: {}, {} bytes]",
                    short(&a.mime_type),
                    a.data.len()
                ),
                max_bytes,
            ),
            RawContent::ResourceLink(r) => push_bounded(
                &mut out,
                &format!("[resource: {}]", short(&r.uri)),
                max_bytes,
            ),
            RawContent::Resource(_) => {
                // Resources can be huge (entire files). Elide rather than
                // serialize the whole blob just to truncate it.
                push_bounded(&mut out, "[resource elided]", max_bytes);
            }
        }
    }
    out
}
