use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use nostr::{EventBuilder, JsonUtil, Kind, PublicKey, Tag, ToBech32};
use tauri::State;

use crate::{
    app_state::AppState,
    models::IdentityInfo,
    relay::{relay_api_base_url, relay_ws_url},
};

fn truncated_npub(pubkey: &PublicKey) -> String {
    let bech32 = pubkey.to_bech32().unwrap_or_else(|_| pubkey.to_hex());
    if bech32.len() > 16 {
        format!("{}…{}", &bech32[..10], &bech32[bech32.len() - 4..])
    } else {
        bech32
    }
}

#[tauri::command]
pub fn get_identity(state: State<'_, AppState>) -> Result<IdentityInfo, String> {
    let keys = state.keys.lock().map_err(|error| error.to_string())?;
    let pubkey = keys.public_key();
    let pubkey_hex = pubkey.to_hex();

    // Prefer the display name set during identity bootstrap (e.g. JWT username).
    let display_name = state
        .display_name
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .unwrap_or_else(|| truncated_npub(&pubkey));

    Ok(IdentityInfo {
        pubkey: pubkey_hex,
        display_name,
    })
}

#[tauri::command]
pub fn get_relay_ws_url() -> String {
    relay_ws_url()
}

#[tauri::command]
pub fn get_relay_http_url() -> String {
    relay_api_base_url()
}

#[tauri::command]
pub fn sign_event(
    kind: u16,
    content: String,
    tags: Vec<Vec<String>>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let keys = state.keys.lock().map_err(|error| error.to_string())?;

    let nostr_tags = tags
        .into_iter()
        .map(|tag| Tag::parse(tag).map_err(|error| format!("invalid tag: {error}")))
        .collect::<Result<Vec<_>, _>>()?;

    let event = EventBuilder::new(Kind::Custom(kind), content)
        .tags(nostr_tags)
        .sign_with_keys(&keys)
        .map_err(|error| format!("sign failed: {error}"))?;

    Ok(event.as_json())
}

#[derive(serde::Serialize)]
pub struct InitializedIdentity {
    pubkey: String,
    display_name: String,
    identity_mode: Option<String>,
    ws_auth_mode: String,
}

#[tauri::command]
pub async fn initialize_identity(
    state: State<'_, AppState>,
) -> Result<InitializedIdentity, String> {
    let identity_mode = discover_identity_mode(&state).await?;

    match identity_mode.as_str() {
        "proxy" | "hybrid" => {
            // Client-generated key — the key is already persisted locally by
            // resolve_persisted_identity(). We just need to register it with
            // the relay so the relay binds uid → pubkey.
            let base_url = crate::relay::relay_api_base_url();
            let register_url = format!("{base_url}/api/identity/register");

            // Sign a NIP-98 event proving ownership of our pubkey.
            let nip98_b64 = {
                let keys = state.keys.lock().map_err(|e| e.to_string())?;
                let tags = vec![
                    Tag::parse(vec!["u", &register_url]).map_err(|e| format!("u tag: {e}"))?,
                    Tag::parse(vec!["method", "POST"]).map_err(|e| format!("method tag: {e}"))?,
                ];
                let event = EventBuilder::new(Kind::HttpAuth, "")
                    .tags(tags)
                    .sign_with_keys(&keys)
                    .map_err(|e| format!("NIP-98 sign failed: {e}"))?;
                BASE64.encode(event.as_json().as_bytes())
            };

            let response = state
                .http_client
                .post(&register_url)
                .header("Authorization", format!("Nostr {nip98_b64}"))
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await
                .map_err(|e| format!("identity register request failed: {e}"))?;

            if !response.status().is_success() {
                let msg = crate::relay::relay_error_message(response).await;
                return Err(format!("identity registration failed: {msg}"));
            }

            #[derive(serde::Deserialize)]
            struct RegisterResponse {
                #[allow(dead_code)]
                pubkey: String,
                username: String,
                #[allow(dead_code)]
                binding_status: String,
            }

            let body: RegisterResponse = response
                .json()
                .await
                .map_err(|e| format!("failed to parse register response: {e}"))?;

            let pubkey_hex = state
                .keys
                .lock()
                .map_err(|e| e.to_string())?
                .public_key()
                .to_hex();

            // Persist the display name so get_identity returns it
            // instead of a truncated npub.
            *state.display_name.lock().map_err(|e| e.to_string())? = Some(body.username.clone());

            Ok(InitializedIdentity {
                pubkey: pubkey_hex,
                display_name: body.username,
                identity_mode: Some(identity_mode),
                ws_auth_mode: "nip42".to_string(),
            })
        }
        _ => {
            // Normal mode: keys are already loaded (from env var or persisted file).
            let keys = state.keys.lock().map_err(|e| e.to_string())?;
            let pubkey = keys.public_key();
            let pubkey_hex = pubkey.to_hex();
            let display_name = truncated_npub(&pubkey);

            Ok(InitializedIdentity {
                pubkey: pubkey_hex,
                display_name,
                identity_mode: None,
                ws_auth_mode: "nip42".to_string(),
            })
        }
    }
}

/// Discover the relay's identity mode from the NIP-11 info document.
/// Falls back to the local `SPROUT_IDENTITY_MODE` env var if the relay
/// is unreachable (e.g. offline dev).
async fn discover_identity_mode(state: &State<'_, AppState>) -> Result<String, String> {
    let base_url = crate::relay::relay_api_base_url();
    let url = format!("{base_url}/info");

    #[derive(serde::Deserialize)]
    struct RelayInfoPartial {
        #[serde(default)]
        identity_mode: Option<String>,
    }

    match state
        .http_client
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(info) = resp.json::<RelayInfoPartial>().await {
                if let Some(mode) = info.identity_mode.filter(|m| !m.is_empty()) {
                    return Ok(mode);
                }
            }
            Ok("disabled".to_string())
        }
        _ => {
            // Relay unreachable — fall back to local env var.
            Ok(std::env::var("SPROUT_IDENTITY_MODE")
                .ok()
                .filter(|s| !s.is_empty() && s != "disabled")
                .unwrap_or_else(|| "disabled".to_string()))
        }
    }
}

#[tauri::command]
pub fn get_nsec(state: State<'_, AppState>) -> Result<String, String> {
    let keys = state.keys.lock().map_err(|error| error.to_string())?;
    keys.secret_key()
        .to_bech32()
        .map_err(|error| format!("encode nsec: {error}"))
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

    if let Some(token) = state.configured_api_token.as_deref() {
        tags.push(
            Tag::parse(vec!["auth_token", token])
                .map_err(|error| format!("auth token tag failed: {error}"))?,
        );
    }

    let event = EventBuilder::new(Kind::Custom(22242), "")
        .tags(tags)
        .sign_with_keys(&keys)
        .map_err(|error| format!("sign failed: {error}"))?;

    Ok(event.as_json())
}
