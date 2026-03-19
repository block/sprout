use nostr::{Keys, ToBech32};
use tauri::{AppHandle, State};

use crate::{
    app_state::AppState,
    managed_agents::{
        build_managed_agent_summary, default_token_scopes, find_managed_agent_mut,
        load_managed_agents, load_personas, managed_agent_avatar_url, managed_agent_log_path,
        read_log_tail, save_managed_agents,
        start_managed_agent_process, stop_managed_agent_process, sync_managed_agent_processes,
        CreateManagedAgentRequest, CreateManagedAgentResponse, ManagedAgentLogResponse,
        ManagedAgentSummary, MintManagedAgentTokenRequest, MintManagedAgentTokenResponse,
        DEFAULT_ACP_COMMAND, DEFAULT_AGENT_ARG, DEFAULT_AGENT_COMMAND,
        DEFAULT_AGENT_PARALLELISM, DEFAULT_AGENT_TURN_TIMEOUT_SECONDS,
        DEFAULT_MCP_COMMAND,
    },
    relay::{mint_managed_agent_api_token, relay_ws_url, sync_managed_agent_profile},
    util::now_iso,
};

#[tauri::command]
pub fn list_managed_agents(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<ManagedAgentSummary>, String> {
    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|error| error.to_string())?;
    let mut records = load_managed_agents(&app)?;
    let mut runtimes = state
        .managed_agent_processes
        .lock()
        .map_err(|error| error.to_string())?;

    if sync_managed_agent_processes(&mut records, &mut runtimes) {
        save_managed_agents(&app, &records)?;
    }

    records
        .iter()
        .map(|record| build_managed_agent_summary(&app, record, &runtimes))
        .collect()
}

#[tauri::command]
pub async fn create_managed_agent(
    input: CreateManagedAgentRequest,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<CreateManagedAgentResponse, String> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err("agent name is required".to_string());
    }
    let requested_persona_id = input
        .persona_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if let Some(parallelism) = input.parallelism {
        if !(1..=32).contains(&parallelism) {
            return Err("parallelism must be between 1 and 32".to_string());
        }
    }

    let resolved_relay_url = input
        .relay_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(relay_ws_url);
    let keys = Keys::generate();
    let pubkey = keys.public_key().to_hex();
    let private_key_nsec = keys
        .secret_key()
        .to_bech32()
        .map_err(|error| format!("failed to encode private key: {error}"))?;
    let token_scopes = if input.mint_token {
        let requested = input
            .token_scopes
            .iter()
            .map(|scope| scope.trim().to_string())
            .filter(|scope| !scope.is_empty())
            .collect::<Vec<_>>();
        if requested.is_empty() {
            default_token_scopes()
        } else {
            requested
        }
    } else {
        Vec::new()
    };
    let token_name = input
        .token_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(name)
        .to_string();
    let api_token = if input.mint_token {
        Some(
            mint_managed_agent_api_token(
                &state.http_client,
                &resolved_relay_url,
                &keys,
                &token_name,
                &token_scopes,
            )
            .await?
            .token,
        )
    } else {
        None
    };
    let mut agent_args = input
        .agent_args
        .into_iter()
        .map(|arg| arg.trim().to_string())
        .filter(|arg| !arg.is_empty())
        .collect::<Vec<_>>();
    if agent_args.is_empty() {
        agent_args.push(DEFAULT_AGENT_ARG.to_string());
    }
    let system_prompt = input
        .system_prompt
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let model = input
        .model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let acp_command = input
        .acp_command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_ACP_COMMAND)
        .to_string();
    let agent_command = input
        .agent_command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_AGENT_COMMAND)
        .to_string();
    let mcp_command = input
        .mcp_command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_MCP_COMMAND)
        .to_string();
    let turn_timeout_seconds = input
        .turn_timeout_seconds
        .filter(|seconds| *seconds > 0)
        .unwrap_or(DEFAULT_AGENT_TURN_TIMEOUT_SECONDS);
    let parallelism = input
        .parallelism
        .filter(|count| (1..=32).contains(count))
        .unwrap_or(DEFAULT_AGENT_PARALLELISM);

    let (agent, spawn_error) = {
        let _store_guard = state
            .managed_agents_store_lock
            .lock()
            .map_err(|error| error.to_string())?;
        let mut records = load_managed_agents(&app)?;
        let mut runtimes = state
            .managed_agent_processes
            .lock()
            .map_err(|error| error.to_string())?;

        if sync_managed_agent_processes(&mut records, &mut runtimes) {
            save_managed_agents(&app, &records)?;
        }
        if let Some(persona_id) = requested_persona_id.as_deref() {
            let personas = load_personas(&app)?;
            if !personas.iter().any(|persona| persona.id == persona_id) {
                return Err(format!("persona {persona_id} not found"));
            }
        }
        if records.iter().any(|record| record.pubkey == pubkey) {
            return Err(format!("agent {pubkey} already exists"));
        }

        records.push(crate::managed_agents::ManagedAgentRecord {
            pubkey: pubkey.clone(),
            name: name.to_string(),
            persona_id: requested_persona_id.clone(),
            private_key_nsec: private_key_nsec.clone(),
            api_token: api_token.clone(),
            relay_url: resolved_relay_url.clone(),
            acp_command,
            agent_command,
            agent_args,
            mcp_command,
            turn_timeout_seconds,
            parallelism,
            system_prompt,
            model,
            start_on_app_launch: input.start_on_app_launch,
            runtime_pid: None,
            created_at: now_iso(),
            updated_at: now_iso(),
            last_started_at: None,
            last_stopped_at: None,
            last_exit_code: None,
            last_error: None,
        });

        let mut spawn_error = None;
        if input.spawn_after_create {
            let record = find_managed_agent_mut(&mut records, &pubkey)?;
            if let Err(error) = start_managed_agent_process(&app, record, &mut runtimes) {
                record.updated_at = now_iso();
                record.last_error = Some(error.clone());
                spawn_error = Some(error);
            }
        }

        save_managed_agents(&app, &records)?;

        let record = records
            .iter()
            .find(|record| record.pubkey == pubkey)
            .ok_or_else(|| "created agent disappeared unexpectedly".to_string())?;
        let agent = build_managed_agent_summary(&app, record, &runtimes)?;

        Ok::<_, String>((agent, spawn_error))
    }?;

    let avatar_url = input
        .avatar_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| managed_agent_avatar_url(agent.agent_command.as_str()));
    let profile_sync_error = match sync_managed_agent_profile(
        &state,
        &resolved_relay_url,
        &pubkey,
        api_token.as_deref(),
        &token_scopes,
        name,
        avatar_url.as_deref(),
    )
    .await
    {
        Ok(()) => None,
        Err(error) => Some(error),
    };

    Ok(CreateManagedAgentResponse {
        agent,
        private_key_nsec,
        api_token,
        profile_sync_error,
        spawn_error,
    })
}

#[tauri::command]
pub fn start_managed_agent(
    pubkey: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ManagedAgentSummary, String> {
    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|error| error.to_string())?;
    let mut records = load_managed_agents(&app)?;
    let mut runtimes = state
        .managed_agent_processes
        .lock()
        .map_err(|error| error.to_string())?;

    if sync_managed_agent_processes(&mut records, &mut runtimes) {
        save_managed_agents(&app, &records)?;
    }

    {
        let record = find_managed_agent_mut(&mut records, &pubkey)?;
        start_managed_agent_process(&app, record, &mut runtimes)?;
    }

    save_managed_agents(&app, &records)?;
    let record = records
        .iter()
        .find(|record| record.pubkey == pubkey)
        .ok_or_else(|| format!("agent {pubkey} not found"))?;
    build_managed_agent_summary(&app, record, &runtimes)
}

#[tauri::command]
pub fn stop_managed_agent(
    pubkey: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ManagedAgentSummary, String> {
    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|error| error.to_string())?;
    let mut records = load_managed_agents(&app)?;
    let mut runtimes = state
        .managed_agent_processes
        .lock()
        .map_err(|error| error.to_string())?;

    if sync_managed_agent_processes(&mut records, &mut runtimes) {
        save_managed_agents(&app, &records)?;
    }

    {
        let record = find_managed_agent_mut(&mut records, &pubkey)?;
        stop_managed_agent_process(record, &mut runtimes)?;
    }

    save_managed_agents(&app, &records)?;
    let record = records
        .iter()
        .find(|record| record.pubkey == pubkey)
        .ok_or_else(|| format!("agent {pubkey} not found"))?;
    build_managed_agent_summary(&app, record, &runtimes)
}

#[tauri::command]
pub fn delete_managed_agent(
    pubkey: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|error| error.to_string())?;
    let mut records = load_managed_agents(&app)?;
    let mut runtimes = state
        .managed_agent_processes
        .lock()
        .map_err(|error| error.to_string())?;

    if sync_managed_agent_processes(&mut records, &mut runtimes) {
        save_managed_agents(&app, &records)?;
    }

    if let Some(record) = records.iter_mut().find(|record| record.pubkey == pubkey) {
        stop_managed_agent_process(record, &mut runtimes)?;
    }

    let initial_len = records.len();
    records.retain(|record| record.pubkey != pubkey);
    if records.len() == initial_len {
        return Err(format!("agent {pubkey} not found"));
    }

    save_managed_agents(&app, &records)
}

#[tauri::command]
pub async fn mint_managed_agent_token(
    input: MintManagedAgentTokenRequest,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<MintManagedAgentTokenResponse, String> {
    let scopes = input
        .scopes
        .into_iter()
        .map(|scope| scope.trim().to_string())
        .filter(|scope| !scope.is_empty())
        .collect::<Vec<_>>();
    let scopes = if scopes.is_empty() {
        default_token_scopes()
    } else {
        scopes
    };
    let (pubkey, relay_url, private_key_nsec, token_name) = {
        let _store_guard = state
            .managed_agents_store_lock
            .lock()
            .map_err(|error| error.to_string())?;
        let mut records = load_managed_agents(&app)?;
        let mut runtimes = state
            .managed_agent_processes
            .lock()
            .map_err(|error| error.to_string())?;

        if sync_managed_agent_processes(&mut records, &mut runtimes) {
            save_managed_agents(&app, &records)?;
        }

        let record = find_managed_agent_mut(&mut records, &input.pubkey)?;
        (
            record.pubkey.clone(),
            record.relay_url.clone(),
            record.private_key_nsec.clone(),
            input
                .token_name
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("{}-token", record.name)),
        )
    };
    let keys = Keys::parse(private_key_nsec.trim())
        .map_err(|error| format!("failed to parse managed agent private key: {error}"))?;
    let minted = mint_managed_agent_api_token(
        &state.http_client,
        &relay_url,
        &keys,
        &token_name,
        &scopes,
    )
    .await?;

    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|error| error.to_string())?;
    let mut records = load_managed_agents(&app)?;
    let mut runtimes = state
        .managed_agent_processes
        .lock()
        .map_err(|error| error.to_string())?;

    if sync_managed_agent_processes(&mut records, &mut runtimes) {
        save_managed_agents(&app, &records)?;
    }

    let record = find_managed_agent_mut(&mut records, &pubkey)?;
    record.api_token = Some(minted.token.clone());
    record.updated_at = now_iso();
    record.last_error = None;

    save_managed_agents(&app, &records)?;
    let record = records
        .iter()
        .find(|record| record.pubkey == pubkey)
        .ok_or_else(|| format!("agent {pubkey} not found"))?;
    let agent = build_managed_agent_summary(&app, record, &runtimes)?;

    Ok(MintManagedAgentTokenResponse {
        agent,
        token: minted.token,
    })
}

#[tauri::command]
pub fn get_managed_agent_log(
    pubkey: String,
    line_count: Option<u32>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ManagedAgentLogResponse, String> {
    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|error| error.to_string())?;
    let records = load_managed_agents(&app)?;
    if !records.iter().any(|record| record.pubkey == pubkey) {
        return Err(format!("agent {pubkey} not found"));
    }

    let log_path = managed_agent_log_path(&app, &pubkey)?;
    Ok(ManagedAgentLogResponse {
        content: read_log_tail(&log_path, line_count.unwrap_or(120) as usize)?,
        log_path: log_path.display().to_string(),
    })
}
