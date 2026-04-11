use nostr::{Keys, ToBech32};
use tauri::{AppHandle, State};

use crate::{
    app_state::AppState,
    managed_agents::{
        build_managed_agent_summary, default_token_scopes, discover_provider_candidates,
        ensure_persona_is_active, find_managed_agent_mut, invoke_provider, load_managed_agents,
        load_personas, managed_agent_avatar_url, managed_agent_log_path, managed_agents_base_dir,
        mint_token_via_api, normalize_agent_args, provider_deploy, read_log_tail,
        resolve_provider_binary, save_managed_agents, start_managed_agent_process,
        stop_managed_agent_process, sync_managed_agent_processes, validate_provider_config,
        BackendKind, BackendProviderInfo, CreateManagedAgentRequest, CreateManagedAgentResponse,
        ManagedAgentLogResponse, ManagedAgentRecord, ManagedAgentSummary,
        MintManagedAgentTokenRequest, MintManagedAgentTokenResponse, DEFAULT_ACP_COMMAND,
        DEFAULT_AGENT_COMMAND, DEFAULT_AGENT_PARALLELISM, DEFAULT_AGENT_TURN_TIMEOUT_SECONDS,
        DEFAULT_MCP_COMMAND,
    },
    relay::{relay_ws_url, sync_managed_agent_profile},
    util::now_iso,
};

/// Build the standard agent JSON payload for provider deploy calls.
fn build_deploy_payload(record: &ManagedAgentRecord) -> serde_json::Value {
    serde_json::json!({
        "name": &record.name,
        "relay_url": &record.relay_url,
        "private_key_nsec": &record.private_key_nsec,
        "api_token": &record.api_token,
        "agent_command": &record.agent_command,
        "agent_args": &record.agent_args,
        "system_prompt": &record.system_prompt,
        "model": &record.model,
        "turn_timeout_seconds": record.turn_timeout_seconds,
        "idle_timeout_seconds": record.idle_timeout_seconds,
        "max_turn_duration_seconds": record.max_turn_duration_seconds,
        "parallelism": record.parallelism,
    })
}

/// Deploy an agent to a provider backend. Resolves the binary, calls deploy via
/// spawn_blocking, and persists the result (backend_agent_id or last_error).
///
/// Idempotency: calling deploy on an already-deployed agent sends the same payload
/// again. Providers are expected to handle this as an update-in-place or no-op —
/// the protocol does not include an explicit `undeploy` operation (deferred to v2).
///
/// Returns Ok(()) on success, Err(message) on failure. Either way the record is
/// updated and saved before returning.
async fn deploy_to_provider(
    app: &AppHandle,
    state: &AppState,
    pubkey: &str,
    provider_id: &str,
    config: &serde_json::Value,
    agent_json: serde_json::Value,
    cached_binary_path: Option<&str>,
) -> Result<(), String> {
    // Resolve via discovered candidates only. Cached path must match BOTH
    // "is a discovered candidate" AND "belongs to this provider_id". A tampered
    // record cannot redirect deploys to a different provider's binary.
    let bin_path = cached_binary_path
        .map(std::path::PathBuf::from)
        .filter(|p| p.exists())
        .map(|p| p.canonicalize().unwrap_or(p))
        .filter(|canonical| {
            discover_provider_candidates().iter().any(|(id, cp)| {
                id == provider_id && cp.canonicalize().ok().as_ref() == Some(canonical)
            })
        })
        .map_or_else(|| resolve_provider_binary(provider_id), Ok)?;

    let config_clone = config.clone();
    let deploy_result =
        tokio::task::spawn_blocking(move || provider_deploy(&bin_path, &agent_json, &config_clone))
            .await
            .map_err(|e| format!("spawn_blocking failed: {e}"))?;

    // Persist result under lock.
    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    let mut records = load_managed_agents(app)?;
    let rec = records
        .iter_mut()
        .find(|r| r.pubkey == pubkey)
        .ok_or_else(|| format!("agent {pubkey} not found"))?;

    match deploy_result {
        Ok(backend_agent_id) => {
            rec.backend_agent_id = Some(backend_agent_id);
            rec.last_started_at = Some(now_iso());
            rec.updated_at = now_iso();
            rec.last_error = None;
        }
        Err(ref e) => {
            rec.last_error = Some(e.clone());
            rec.updated_at = now_iso();
            save_managed_agents(app, &records)?;
            return Err(e.clone());
        }
    }
    save_managed_agents(app, &records)?;
    Ok(())
}

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
    let (
        agent_keys,
        private_key_nsec,
        pubkey,
        resolved_relay_url,
        token_scopes,
        token_name,
        mint_token,
        input,
    ) = {
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
            ensure_persona_is_active(&personas, persona_id)?;
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
        (
            keys,
            private_key_nsec,
            pubkey,
            resolved_relay_url,
            token_scopes,
            token_name,
            mint_token,
            input,
        )
    };

    // ── Pre-Phase 2: validate provider config BEFORE any side effects ────────
    if let BackendKind::Provider { ref config, ref id } = input.backend {
        // Provider agents MUST mint a token so the relay establishes ownership.
        // Without ownership the harness ignores !shutdown — the agent becomes
        // uncontrollable. Reject early rather than deploy an unstoppable agent.
        if !mint_token {
            return Err(
                "provider-backed agents require a minted token (ownership is established during mint)"
                    .to_string(),
            );
        }
        // Enforce minimum scopes for remote agents. The harness needs users:read
        // to query its owner (for !shutdown). Without it, the agent is unstoppable.
        const REQUIRED_PROVIDER_SCOPES: &[&str] = &[
            "messages:read",
            "messages:write",
            "channels:read",
            "users:read",
        ];
        for required in REQUIRED_PROVIDER_SCOPES {
            if !token_scopes.iter().any(|s| s == required) {
                return Err(format!(
                    "provider-backed agents require the '{required}' scope"
                ));
            }
        }
        validate_provider_config(config)?;
        // Validate via discovered candidates — not raw resolve_command.
        resolve_provider_binary(id)?;
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
        // Provider config was already validated in Pre-Phase 2; cache the discovered binary path for deploy_to_provider.
        let provider_binary_path = if let BackendKind::Provider { ref id, .. } = input.backend {
            // Use resolve_provider_binary (discovered candidates only).
            resolve_provider_binary(id)
                .ok()
                .map(|p| p.display().to_string())
        } else {
            None
        };

        let agent_command = input
            .agent_command
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_AGENT_COMMAND)
            .to_string();
        let agent_args = normalize_agent_args(
            &agent_command,
            input
                .agent_args
                .iter()
                .map(|arg| arg.trim().to_string())
                .filter(|arg| !arg.is_empty())
                .collect::<Vec<_>>(),
        );

        // For pack-backed personas, resolve the installed pack path and the
        // persona's internal name (slug). ACP's resolve_persona_by_name()
        // matches on this internal name, NOT display_name.
        let pack_metadata: Option<(std::path::PathBuf, String)> =
            requested_persona_id.as_deref().and_then(|pid| {
                let personas = load_personas(&app).ok()?;
                let persona = personas.iter().find(|p| p.id == pid)?;
                let pack_id = persona.source_pack.as_deref()?;
                let slug = persona.source_pack_persona_slug.as_deref()?;
                let base = managed_agents_base_dir(&app).ok()?;
                let pack_path = base.join("packs").join(pack_id);
                // Use the validated slug stored during import — no need to
                // re-resolve the pack. The slug is [a-zA-Z0-9_-]+ by construction.
                Some((pack_path, slug.to_owned()))
            });

        let record = crate::managed_agents::ManagedAgentRecord {
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
            agent_command,
            agent_args,
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
            // 0 or None → harness uses its own default (320s idle, 3600s max), and the CLI also clamps 0 → minimum.
            idle_timeout_seconds: input.idle_timeout_seconds.filter(|s| *s > 0),
            max_turn_duration_seconds: input.max_turn_duration_seconds.filter(|s| *s > 0),
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
            // Provider agents don't auto-start with the desktop — they're
            // managed externally. Force false to avoid persisting a flag the
            // app will never honor.
            start_on_app_launch: if input.backend != BackendKind::Local {
                false
            } else {
                input.start_on_app_launch
            },
            runtime_pid: None,
            backend: input.backend.clone(),
            backend_agent_id: None,
            provider_binary_path,
            // If the persona came from a pack, record the installed pack path
            // and the persona's internal name within the pack so the runtime
            // can resolve pack-specific config at startup via ACP.
            //
            // Important: persona_name_in_pack must be the internal `name` slug
            // (e.g., "lep"), NOT the display_name (e.g., "Lep"). ACP's
            // resolve_persona_by_name() matches on the internal name.
            persona_pack_path: pack_metadata.as_ref().map(|(path, _)| path.clone()),
            persona_name_in_pack: pack_metadata.as_ref().map(|(_, name)| name.clone()),
            created_at: now_iso(),
            updated_at: now_iso(),
            last_started_at: None,
            last_stopped_at: None,
            last_exit_code: None,
            last_error: None,
        };

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
    let profile_sync_error = (sync_managed_agent_profile(
        &state,
        &resolved_relay_url,
        &agent_keys,
        api_token.as_deref(),
        &token_scopes,
        &name,
        avatar_url.as_deref(),
    )
    .await)
        .err();

    // ── Phase 5: provider deploy (async, outside lock) ───────────────────────
    let spawn_error = if input.spawn_after_create && input.backend != BackendKind::Local {
        if let BackendKind::Provider { ref id, ref config } = input.backend {
            // Read the saved record to build the deploy payload (record has the
            // canonical field values after Phase 3 normalization).
            let agent_json = {
                let _g = state
                    .managed_agents_store_lock
                    .lock()
                    .map_err(|e| e.to_string())?;
                let records = load_managed_agents(&app)?;
                let rec = records
                    .iter()
                    .find(|r| r.pubkey == pubkey)
                    .ok_or_else(|| "agent disappeared".to_string())?;
                build_deploy_payload(rec)
            };
            match deploy_to_provider(&app, &state, &pubkey, id, config, agent_json, None).await {
                Ok(()) => spawn_error,
                Err(e) => Some(e),
            }
        } else {
            spawn_error
        }
    } else {
        spawn_error
    };

    // Rebuild summary if provider deploy may have updated backend_agent_id.
    let final_agent = if input.backend != BackendKind::Local && spawn_error.is_none() {
        let _store_guard = state
            .managed_agents_store_lock
            .lock()
            .map_err(|e| e.to_string())?;
        let records = load_managed_agents(&app)?;
        let runtimes = state
            .managed_agent_processes
            .lock()
            .map_err(|e| e.to_string())?;
        let record = records
            .iter()
            .find(|r| r.pubkey == pubkey)
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
    // Collect backend info and handle local vs provider under lock.
    let (backend, cached_binary_path, agent_json) = {
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

        let payload = build_deploy_payload(record);
        (
            record.backend.clone(),
            record.provider_binary_path.clone(),
            payload,
        )
    };

    // Provider backend: deploy via shared helper (async, outside lock).
    if let BackendKind::Provider { ref id, ref config } = backend {
        deploy_to_provider(
            &app,
            &state,
            &pubkey,
            id,
            config,
            agent_json,
            cached_binary_path.as_deref(),
        )
        .await?;

        // Return updated summary.
        let _store_guard = state
            .managed_agents_store_lock
            .lock()
            .map_err(|e| e.to_string())?;
        let records = load_managed_agents(&app)?;
        let runtimes = state
            .managed_agent_processes
            .lock()
            .map_err(|e| e.to_string())?;
        let record = records
            .iter()
            .find(|r| r.pubkey == pubkey)
            .ok_or_else(|| format!("agent {pubkey} not found"))?;
        return build_managed_agent_summary(&app, record, &runtimes);
    }

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
        stop_managed_agent_process(&app, record, &mut runtimes)?;
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
    force_remote_delete: Option<bool>,
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

    // Guard: reject deletion of deployed remote agents unless explicitly forced.
    // This turns "don't orphan remote infra" from a UI convention into a backend
    // invariant — a buggy or compromised IPC caller cannot silently orphan a live
    // remote deployment. The frontend sends force_remote_delete: true only after
    // the user confirms the orphan warning.
    if let Some(record) = records.iter().find(|r| r.pubkey == pubkey) {
        if record.backend != BackendKind::Local
            && record.backend_agent_id.is_some()
            && !force_remote_delete.unwrap_or(false)
        {
            return Err(
                "cannot delete a deployed remote agent without force_remote_delete: true"
                    .to_string(),
            );
        }
    }

    if let Some(record) = records.iter_mut().find(|record| record.pubkey == pubkey) {
        // For local agents: kills the process. For remote agents: no-op (the frontend
        // sends !shutdown via WebSocket before calling delete). Either way, safe.
        stop_managed_agent_process(&app, record, &mut runtimes)?;
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
    let minted_token =
        mint_token_via_api(&state, &agent_keys, &relay_url, &token_name, &scopes, None).await?;

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

    Ok(MintManagedAgentTokenResponse {
        agent,
        token: api_token,
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
    let record = records
        .iter()
        .find(|record| record.pubkey == pubkey)
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
    // request_id is for provider-side logging — not validated in the response
    // (stdin→stdout is 1:1 per process invocation).
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
