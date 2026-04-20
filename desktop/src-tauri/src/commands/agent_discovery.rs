use reqwest::Method;
use tauri::{AppHandle, State};

use crate::{
    app_state::AppState,
    managed_agents::{
        command_availability, discover_local_acp_providers, AcpProviderInfo,
        DiscoverManagedAgentPrereqsRequest, ManagedAgentPrereqsInfo, RelayAgentInfo,
        DEFAULT_ACP_COMMAND, DEFAULT_MCP_COMMAND,
    },
    relay::{build_authed_request, send_json_request},
};

#[tauri::command]
pub fn discover_acp_providers() -> Vec<AcpProviderInfo> {
    discover_local_acp_providers()
}

#[tauri::command]
pub fn discover_managed_agent_prereqs(
    input: DiscoverManagedAgentPrereqsRequest,
    app: AppHandle,
) -> ManagedAgentPrereqsInfo {
    let acp_command = input
        .acp_command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_ACP_COMMAND);
    let mcp_command = input
        .mcp_command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_MCP_COMMAND);

    ManagedAgentPrereqsInfo {
        acp: command_availability(acp_command, Some(&app)),
        mcp: command_availability(mcp_command, Some(&app)),
    }
}

#[tauri::command]
pub async fn list_relay_agents(state: State<'_, AppState>) -> Result<Vec<RelayAgentInfo>, String> {
    let request = build_authed_request(&state.http_client, Method::GET, "/api/agents", &state)?;
    send_json_request(request).await
}
