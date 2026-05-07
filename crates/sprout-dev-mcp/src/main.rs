#![forbid(unsafe_code)]
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
    transport::stdio,
    ErrorData, ServerHandler, ServiceExt,
};
use std::path::Path;
use std::sync::Arc;

mod log;
mod rg;
mod shell;
mod shim;
mod str_replace;

#[derive(Clone)]
struct DevMcp {
    state: Arc<shell::SharedState>,
    tool_router: ToolRouter<DevMcp>,
}

#[tool_router]
impl DevMcp {
    fn new(state: Arc<shell::SharedState>) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        name = "shell",
        description = "Run a bash command. Ephemeral process per call. Output capture is hard-capped at 10MB per stream and shown tail-heavy (~8KB to the LLM); full captured output is saved to an artifact file when truncated. timeout_ms is capped at 600000. `rg` is on PATH (use it instead of grep)."
    )]
    async fn shell(
        &self,
        Parameters(p): Parameters<shell::ShellParams>,
    ) -> Result<CallToolResult, ErrorData> {
        shell::run(&self.state, p).await
    }

    #[tool(
        name = "str_replace",
        description = "Atomic find-and-replace in a file. `old_str` must occur exactly once. Returns a unified diff. Path is resolved relative to workdir (defaults to server cwd). Prefer this over sed/awk."
    )]
    async fn str_replace(
        &self,
        Parameters(p): Parameters<str_replace::StrReplaceParams>,
    ) -> Result<String, ErrorData> {
        str_replace::run(&self.state, p)
    }
}

#[tool_handler]
impl ServerHandler for DevMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(rmcp::model::Implementation::new(
                "sprout-dev-mcp",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(self.state.bootstrap_instructions.clone())
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let argv0 = std::env::args().next().unwrap_or_default();
    let cmd = Path::new(&argv0)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if cmd == "rg" {
        let args: Vec<String> = std::env::args().skip(1).collect();
        std::process::exit(rg::run(args));
    }

    let cwd = std::env::current_dir()?;
    let shim = shim::Shim::install()?;
    let state = Arc::new(shell::SharedState::new(cwd, shim)?);

    let service = DevMcp::new(state).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
