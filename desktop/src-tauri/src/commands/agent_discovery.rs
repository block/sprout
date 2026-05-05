use tauri::{AppHandle, State};

use crate::{
    app_state::AppState,
    managed_agents::{
        command_availability, discover_local_acp_providers, AcpProviderInfo,
        DiscoverManagedAgentPrereqsRequest, ManagedAgentPrereqsInfo, RelayAgentInfo,
        DEFAULT_ACP_COMMAND, DEFAULT_MCP_COMMAND,
    },
    nostr_convert,
    relay::query_relay,
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
    // Query kind:10100 agent profile events from the relay.
    let events = query_relay(
        &state,
        &[serde_json::json!({
            "kinds": [10100],
        })],
    )
    .await?;

    // The convert helper returns `{"agents": [...]}`. Extract and re-deserialize
    // into the strongly-typed `Vec<RelayAgentInfo>` the frontend expects.
    let value = nostr_convert::agents_from_events(&events);
    let agents = value
        .get("agents")
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));
    serde_json::from_value(agents).map_err(|e| format!("agent parse failed: {e}"))
}
