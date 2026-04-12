mod app_state;
mod commands;
mod events;
mod huddle;
mod managed_agents;
mod migration;
mod models;
mod relay;
mod util;

use app_state::{build_app_state, resolve_persisted_identity, AppState};
use commands::*;
use huddle::{
    add_agent_to_huddle, check_pipeline_hotstart, confirm_huddle_active, download_voice_models,
    end_huddle, get_huddle_agent_pubkeys, get_huddle_state, get_model_status, join_huddle,
    leave_huddle, push_audio_pcm, set_tts_enabled, speak_agent_message, start_huddle,
    start_stt_pipeline,
};
use managed_agents::{
    ensure_nest, find_managed_agent_mut, kill_stale_tracked_processes, load_managed_agents,
    save_managed_agents, start_managed_agent_process, sync_managed_agent_processes, BackendKind,
    ManagedAgentProcess,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::{http, Manager, RunEvent};
use tauri_plugin_window_state::StateFlags;

fn restore_managed_agents_on_launch(
    app: &tauri::AppHandle,
    shutdown_started: &AtomicBool,
) -> Result<(), String> {
    if shutdown_started.load(Ordering::SeqCst) {
        return Ok(());
    }

    let state = app.state::<AppState>();
    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|error| error.to_string())?;

    if shutdown_started.load(Ordering::SeqCst) {
        return Ok(());
    }

    let mut records = load_managed_agents(app)?;
    let mut runtimes = state
        .managed_agent_processes
        .lock()
        .map_err(|error| error.to_string())?;
    let mut changed = sync_managed_agent_processes(&mut records, &mut runtimes);
    changed |= kill_stale_tracked_processes(&mut records, &runtimes);

    // PID-file sweep: kill any orphaned agent processes we have receipts for
    // that weren’t tracked in records (e.g. escaped process groups, double-forked).
    let tracked_pids: Vec<u32> = records
        .iter()
        .filter_map(|r| r.runtime_pid)
        .chain(runtimes.values().map(|rt| rt.child.id()))
        .collect();
    managed_agents::sweep_orphaned_agent_processes(app, &tracked_pids);

    let pubkeys_to_restore = records
        .iter()
        .filter(|record| record.start_on_app_launch && record.backend == BackendKind::Local)
        .map(|record| record.pubkey.clone())
        .collect::<Vec<_>>();

    for pubkey in pubkeys_to_restore {
        if shutdown_started.load(Ordering::SeqCst) {
            break;
        }

        let record = find_managed_agent_mut(&mut records, &pubkey)?;
        match start_managed_agent_process(app, record, &mut runtimes) {
            Ok(()) => {
                changed = true;
            }
            Err(error) => {
                record.updated_at = util::now_iso();
                record.last_error = Some(error);
                changed = true;
            }
        }
    }

    if changed {
        save_managed_agents(app, &records)?;
    }

    Ok(())
}

fn shutdown_managed_agents(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|error| error.to_string())?;
    let mut records = load_managed_agents(app)?;
    let mut runtimes = state
        .managed_agent_processes
        .lock()
        .map_err(|error| error.to_string())?;
    let mut changed = sync_managed_agent_processes(&mut records, &mut runtimes);
    changed |= kill_stale_tracked_processes(&mut records, &runtimes);

    // Stop all tracked agents. Send SIGTERM to all process
    // groups first, then wait for exits in parallel to avoid serial 1s waits.
    struct AgentToStop {
        idx: usize,
        pid: u32,
        runtime: Option<ManagedAgentProcess>,
    }

    let mut to_stop: Vec<AgentToStop> = Vec::new();
    for (idx, record) in records.iter_mut().enumerate() {
        if record.backend != BackendKind::Local {
            continue;
        }
        if record.runtime_pid.is_none() && !runtimes.contains_key(&record.pubkey) {
            continue;
        }
        let runtime = runtimes.remove(&record.pubkey);
        let Some(pid) = runtime
            .as_ref()
            .map(|rt| rt.child.id())
            .or(record.runtime_pid)
        else {
            continue;
        };
        to_stop.push(AgentToStop { idx, pid, runtime });
    }

    if !to_stop.is_empty() {
        changed = true;

        // Fan-out: send SIGTERM to all process groups at once.
        #[cfg(unix)]
        for agent in &to_stop {
            let pgid = -(agent.pid as i32);
            unsafe {
                libc::kill(pgid, libc::SIGTERM);
            }
        }

        // Wait up to 2s for all to exit, checking in a polling loop.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if to_stop
                .iter()
                .all(|a| !managed_agents::process_is_running(a.pid))
            {
                break;
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        // Fan-out: SIGKILL any survivors.
        #[cfg(unix)]
        for agent in &to_stop {
            if managed_agents::process_is_running(agent.pid) {
                let pgid = -(agent.pid as i32);
                unsafe {
                    libc::kill(pgid, libc::SIGKILL);
                }
            }
        }

        // Reap children and update records.
        for mut agent in to_stop {
            if let Some(ref mut rt) = agent.runtime {
                // Best-effort reap — don’t block shutdown if the child is stuck
                // in uninterruptible sleep. The zombie will be cleaned up when
                // our process exits and launchd reaps it.
                let _ = rt.child.try_wait();
                // Write log marker (best-effort).
                let record = &records[agent.idx];
                let _ = managed_agents::append_log_marker(
                    &rt.log_path,
                    &format!(
                        "=== stopped {} ({}) at {} ===",
                        record.name,
                        record.pubkey,
                        util::now_iso()
                    ),
                );
            }
            let record = &mut records[agent.idx];
            record.runtime_pid = None;
            record.last_stopped_at = Some(util::now_iso());
            record.updated_at = util::now_iso();
            record.last_exit_code = None;
            record.last_error = None;
        }
    }

    // Final sweep: kill any orphaned agent processes we have PID file receipts
    // for that escaped process-group kills or weren't tracked in records.
    // All tracked PIDs have already been killed above, so pass an empty skip list.
    managed_agents::sweep_orphaned_agent_processes(app, &[]);

    if changed {
        save_managed_agents(app, &records)?;
    }

    Ok(())
}

/// Defense-in-depth cap: refuse to buffer responses larger than this into RAM.
/// Range requests (≤16 MiB from server) always fit. Full GETs for huge videos
/// get a clear 413 instead of OOM — the <video> element always uses range
/// requests for seeking, so this only catches edge cases.
const MAX_PROXY_RESPONSE: u64 = 20 * 1024 * 1024;

/// Proxy media requests through the Rust backend so they traverse the WARP tunnel.
///
/// WKWebView's networking stack bypasses WARP, causing 403s from Cloudflare Access.
/// This handler routes `sprout-media://localhost/{path}` through reqwest, which
/// runs in the Tauri process and goes through WARP.
async fn handle_sprout_media(
    app: &tauri::AppHandle,
    request: &http::Request<Vec<u8>>,
) -> http::Response<Vec<u8>> {
    let state = app.state::<AppState>();
    let base = relay::relay_api_base_url();

    // Preserve path + query (thumbnails may have query params).
    // Only proxy /media/ paths — reject anything else.
    let path_and_query = request
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");

    if !path_and_query.starts_with("/media/") {
        return error_response(404, "not found");
    }

    let has_range = request.headers().contains_key("range");
    let upstream_url = format!("{base}{path_and_query}");

    // Forward Range header if present — enables video seeking through the proxy.
    let mut upstream = state
        .http_client
        .get(&upstream_url)
        .timeout(std::time::Duration::from_secs(60));
    if let Some(range) = request.headers().get("range") {
        if let Ok(v) = range.to_str() {
            upstream = upstream.header("range", v);
        }
    }

    let result = upstream.send().await;

    match result {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let content_type = resp
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/octet-stream")
                .to_string();

            // Propagate range-related headers so <video> seeking works.
            let content_range = resp
                .headers()
                .get("content-range")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            let accept_ranges = resp
                .headers()
                .get("accept-ranges")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            let content_length = resp
                .headers()
                .get("content-length")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            // OOM guard: if this is a non-range GET and the upstream body is
            // larger than our cap, bail with 413 instead of buffering into RAM.
            // Tauri's protocol handler requires Vec<u8> so we can't truly stream.
            if !has_range {
                if let Some(ref cl) = content_length {
                    if let Ok(len) = cl.parse::<u64>() {
                        if len > MAX_PROXY_RESPONSE {
                            return error_response(
                                413,
                                "response too large — use range requests for video playback",
                            );
                        }
                    }
                }
            }

            match resp.bytes().await {
                Ok(bytes) => {
                    let mut builder = http::Response::builder()
                        .status(status)
                        .header("content-type", &content_type);
                    if let Some(ref cr) = content_range {
                        builder = builder.header("content-range", cr);
                    }
                    if let Some(ref ar) = accept_ranges {
                        builder = builder.header("accept-ranges", ar);
                    }
                    if let Some(ref cl) = content_length {
                        builder = builder.header("content-length", cl);
                    }
                    builder
                        .body(bytes.to_vec())
                        .unwrap_or_else(|_| error_response(500, "response build failed"))
                }
                Err(_) => error_response(502, "failed to read upstream body"),
            }
        }
        Err(_) => error_response(502, "upstream request failed"),
    }
}

fn error_response(status: u16, msg: &str) -> http::Response<Vec<u8>> {
    http::Response::builder()
        .status(status)
        .header("content-type", "text/plain")
        .body(msg.as_bytes().to_vec())
        .unwrap_or_else(|_| {
            http::Response::builder()
                .status(500)
                .body(Vec::new())
                .unwrap()
        })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::all() & !StateFlags::VISIBLE)
                .build(),
        )
        .plugin(tauri_plugin_websocket::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init());

    // Only register the updater in release builds that were compiled with a
    // real updater configuration. Local unsigned builds omit that config and
    // should still launch for debugging.
    #[cfg(sprout_updater_enabled)]
    let builder = if cfg!(debug_assertions) {
        builder
    } else {
        builder.plugin(tauri_plugin_updater::Builder::new().build())
    };

    #[cfg(not(sprout_updater_enabled))]
    let builder = builder;

    let shutdown_started = Arc::new(AtomicBool::new(false));
    let restore_shutdown_started = Arc::clone(&shutdown_started);
    let app = builder
        .register_asynchronous_uri_scheme_protocol("sprout-media", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            tauri::async_runtime::spawn(async move {
                let response = handle_sprout_media(&app, &request).await;
                responder.respond(response);
            });
        })
        .manage(build_app_state())
        .setup(move |app| {
            let app_handle = app.handle().clone();
            let shutdown_started = Arc::clone(&restore_shutdown_started);

            // Migrate data from the legacy `com.wesb.sprout` directory before
            // resolving identity, so the persisted key is available at the new
            // path on first launch after the identifier change.
            migration::migrate_legacy_data_dir(&app_handle);

            // Resolve persisted identity key (env var → file → generate+save).
            // This is fatal — the app should not start with an ephemeral identity
            // that will be lost on restart, as that silently breaks channel
            // memberships, DMs, and relay identity.
            let state = app_handle.state::<AppState>();
            resolve_persisted_identity(&app_handle, &state)
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

            // Create the Sprout nest (~/.sprout) before agents are restored,
            // so default_agent_workdir() resolves to the nest directory.
            // Non-fatal: agents fall back to $HOME if nest creation fails.
            if let Err(error) = ensure_nest() {
                eprintln!("sprout-desktop: failed to create nest: {error}");
            }

            // Keep launch-time agent restoration off the synchronous setup path
            // so the frontend can mount and reveal the window promptly.
            tauri::async_runtime::spawn_blocking(move || {
                if let Err(error) =
                    restore_managed_agents_on_launch(&app_handle, shutdown_started.as_ref())
                {
                    eprintln!("sprout-desktop: failed to restore managed agents: {error}");
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_identity,
            get_profile,
            update_profile,
            get_user_profile,
            get_users_batch,
            get_user_notes,
            search_users,
            get_presence,
            set_presence,
            get_relay_ws_url,
            get_relay_http_url,
            discover_acp_providers,
            discover_managed_agent_prereqs,
            sign_event,
            create_auth_event,
            get_channels,
            create_channel,
            open_dm,
            hide_dm,
            get_channel_details,
            get_channel_members,
            update_channel,
            set_channel_topic,
            set_channel_purpose,
            archive_channel,
            unarchive_channel,
            delete_channel,
            add_channel_members,
            remove_channel_member,
            join_channel,
            leave_channel,
            get_canvas,
            set_canvas,
            get_feed,
            search_messages,
            send_channel_message,
            get_forum_posts,
            get_forum_thread,
            edit_message,
            delete_message,
            add_reaction,
            remove_reaction,
            get_event,
            upload_media,
            pick_and_upload_media,
            upload_media_bytes,
            list_tokens,
            mint_token,
            revoke_token,
            revoke_all_tokens,
            list_relay_agents,
            list_managed_agents,
            create_managed_agent,
            start_managed_agent,
            stop_managed_agent,
            set_managed_agent_start_on_app_launch,
            delete_managed_agent,
            mint_managed_agent_token,
            get_managed_agent_log,
            get_agent_models,
            update_managed_agent,
            discover_backend_providers,
            probe_backend_provider,
            list_personas,
            create_persona,
            update_persona,
            delete_persona,
            set_persona_active,
            list_teams,
            create_team,
            update_team,
            delete_team,
            export_team_to_json,
            parse_team_file,
            parse_persona_files,
            export_persona_to_json,
            get_channel_workflows,
            get_workflow,
            create_workflow,
            update_workflow,
            delete_workflow,
            get_workflow_runs,
            get_run_approvals,
            trigger_workflow,
            grant_approval,
            deny_approval,
            publish_note,
            get_contact_list,
            set_contact_list,
            get_notes_timeline,
            start_huddle,
            join_huddle,
            leave_huddle,
            end_huddle,
            get_huddle_state,
            push_audio_pcm,
            start_stt_pipeline,
            download_voice_models,
            get_model_status,
            set_tts_enabled,
            speak_agent_message,
            add_agent_to_huddle,
            check_pipeline_hotstart,
            confirm_huddle_active,
            get_huddle_agent_pubkeys,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    let shutdown_done = AtomicBool::new(false);
    app.run(move |app_handle, event| match event {
        RunEvent::ExitRequested { .. } | RunEvent::Exit => {
            shutdown_started.store(true, Ordering::SeqCst);
            if !shutdown_done.swap(true, Ordering::SeqCst) {
                if let Err(error) = shutdown_managed_agents(app_handle) {
                    eprintln!("sprout-desktop: failed to stop managed agents: {error}");
                }
            }
        }
        _ => {}
    });
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::{models::ChannelInfo, util::percent_encode};

    #[test]
    fn channel_info_defaults_is_member_for_legacy_payloads() {
        let channel: ChannelInfo = serde_json::from_value(json!({
            "id": "9a1657ac-f7aa-5db0-b632-d8bbeb6dfb50",
            "name": "general",
            "channel_type": "stream",
            "visibility": "open",
            "description": "General discussion",
            "topic": null,
            "purpose": null,
            "member_count": 3,
            "last_message_at": null,
            "archived_at": null,
            "participants": [],
            "participant_pubkeys": []
        }))
        .expect("legacy payload should deserialize");

        assert!(channel.is_member);
    }

    #[test]
    fn percent_encode_leaves_unreserved_chars() {
        assert_eq!(
            percent_encode("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.~"),
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.~"
        );
    }

    #[test]
    fn percent_encode_escapes_unicode_and_reserved_chars() {
        assert_eq!(percent_encode("👍"), "%F0%9F%91%8D");
        assert_eq!(percent_encode("a/b?c"), "a%2Fb%3Fc");
    }
}
