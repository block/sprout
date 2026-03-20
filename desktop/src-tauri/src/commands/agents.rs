use nostr::{Keys, ToBech32};
use tauri::{AppHandle, State};

use crate::{
    app_state::AppState,
    managed_agents::{
        build_managed_agent_summary, default_token_scopes, discover_provider_candidates, validate_provider_config,
        find_managed_agent_mut, invoke_provider, load_managed_agents, load_personas,
        managed_agent_avatar_url, managed_agent_log_path, mint_token_via_api,
        provider_deploy, read_log_tail, resolve_command, save_managed_agents,
        start_managed_agent_process, stop_managed_agent_process, sync_managed_agent_processes,
        BackendKind, BackendProviderInfo, CreateManagedAgentRequest, CreateManagedAgentResponse,
        ManagedAgentLogResponse, ManagedAgentSummary, MintManagedAgentTokenRequest,
        MintManagedAgentTokenResponse, DEFAULT_AGENT_ARG, DEFAULT_ACP_COMMAND,
        DEFAULT_AGENT_COMMAND, DEFAULT_AGENT_PARALLELISM, DEFAULT_AGENT_TURN_TIMEOUT_SECONDS,
        DEFAULT_MCP_COMMAND,
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

    // ── Pre-Phase 2: validate provider config BEFORE any side effects ────────
    if let BackendKind::Provider { ref config, ref id } = input.backend {
        validate_provider_config(config)?;
        let bin_name = format!("sprout-backend-{id}");
        if resolve_command(&bin_name, Some(&app)).is_none() {
            return Err(format!("provider binary '{bin_name}' not found in PATH"));
        }
    }

    // ── Phase 2: mint token via REST API (async, outside lock) ───────────────
    // Pass the desktop user's pubkey as the agent owner so the relay records
    // the ownership chain. Only NIP-98 bootstrap mints can set owner_pubkey.
    let user_pubkey_hex = {
        let keys = state.keys.lock().map_err(|e| e.to_string())?;
        keys.public_key().to_hex()
    };
    let api_token: Option<String> = if mint_token {
        let token = mint_token_via_api(
            &state,
            &agent_keys,
            &resolved_relay_url,
            &token_name,
            &token_scopes,
            Some(&user_pubkey_hex),
        )
        .await?;
        Some(token)
    } else {
        None
    };

    // Agent ownership is set atomically during token mint via owner_pubkey
    // in the request body — no separate API call needed.

    // Extract deploy-relevant fields before Phase 3 moves `input`.
    let deploy_agent_command = input.agent_command.as_deref().unwrap_or(DEFAULT_AGENT_COMMAND).to_string();
    let deploy_agent_args: Vec<String> = input.agent_args.iter().map(|a| a.trim().to_string()).filter(|a| !a.is_empty()).collect();
    let deploy_system_prompt = input.system_prompt.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(String::from);
    let deploy_model = input.model.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(String::from);
    let deploy_turn_timeout = input.turn_timeout_seconds.filter(|s| *s > 0).unwrap_or(DEFAULT_AGENT_TURN_TIMEOUT_SECONDS);
    let deploy_parallelism = input.parallelism.filter(|p| (1..=32).contains(p)).unwrap_or(DEFAULT_AGENT_PARALLELISM);

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
        // Validate provider config if backend is Provider.
        if let BackendKind::Provider { ref config, .. } = input.backend {
            validate_provider_config(config)?;
        }
        // Resolve provider binary path if backend is Provider.
        let provider_binary_path = if let BackendKind::Provider { ref id, .. } = input.backend {
            let bin_name = format!("sprout-backend-{id}");
            resolve_command(&bin_name, Some(&app)).map(|p| p.display().to_string())
        } else {
            None
        };

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
            backend: input.backend.clone(),
            backend_agent_id: None,
            provider_binary_path,
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
        if input.spawn_after_create && input.backend == BackendKind::Local {
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

    // ── Phase 5: provider deploy (async, outside lock) ───────────────────────
    let spawn_error = if input.spawn_after_create && input.backend != BackendKind::Local {
        if let BackendKind::Provider { ref id, ref config } = input.backend {
            let provider_bin_name = format!("sprout-backend-{id}");
            let binary = resolve_command(&provider_bin_name, Some(&app));
            match binary {
                None => Some(format!("provider binary '{provider_bin_name}' not found")),
                Some(bin_path) => {
                    let agent_json = serde_json::json!({
                        "name": &name,
                        "relay_url": &resolved_relay_url,
                        "private_key_nsec": &private_key_nsec,
                        "api_token": &api_token,
                        "agent_command": &deploy_agent_command,
                        "agent_args": &deploy_agent_args,
                        "system_prompt": &deploy_system_prompt,
                        "model": &deploy_model,
                        "turn_timeout_seconds": deploy_turn_timeout,
                        "parallelism": deploy_parallelism,
                    });
                    let config_clone = config.clone();
                    match tokio::task::spawn_blocking(move || {
                        provider_deploy(&bin_path, &agent_json, &config_clone)
                    })
                    .await
                    {
                        Ok(Ok(backend_agent_id)) => {
                            // Persist the backend_agent_id back to the record.
                            let _store_guard = state
                                .managed_agents_store_lock
                                .lock()
                                .map_err(|e| e.to_string())?;
                            let mut records = load_managed_agents(&app)?;
                            if let Some(rec) = records.iter_mut().find(|r| r.pubkey == pubkey) {
                                rec.backend_agent_id = Some(backend_agent_id);
                                rec.last_started_at = Some(now_iso());
                                rec.updated_at = now_iso();
                            }
                            save_managed_agents(&app, &records)?;
                            spawn_error
                        }
                        Ok(Err(e)) => {
                            // Persist last_error so the table shows the failure.
                            if let Ok(_guard) = state.managed_agents_store_lock.lock() {
                                if let Ok(mut records) = load_managed_agents(&app) {
                                    if let Some(rec) = records.iter_mut().find(|r| r.pubkey == pubkey) {
                                        rec.last_error = Some(e.clone());
                                        rec.updated_at = now_iso();
                                    }
                                    let _ = save_managed_agents(&app, &records);
                                }
                            }
                            Some(e)
                        }
                        Err(e) => Some(format!("spawn_blocking failed: {e}")),
                    }
                }
            }
        } else {
            spawn_error
        }
    } else {
        spawn_error
    };

    // Rebuild summary if provider deploy may have updated backend_agent_id.
    let final_agent = if input.backend != BackendKind::Local && spawn_error.is_none() {
        let _store_guard = state.managed_agents_store_lock.lock().map_err(|e| e.to_string())?;
        let records = load_managed_agents(&app)?;
        let runtimes = state.managed_agent_processes.lock().map_err(|e| e.to_string())?;
        let record = records.iter().find(|r| r.pubkey == pubkey)
            .ok_or_else(|| "agent disappeared".to_string())?;
        build_managed_agent_summary(&app, record, &runtimes)?
    } else {
        agent
    };

    Ok(CreateManagedAgentResponse {
        agent: final_agent,
        private_key_nsec,
        api_token,
        profile_sync_error,
        spawn_error,
    })
}

#[tauri::command]
pub async fn start_managed_agent(
    pubkey: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ManagedAgentSummary, String> {
    // Collect what we need before releasing the lock for async work.
    let (backend, provider_binary_path, agent_json, relay_url) = {
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

        if record.backend == BackendKind::Local {
            // Local: spawn in-process and return immediately.
            start_managed_agent_process(&app, record, &mut runtimes)?;
            save_managed_agents(&app, &records)?;
            let record = records
                .iter()
                .find(|r| r.pubkey == pubkey)
                .ok_or_else(|| format!("agent {pubkey} not found"))?;
            return build_managed_agent_summary(&app, record, &runtimes);
        }

        // No guard on backend_agent_id — deploy is idempotent.
        // The provider must handle stale state (kill old process, redeploy).
        // backend_agent_id is overwritten with the new deploy response.

        let agent_json = serde_json::json!({
            "name": &record.name,
            "relay_url": &record.relay_url,
            "private_key_nsec": &record.private_key_nsec,
            "api_token": &record.api_token,
            "agent_command": &record.agent_command,
            "agent_args": &record.agent_args,
            "system_prompt": &record.system_prompt,
            "model": &record.model,
            "turn_timeout_seconds": record.turn_timeout_seconds,
            "parallelism": record.parallelism,
        });
        (
            record.backend.clone(),
            record.provider_binary_path.clone(),
            agent_json,
            record.relay_url.clone(),
        )
    };

    // Provider backend: deploy via binary (async, outside lock).
    if let BackendKind::Provider { ref id, ref config } = backend {
        let bin_path = match provider_binary_path
            .as_deref()
            .and_then(|p| Some(std::path::PathBuf::from(p)))
            .filter(|p| p.exists())
            .or_else(|| resolve_command(&format!("sprout-backend-{id}"), Some(&app)))
        {
            Some(p) => p,
            None => return Err(format!("provider binary 'sprout-backend-{id}' not found")),
        };

        let config_clone = config.clone();
        let deploy_result = tokio::task::spawn_blocking(move || {
            provider_deploy(&bin_path, &agent_json, &config_clone)
        })
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?;

        let _store_guard = state
            .managed_agents_store_lock
            .lock()
            .map_err(|e| e.to_string())?;
        let mut records = load_managed_agents(&app)?;
        let runtimes = state
            .managed_agent_processes
            .lock()
            .map_err(|e| e.to_string())?;
        match deploy_result {
            Ok(backend_agent_id) => {
                if let Some(rec) = records.iter_mut().find(|r| r.pubkey == pubkey) {
                    rec.backend_agent_id = Some(backend_agent_id);
                    rec.last_started_at = Some(now_iso());
                    rec.updated_at = now_iso();
                    rec.last_error = None;
                }
            }
            Err(ref e) => {
                // Redeploy failed — persist the error but keep backend_agent_id
                // (the previous deployment may still be running).
                if let Some(rec) = records.iter_mut().find(|r| r.pubkey == pubkey) {
                    rec.last_error = Some(e.clone());
                    rec.updated_at = now_iso();
                }
                save_managed_agents(&app, &records)?;
                return Err(e.clone());
            }
        }
        save_managed_agents(&app, &records)?;
        let record = records
            .iter()
            .find(|r| r.pubkey == pubkey)
            .ok_or_else(|| format!("agent {pubkey} not found"))?;
        return build_managed_agent_summary(&app, record, &runtimes);
    }

    let _ = relay_url; // suppress unused warning if Provider arm not reached
    Err(format!("agent {pubkey} has unsupported backend kind"))
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
        // Remote agents are stopped via !shutdown @mention from the frontend,
        // not via this backend command. Reject the call.
        if record.backend != BackendKind::Local {
            return Err(
                "remote agents are stopped via !shutdown message, not this command".to_string(),
            );
        }
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
        // For local agents: kills the process. For remote agents: no-op (the frontend
        // sends !shutdown via WebSocket before calling delete). Either way, safe.
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
    // Re-minting: do NOT send owner_pubkey. Ownership was established during
    // the first mint (create flow). Sending it again would be rejected by the
    // relay if the owner is already set to a different pubkey.
    let minted_token = mint_token_via_api(&state, &agent_keys, &relay_url, &token_name, &scopes, None).await?;

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
    let record = records.iter().find(|record| record.pubkey == pubkey)
        .ok_or_else(|| format!("agent {pubkey} not found"))?;
    if record.backend != BackendKind::Local {
        return Err("logs are not available for remote agents".to_string());
    }

    let log_path = managed_agent_log_path(&app, &pubkey)?;
    Ok(ManagedAgentLogResponse {
        content: read_log_tail(&log_path, line_count.unwrap_or(120) as usize)?,
        log_path: log_path.display().to_string(),
    })
}

// ── New backend-provider commands ────────────────────────────────────────────

#[tauri::command]
pub fn discover_backend_providers() -> Vec<BackendProviderInfo> {
    discover_provider_candidates()
        .into_iter()
        .map(|(id, path)| BackendProviderInfo {
            id,
            binary_path: path.display().to_string(),
        })
        .collect()
}

#[tauri::command]
pub async fn probe_backend_provider(binary_path: String) -> Result<serde_json::Value, String> {
    // Validate that the requested path is actually a discovered sprout-backend-* binary.
    // This prevents arbitrary binary execution via a compromised frontend or IPC.
    let candidates = discover_provider_candidates();
    let path = std::path::PathBuf::from(&binary_path);
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("binary not found: {binary_path}: {e}"))?;
    let is_known = candidates
        .iter()
        .any(|(_, p)| p.canonicalize().ok().as_ref() == Some(&canonical));
    if !is_known {
        return Err(format!(
            "binary '{binary_path}' is not a discovered sprout-backend-* provider"
        ));
    }
    let request = serde_json::json!({
        "op": "info",
        "request_id": uuid::Uuid::new_v4().to_string(),
    });
    tokio::task::spawn_blocking(move || {
        invoke_provider(&canonical, &request, std::time::Duration::from_secs(10))
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))?
}

// Remote agent shutdown is handled entirely by the frontend:
// 1. Frontend sends "!shutdown" @mention via WebSocket (signed by user's key)
// 2. Harness sees it, exits gracefully, sets presence to "offline"
// 3. Desktop's existing presence polling sees "offline" — UI updates automatically
// No backend Tauri command needed. Presence IS the status.
