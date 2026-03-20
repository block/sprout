mod app_state;
mod commands;
mod managed_agents;
mod models;
mod relay;
mod util;

use app_state::{build_app_state, resolve_persisted_identity, AppState};
use commands::*;
use managed_agents::{
    find_managed_agent_mut, load_managed_agents, save_managed_agents, start_managed_agent_process,
    stop_managed_agent_process, sync_managed_agent_processes,
};
use tauri::{Manager, RunEvent};
use tauri_plugin_window_state::StateFlags;

fn restore_managed_agents_on_launch(app: &tauri::AppHandle) -> Result<(), String> {
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
    let pubkeys_to_restore = records
        .iter()
        .filter(|record| record.start_on_app_launch)
        .map(|record| record.pubkey.clone())
        .collect::<Vec<_>>();

    for pubkey in pubkeys_to_restore {
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

    for record in records.iter_mut() {
        if record.runtime_pid.is_none() && !runtimes.contains_key(&record.pubkey) {
            continue;
        }

        stop_managed_agent_process(record, &mut runtimes)?;
        changed = true;
    }

    if changed {
        save_managed_agents(app, &records)?;
    }

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::all() & !StateFlags::VISIBLE)
                .build(),
        )
        .plugin(tauri_plugin_websocket::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(build_app_state())
        .setup(|app| {
            let app_handle = app.handle().clone();

            // Resolve persisted identity key (env var → file → generate+save).
            // This is fatal — the app should not start with an ephemeral identity
            // that will be lost on restart, as that silently breaks channel
            // memberships, DMs, and relay identity.
            let state = app_handle.state::<AppState>();
            resolve_persisted_identity(&app_handle, &state)
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

            if let Err(error) = restore_managed_agents_on_launch(&app_handle) {
                eprintln!("sprout-desktop: failed to restore managed agents: {error}");
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_identity,
            get_profile,
            update_profile,
            get_user_profile,
            get_users_batch,
            search_users,
            get_presence,
            set_presence,
            get_relay_ws_url,
            discover_acp_providers,
            discover_managed_agent_prereqs,
            sign_event,
            create_auth_event,
            get_channels,
            create_channel,
            open_dm,
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
            get_feed,
            search_messages,
            send_channel_message,
            get_forum_posts,
            get_forum_thread,
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
            list_personas,
            create_persona,
            update_persona,
            delete_persona,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if matches!(event, RunEvent::ExitRequested { .. }) {
            if let Err(error) = shutdown_managed_agents(app_handle) {
                eprintln!("sprout-desktop: failed to stop managed agents: {error}");
            }
        }
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
