//! External-facing NIP-01 WebSocket server for standard Nostr clients.
//!
//! Handles NIP-11 relay info, NIP-42 AUTH challenge/response, invite token
//! validation, pre-auth REQ buffering, and kind:40/41 interception from
//! the local [`ChannelMap`].

use std::sync::Arc;

use axum::{
    Router,
    extract::{FromRequest, Query, State, WebSocketUpgrade, ws::{Message, WebSocket}},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use nostr::prelude::*;
use serde::Deserialize;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::channel_map::ChannelMap;
use crate::invite_store::InviteStore;
use crate::translate::Translator;
use crate::upstream::UpstreamClient;


// ─── Shared state ────────────────────────────────────────────────────────────

/// Shared state injected into every axum handler.
#[derive(Clone)]
pub struct ProxyState {
    /// Bidirectional UUID ↔ kind:40 event ID map (loaded at startup).
    pub channel_map: Arc<ChannelMap>,
    /// In-memory invite token registry.
    pub invite_store: Arc<InviteStore>,
    /// Event translator: NIP-28 ↔ Sprout internal format.
    pub translator: Arc<Translator>,
    /// Upstream relay client — used to send events, REQs, and CLOSEs.
    pub upstream: Arc<UpstreamClient>,
    /// Broadcast channel: raw NIP-01 JSON strings FROM the upstream relay.
    /// Each WebSocket connection subscribes its own receiver.
    pub upstream_events: tokio::sync::broadcast::Sender<String>,
    /// Optional shared secret for the admin endpoint.
    /// If `Some`, requests must include `Authorization: Bearer <secret>`.
    /// If `None`, the endpoint is unauthenticated (dev mode).
    pub admin_secret: Option<String>,
}

// ─── Router ──────────────────────────────────────────────────────────────────

/// Query parameters accepted on the root WebSocket endpoint.
#[derive(Deserialize)]
pub struct WsParams {
    /// Invite token string (required for WebSocket connections).
    token: Option<String>,
}

/// Build the axum [`Router`] for the proxy server.
///
/// Routes:
/// - `GET /`              — NIP-11 JSON *or* WebSocket upgrade (content-negotiated)
/// - `POST /admin/invite` — Create an invite token (protected by `SPROUT_PROXY_ADMIN_SECRET`)
pub fn router(state: ProxyState) -> Router {
    Router::new()
        .route("/", get(root_handler))
        .route("/admin/invite", axum::routing::post(create_invite))
        .with_state(state)
}

// ─── Root handler (NIP-11 / WebSocket) ───────────────────────────────────────

/// Content-negotiate between NIP-11 JSON and WebSocket upgrade.
///
/// Uses `axum::extract::Request` to manually attempt the WS upgrade so that
/// plain HTTP GET requests (NIP-11 clients, browser visits) are not rejected
/// by the extractor. Mirrors the pattern used in `sprout-relay/src/router.rs`.
async fn root_handler(
    State(state): State<ProxyState>,
    headers: HeaderMap,
    Query(params): Query<WsParams>,
    req: axum::extract::Request,
) -> Response {
    // NIP-11: clients that send `Accept: application/nostr+json` want relay info.
    let wants_nip11 = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("application/nostr+json"))
        .unwrap_or(false);

    if wants_nip11 {
        return nip11_response().into_response();
    }

    // Try WebSocket upgrade; fall back to NIP-11 JSON for plain HTTP.
    match WebSocketUpgrade::from_request(req, &state).await {
        Ok(ws) => {
            let token = params.token.unwrap_or_default();
            ws.on_upgrade(move |socket| handle_ws(socket, state, token))
        }
        Err(_) => nip11_response().into_response(),
    }
}

fn nip11_response() -> impl IntoResponse {
    let nip11 = serde_json::json!({
        "name": "sprout-proxy",
        "description": "Sprout NIP-28 guest proxy for standard Nostr clients",
        "supported_nips": [1, 11, 28, 42],
        "software": "sprout-proxy",
        "version": env!("CARGO_PKG_VERSION"),
        "limitation": {
            "auth_required": true
        }
    });
    (
        [
            (axum::http::header::CONTENT_TYPE, "application/nostr+json"),
            (axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        ],
        serde_json::to_string_pretty(&nip11).unwrap(),
    )
}

// ─── WebSocket handler ───────────────────────────────────────────────────────

/// Helper: serialize a [`RelayMessage`] and send it over the socket.
/// Returns `true` if the send succeeded.
async fn send_relay_msg(socket: &mut WebSocket, msg: RelayMessage) -> bool {
    let json = msg.as_json();
    socket.send(Message::Text(json.into())).await.is_ok()
}

async fn handle_ws(mut socket: WebSocket, state: ProxyState, token: String) {
    // Per-connection prefix for subscription ID namespacing.
    // All sub IDs sent upstream are prefixed with this to prevent collisions
    // across clients sharing the single upstream connection.
    let conn_prefix = uuid::Uuid::new_v4().simple().to_string()[..8].to_string();

    // ── 1. Validate invite token ──────────────────────────────────────────
    let allowed_channels = match state.invite_store.validate_and_consume(&token) {
        Ok(channels) => channels,
        Err(e) => {
            let _ = send_relay_msg(
                &mut socket,
                RelayMessage::notice(format!("error: {e}")),
            )
            .await;
            return;
        }
    };

    // ── 2. Send NIP-42 AUTH challenge ─────────────────────────────────────
    let challenge = uuid::Uuid::new_v4().to_string();
    if !send_relay_msg(&mut socket, RelayMessage::auth(challenge.clone())).await {
        return;
    }

    // ── 3. Pre-auth loop: buffer REQs, wait for AUTH ──────────────────────
    // Returns `Some(pubkey)` on successful auth, `None` if the connection
    // should be dropped (timeout, disconnect, buffer overflow).
    let mut pre_auth_buffer: Vec<String> = Vec::new();
    let mut pre_auth_bytes: usize = 0;

    let auth_deadline =
        tokio::time::Instant::now() + std::time::Duration::from_secs(30);

    let client_pubkey: PublicKey = loop {
        let msg = tokio::select! {
            msg = socket.recv() => msg,
            _ = tokio::time::sleep_until(auth_deadline) => {
                let _ = send_relay_msg(
                    &mut socket,
                    RelayMessage::notice("auth-required: authentication timeout"),
                )
                .await;
                return;
            }
        };

        let text = match msg {
            Some(Ok(Message::Text(t))) => t.to_string(),
            Some(Ok(Message::Close(_))) | None => return,
            _ => continue,
        };

        match ClientMessage::from_json(&text) {
            Ok(ClientMessage::Auth(auth_event)) => {
                // Must be kind 22242
                if auth_event.kind != Kind::Authentication {
                    let _ = send_relay_msg(
                        &mut socket,
                        RelayMessage::ok(
                            auth_event.id,
                            false,
                            "invalid: wrong kind for AUTH",
                        ),
                    )
                    .await;
                    continue;
                }

                // Challenge tag must match
                let has_challenge = auth_event.tags.iter().any(|t| {
                    let s = t.as_slice();
                    s.len() >= 2 && s[0] == "challenge" && s[1] == challenge
                });
                if !has_challenge {
                    let _ = send_relay_msg(
                        &mut socket,
                        RelayMessage::ok(
                            auth_event.id,
                            false,
                            "invalid: wrong challenge",
                        ),
                    )
                    .await;
                    continue;
                }

                // Signature must be valid
                if auth_event.verify().is_err() {
                    let _ = send_relay_msg(
                        &mut socket,
                        RelayMessage::ok(
                            auth_event.id,
                            false,
                            "invalid: bad signature",
                        ),
                    )
                    .await;
                    continue;
                }

                // Auth success — break with the authenticated pubkey
                let pubkey = auth_event.pubkey;
                let event_id = auth_event.id;
                let _ = send_relay_msg(
                    &mut socket,
                    RelayMessage::ok(event_id, true, ""),
                )
                .await;
                info!(
                    pubkey = %pubkey,
                    channels = allowed_channels.len(),
                    "client authenticated"
                );
                break pubkey;
            }

            Ok(ClientMessage::Req { .. }) => {
                // Buffer pre-auth REQs (cap at 20 messages / 64 KiB)
                if pre_auth_buffer.len() >= 20 || pre_auth_bytes + text.len() > 65_536 {
                    let _ = send_relay_msg(
                        &mut socket,
                        RelayMessage::notice(
                            "error: pre-auth buffer full, authenticate first",
                        ),
                    )
                    .await;
                    return;
                }
                pre_auth_bytes += text.len();
                pre_auth_buffer.push(text);
            }

            Ok(ClientMessage::Event(_)) => {
                let _ = send_relay_msg(
                    &mut socket,
                    RelayMessage::notice(
                        "auth-required: authenticate before sending events",
                    ),
                )
                .await;
            }

            _ => {
                // Ignore unknown / unparseable messages during pre-auth
            }
        }
    };

    // ── 4. Replay buffered REQs ───────────────────────────────────────────
    for buffered in pre_auth_buffer {
        handle_client_message(
            &mut socket,
            &state,
            &buffered,
            &allowed_channels,
            &client_pubkey,
            &conn_prefix,
        )
        .await;
    }

    // ── 5. Subscribe to upstream broadcast ───────────────────────────────
    let mut upstream_rx = state.upstream_events.subscribe();

    // ── 6. Main authenticated message loop ───────────────────────────────
    loop {
        tokio::select! {
            // Inbound from client
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        handle_client_message(
                            &mut socket,
                            &state,
                            &text.to_string(),
                            &allowed_channels,
                            &client_pubkey,
                            &conn_prefix,
                        )
                        .await;
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }

            // Outbound from upstream relay — translate and filter per-client
            upstream = upstream_rx.recv() => {
                match upstream {
                    Ok(text) => {
                        match RelayMessage::from_json(&text) {
                            Ok(RelayMessage::Event { subscription_id, event }) => {
                                // Only process events for subscriptions owned by this connection.
                                let sub_str = subscription_id.to_string();
                                if !sub_str.starts_with(&conn_prefix) {
                                    continue; // Not ours — another client's subscription.
                                }
                                // Strip the connection prefix before sending to client.
                                let client_sub_id = SubscriptionId::new(&sub_str[conn_prefix.len() + 1..]);
                                // Translate outbound: kind:40001 → kind:42, #h → #e
                                match state.translator.translate_outbound(&event, &allowed_channels) {
                                    Ok(Some(translated)) => {
                                        let out = RelayMessage::event(client_sub_id, translated);
                                        if socket.send(Message::Text(out.as_json().into())).await.is_err() {
                                            break;
                                        }
                                    }
                                    Ok(None) => {
                                        // Not translatable or not a stream message — drop silently.
                                    }
                                    Err(e) => {
                                        // Permission denied or channel not found — skip silently.
                                        debug!(error = %e, "dropping upstream event (not in scope)");
                                    }
                                }
                            }
                            Ok(RelayMessage::EndOfStoredEvents(ref sub_id)) => {
                                let sub_str = sub_id.to_string();
                                if sub_str.starts_with(&conn_prefix) {
                                    let client_sub_id = SubscriptionId::new(&sub_str[conn_prefix.len() + 1..]);
                                    let out = RelayMessage::eose(client_sub_id);
                                    if socket.send(Message::Text(out.as_json().into())).await.is_err() {
                                        break;
                                    }
                                }
                            }
                            Ok(RelayMessage::Closed { ref subscription_id, ref message }) => {
                                let sub_str = subscription_id.to_string();
                                if sub_str.starts_with(&conn_prefix) {
                                    let client_sub_id = SubscriptionId::new(&sub_str[conn_prefix.len() + 1..]);
                                    let out = RelayMessage::closed(client_sub_id, message.clone());
                                    if socket.send(Message::Text(out.as_json().into())).await.is_err() {
                                        break;
                                    }
                                }
                            }
                            Ok(other) => {
                                // OK, NOTICE, AUTH, COUNT — forward as-is (not sub-scoped).
                                if socket.send(Message::Text(other.as_json().into())).await.is_err() {
                                    break;
                                }
                            }
                            Err(_) => {
                                // Unparseable upstream message — forward raw so client can decide.
                                if socket.send(Message::Text(text.into())).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        warn!(skipped = n, "upstream broadcast lagged");
                        // Keep going — the client may have missed some events but
                        // we don't want to drop the connection.
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        error!("upstream broadcast channel closed");
                        break;
                    }
                }
            }
        }
    }

    debug!(pubkey = %client_pubkey, "client disconnected");
}

// ─── Client message dispatcher ───────────────────────────────────────────────

async fn handle_client_message(
    socket: &mut WebSocket,
    state: &ProxyState,
    raw_msg: &str,
    allowed_channels: &[Uuid],
    client_pubkey: &PublicKey,
    conn_prefix: &str,
) {
    let msg = match ClientMessage::from_json(raw_msg) {
        Ok(m) => m,
        Err(_) => {
            let _ = send_relay_msg(
                socket,
                RelayMessage::notice("error: invalid message"),
            )
            .await;
            return;
        }
    };

    match msg {
        ClientMessage::Req { subscription_id, filters } => {
            handle_req(socket, state, subscription_id, filters, allowed_channels, conn_prefix).await;
        }
        ClientMessage::Event(event) => {
            let event_id = event.id;
            // Translate inbound: kind:42 → kind:40001, #e → #h, re-sign with shadow key.
            match state.translator.translate_inbound(&event, &client_pubkey.to_hex(), allowed_channels) {
                Ok(translated) => {
                    if let Err(e) = state.upstream.send_event(translated).await {
                        warn!("upstream send_event failed: {e}");
                        let ok_msg = RelayMessage::ok(event_id, false, &format!("error: upstream unavailable"));
                        let _ = socket.send(Message::Text(ok_msg.as_json().into())).await;
                    }
                }
                Err(e) => {
                    let ok_msg = RelayMessage::ok(event_id, false, &format!("error: {e}"));
                    let _ = socket.send(Message::Text(ok_msg.as_json().into())).await;
                }
            }
        }
        ClientMessage::Close(sub_id) => {
            let prefixed = SubscriptionId::new(format!("{conn_prefix}:{}", sub_id));
            if let Err(e) = state.upstream.send_close(prefixed).await {
                warn!("upstream send_close failed: {e}");
            }
        }
        // AUTH after initial handshake is silently ignored.
        ClientMessage::Auth(_) => {}
        _ => {}
    }
}

// ─── REQ handler ─────────────────────────────────────────────────────────────

async fn handle_req(
    socket: &mut WebSocket,
    state: &ProxyState,
    sub_id: SubscriptionId,
    filters: Vec<Filter>,
    allowed_channels: &[Uuid],
    conn_prefix: &str,
) {
    for filter in &filters {
        // Collect requested kind numbers (empty = all kinds, treated as "not
        // specifically kind:40/41").
        let kinds: Vec<u16> = filter
            .kinds
            .as_ref()
            .map(|k| k.iter().map(|kind| kind.as_u16()).collect())
            .unwrap_or_default();

        let wants_40 = kinds.contains(&40);
        let wants_41 = kinds.contains(&41);

        if wants_40 || wants_41 {
            // Serve synthesized channel events from the local ChannelMap.
            // These are NEVER forwarded to the upstream relay.
            let channels = state.channel_map.all_channels();
            for ch in &channels {
                if !allowed_channels.contains(&ch.uuid) {
                    continue;
                }
                if wants_40 {
                    let kind40 =
                        state.channel_map.synthesize_kind40(&ch.name, ch.created_at_unix);
                    let _ = send_relay_msg(
                        socket,
                        RelayMessage::event(sub_id.clone(), kind40),
                    )
                    .await;
                }
                if wants_41 {
                    let kind41 = state.channel_map.synthesize_kind41(ch);
                    let _ = send_relay_msg(
                        socket,
                        RelayMessage::event(sub_id.clone(), kind41),
                    )
                    .await;
                }
            }
            // EOSE for locally-served subscription
            let _ = send_relay_msg(socket, RelayMessage::eose(sub_id.clone())).await;
            return; // Do NOT forward kind:40/41 REQs upstream
        }
    }

    // All other kinds (42 messages, metadata, etc.) → translate and forward upstream.
    // Translate kind:42 filters to kind:40001 + #h tags.
    let translated_filters: Vec<Filter> = filters
        .iter()
        .map(|f| state.translator.translate_filter_inbound(f, allowed_channels))
        .collect();

    let prefixed_sub_id = SubscriptionId::new(format!("{conn_prefix}:{}", sub_id));
    if let Err(e) = state.upstream.send_req(prefixed_sub_id, translated_filters).await {
        warn!("upstream send_req failed: {e}");
    }
}

// ─── Admin: create invite token ───────────────────────────────────────────────

#[derive(Deserialize)]
struct CreateInviteRequest {
    /// Comma-separated channel UUIDs this token grants access to.
    channels: String,
    /// Hours until the token expires (default: 24).
    #[serde(default = "default_hours")]
    hours: u32,
    /// Maximum number of times the token may be used (default: 1).
    #[serde(default = "default_max_uses")]
    max_uses: u32,
}

fn default_hours() -> u32 {
    24
}
fn default_max_uses() -> u32 {
    1
}

async fn create_invite(
    State(state): State<ProxyState>,
    headers: HeaderMap,
    axum::Json(req): axum::Json<CreateInviteRequest>,
) -> impl IntoResponse {
    // ── Admin secret check ────────────────────────────────────────────────
    if let Some(ref secret) = state.admin_secret {
        let provided = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));

        match provided {
            Some(token) if token == secret => {
                // Authorized — proceed.
            }
            _ => {
                return (
                    StatusCode::UNAUTHORIZED,
                    axum::Json(serde_json::json!({
                        "error": "unauthorized: missing or invalid Authorization header"
                    })),
                )
                    .into_response();
            }
        }
    }

    let channel_ids: Vec<Uuid> = req
        .channels
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();

    let token_str = format!("sprout_invite_{}", Uuid::new_v4().simple());
    let expires_at = chrono::Utc::now()
        + chrono::Duration::hours(req.hours as i64);

    let token = crate::InviteToken::new(
        &token_str,
        channel_ids.clone(),
        expires_at,
        req.max_uses,
    );
    state.invite_store.insert(token);

    info!(
        token = %token_str,
        channels = channel_ids.len(),
        hours = req.hours,
        "invite token created"
    );

    (
        StatusCode::OK,
        axum::Json(serde_json::json!({
            "token": token_str,
            "channels": channel_ids,
            "expires_at": expires_at.to_rfc3339(),
            "max_uses": req.max_uses,
        })),
    )
        .into_response()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::broadcast;

    fn make_state() -> ProxyState {
        let keys = Keys::generate();
        let channel_map = Arc::new(crate::channel_map::ChannelMap::new(keys.clone()));
        let invite_store = Arc::new(InviteStore::new());
        let (upstream_events, _) = broadcast::channel(16);
        let shadow_keys = Arc::new(
            crate::shadow_keys::ShadowKeyManager::new(b"test-salt-server-tests")
                .expect("shadow key manager"),
        );
        let translator = Arc::new(crate::translate::Translator::new(
            shadow_keys,
            channel_map.clone(),
        ));
        let upstream = Arc::new(UpstreamClient::new("ws://localhost:3000", "sprout_test"));
        ProxyState {
            channel_map,
            invite_store,
            translator,
            upstream,
            upstream_events,
            admin_secret: None,
        }
    }

    #[test]
    fn router_builds() {
        let state = make_state();
        let _r = router(state);
    }

    #[test]
    fn nip11_json_is_valid() {
        // Just ensure the NIP-11 JSON serializes without panic
        let response = nip11_response();
        let _ = response.into_response();
    }

    #[test]
    fn default_hours_and_max_uses() {
        assert_eq!(default_hours(), 24);
        assert_eq!(default_max_uses(), 1);
    }
}
