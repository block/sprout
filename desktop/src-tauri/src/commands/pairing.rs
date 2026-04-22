use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use nostr::ToBech32;
use serde::Serialize;
use sprout_core::kind::KIND_PAIRING;
use sprout_core::pairing::qr::encode_qr;
use sprout_core::pairing::session::PairingSession;
use sprout_core::pairing::types::{AbortReason, PayloadType};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use crate::app_state::AppState;
use crate::relay::{relay_api_base_url, relay_ws_url};

use super::tokens::{mint_token_internal_with_auth_mode, MintTokenAuthMode};

#[derive(Serialize, Clone)]
struct PairingSasPayload {
    sas: String,
}

#[derive(Serialize, Clone)]
struct PairingAbortedPayload {
    reason: String,
}

#[derive(Serialize, Clone)]
struct PairingErrorPayload {
    message: String,
}

/// Managed Tauri state for an active pairing session.
pub struct PairingHandle {
    session: Arc<tokio::sync::Mutex<Option<PairingSession>>>,
    cancel: std::sync::Mutex<Option<CancellationToken>>,
    /// Send JSON-serialized events to the background WS task for relay publication.
    outbound_tx: std::sync::Mutex<Option<mpsc::Sender<String>>>,
    /// Pre-built payload string (contains nsec) to send after SAS confirmation.
    /// Wrapped in Zeroizing so the nsec is cleared from memory on drop.
    payload: std::sync::Mutex<Option<Zeroizing<String>>>,
}

impl PairingHandle {
    pub fn new() -> Self {
        Self {
            session: Arc::new(tokio::sync::Mutex::new(None)),
            cancel: std::sync::Mutex::new(None),
            outbound_tx: std::sync::Mutex::new(None),
            payload: std::sync::Mutex::new(None),
        }
    }

    fn clear(&self) {
        *self.cancel.lock().unwrap_or_else(|e| e.into_inner()) = None;
        *self.outbound_tx.lock().unwrap_or_else(|e| e.into_inner()) = None;
        *self.payload.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

const MOBILE_SCOPES: &[&str] = &[
    "messages:read",
    "messages:write",
    "channels:read",
    "channels:write",
    "users:read",
    "files:read",
    "files:write",
];
const EXPIRES_IN_DAYS: u32 = 90;

fn mobile_pairing_mint_auth_mode(state: &AppState) -> MintTokenAuthMode {
    if state.configured_api_token.is_some() {
        MintTokenAuthMode::BootstrapNip98
    } else {
        MintTokenAuthMode::Auto
    }
}

/// Start a NIP-AB pairing session as the source device.
///
/// Mints a token, creates a `PairingSession`, connects to the relay,
/// and returns the `nostrpair://` QR URI for the frontend to display.
#[tauri::command]
pub async fn start_pairing(
    app: AppHandle,
    state: State<'_, AppState>,
    pairing: State<'_, PairingHandle>,
) -> Result<String, String> {
    if let Some(token) = pairing.cancel.lock().map_err(|e| e.to_string())?.take() {
        token.cancel();
    }
    pairing.clear();

    let scopes: Vec<String> = MOBILE_SCOPES.iter().map(|s| s.to_string()).collect();
    let token_name = format!("mobile-pairing-{}", chrono::Utc::now().timestamp());
    let mint_result = mint_token_internal_with_auth_mode(
        &state,
        &token_name,
        &scopes,
        None,
        Some(EXPIRES_IN_DAYS),
        mobile_pairing_mint_auth_mode(&state),
    )
    .await?;

    let (nsec, pubkey_hex) = {
        let keys = state.keys.lock().map_err(|e| e.to_string())?;
        let nsec = keys
            .secret_key()
            .to_bech32()
            .map_err(|e| format!("encode nsec: {e}"))?;
        let pubkey = keys.public_key().to_hex();
        (nsec, pubkey)
    };

    let ws_url = relay_ws_url();
    let http_url = relay_api_base_url();

    let (session, qr_payload) = PairingSession::new_source(ws_url.clone());
    let qr_uri = encode_qr(&qr_payload);

    let payload_json = serde_json::json!({
        "relayUrl": http_url,
        "token": mint_result.token,
        "pubkey": pubkey_hex,
        "nsec": nsec,
    });

    {
        let mut s = pairing.session.lock().await;
        *s = Some(session);
    }
    *pairing.payload.lock().map_err(|e| e.to_string())? =
        Some(Zeroizing::new(payload_json.to_string()));

    let (outbound_tx, outbound_rx) = mpsc::channel::<String>(16);
    let cancel = CancellationToken::new();

    *pairing.outbound_tx.lock().map_err(|e| e.to_string())? = Some(outbound_tx);
    *pairing.cancel.lock().map_err(|e| e.to_string())? = Some(cancel.clone());

    let session_arc = Arc::clone(&pairing.session);
    tauri::async_runtime::spawn(pairing_ws_task(
        ws_url,
        session_arc,
        cancel,
        outbound_rx,
        app,
    ));

    Ok(qr_uri)
}

/// User confirmed the SAS codes match. Sends sas-confirm + payload.
#[tauri::command]
pub async fn confirm_pairing_sas(pairing: State<'_, PairingHandle>) -> Result<(), String> {
    let tx = pairing
        .outbound_tx
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or("no active pairing session")?;

    let sas_confirm_json = {
        let mut guard = pairing.session.lock().await;
        let session = guard.as_mut().ok_or("no active pairing session")?;
        let event = session.confirm_sas().map_err(|e| e.to_string())?;
        event_to_relay_json(&event)
    };

    tx.send(sas_confirm_json)
        .await
        .map_err(|_| "failed to send sas-confirm")?;

    let payload = pairing
        .payload
        .lock()
        .map_err(|e| e.to_string())?
        .take()
        .ok_or("no payload prepared")?;

    let payload_json = {
        let mut guard = pairing.session.lock().await;
        let session = guard.as_mut().ok_or("no active pairing session")?;
        let event = session
            .send_payload(PayloadType::Custom, payload)
            .map_err(|e| e.to_string())?;
        event_to_relay_json(&event)
    };

    tx.send(payload_json)
        .await
        .map_err(|_| "failed to send payload")?;

    Ok(())
}

/// Cancel the active pairing session.
#[tauri::command]
pub async fn cancel_pairing(pairing: State<'_, PairingHandle>) -> Result<(), String> {
    let abort_json = {
        let mut guard = pairing.session.lock().await;
        if let Some(session) = guard.as_mut() {
            session
                .abort(AbortReason::UserDenied)
                .ok()
                .flatten()
                .map(|e| event_to_relay_json(&e))
        } else {
            None
        }
    };

    if let Some(json) = abort_json {
        let tx = pairing
            .outbound_tx
            .lock()
            .map_err(|e| e.to_string())?
            .clone();
        if let Some(tx) = tx {
            let _ = tx.send(json).await;
        }
    }

    if let Some(token) = pairing.cancel.lock().map_err(|e| e.to_string())?.take() {
        token.cancel();
    }
    pairing.clear();

    {
        let mut s = pairing.session.lock().await;
        *s = None;
    }

    Ok(())
}

async fn pairing_ws_task(
    relay_url: String,
    session: Arc<tokio::sync::Mutex<Option<PairingSession>>>,
    cancel: CancellationToken,
    mut outbound_rx: mpsc::Receiver<String>,
    app: AppHandle,
) {
    if let Err(e) =
        pairing_ws_task_inner(&relay_url, &session, &cancel, &mut outbound_rx, &app).await
    {
        let _ = app.emit("pairing-error", PairingErrorPayload { message: e });
    }
    let mut s = session.lock().await;
    *s = None;
}

async fn pairing_ws_task_inner(
    relay_url: &str,
    session: &Arc<tokio::sync::Mutex<Option<PairingSession>>>,
    cancel: &CancellationToken,
    outbound_rx: &mut mpsc::Receiver<String>,
    app: &AppHandle,
) -> Result<(), String> {
    let (ws, _) = connect_async(relay_url)
        .await
        .map_err(|e| format!("WebSocket connection failed: {e}"))?;
    let (mut write, mut read) = ws.split();

    handle_nip42_auth(&mut read, &mut write, session, relay_url).await?;

    let our_pk = {
        let guard = session.lock().await;
        guard.as_ref().ok_or("session gone")?.pubkey().to_hex()
    };
    let sub_msg = serde_json::json!([
        "REQ", "pair",
        { "kinds": [KIND_PAIRING], "#p": [our_pk] }
    ]);
    write
        .send(Message::Text(sub_msg.to_string().into()))
        .await
        .map_err(|e| format!("subscribe failed: {e}"))?;

    wait_for_eose(&mut read, "pair", Duration::from_secs(10)).await?;

    let hard_timeout = tokio::time::sleep(Duration::from_secs(130));
    tokio::pin!(hard_timeout);

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = &mut hard_timeout => {
                let _ = app.emit("pairing-error", PairingErrorPayload {
                    message: "Session timed out".into(),
                });
                break;
            }
            Some(json_msg) = outbound_rx.recv() => {
                if let Err(e) = write.send(Message::Text(json_msg.into())).await {
                    return Err(format!("publish failed: {e}"));
                }
            }
            msg = read.next() => {
                let Some(msg) = msg else {
                    return Err("relay connection closed".into());
                };
                let msg = msg.map_err(|e| format!("WS read error: {e}"))?;
                let Message::Text(text) = msg else { continue };

                if let Some(event) = parse_relay_event(text.as_str(), "pair") {
                    let mut guard = session.lock().await;
                    let Some(s) = guard.as_mut() else { break };

                    match s.handle_abort(&event) {
                        Ok(reason) => {
                            let _ = app.emit("pairing-aborted", PairingAbortedPayload {
                                reason: format!("{reason:?}"),
                            });
                            break;
                        }
                        Err(_) => {}
                    }

                    if let Ok(sas) = s.handle_offer(&event) {
                        let _ = app.emit("pairing-sas-received", PairingSasPayload { sas });
                        continue;
                    }

                    match s.handle_complete(&event) {
                        Ok(()) => {
                            let _ = app.emit("pairing-complete", serde_json::json!({}));
                            break;
                        }
                        Err(ref e) if format!("{e}").contains("success=false") => {
                            let _ = app.emit("pairing-error", PairingErrorPayload {
                                message: "Mobile device reported failure importing credentials".into(),
                            });
                            break;
                        }
                        Err(_) => {}
                    }
                }
            }
        }
    }

    Ok(())
}

async fn handle_nip42_auth<R, W>(
    read: &mut R,
    write: &mut W,
    session: &Arc<tokio::sync::Mutex<Option<PairingSession>>>,
    relay_url: &str,
) -> Result<(), String>
where
    R: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
    W: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let auth_result = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let msg = read
                .next()
                .await
                .ok_or_else(|| "relay closed during auth".to_string())?
                .map_err(|e| format!("WS error during auth: {e}"))?;
            if let Message::Text(text) = msg {
                if let Some(challenge) = parse_auth_challenge(text.as_str()) {
                    return Ok(challenge);
                }
            }
        }
    })
    .await;

    let challenge: String = match auth_result {
        Ok(Ok(c)) => c,
        Ok(Err(e)) => return Err(e),
        Err(_) => return Ok(()),
    };

    let relay_url_parsed: url::Url = relay_url
        .parse()
        .map_err(|e| format!("invalid relay URL: {e}"))?;
    let auth_json = {
        let guard = session.lock().await;
        let s = guard.as_ref().ok_or("session gone during auth")?;
        let auth_event = s
            .sign_event(nostr_compat::EventBuilder::auth(
                challenge,
                relay_url_parsed,
            ))
            .map_err(|e| format!("sign auth event: {e}"))?;
        format!(
            "[\"AUTH\",{}]",
            nostr_compat::JsonUtil::as_json(&auth_event)
        )
    };

    write
        .send(Message::Text(auth_json.into()))
        .await
        .map_err(|e| format!("send auth: {e}"))?;

    let _ = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let msg = read
                .next()
                .await
                .ok_or_else(|| "relay closed during auth OK".to_string())?
                .map_err(|e| format!("WS error during auth OK: {e}"))?;
            if let Message::Text(text) = msg {
                if text.contains("\"OK\"") || text.contains("[\"OK\"") {
                    return Ok::<(), String>(());
                }
            }
        }
    })
    .await;

    Ok(())
}

/// Serialize a nostr 0.36 Event to `["EVENT", <event>]` JSON string.
fn event_to_relay_json(event: &nostr_compat::Event) -> String {
    format!("[\"EVENT\",{}]", nostr_compat::JsonUtil::as_json(event))
}

/// Parse a relay EVENT message into a nostr 0.36 Event (sprout-core compatible).
fn parse_relay_event(text: &str, sub_id: &str) -> Option<nostr_compat::Event> {
    let arr: serde_json::Value = serde_json::from_str(text).ok()?;
    let arr = arr.as_array()?;
    if arr.len() < 3 {
        return None;
    }
    if arr[0].as_str()? != "EVENT" {
        return None;
    }
    if arr[1].as_str()? != sub_id {
        return None;
    }
    serde_json::from_value(arr[2].clone()).ok()
}

fn parse_auth_challenge(text: &str) -> Option<String> {
    let arr: serde_json::Value = serde_json::from_str(text).ok()?;
    let arr = arr.as_array()?;
    if arr.len() >= 2 && arr[0].as_str()? == "AUTH" {
        return arr[1].as_str().map(|s| s.to_string());
    }
    None
}

async fn wait_for_eose<S>(read: &mut S, sub_id: &str, dur: Duration) -> Result<(), String>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    tokio::time::timeout(dur, async {
        loop {
            let msg = read
                .next()
                .await
                .ok_or_else(|| "relay closed waiting for EOSE".to_string())?
                .map_err(|e| format!("WS error waiting for EOSE: {e}"))?;
            if let Message::Text(text) = msg {
                if let Ok(arr) = serde_json::from_str::<serde_json::Value>(text.as_str()) {
                    if let Some(arr) = arr.as_array() {
                        if arr.len() >= 2
                            && arr[0].as_str() == Some("EOSE")
                            && arr[1].as_str() == Some(sub_id)
                        {
                            return Ok(());
                        }
                    }
                }
            }
        }
    })
    .await
    .map_err(|_| "timeout waiting for EOSE".to_string())?
}

#[cfg(test)]
mod tests {
    use super::MOBILE_SCOPES;
    use super::*;
    use crate::app_state::build_app_state;

    #[test]
    fn mobile_pairing_token_includes_file_write_scope() {
        assert!(MOBILE_SCOPES.contains(&"files:write"));
    }

    #[test]
    fn mobile_pairing_uses_bootstrap_mint_when_configured_token_is_present() {
        let mut state = build_app_state();
        state.configured_api_token = Some("desktop-token".to_string());

        assert_eq!(
            mobile_pairing_mint_auth_mode(&state),
            MintTokenAuthMode::BootstrapNip98
        );
    }

    #[test]
    fn mobile_pairing_keeps_auto_mint_without_configured_token() {
        let mut state = build_app_state();
        state.configured_api_token = None;

        assert_eq!(
            mobile_pairing_mint_auth_mode(&state),
            MintTokenAuthMode::Auto
        );
    }
}
