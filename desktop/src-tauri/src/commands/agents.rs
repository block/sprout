use nostr::{Keys, ToBech32};
use tauri::{AppHandle, State};

use crate::{
    app_state::AppState,
    managed_agents::{
        build_managed_agent_summary, default_token_scopes, find_managed_agent_mut,
        load_managed_agents, load_personas, managed_agent_avatar_url, managed_agent_log_path,
        mint_token_via_api, read_log_tail, save_managed_agents, start_managed_agent_process,
        stop_managed_agent_process, sync_managed_agent_processes, CreateManagedAgentRequest,
        CreateManagedAgentResponse, ManagedAgentLogResponse, ManagedAgentSummary,
        MintManagedAgentTokenRequest, MintManagedAgentTokenResponse, DEFAULT_AGENT_ARG,
        DEFAULT_ACP_COMMAND, DEFAULT_AGENT_COMMAND, DEFAULT_AGENT_PARALLELISM,
        DEFAULT_AGENT_TURN_TIMEOUT_SECONDS, DEFAULT_MCP_COMMAND,
    },
    relay::{relay_ws_url, sync_managed_agent_profile},
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
    let name = input.name.trim().to_string();
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

    // ── Phase 1: generate keys and collect mint parameters (sync lock) ────────
    // We do NOT mint here — minting is async and must happen outside the lock.
    let (agent_keys, private_key_nsec, pubkey, resolved_relay_url, token_scopes, token_name, mint_token, input) = {
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
        let keys = Keys::generate();
        let pubkey = keys.public_key().to_hex();
        if records.iter().any(|record| record.pubkey == pubkey) {
            return Err(format!("agent {pubkey} already exists"));
        }
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
            .unwrap_or(name.as_str())
            .to_string();

        let resolved_relay_url = input
            .relay_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(relay_ws_url);

        let mint_token = input.mint_token;
        (keys, private_key_nsec, pubkey, resolved_relay_url, token_scopes, token_name, mint_token, input)
    };

    // ── Phase 2: mint token via REST API (async, outside lock) ───────────────
    let api_token: Option<String> = if mint_token {
        let token = mint_token_via_api(
            &state,
            &agent_keys,
            &resolved_relay_url,
            &token_name,
            &token_scopes,
        )
        .await?;
        Some(token)
    } else {
        None
    };

    // ── Phase 3: save record and optionally spawn (sync lock) ─────────────────
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

        // Guard against a duplicate pubkey appearing between phase 1 and phase 3
        // (extremely unlikely but safe to check).
        if records.iter().any(|record| record.pubkey == pubkey) {
            return Err(format!("agent {pubkey} already exists"));
        }
        let mut record = crate::managed_agents::ManagedAgentRecord {
            pubkey: pubkey.clone(),
            name: name.clone(),
            persona_id: requested_persona_id.clone(),
            private_key_nsec: private_key_nsec.clone(),
            api_token: api_token.clone(),
            relay_url: resolved_relay_url.clone(),
            acp_command: input
                .acp_command
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(DEFAULT_ACP_COMMAND)
                .to_string(),
            agent_command: input
                .agent_command
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(DEFAULT_AGENT_COMMAND)
                .to_string(),
            agent_args: input
                .agent_args
                .into_iter()
                .map(|arg| arg.trim().to_string())
                .filter(|arg| !arg.is_empty())
                .collect::<Vec<_>>(),
            mcp_command: input
                .mcp_command
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(DEFAULT_MCP_COMMAND)
                .to_string(),
            turn_timeout_seconds: input
                .turn_timeout_seconds
                .filter(|seconds| *seconds > 0)
                .unwrap_or(DEFAULT_AGENT_TURN_TIMEOUT_SECONDS),
            parallelism: input
                .parallelism
                .filter(|count| (1..=32).contains(count))
                .unwrap_or(DEFAULT_AGENT_PARALLELISM),
            system_prompt: input
                .system_prompt
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            model: input
                .model
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            start_on_app_launch: input.start_on_app_launch,
            runtime_pid: None,
            created_at: now_iso(),
            updated_at: now_iso(),
            last_started_at: None,
            last_stopped_at: None,
            last_exit_code: None,
            last_error: None,
        };

        if record.agent_args.is_empty() {
            record.agent_args.push(DEFAULT_AGENT_ARG.to_string());
        }

        records.push(record);

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

        (agent, spawn_error)
    };

    // ── Phase 4: sync agent profile on relay (async, outside lock) ───────────
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
        &name,
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
    // ── Phase 1: load agent record and collect mint parameters (sync lock) ────
    let (agent_keys, relay_url, scopes, token_name) = {
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

        let scopes = {
            let requested = input
                .scopes
                .into_iter()
                .map(|scope| scope.trim().to_string())
                .filter(|scope| !scope.is_empty())
                .collect::<Vec<_>>();
            if requested.is_empty() {
                default_token_scopes()
            } else {
                requested
            }
        };

        let token_name = input
            .token_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("{}-token", record.name));

        // Reconstruct the agent's keypair from the stored nsec so we can sign
        // the NIP-98 auth event as the agent (not the desktop user).
        let agent_keys = Keys::parse(&record.private_key_nsec)
            .map_err(|e| format!("failed to parse agent secret key: {e}"))?;

        (agent_keys, record.relay_url.clone(), scopes, token_name)
    };

    // ── Phase 2: mint token via REST API (async, outside lock) ───────────────
    let minted_token = mint_token_via_api(&state, &agent_keys, &relay_url, &token_name, &scopes).await?;

    // ── Phase 3: persist new token to agent record (sync lock) ───────────────
    let (agent, api_token) = {
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
        record.api_token = Some(minted_token.clone());
        record.updated_at = now_iso();
        record.last_error = None;
        let pubkey = record.pubkey.clone();

        save_managed_agents(&app, &records)?;

        let record = records
            .iter()
            .find(|record| record.pubkey == pubkey)
            .ok_or_else(|| format!("agent {pubkey} not found"))?;
        let agent = build_managed_agent_summary(&app, record, &runtimes)?;

        (agent, minted_token)
    };

    Ok(MintManagedAgentTokenResponse { agent, token: api_token })
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
