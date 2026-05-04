use nostr::{nips::nip44, EventBuilder, JsonUtil, Keys, Kind, Tag, Timestamp, ToBech32};
use nostr_compat::{
    Event as CompatEvent, JsonUtil as CompatJsonUtil, Keys as CompatKeys,
    PublicKey as CompatPublicKey,
};
use tauri::Manager;
use tauri::State;

use crate::{
    app_state::AppState,
    models::IdentityInfo,
    relay::{self, relay_api_base_url_with_override, relay_ws_url_with_override},
};

#[tauri::command]
pub fn get_identity(state: State<'_, AppState>) -> Result<IdentityInfo, String> {
    let keys = state.keys.lock().map_err(|error| error.to_string())?;
    let pubkey = keys.public_key();
    let pubkey_hex = pubkey.to_hex();
    let bech32 = pubkey
        .to_bech32()
        .map_err(|error| format!("bech32 encode failed: {error}"))?;
    let display_name = if bech32.len() > 16 {
        format!("{}…{}", &bech32[..10], &bech32[bech32.len() - 4..])
    } else {
        bech32
    };

    Ok(IdentityInfo {
        pubkey: pubkey_hex,
        display_name,
    })
}

#[tauri::command]
pub fn get_default_relay_url() -> String {
    relay::relay_ws_url()
}

#[tauri::command]
pub fn get_relay_ws_url(state: State<'_, AppState>) -> String {
    relay_ws_url_with_override(&state)
}

#[tauri::command]
pub fn get_relay_http_url(state: State<'_, AppState>) -> String {
    relay_api_base_url_with_override(&state)
}

#[tauri::command]
pub fn get_media_proxy_port(state: State<'_, AppState>) -> u16 {
    state
        .media_proxy_port
        .load(std::sync::atomic::Ordering::Relaxed)
}

#[tauri::command]
pub fn sign_event(
    kind: u16,
    content: String,
    created_at: Option<u64>,
    tags: Vec<Vec<String>>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let keys = state.keys.lock().map_err(|error| error.to_string())?;

    let nostr_tags = tags
        .into_iter()
        .map(|tag| Tag::parse(tag).map_err(|error| format!("invalid tag: {error}")))
        .collect::<Result<Vec<_>, _>>()?;

    let mut builder = EventBuilder::new(Kind::Custom(kind), content).tags(nostr_tags);
    if let Some(created_at) = created_at {
        builder = builder.custom_created_at(Timestamp::from(created_at));
    }

    let event = builder
        .sign_with_keys(&keys)
        .map_err(|error| format!("sign failed: {error}"))?;

    Ok(event.as_json())
}

#[tauri::command]
pub fn decrypt_observer_event(
    event_json: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let nsec = {
        let keys = state.keys.lock().map_err(|error| error.to_string())?;
        keys.secret_key()
            .to_bech32()
            .map_err(|error| format!("encode nsec: {error}"))?
    };
    let keys = CompatKeys::parse(&nsec).map_err(|error| format!("parse nsec: {error}"))?;
    let event =
        CompatEvent::from_json(event_json).map_err(|error| format!("invalid event: {error}"))?;

    // Defense-in-depth: verify event ID and signature before decrypting.
    if !event.verify_id() {
        return Err("observer event has invalid ID".into());
    }
    if !event.verify_signature() {
        return Err("observer event has invalid signature".into());
    }

    sprout_core::observer::decrypt_observer_payload(&keys, &event)
        .map_err(|error| format!("decrypt observer event failed: {error}"))
}

#[tauri::command]
pub fn build_observer_control_event(
    agent_pubkey: String,
    payload: serde_json::Value,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let nsec = {
        let keys = state.keys.lock().map_err(|error| error.to_string())?;
        keys.secret_key()
            .to_bech32()
            .map_err(|error| format!("encode nsec: {error}"))?
    };
    let keys = CompatKeys::parse(&nsec).map_err(|error| format!("parse nsec: {error}"))?;
    let agent_pubkey = CompatPublicKey::from_hex(agent_pubkey.trim())
        .map_err(|error| format!("invalid agent pubkey: {error}"))?;
    let agent_pubkey_hex = agent_pubkey.to_hex();
    let encrypted = sprout_core::observer::encrypt_observer_payload(&keys, &agent_pubkey, &payload)
        .map_err(|error| format!("encrypt observer control failed: {error}"))?;
    let builder = sprout_sdk::build_agent_observer_frame(
        &agent_pubkey_hex,
        &agent_pubkey_hex,
        sprout_core::observer::OBSERVER_FRAME_CONTROL,
        &encrypted,
    )
    .map_err(|error| format!("build observer control failed: {error}"))?;
    let event = builder
        .sign_with_keys(&keys)
        .map_err(|error| format!("sign observer control failed: {error}"))?;
    Ok(event.as_json())
}

#[tauri::command]
pub fn get_nsec(state: State<'_, AppState>) -> Result<String, String> {
    let keys = state.keys.lock().map_err(|error| error.to_string())?;
    keys.secret_key()
        .to_bech32()
        .map_err(|error| format!("encode nsec: {error}"))
}

#[tauri::command]
pub fn import_identity(
    nsec: String,
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<IdentityInfo, String> {
    let trimmed = nsec.trim();
    let keys = Keys::parse(trimmed).map_err(|e| format!("Invalid private key: {e}"))?;

    // Persist to identity.key
    let data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("app data dir: {e}"))?;
    std::fs::create_dir_all(&data_dir).map_err(|e| format!("create app data dir: {e}"))?;
    let key_path = data_dir.join("identity.key");
    crate::app_state::save_key_file(&key_path, &keys)?;

    // Update in-memory keys
    let pubkey = keys.public_key();
    *state.keys.lock().map_err(|e| e.to_string())? = keys;

    // Clear any session token: it was minted for the previous pubkey and
    // would be invalid (or worse, identify us as the previous pubkey) for
    // any subsequent relay requests.
    if let Ok(mut token) = state.session_token.lock() {
        *token = None;
    }

    let pubkey_hex = pubkey.to_hex();
    let bech32 = pubkey
        .to_bech32()
        .map_err(|error| format!("bech32 encode failed: {error}"))?;
    let display_name = if bech32.len() > 16 {
        format!("{}…{}", &bech32[..10], &bech32[bech32.len() - 4..])
    } else {
        bech32
    };

    eprintln!("sprout-desktop: imported identity pubkey {}", pubkey_hex);

    Ok(IdentityInfo {
        pubkey: pubkey_hex,
        display_name,
    })
}

#[tauri::command]
pub fn create_auth_event(
    challenge: String,
    relay_url: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let keys = state.keys.lock().map_err(|error| error.to_string())?;

    let mut tags = vec![
        Tag::parse(vec!["relay", &relay_url])
            .map_err(|error| format!("relay tag failed: {error}"))?,
        Tag::parse(vec!["challenge", &challenge])
            .map_err(|error| format!("challenge tag failed: {error}"))?,
    ];

    // Use configured API token first, then fall back to session token
    // (set by workspace apply).
    let auth_token = state
        .configured_api_token
        .as_deref()
        .map(String::from)
        .or_else(|| {
            state
                .session_token
                .lock()
                .ok()
                .and_then(|guard| guard.clone())
        });

    if let Some(token) = auth_token {
        tags.push(
            Tag::parse(vec!["auth_token", &token])
                .map_err(|error| format!("auth token tag failed: {error}"))?,
        );
    }

    let event = EventBuilder::new(Kind::Custom(22242), "")
        .tags(tags)
        .sign_with_keys(&keys)
        .map_err(|error| format!("sign failed: {error}"))?;

    Ok(event.as_json())
}

#[tauri::command]
pub fn nip44_encrypt_to_self(
    plaintext: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let keys = state.keys.lock().map_err(|e| e.to_string())?;
    nip44::encrypt(
        keys.secret_key(),
        &keys.public_key(),
        &plaintext,
        nip44::Version::V2,
    )
    .map_err(|e| format!("nip44 encrypt failed: {e}"))
}

#[tauri::command]
pub fn nip44_decrypt_from_self(
    ciphertext: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let keys = state.keys.lock().map_err(|e| e.to_string())?;
    nip44::decrypt(keys.secret_key(), &keys.public_key(), &ciphertext)
        .map_err(|e| format!("nip44 decrypt failed: {e}"))
}
