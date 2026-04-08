//! External-facing NIP-01 WebSocket server for standard Nostr clients.
//!
//! Handles NIP-11 relay info, NIP-42 AUTH challenge/response (with
//! reactive-auth–compatible CLOSED/OK rejections for pre-auth messages),
//! guest and invite token authentication, and kind:40/41 interception
//! from the local [`ChannelMap`].

use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket},
        FromRequest, Query, State, WebSocketUpgrade,
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use nostr::prelude::*;
use serde::Deserialize;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::channel_map::ChannelMap;
use crate::guest_store::GuestStore;
use crate::invite_store::InviteStore;
use crate::translate::Translator;
use crate::upstream::UpstreamClient;

// ─── Shared state ────────────────────────────────────────────────────────────

/// Shared state injected into every axum handler.
#[derive(Clone)]
pub struct ProxyState {
    /// Bidirectional UUID ↔ kind:40 event ID map (loaded at startup).
    pub channel_map: Arc<ChannelMap>,
    /// Pubkey-based guest registry (persistent access, no token needed).
    pub guest_store: Arc<GuestStore>,
    /// In-memory invite token registry (temporary access via bearer token).
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
    /// This proxy's own WebSocket URL (e.g. "ws://0.0.0.0:4869").
    /// Used for NIP-42 relay tag validation.
    pub relay_url: String,
    /// UUIDs of channels accessible via the `/public` read-only endpoint.
    /// Configured via `SPROUT_PROXY_PUBLIC_CHANNELS` env var.
    pub public_channels: Arc<Vec<Uuid>>,
    /// Active public (unauthenticated) connection count for rate limiting.
    pub public_connection_count: Arc<std::sync::atomic::AtomicUsize>,
    /// Maximum lifetime (seconds) for a public WebSocket connection.
    /// Configured via `SPROUT_PROXY_PUBLIC_LIFETIME_SECS` env var.
    /// Falls back to `DEFAULT_PUBLIC_LIFETIME_SECS` (3600) if not set.
    pub public_lifetime_secs: u64,
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
/// - `GET /`               — NIP-11 JSON *or* WebSocket upgrade (content-negotiated)
/// - `POST /admin/invite`  — Create an invite token (temporary access)
/// - `POST /admin/guests`  — Register a guest pubkey (persistent access)
/// - `DELETE /admin/guests` — Revoke a guest pubkey
/// - `GET /admin/guests`   — List all registered guests
///
/// All `/admin/*` routes are protected by `SPROUT_PROXY_ADMIN_SECRET` if set.
///
/// `/public` is only registered when at least one public channel is configured.
pub fn router(state: ProxyState) -> Router {
    let has_public = !state.public_channels.is_empty();
    let mut app = Router::new()
        .route("/", get(root_handler))
        .route("/admin/invite", axum::routing::post(create_invite))
        .route(
            "/admin/guests",
            axum::routing::post(register_guest)
                .delete(revoke_guest)
                .get(list_guests),
        );
    if has_public {
        app = app.route("/public", get(public_handler));
    }
    app.with_state(state)
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

fn nip11_response() -> Response {
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
    match serde_json::to_string_pretty(&nip11) {
        Ok(body) => (
            [
                (axum::http::header::CONTENT_TYPE, "application/nostr+json"),
                (axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            ],
            body,
        )
            .into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

// ─── Constant-time string comparison ─────────────────────────────────────────

/// Compare two strings in constant time to prevent timing side-channel attacks.
/// Returns `true` only if both strings are identical.
///
/// Uses hash-then-compare to eliminate the length oracle: both inputs are hashed
/// to fixed 32-byte values before comparison, so string length is never leaked.
fn constant_time_eq(a: &str, b: &str) -> bool {
    use sha2::{Digest, Sha256};
    let hash_a: [u8; 32] = Sha256::digest(a.as_bytes()).into();
    let hash_b: [u8; 32] = Sha256::digest(b.as_bytes()).into();
    // Fixed-length comparison — no length oracle
    hash_a
        .iter()
        .zip(hash_b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
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

    // ── 1. Send NIP-42 AUTH challenge ─────────────────────────────────────
    // Token validation is deferred until after NIP-42 auth completes.
    // Registered guests (in GuestStore) don't need a token at all.
    let challenge = uuid::Uuid::new_v4().to_string();
    if !send_relay_msg(&mut socket, RelayMessage::auth(challenge.clone())).await {
        return;
    }

    // ── 3. Pre-auth loop: reject pre-auth REQs/EVENTs, wait for AUTH ─────
    // Returns `(pubkey, channels)` on successful auth, or drops the connection
    // on timeout / disconnect / invalid auth.
    let auth_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);

    let (client_pubkey, allowed_channels): (PublicKey, Vec<Uuid>) = loop {
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
                        RelayMessage::ok(auth_event.id, false, "invalid: wrong kind for AUTH"),
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
                        RelayMessage::ok(auth_event.id, false, "invalid: wrong challenge"),
                    )
                    .await;
                    continue;
                }

                // FIX 4: Timestamp recency check — must be within 10 minutes of now.
                let time_diff = Timestamp::now()
                    .as_u64()
                    .abs_diff(auth_event.created_at.as_u64());
                if time_diff >= 600 {
                    let _ = send_relay_msg(
                        &mut socket,
                        RelayMessage::ok(
                            auth_event.id,
                            false,
                            "invalid: auth event timestamp too far from now",
                        ),
                    )
                    .await;
                    continue;
                }

                // FIX F: Validate relay tag (non-fatal — many clients omit it).
                let has_relay = auth_event.tags.iter().any(|t| {
                    let s = t.as_slice();
                    s.len() >= 2 && s[0] == "relay" && s[1] == state.relay_url
                });
                if !has_relay {
                    debug!("NIP-42 AUTH missing or mismatched relay tag (non-fatal)");
                }

                // Signature must be valid
                if auth_event.verify().is_err() {
                    let _ = send_relay_msg(
                        &mut socket,
                        RelayMessage::ok(auth_event.id, false, "invalid: bad signature"),
                    )
                    .await;
                    continue;
                }

                // ── Resolve channel access ────────────────────────────
                // Priority: GuestStore (pubkey-based) > invite token.
                let pubkey = auth_event.pubkey;
                let event_id = auth_event.id;

                let channels = if let Some(guest_channels) = state.guest_store.lookup(&pubkey) {
                    // Registered guest — no token needed.
                    info!(pubkey = %pubkey, channels = guest_channels.len(), "guest authenticated (pubkey-based)");
                    guest_channels
                } else if !token.is_empty() {
                    // Fall back to invite token.
                    match state.invite_store.validate_and_consume(&token) {
                        Ok(ch) => {
                            info!(pubkey = %pubkey, channels = ch.len(), "guest authenticated (invite token)");
                            ch
                        }
                        Err(e) => {
                            let _ = send_relay_msg(
                                &mut socket,
                                RelayMessage::notice(format!("error: token invalid: {e}")),
                            )
                            .await;
                            return;
                        }
                    }
                } else {
                    // No guest registration, no token → reject.
                    let _ = send_relay_msg(
                        &mut socket,
                        RelayMessage::ok(
                            event_id,
                            false,
                            "restricted: pubkey not registered and no invite token provided",
                        ),
                    )
                    .await;
                    return;
                };

                let _ = send_relay_msg(&mut socket, RelayMessage::ok(event_id, true, "")).await;
                break (pubkey, channels);
            }

            Ok(ClientMessage::Req {
                subscription_id, ..
            }) => {
                // NIP-42: reject pre-auth REQs with CLOSED so clients like nak
                // can detect the auth-required rejection, authenticate, and
                // re-send the REQ. Buffering silently would leave reactive-auth
                // clients stuck waiting forever.
                let _ = send_relay_msg(
                    &mut socket,
                    RelayMessage::closed(
                        subscription_id,
                        "auth-required: authenticate before subscribing",
                    ),
                )
                .await;
            }

            Ok(ClientMessage::Event(event)) => {
                // NIP-42: respond with OK false so clients like nak can detect
                // the auth-required rejection and retry after authenticating.
                let _ = send_relay_msg(
                    &mut socket,
                    RelayMessage::ok(
                        event.id,
                        false,
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

    // FIX 1: pending_oks maps upstream_event_id_hex → client_original_event_id
    // FIX 5: active_subs tracks ALL prefixed sub IDs (for cap counting and cleanup).
    //        upstream_subs tracks only subs that were forwarded upstream (for CLOSE routing).
    let mut pending_oks: HashMap<String, EventId> = HashMap::new();
    let mut active_subs: HashSet<String> = HashSet::new();
    let mut upstream_subs: HashSet<String> = HashSet::new();

    // ── 4. Subscribe to upstream broadcast ────────────────────────────────
    let mut upstream_rx = state.upstream_events.subscribe();

    // ── 5. Main authenticated message loop ────────────────────────────────
    loop {
        tokio::select! {
            // Inbound from client
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        handle_client_message(
                            &mut socket,
                            &state,
                            &text,
                            &allowed_channels,
                            &client_pubkey,
                            &conn_prefix,
                            &mut pending_oks,
                            &mut active_subs,
                            &mut upstream_subs,
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
                                // Translate outbound: kind:9 → kind:42, #h → #e
                                match state
                                    .translator
                                    .translate_outbound(&event, &allowed_channels)
                                    .await
                                {
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
                                    // Clean up tracking — upstream killed this sub,
                                    // so free the slot before forwarding to client.
                                    active_subs.remove(&sub_str);
                                    upstream_subs.remove(&sub_str);
                                    let client_sub_id = SubscriptionId::new(&sub_str[conn_prefix.len() + 1..]);
                                    let out = RelayMessage::closed(client_sub_id, message.clone());
                                    if socket.send(Message::Text(out.as_json().into())).await.is_err() {
                                        break;
                                    }
                                }
                            }
                            // FIX 1: Route OK messages to the correct client using pending_oks map.
                            Ok(RelayMessage::Ok { event_id, status, message }) => {
                                let upstream_id_hex = event_id.to_hex();
                                if let Some(client_event_id) = pending_oks.remove(&upstream_id_hex) {
                                    // This OK is for an event we sent — rewrite with client's original ID.
                                    let out = RelayMessage::ok(client_event_id, status, message);
                                    if socket.send(Message::Text(out.as_json().into())).await.is_err() {
                                        break;
                                    }
                                }
                                // If not in pending_oks, this OK belongs to another client — skip it.
                            }
                            // FIX 1: NOTICE messages from upstream contain operational details.
                            // Log them but do NOT forward to clients.
                            Ok(RelayMessage::Notice { message: notice_msg }) => {
                                debug!(notice = %notice_msg, "upstream notice (not forwarded to client)");
                            }
                            Ok(_other) => {
                                // AUTH, COUNT, and other control-plane messages from
                                // upstream are internal to the proxy↔relay connection.
                                // Do NOT forward to clients — they leak relay internals.
                                debug!("dropping upstream control-plane message (not forwarded)");
                            }
                            Err(_) => {
                                // Unparseable upstream message — drop silently.
                                // Forwarding raw frames could leak relay internals.
                                debug!("dropping unparseable upstream message");
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

    // On disconnect, send CLOSE only for subs that were forwarded upstream.
    for prefixed_sub in upstream_subs {
        let sub_id = SubscriptionId::new(prefixed_sub);
        if let Err(e) = state.upstream.send_close(sub_id).await {
            warn!("upstream send_close on disconnect failed: {e}");
        }
    }

    debug!(pubkey = %client_pubkey, "client disconnected");
}

// ─── Client message dispatcher ───────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
async fn handle_client_message(
    socket: &mut WebSocket,
    state: &ProxyState,
    raw_msg: &str,
    allowed_channels: &[Uuid],
    client_pubkey: &PublicKey,
    conn_prefix: &str,
    pending_oks: &mut HashMap<String, EventId>,
    active_subs: &mut HashSet<String>,
    upstream_subs: &mut HashSet<String>,
) {
    let msg = match ClientMessage::from_json(raw_msg) {
        Ok(m) => m,
        Err(_) => {
            let _ = send_relay_msg(socket, RelayMessage::notice("error: invalid message")).await;
            return;
        }
    };

    match msg {
        ClientMessage::Req {
            subscription_id,
            filters,
        } => {
            handle_req(
                socket,
                state,
                subscription_id,
                filters,
                allowed_channels,
                conn_prefix,
                active_subs,
                upstream_subs,
            )
            .await;
        }
        ClientMessage::Event(event) => {
            let event_id = event.id;

            // Verify the event is signed by the authenticated client.
            // Without this, an AUTHed connection could submit arbitrary events
            // that get re-signed under the client's shadow identity.
            if event.pubkey != *client_pubkey {
                let ok_msg = RelayMessage::ok(
                    event_id,
                    false,
                    "invalid: event pubkey does not match authenticated identity",
                );
                let _ = socket.send(Message::Text(ok_msg.as_json().into())).await;
                return;
            }
            if event.verify().is_err() {
                let ok_msg = RelayMessage::ok(event_id, false, "invalid: bad event signature");
                let _ = socket.send(Message::Text(ok_msg.as_json().into())).await;
                return;
            }

            // Translate inbound: kind:42 → kind:9, #e → #h, re-sign with shadow key.
            match state.translator.translate_inbound(
                &event,
                &client_pubkey.to_hex(),
                allowed_channels,
            ) {
                Ok(translated) => {
                    // FIX H: Cap pending_oks to prevent unbounded growth if upstream never ACKs.
                    if pending_oks.len() >= 1000 {
                        let ok_msg =
                            RelayMessage::ok(event_id, false, "error: too many pending events");
                        let _ = socket.send(Message::Text(ok_msg.as_json().into())).await;
                        return;
                    }
                    // FIX 1: Store mapping from upstream event ID → client original event ID
                    // so we can route the OK response back correctly.
                    // FIX C: Capture upstream_id before moving `translated` into send_event,
                    // so we can remove the correct key on failure.
                    let upstream_id = translated.id;
                    pending_oks.insert(upstream_id.to_hex(), event_id);
                    if let Err(e) = state.upstream.send_event(translated).await {
                        warn!("upstream send_event failed: {e}");
                        // FIX C: Remove by translated (upstream) ID, not client event ID.
                        pending_oks.remove(&upstream_id.to_hex());
                        let ok_msg = RelayMessage::ok(
                            event_id,
                            false,
                            "error: upstream unavailable".to_string(),
                        );
                        let _ = socket.send(Message::Text(ok_msg.as_json().into())).await;
                    }
                }
                Err(e) => {
                    let ok_msg = RelayMessage::ok(event_id, false, format!("error: {e}"));
                    let _ = socket.send(Message::Text(ok_msg.as_json().into())).await;
                }
            }
        }
        ClientMessage::Close(sub_id) => {
            let prefixed = format!("{conn_prefix}:{}", sub_id);
            active_subs.remove(&prefixed);
            // Only send upstream CLOSE for subs that had upstream REQs.
            if upstream_subs.remove(&prefixed) {
                let prefixed_sub_id = SubscriptionId::new(prefixed);
                if let Err(e) = state.upstream.send_close(prefixed_sub_id).await {
                    warn!("upstream send_close failed: {e}");
                }
            }
        }
        // AUTH after initial handshake is silently ignored.
        ClientMessage::Auth(_) => {}
        _ => {}
    }
}

// ─── Filter splitting (pure, testable) ───────────────────────────────────────

/// Split a list of NIP-28 filters into local (kind:40/41) and upstream groups.
///
/// **Routing rules:**
/// - kind:40 → local only (channel creation is synthesized from ChannelMap)
/// - kind:41 → BOTH local (synthesized metadata) AND upstream (edit events, kind:40003)
/// - kind:42 and others → upstream only
/// - no kinds → both local (with 40/41 injected) and upstream
///
/// Returns `(local_filters, upstream_filters)`.
fn split_filters(filters: &[Filter]) -> (Vec<Filter>, Vec<Filter>) {
    let mut local_filters: Vec<Filter> = Vec::new();
    let mut upstream_filters: Vec<Filter> = Vec::new();

    for filter in filters {
        let kinds: Vec<u16> = filter
            .kinds
            .as_ref()
            .map(|k| k.iter().map(|kind| kind.as_u16()).collect())
            .unwrap_or_default();

        // kind:40 and kind:41 are served locally (synthesized metadata).
        let has_local = kinds.iter().any(|k| *k == 40 || *k == 41);
        // kind:41 ALSO goes upstream (translates to kind:40003 for edit events).
        // kind:42 and everything else goes upstream.
        let has_upstream = kinds.iter().any(|k| *k != 40);

        if has_local {
            let local_kinds: Vec<Kind> = kinds
                .iter()
                .filter(|k| **k == 40 || **k == 41)
                .map(|k| Kind::Custom(*k))
                .collect();
            let mut local_f = filter.clone();
            if let Some(ref all_kinds) = filter.kinds {
                local_f = local_f.remove_kinds(all_kinds.iter().cloned());
            }
            local_f = local_f.kinds(local_kinds);
            local_filters.push(local_f);
        }
        if has_upstream {
            // Upstream gets everything except kind:40 (which is local-only).
            let upstream_kinds: Vec<Kind> = kinds
                .iter()
                .filter(|k| **k != 40)
                .map(|k| Kind::Custom(*k))
                .collect();
            let mut upstream_f = filter.clone();
            if let Some(ref all_kinds) = filter.kinds {
                upstream_f = upstream_f.remove_kinds(all_kinds.iter().cloned());
            }
            upstream_f = upstream_f.kinds(upstream_kinds);
            upstream_filters.push(upstream_f);
        }
        if !has_local && !has_upstream {
            // No kinds specified — "subscribe to everything".
            // Forward upstream AND serve local kind:40/41 metadata.
            upstream_filters.push(filter.clone());
            let mut local_f = filter.clone();
            local_f = local_f.kinds([Kind::ChannelCreation, Kind::ChannelMetadata]);
            local_filters.push(local_f);
        }
    }

    (local_filters, upstream_filters)
}

/// Collect locally-served events for kind:40/41 filters from the channel map.
///
/// Returns the events that match the filter constraints (kinds, #e, authors,
/// since, until, ids, limit). The caller is responsible for sending them.
fn collect_local_events(
    filter: &Filter,
    channel_map: &ChannelMap,
    allowed_channels: &[Uuid],
) -> Vec<Event> {
    let kinds: Vec<u16> = filter
        .kinds
        .as_ref()
        .map(|k| k.iter().map(|kind| kind.as_u16()).collect())
        .unwrap_or_default();

    let wants_40 = kinds.contains(&40);
    let wants_41 = kinds.contains(&41);

    // Apply `authors` filter — all synthesized events share the server pubkey.
    let server_pubkey = channel_map.server_keys().public_key();
    if let Some(ref authors) = filter.authors {
        if !authors.contains(&server_pubkey) {
            return Vec::new();
        }
    }

    let e_tag_key = nostr::SingleLetterTag::lowercase(nostr::Alphabet::E);
    let e_filter_values = filter.generic_tags.get(&e_tag_key);

    let channels = channel_map.all_channels();
    let limit: usize = filter.limit.unwrap_or(usize::MAX);
    let mut events: Vec<Event> = Vec::new();

    for ch in &channels {
        if events.len() >= limit {
            break;
        }
        if !allowed_channels.contains(&ch.uuid) {
            continue;
        }
        if let Some(e_values) = e_filter_values {
            if !e_values.contains(&ch.kind40_event_id) {
                continue;
            }
        }
        if let Some(since) = filter.since {
            if Timestamp::from(ch.created_at_unix) < since {
                continue;
            }
        }
        if let Some(until) = filter.until {
            if Timestamp::from(ch.created_at_unix) > until {
                continue;
            }
        }

        if wants_40 && events.len() < limit {
            let kind40 = channel_map.synthesize_kind40(&ch.uuid.to_string(), ch.created_at_unix);
            let id_ok = filter
                .ids
                .as_ref()
                .is_none_or(|ids| ids.contains(&kind40.id));
            if id_ok {
                events.push(kind40);
            }
        }
        if wants_41 && events.len() < limit {
            let kind41 = channel_map.synthesize_kind41(ch);
            let id_ok = filter
                .ids
                .as_ref()
                .is_none_or(|ids| ids.contains(&kind41.id));
            if id_ok {
                events.push(kind41);
            }
        }
    }

    events
}

// ─── REQ handler ─────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
async fn handle_req(
    socket: &mut WebSocket,
    state: &ProxyState,
    sub_id: SubscriptionId,
    filters: Vec<Filter>,
    allowed_channels: &[Uuid],
    conn_prefix: &str,
    active_subs: &mut HashSet<String>,
    upstream_subs: &mut HashSet<String>,
) {
    // Nostr replacement semantics: a new REQ with the same sub ID replaces
    // the previous one. Tear down any existing upstream subscription first.
    let prefixed_existing = format!("{conn_prefix}:{}", sub_id);
    if upstream_subs.remove(&prefixed_existing) {
        let old_sub_id = SubscriptionId::new(&prefixed_existing);
        if let Err(e) = state.upstream.send_close(old_sub_id).await {
            warn!("upstream send_close for replaced sub failed: {e}");
        }
    }
    // Remove from active_subs too — it will be re-added if the new REQ succeeds.
    active_subs.remove(&prefixed_existing);

    let (owned_local_filters, owned_upstream_filters) = split_filters(&filters);

    // Serve local filters from ChannelMap via the extracted pure function.
    for filter in &owned_local_filters {
        let events = collect_local_events(filter, &state.channel_map, allowed_channels);
        for event in events {
            let _ = send_relay_msg(socket, RelayMessage::event(sub_id.clone(), event)).await;
        }
    }

    if owned_upstream_filters.is_empty() {
        // Only local filters — send EOSE immediately after serving them.
        // Track the sub even for local-only REQs so the per-connection cap
        // is accurate and cleanup on disconnect is complete.
        let prefixed_local = format!("{conn_prefix}:{}", sub_id);
        active_subs.insert(prefixed_local);
        let _ = send_relay_msg(socket, RelayMessage::eose(sub_id.clone())).await;
        return;
    }

    // Forward upstream filters (translated) to the upstream relay.
    // The upstream EOSE will serve as the combined EOSE for mixed REQs.
    let translated_filters: Vec<Filter> = owned_upstream_filters
        .iter()
        .map(|f| {
            state
                .translator
                .translate_filter_inbound(f, allowed_channels)
        })
        .collect();

    let prefixed_sub_id_str = format!("{conn_prefix}:{}", sub_id);
    let prefixed_sub_id = SubscriptionId::new(prefixed_sub_id_str.clone());

    // Only track the sub after send_req succeeds. If the upstream send
    // fails, the sub was never established — don't consume a slot.
    match state
        .upstream
        .send_req(prefixed_sub_id, translated_filters)
        .await
    {
        Ok(()) => {
            active_subs.insert(prefixed_sub_id_str.clone());
            upstream_subs.insert(prefixed_sub_id_str);
        }
        Err(e) => {
            warn!("upstream send_req failed: {e}");
            // Notify client that the subscription couldn't be established.
            let _ = send_relay_msg(
                socket,
                RelayMessage::closed(sub_id, "error: upstream unavailable"),
            )
            .await;
        }
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
    /// Maximum number of times the token may be used (default: 10).
    #[serde(default = "default_max_uses")]
    max_uses: u32,
}

fn default_hours() -> u32 {
    24
}

/// FIX 7: Default max_uses changed from 1 to 10.
fn default_max_uses() -> u32 {
    10
}

async fn create_invite(
    State(state): State<ProxyState>,
    headers: HeaderMap,
    axum::Json(req): axum::Json<CreateInviteRequest>,
) -> impl IntoResponse {
    if let Some(err) = check_admin_secret(&state.admin_secret, &headers) {
        return err;
    }

    let channel_ids: Vec<Uuid> = req
        .channels
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();

    if channel_ids.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({ "error": "at least one valid channel UUID required" })),
        )
            .into_response();
    }

    if req.hours == 0 || req.max_uses == 0 {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({ "error": "hours and max_uses must be > 0" })),
        )
            .into_response();
    }

    let token_str = format!("sprout_invite_{}", Uuid::new_v4().simple());
    let expires_at = chrono::Utc::now() + chrono::Duration::hours(req.hours as i64);

    let token = crate::InviteToken::new(&token_str, channel_ids.clone(), expires_at, req.max_uses);
    state.invite_store.insert(token);

    info!(
        token_prefix = %&token_str[..20],
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

// ─── Admin: check secret helper ───────────────────────────────────────────────

/// Verify the admin secret from the Authorization header. Returns an error
/// response if the secret is required but missing/wrong, or `None` if OK.
fn check_admin_secret(admin_secret: &Option<String>, headers: &HeaderMap) -> Option<Response> {
    if let Some(ref secret) = admin_secret {
        let provided = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));
        match provided {
            Some(token) if constant_time_eq(token, secret) => None,
            _ => Some(
                (
                    StatusCode::UNAUTHORIZED,
                    axum::Json(serde_json::json!({
                        "error": "unauthorized: missing or invalid Authorization header"
                    })),
                )
                    .into_response(),
            ),
        }
    } else {
        None // No secret configured — dev mode, allow all.
    }
}

// ─── Admin: guest registration ────────────────────────────────────────────────

#[derive(Deserialize)]
struct RegisterGuestRequest {
    /// Hex-encoded Nostr public key (64 chars).
    pubkey: String,
    /// Comma-separated channel UUIDs this guest can access.
    channels: String,
}

async fn register_guest(
    State(state): State<ProxyState>,
    headers: HeaderMap,
    axum::Json(req): axum::Json<RegisterGuestRequest>,
) -> impl IntoResponse {
    if let Some(err) = check_admin_secret(&state.admin_secret, &headers) {
        return err;
    }

    let pubkey = match PublicKey::from_hex(&req.pubkey) {
        Ok(pk) => pk,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({ "error": format!("invalid pubkey: {e}") })),
            )
                .into_response();
        }
    };

    let channel_ids: Vec<Uuid> = req
        .channels
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();

    if channel_ids.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({ "error": "at least one valid channel UUID required" })),
        )
            .into_response();
    }

    state.guest_store.register(pubkey, channel_ids.clone());
    info!(pubkey = %pubkey, channels = channel_ids.len(), "guest registered");

    (
        StatusCode::OK,
        axum::Json(serde_json::json!({
            "pubkey": req.pubkey,
            "channels": channel_ids,
        })),
    )
        .into_response()
}

#[derive(Deserialize)]
struct RevokeGuestRequest {
    /// Hex-encoded Nostr public key to revoke.
    pubkey: String,
}

async fn revoke_guest(
    State(state): State<ProxyState>,
    headers: HeaderMap,
    axum::Json(req): axum::Json<RevokeGuestRequest>,
) -> impl IntoResponse {
    if let Some(err) = check_admin_secret(&state.admin_secret, &headers) {
        return err;
    }

    let pubkey = match PublicKey::from_hex(&req.pubkey) {
        Ok(pk) => pk,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({ "error": format!("invalid pubkey: {e}") })),
            )
                .into_response();
        }
    };

    let removed = state.guest_store.remove(&pubkey);
    if removed {
        info!(pubkey = %pubkey, "guest revoked");
        (
            StatusCode::OK,
            axum::Json(serde_json::json!({ "revoked": true })),
        )
            .into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({ "error": "pubkey not registered" })),
        )
            .into_response()
    }
}

async fn list_guests(State(state): State<ProxyState>, headers: HeaderMap) -> impl IntoResponse {
    if let Some(err) = check_admin_secret(&state.admin_secret, &headers) {
        return err;
    }

    let guests: Vec<serde_json::Value> = state
        .guest_store
        .all()
        .into_iter()
        .map(|(pk, channels)| {
            serde_json::json!({
                "pubkey": pk.to_hex(),
                "channels": channels,
            })
        })
        .collect();

    (
        StatusCode::OK,
        axum::Json(serde_json::json!({ "guests": guests })),
    )
        .into_response()
}

// ─── Public read-only endpoint ────────────────────────────────────────────────

/// Resource limits for unauthenticated `/public` connections.
/// These prevent DoS from anonymous internet traffic.
const MAX_PUBLIC_CONNECTIONS: usize = 100;
const MAX_PUBLIC_SUBS_PER_CONN: usize = 5;
/// Maximum number of filters allowed in a single REQ from an anonymous client.
/// Prevents expensive multi-filter fan-out queries from unauthenticated callers.
const MAX_PUBLIC_FILTERS_PER_REQ: usize = 3;
/// Maximum raw message size (bytes) accepted from an anonymous client.
/// Rejects oversized payloads before JSON parsing to limit CPU exposure.
const MAX_PUBLIC_MSG_BYTES: usize = 4096;
/// Maximum number of events returned per subscription for anonymous clients.
/// Prevents unbounded backfill queries from unauthenticated callers.
const MAX_PUBLIC_LIMIT: usize = 200;
const PUBLIC_IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
/// Default lifetime cap for public connections (1 hour).
/// Used when `ProxyState::public_lifetime_secs` is 0 or not configured.
pub const DEFAULT_PUBLIC_LIFETIME_SECS: u64 = 3600;

/// Content-negotiate between public NIP-11 JSON and WebSocket upgrade.
/// Mirrors [`root_handler`] but uses the public NIP-11 document and
/// the read-only WebSocket handler.
async fn public_handler(
    State(state): State<ProxyState>,
    headers: HeaderMap,
    req: axum::extract::Request,
) -> Response {
    let wants_nip11 = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("application/nostr+json"))
        .unwrap_or(false);

    if wants_nip11 {
        return public_nip11_response().into_response();
    }

    match WebSocketUpgrade::from_request(req, &state).await {
        Ok(ws) => ws
            .max_message_size(MAX_PUBLIC_MSG_BYTES)
            .max_frame_size(MAX_PUBLIC_MSG_BYTES)
            .on_upgrade(move |socket| handle_public_ws(socket, state)),
        Err(_) => public_nip11_response().into_response(),
    }
}

fn public_nip11_response() -> Response {
    let nip11 = serde_json::json!({
        "name": "sprout-proxy (public)",
        "description": "Sprout NIP-28 public read-only relay — no authentication required",
        "supported_nips": [1, 11, 28],
        "software": "sprout-proxy",
        "version": env!("CARGO_PKG_VERSION"),
        "limitation": {
            "auth_required": false,
            "max_subscriptions": MAX_PUBLIC_SUBS_PER_CONN,
        }
    });
    match serde_json::to_string_pretty(&nip11) {
        Ok(body) => (
            [
                (axum::http::header::CONTENT_TYPE, "application/nostr+json"),
                (axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            ],
            body,
        )
            .into_response(),
        Err(_) => axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

/// Read-only WebSocket handler for the `/public` endpoint.
///
/// Structurally separate from [`handle_ws`] — there is no EVENT branch,
/// no AUTH challenge, no shadow key derivation for the reader, no guest
/// or invite store interaction. The write path does not exist.
async fn handle_public_ws(mut socket: WebSocket, state: ProxyState) {
    // ── Connection cap ────────────────────────────────────────────────────
    // Soft cap — Relaxed ordering is intentional; brief overshoot
    // by concurrent arrivals is acceptable.
    let current = state
        .public_connection_count
        .fetch_add(1, Ordering::Relaxed);
    if current >= MAX_PUBLIC_CONNECTIONS {
        state
            .public_connection_count
            .fetch_sub(1, Ordering::Relaxed);
        let _ = send_relay_msg(
            &mut socket,
            RelayMessage::notice("error: too many public connections — try again later"),
        )
        .await;
        return;
    }

    // Ensure counter is decremented on all exit paths.
    let _guard = PublicConnGuard(state.public_connection_count.clone());

    let conn_prefix = uuid::Uuid::new_v4().simple().to_string()[..8].to_string();
    let prefix_with_sep = format!("{conn_prefix}:");
    let allowed_channels: &[Uuid] = &state.public_channels;
    let mut active_subs: HashSet<String> = HashSet::new();
    let mut upstream_subs: HashSet<String> = HashSet::new();
    let mut upstream_rx = state.upstream_events.subscribe();

    let connected_at = tokio::time::Instant::now();
    let mut last_activity = tokio::time::Instant::now();

    debug!("public client connected");

    let lifetime_secs = if state.public_lifetime_secs > 0 {
        state.public_lifetime_secs
    } else {
        DEFAULT_PUBLIC_LIFETIME_SECS
    };
    let lifetime_cap = std::time::Duration::from_secs(lifetime_secs);

    // ── Main read-only message loop ───────────────────────────────────────
    loop {
        tokio::select! {
            // Lifetime cap — hard deadline enforced inside select! so it fires
            // even when other branches are blocking.
            _ = tokio::time::sleep_until(connected_at + lifetime_cap) => {
                let _ = send_relay_msg(
                    &mut socket,
                    RelayMessage::notice("connection lifetime exceeded — reconnect to continue"),
                )
                .await;
                break;
            }

            // Bidirectional idle timeout — disconnect if no inbound or outbound activity.
            _ = tokio::time::sleep_until(last_activity + PUBLIC_IDLE_TIMEOUT) => {
                let _ = send_relay_msg(
                    &mut socket,
                    RelayMessage::notice("idle timeout — disconnecting"),
                )
                .await;
                break;
            }

            // Inbound from client (read-only: REQ and CLOSE only).
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        last_activity = tokio::time::Instant::now();
                        handle_public_client_message(
                            &mut socket,
                            &state,
                            &text,
                            allowed_channels,
                            &conn_prefix,
                            &mut active_subs,
                            &mut upstream_subs,
                        )
                        .await;
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    // Ping/Pong frames reset the idle timer — many Nostr clients
                    // use WebSocket keepalives to maintain quiet subscriptions.
                    Some(Ok(Message::Ping(_) | Message::Pong(_))) => {
                        last_activity = tokio::time::Instant::now();
                    }
                    // Binary frames are not used in the Nostr protocol.
                    Some(Ok(Message::Binary(_))) => {
                        let _ = send_relay_msg(
                            &mut socket,
                            RelayMessage::notice("error: binary frames not supported"),
                        )
                        .await;
                    }
                    _ => {}
                }
            }

            // Outbound from upstream relay — translate and filter.
            upstream = upstream_rx.recv() => {
                match upstream {
                    Ok(text) => {
                        match RelayMessage::from_json(&text) {
                            Ok(RelayMessage::Event { subscription_id, event }) => {
                                let sub_str = subscription_id.to_string();
                                let Some(client_sub) = sub_str.strip_prefix(&prefix_with_sep) else {
                                    continue;
                                };
                                let client_sub_id = SubscriptionId::new(client_sub);
                                match state
                                    .translator
                                    .translate_outbound(&event, allowed_channels)
                                    .await
                                {
                                    Ok(Some(translated)) => {
                                        let out = RelayMessage::event(client_sub_id, translated);
                                        if socket.send(Message::Text(out.as_json().into())).await.is_ok() {
                                            last_activity = tokio::time::Instant::now();
                                        } else {
                                            break;
                                        }
                                    }
                                    Ok(None) => {}
                                    Err(e) => {
                                        debug!(error = %e, "dropping upstream event (not in public scope)");
                                    }
                                }
                            }
                            Ok(RelayMessage::EndOfStoredEvents(ref sub_id)) => {
                                let sub_str = sub_id.to_string();
                                if let Some(client_sub) = sub_str.strip_prefix(&prefix_with_sep) {
                                    let client_sub_id = SubscriptionId::new(client_sub);
                                    let out = RelayMessage::eose(client_sub_id);
                                    if socket.send(Message::Text(out.as_json().into())).await.is_ok() {
                                        last_activity = tokio::time::Instant::now();
                                    } else {
                                        break;
                                    }
                                }
                            }
                            Ok(RelayMessage::Closed { ref subscription_id, ref message }) => {
                                let sub_str = subscription_id.to_string();
                                if let Some(client_sub) = sub_str.strip_prefix(&prefix_with_sep) {
                                    // Clean up tracking — upstream killed this sub,
                                    // so free the slot before forwarding to client.
                                    active_subs.remove(&sub_str);
                                    upstream_subs.remove(&sub_str);
                                    let client_sub_id = SubscriptionId::new(client_sub);
                                    let out = RelayMessage::closed(client_sub_id, message.clone());
                                    if socket.send(Message::Text(out.as_json().into())).await.is_ok() {
                                        last_activity = tokio::time::Instant::now();
                                    } else {
                                        break;
                                    }
                                }
                            }
                            // Public readers never send events, so no pending_oks to route.
                            // Drop NOTICE, AUTH, OK, and other control-plane messages.
                            Ok(_) => {}
                            Err(_) => {}
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        warn!(skipped = n, "public client: upstream broadcast lagged");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        error!("upstream broadcast channel closed");
                        break;
                    }
                }
            }
        }
    }

    // Clean up upstream subscriptions on disconnect.
    for prefixed_sub in upstream_subs {
        let sub_id = SubscriptionId::new(prefixed_sub);
        if let Err(e) = state.upstream.send_close(sub_id).await {
            warn!("upstream send_close on public disconnect failed: {e}");
        }
    }

    debug!("public client disconnected");
}

/// Read-only client message handler. Handles REQ and CLOSE only.
/// EVENT and all other message types are rejected.
async fn handle_public_client_message(
    socket: &mut WebSocket,
    state: &ProxyState,
    raw_msg: &str,
    allowed_channels: &[Uuid],
    conn_prefix: &str,
    active_subs: &mut HashSet<String>,
    upstream_subs: &mut HashSet<String>,
) {
    // Reject oversized messages before JSON parsing to limit CPU exposure.
    if raw_msg.len() > MAX_PUBLIC_MSG_BYTES {
        let _ = send_relay_msg(socket, RelayMessage::notice("error: message too large")).await;
        return;
    }

    let msg = match ClientMessage::from_json(raw_msg) {
        Ok(m) => m,
        Err(_) => {
            let _ = send_relay_msg(socket, RelayMessage::notice("error: invalid message")).await;
            return;
        }
    };

    match msg {
        ClientMessage::Req {
            subscription_id,
            filters,
        } => {
            // Enforce per-connection subscription limit.
            // Track all sub IDs (including local-only) to prevent bypass via kind:40/41 REQs.
            let prefixed = format!("{conn_prefix}:{}", subscription_id);
            if active_subs.len() >= MAX_PUBLIC_SUBS_PER_CONN && !active_subs.contains(&prefixed) {
                let _ = send_relay_msg(
                    socket,
                    RelayMessage::closed(
                        subscription_id,
                        "error: too many subscriptions — close one first",
                    ),
                )
                .await;
                return;
            }
            // Reject REQs with too many filters to prevent expensive fan-out queries.
            if filters.len() > MAX_PUBLIC_FILTERS_PER_REQ {
                let _ = send_relay_msg(
                    socket,
                    RelayMessage::closed(
                        subscription_id,
                        "error: too many filters — max 3 per REQ",
                    ),
                )
                .await;
                return;
            }
            // Clamp each filter's limit to prevent unbounded backfill from
            // anonymous clients.
            let clamped_filters: Vec<Filter> = filters
                .into_iter()
                .map(|mut f| {
                    let current = f.limit.unwrap_or(MAX_PUBLIC_LIMIT);
                    f.limit = Some(current.min(MAX_PUBLIC_LIMIT));
                    f
                })
                .collect();
            handle_req(
                socket,
                state,
                subscription_id,
                clamped_filters,
                allowed_channels,
                conn_prefix,
                active_subs,
                upstream_subs,
            )
            .await;
        }
        ClientMessage::Close(sub_id) => {
            let prefixed = format!("{conn_prefix}:{}", sub_id);
            active_subs.remove(&prefixed);
            // Only send upstream CLOSE for subs that had upstream REQs.
            if upstream_subs.remove(&prefixed) {
                let prefixed_sub_id = SubscriptionId::new(prefixed);
                if let Err(e) = state.upstream.send_close(prefixed_sub_id).await {
                    warn!("upstream send_close failed: {e}");
                }
            }
        }
        ClientMessage::Event(event) => {
            // Structurally reject — this endpoint is read-only.
            let _ = send_relay_msg(
                socket,
                RelayMessage::ok(event.id, false, "restricted: read-only access"),
            )
            .await;
        }
        // AUTH on a public endpoint is meaningless — ignore silently.
        ClientMessage::Auth(_) => {}
        _ => {}
    }
}

/// RAII guard that decrements the public connection counter on drop.
struct PublicConnGuard(Arc<std::sync::atomic::AtomicUsize>);

impl Drop for PublicConnGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
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
        let guest_store = Arc::new(GuestStore::new());
        let invite_store = Arc::new(InviteStore::new());
        let (upstream_events, _) = broadcast::channel(16);
        let shadow_keys = Arc::new(
            crate::shadow_keys::ShadowKeyManager::new(b"test-salt-server-tests")
                .expect("shadow key manager"),
        );
        let translator = Arc::new(crate::translate::Translator::new(
            shadow_keys,
            channel_map.clone(),
            "http://localhost:3000",
            "sprout_test",
            "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
        ));
        let upstream = Arc::new(UpstreamClient::new("ws://localhost:3000", "sprout_test"));
        ProxyState {
            channel_map,
            guest_store,
            invite_store,
            translator,
            upstream,
            upstream_events,
            admin_secret: None,
            relay_url: "ws://127.0.0.1:4869".to_string(),
            public_channels: Arc::new(Vec::new()),
            public_connection_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            public_lifetime_secs: 3600,
        }
    }

    /// Like `make_state()` but registers one channel and includes it in
    /// `public_channels`, so the `/public` route is registered by `router()`.
    fn make_state_with_public_channel() -> (ProxyState, Uuid) {
        let keys = Keys::generate();
        let channel_map = Arc::new(crate::channel_map::ChannelMap::new(keys.clone()));
        let guest_store = Arc::new(GuestStore::new());
        let invite_store = Arc::new(InviteStore::new());
        let (upstream_events, _) = broadcast::channel(16);
        let shadow_keys = Arc::new(
            crate::shadow_keys::ShadowKeyManager::new(b"test-salt-server-tests")
                .expect("shadow key manager"),
        );
        let translator = Arc::new(crate::translate::Translator::new(
            shadow_keys,
            channel_map.clone(),
            "http://localhost:3000",
            "sprout_test",
            "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
        ));
        let upstream = Arc::new(UpstreamClient::new("ws://localhost:3000", "sprout_test"));

        // Register a test channel so the map has something to serve.
        let dto = crate::channel_map::ChannelDto {
            id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            name: "test-public-channel".to_string(),
            created_at: "2026-01-15T12:00:00Z".to_string(),
            visibility: "open".to_string(),
            description: "A test public channel".to_string(),
            created_by: "0101010101010101010101010101010101010101010101010101010101010101"
                .to_string(),
        };
        channel_map.register(&dto).expect("register must succeed");
        let uuid: Uuid = "550e8400-e29b-41d4-a716-446655440000".parse().unwrap();

        let state = ProxyState {
            channel_map,
            guest_store,
            invite_store,
            translator,
            upstream,
            upstream_events,
            admin_secret: None,
            relay_url: "ws://127.0.0.1:4869".to_string(),
            public_channels: Arc::new(vec![uuid]),
            public_connection_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            public_lifetime_secs: 3600,
        };
        (state, uuid)
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
    fn public_nip11_json_is_valid() {
        // Verify the public NIP-11 response serializes without panic
        // and contains expected fields.
        let resp = public_nip11_response().into_response();
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "application/nostr+json"
        );
    }

    #[test]
    fn public_nip11_contains_expected_fields() {
        let resp = public_nip11_response().into_response();
        let body = resp.into_body();
        let bytes = tokio::runtime::Runtime::new().unwrap().block_on(async {
            use http_body_util::BodyExt;
            body.collect().await.unwrap().to_bytes()
        });
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["name"], "sprout-proxy (public)");
        assert_eq!(json["supported_nips"], serde_json::json!([1, 11, 28]));
        assert_eq!(json["limitation"]["auth_required"], false);
        assert_eq!(
            json["limitation"]["max_subscriptions"],
            MAX_PUBLIC_SUBS_PER_CONN
        );
        assert!(json["version"].is_string());
    }

    #[test]
    fn public_conn_guard_decrements() {
        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(5));
        {
            let _guard = PublicConnGuard(counter.clone());
            assert_eq!(counter.load(std::sync::atomic::Ordering::Relaxed), 5);
        }
        // Guard dropped — counter should be decremented
        assert_eq!(counter.load(std::sync::atomic::Ordering::Relaxed), 4);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn public_constants_are_sane() {
        // Verify DoS limits are within reasonable bounds
        assert!(MAX_PUBLIC_CONNECTIONS > 0 && MAX_PUBLIC_CONNECTIONS <= 1000);
        assert!(MAX_PUBLIC_SUBS_PER_CONN > 0 && MAX_PUBLIC_SUBS_PER_CONN <= 50);
        assert!(MAX_PUBLIC_FILTERS_PER_REQ > 0 && MAX_PUBLIC_FILTERS_PER_REQ <= 20);
        assert!(MAX_PUBLIC_MSG_BYTES >= 1024 && MAX_PUBLIC_MSG_BYTES <= 65536);
        assert!(PUBLIC_IDLE_TIMEOUT.as_secs() >= 30);
    }

    #[test]
    fn public_router_includes_public_route() {
        // Verify the /public route is registered when public_channels is non-empty
        let (state, _uuid) = make_state_with_public_channel();
        let app = router(state);
        // Build a NIP-11 request to /public
        let req = axum::http::Request::builder()
            .uri("/public")
            .header("accept", "application/nostr+json")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = tokio::runtime::Runtime::new().unwrap().block_on(async {
            use tower::util::ServiceExt;
            app.oneshot(req).await.unwrap()
        });
        assert_eq!(resp.status(), 200);
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "application/nostr+json"
        );
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn public_msg_size_limit_is_reasonable() {
        // MAX_PUBLIC_MSG_BYTES should be large enough for valid REQ/CLOSE
        // messages but small enough to reject abuse.
        // A typical REQ: ["REQ","sub1",{"kinds":[42],"limit":100}] is ~45 bytes.
        // 4096 bytes is generous for 3 filters with complex conditions.
        assert!(MAX_PUBLIC_MSG_BYTES >= 1024, "too small for valid REQs");
        assert!(
            MAX_PUBLIC_MSG_BYTES <= 8192,
            "too large — increases CPU exposure"
        );
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn public_max_limit_is_bounded() {
        // Verify the replay cap exists and is reasonable.
        // MAX_PUBLIC_LIMIT prevents unbounded backfill from anonymous clients.
        assert!(MAX_PUBLIC_LIMIT > 0, "must allow some results");
        assert!(
            MAX_PUBLIC_LIMIT <= 500,
            "too high — enables expensive backfills"
        );
    }

    #[test]
    fn public_endpoint_rejects_non_websocket_post() {
        let (state, _uuid) = make_state_with_public_channel();
        let app = router(state);
        // POST to /public should get rejected (not a GET)
        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/public")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = tokio::runtime::Runtime::new().unwrap().block_on(async {
            use tower::util::ServiceExt;
            app.oneshot(req).await.unwrap()
        });
        // Should return 405 Method Not Allowed (axum rejects non-GET on get() routes)
        assert_eq!(resp.status().as_u16(), 405);
    }

    #[test]
    fn public_nip11_cors_header() {
        let (state, _uuid) = make_state_with_public_channel();
        let app = router(state);
        let req = axum::http::Request::builder()
            .uri("/public")
            .header("accept", "application/nostr+json")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = tokio::runtime::Runtime::new().unwrap().block_on(async {
            use tower::util::ServiceExt;
            app.oneshot(req).await.unwrap()
        });
        assert_eq!(resp.status(), 200);
        assert_eq!(
            resp.headers().get("access-control-allow-origin").unwrap(),
            "*"
        );
    }

    #[test]
    fn public_route_not_registered_when_no_channels() {
        // When public_channels is empty, /public should not be registered.
        let state = make_state();
        let app = router(state);
        let req = axum::http::Request::builder()
            .uri("/public")
            .header("accept", "application/nostr+json")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = tokio::runtime::Runtime::new().unwrap().block_on(async {
            use tower::util::ServiceExt;
            app.oneshot(req).await.unwrap()
        });
        // /public is not registered → 404
        assert_eq!(resp.status().as_u16(), 404);
    }

    #[test]
    fn channel_isolation_public_scope() {
        // Core security invariant: events from non-public channels must not
        // be visible through the public endpoint's allowed_channels scope.
        let keys = Keys::generate();
        let map = crate::channel_map::ChannelMap::new(keys);

        // Register two channels — one public, one private.
        let dto_public = crate::channel_map::ChannelDto {
            id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            name: "public-channel".to_string(),
            created_at: "2026-01-15T12:00:00Z".to_string(),
            visibility: "open".to_string(),
            description: "Public".to_string(),
            created_by: "0101010101010101010101010101010101010101010101010101010101010101"
                .to_string(),
        };
        let dto_private = crate::channel_map::ChannelDto {
            id: "660e8400-e29b-41d4-a716-446655440001".to_string(),
            name: "private-channel".to_string(),
            created_at: "2026-01-15T12:00:00Z".to_string(),
            visibility: "open".to_string(),
            description: "Private".to_string(),
            created_by: "0202020202020202020202020202020202020202020202020202020202020202"
                .to_string(),
        };
        map.register(&dto_public).expect("register public");
        map.register(&dto_private).expect("register private");

        let public_uuid: Uuid = "550e8400-e29b-41d4-a716-446655440000".parse().unwrap();
        let private_uuid: Uuid = "660e8400-e29b-41d4-a716-446655440001".parse().unwrap();
        let map = Arc::new(map);

        // Query with only the public channel in scope (simulating /public endpoint).
        let filter = Filter::new().kind(Kind::ChannelCreation);
        let public_scope = vec![public_uuid];
        let events = collect_local_events(&filter, &map, &public_scope);

        // Must see exactly one channel — the public one.
        assert_eq!(events.len(), 1, "public scope must yield exactly 1 channel");

        // Verify it's the public channel, not the private one.
        let content: serde_json::Value = serde_json::from_str(&events[0].content).unwrap();
        // kind:40 uses the UUID as the "name" field (display name is in kind:41).
        assert_eq!(
            content["name"], "550e8400-e29b-41d4-a716-446655440000",
            "public scope must only expose the public channel"
        );

        // Query with both channels in scope (simulating authenticated access).
        let full_scope = vec![public_uuid, private_uuid];
        let all_events = collect_local_events(&filter, &map, &full_scope);
        assert_eq!(
            all_events.len(),
            2,
            "authenticated scope must see both channels"
        );
    }

    #[test]
    fn public_default_lifetime_is_one_hour() {
        assert_eq!(DEFAULT_PUBLIC_LIFETIME_SECS, 3600);
    }

    #[test]
    fn default_hours_and_max_uses() {
        assert_eq!(default_hours(), 24);
        // FIX 7: default max_uses is now 10
        assert_eq!(default_max_uses(), 10);
    }

    #[test]
    fn constant_time_eq_works() {
        assert!(constant_time_eq("hello", "hello"));
        assert!(!constant_time_eq("hello", "world"));
        assert!(!constant_time_eq("hello", "hell"));
        assert!(!constant_time_eq("", "a"));
        assert!(constant_time_eq("", ""));
        // Ensure different-length strings with same prefix don't match
        assert!(!constant_time_eq("abc", "abcd"));
    }

    // ── split_filters tests ──────────────────────────────────────────────

    #[test]
    fn split_filters_pure_local() {
        // kind:40 is the only pure-local kind (channel creation is synthesized).
        let f = Filter::new().kind(Kind::ChannelCreation);
        let (local, upstream) = split_filters(&[f]);
        assert_eq!(local.len(), 1);
        assert!(upstream.is_empty(), "kind:40 is local-only");
    }

    #[test]
    fn split_filters_kind41_goes_both() {
        // kind:41 goes to BOTH local (synthesized metadata) and upstream (edits).
        let f = Filter::new().kind(Kind::ChannelMetadata);
        let (local, upstream) = split_filters(&[f]);
        assert_eq!(local.len(), 1, "kind:41 must produce a local filter");
        assert_eq!(
            upstream.len(),
            1,
            "kind:41 must also produce an upstream filter"
        );
    }

    #[test]
    fn split_filters_pure_upstream() {
        let f = Filter::new().kind(Kind::Custom(42));
        let (local, upstream) = split_filters(&[f]);
        assert!(local.is_empty());
        assert_eq!(upstream.len(), 1);
    }

    #[test]
    fn split_filters_mixed_kind() {
        let f = Filter::new().kinds([Kind::ChannelCreation, Kind::Custom(42)]);
        let (local, upstream) = split_filters(&[f]);
        assert_eq!(local.len(), 1, "mixed filter must produce a local portion");
        assert_eq!(
            upstream.len(),
            1,
            "mixed filter must produce an upstream portion"
        );
        let local_k: Vec<u16> = local[0]
            .kinds
            .as_ref()
            .unwrap()
            .iter()
            .map(|k| k.as_u16())
            .collect();
        assert!(local_k.contains(&40));
        assert!(!local_k.contains(&42));
        let up_k: Vec<u16> = upstream[0]
            .kinds
            .as_ref()
            .unwrap()
            .iter()
            .map(|k| k.as_u16())
            .collect();
        assert!(up_k.contains(&42));
        assert!(!up_k.contains(&40));
    }

    #[test]
    fn split_filters_no_kind_duplicates() {
        let f = Filter::new();
        let (local, upstream) = split_filters(&[f]);
        assert_eq!(local.len(), 1);
        assert_eq!(upstream.len(), 1);
        let local_k: Vec<u16> = local[0]
            .kinds
            .as_ref()
            .unwrap()
            .iter()
            .map(|k| k.as_u16())
            .collect();
        assert!(local_k.contains(&40));
        assert!(local_k.contains(&41));
        assert!(upstream[0].kinds.is_none());
    }

    // ── collect_local_events tests ───────────────────────────────────────

    fn make_channel_map_with_channel() -> (Arc<ChannelMap>, Uuid) {
        let keys = Keys::generate();
        let map = ChannelMap::new(keys);
        let dto = crate::channel_map::ChannelDto {
            id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            name: "test-channel".to_string(),
            created_at: "2026-01-15T12:00:00Z".to_string(),
            visibility: "open".to_string(),
            description: "A test channel".to_string(),
            created_by: "0101010101010101010101010101010101010101010101010101010101010101"
                .to_string(),
        };
        map.register(&dto).expect("register must succeed");
        let uuid: Uuid = "550e8400-e29b-41d4-a716-446655440000".parse().unwrap();
        (Arc::new(map), uuid)
    }

    #[test]
    fn collect_local_kind40_basic() {
        let (map, uuid) = make_channel_map_with_channel();
        let filter = Filter::new().kind(Kind::ChannelCreation);
        let events = collect_local_events(&filter, &map, &[uuid]);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind.as_u16(), 40);
    }

    #[test]
    fn collect_local_kind41_basic() {
        let (map, uuid) = make_channel_map_with_channel();
        let filter = Filter::new().kind(Kind::ChannelMetadata);
        let events = collect_local_events(&filter, &map, &[uuid]);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind.as_u16(), 41);
    }

    #[test]
    fn collect_local_both_kinds() {
        let (map, uuid) = make_channel_map_with_channel();
        let filter = Filter::new().kinds([Kind::ChannelCreation, Kind::ChannelMetadata]);
        let events = collect_local_events(&filter, &map, &[uuid]);
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn collect_local_authors_filter_matches() {
        let (map, uuid) = make_channel_map_with_channel();
        let server_pk = map.server_keys().public_key();
        let filter = Filter::new().kind(Kind::ChannelCreation).author(server_pk);
        let events = collect_local_events(&filter, &map, &[uuid]);
        assert_eq!(events.len(), 1, "server pubkey must match");
    }

    #[test]
    fn collect_local_authors_filter_rejects() {
        let (map, uuid) = make_channel_map_with_channel();
        let random_pk = Keys::generate().public_key();
        let filter = Filter::new().kind(Kind::ChannelCreation).author(random_pk);
        let events = collect_local_events(&filter, &map, &[uuid]);
        assert!(events.is_empty(), "random pubkey must not match");
    }

    #[test]
    fn collect_local_channel_not_in_scope() {
        let (map, _uuid) = make_channel_map_with_channel();
        let other: Uuid = "00000000-0000-0000-0000-000000000001".parse().unwrap();
        let filter = Filter::new().kind(Kind::ChannelCreation);
        let events = collect_local_events(&filter, &map, &[other]);
        assert!(events.is_empty());
    }

    #[test]
    fn collect_local_limit_respected() {
        let (map, uuid) = make_channel_map_with_channel();
        let filter = Filter::new()
            .kinds([Kind::ChannelCreation, Kind::ChannelMetadata])
            .limit(1);
        let events = collect_local_events(&filter, &map, &[uuid]);
        assert_eq!(events.len(), 1, "limit:1 must cap at 1 event");
    }

    #[test]
    fn collect_local_since_excludes() {
        let (map, uuid) = make_channel_map_with_channel();
        // Channel created 2026-01-15T12:00:00Z. Since after that → empty.
        let filter = Filter::new()
            .kind(Kind::ChannelCreation)
            .since(Timestamp::from(1768478401u64));
        let events = collect_local_events(&filter, &map, &[uuid]);
        assert!(events.is_empty());
    }

    #[test]
    fn collect_local_until_excludes() {
        let (map, uuid) = make_channel_map_with_channel();
        let filter = Filter::new()
            .kind(Kind::ChannelCreation)
            .until(Timestamp::from(1000000000u64));
        let events = collect_local_events(&filter, &map, &[uuid]);
        assert!(events.is_empty());
    }

    // ── WebSocket behavioral tests for /public endpoint ─────────────────

    /// Start the proxy router on a random port and return the bound address.
    /// Uses `make_state_with_public_channel()` so the `/public` route is registered.
    async fn start_test_server() -> std::net::SocketAddr {
        let (state, _uuid) = make_state_with_public_channel();
        let app = router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind to random port");
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.ok();
        });
        addr
    }

    /// Connect to the /public WebSocket endpoint and return the stream.
    async fn connect_public(
        addr: std::net::SocketAddr,
    ) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>
    {
        let url = format!("ws://{addr}/public");
        let (ws, _resp) = tokio_tungstenite::connect_async(&url)
            .await
            .expect("WebSocket connect to /public");
        ws
    }

    /// Read the next text frame, with a timeout to prevent hanging tests.
    async fn read_text(
        ws: &mut tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    ) -> String {
        use futures_util::StreamExt;
        let msg = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
            .await
            .expect("read timed out")
            .expect("stream ended")
            .expect("read error");
        match msg {
            tokio_tungstenite::tungstenite::Message::Text(t) => t.to_string(),
            other => panic!("expected Text frame, got {other:?}"),
        }
    }

    /// Send a text frame.
    async fn send_text(
        ws: &mut tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        msg: &str,
    ) {
        use futures_util::SinkExt;
        ws.send(tokio_tungstenite::tungstenite::Message::Text(msg.into()))
            .await
            .expect("send failed");
    }

    #[tokio::test]
    async fn public_ws_rejects_event() {
        let addr = start_test_server().await;
        let mut ws = connect_public(addr).await;

        // Build a valid EVENT message.
        let keys = Keys::generate();
        let event = EventBuilder::text_note("hello", [])
            .sign_with_keys(&keys)
            .unwrap();
        let client_msg = ClientMessage::event(event);
        send_text(&mut ws, &client_msg.as_json()).await;

        let resp = read_text(&mut ws).await;
        let relay_msg: serde_json::Value = serde_json::from_str(&resp).unwrap();
        // Expect ["OK", <event_id>, false, "restricted: read-only access"]
        assert_eq!(relay_msg[0], "OK");
        assert_eq!(relay_msg[2], false);
        assert!(
            relay_msg[3].as_str().unwrap().contains("read-only"),
            "expected read-only rejection, got: {resp}"
        );
    }

    #[tokio::test]
    async fn public_ws_rejects_oversized_message() {
        use futures_util::StreamExt;

        let addr = start_test_server().await;
        let mut ws = connect_public(addr).await;

        // Send a message larger than MAX_PUBLIC_MSG_BYTES (4096).
        // The WebSocket layer now enforces max_message_size / max_frame_size,
        // so the server resets the connection before the app layer sees the
        // payload — no NOTICE is sent; the stream simply closes.
        let oversized = "x".repeat(MAX_PUBLIC_MSG_BYTES + 1);
        send_text(&mut ws, &oversized).await;

        let result = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
            .await
            .expect("read timed out");

        // The connection must be terminated — either a clean Close frame or a
        // protocol-level reset.  Any non-error outcome that is not a Close
        // frame is unexpected.
        match result {
            None => {}                                                        // stream ended cleanly
            Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) => {} // clean close
            Some(Err(_)) => {} // protocol reset — acceptable
            Some(Ok(other)) => panic!("expected connection close, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn public_ws_enforces_subscription_limit() {
        let addr = start_test_server().await;
        let mut ws = connect_public(addr).await;

        // Use kind:40 (ChannelCreation) — handled locally, so EOSE comes back
        // immediately without needing an upstream relay.
        for i in 0..MAX_PUBLIC_SUBS_PER_CONN {
            let req = format!(r#"["REQ","sub-{i}",{{"kinds":[40],"limit":0}}]"#,);
            send_text(&mut ws, &req).await;
            // Drain the EOSE response.
            let eose = read_text(&mut ws).await;
            assert!(
                eose.contains("EOSE"),
                "expected EOSE for sub-{i}, got: {eose}"
            );
        }

        // The next subscription should be rejected before it reaches handle_req.
        let overflow_req = r#"["REQ","sub-overflow",{"kinds":[40],"limit":0}]"#;
        send_text(&mut ws, overflow_req).await;

        let resp = read_text(&mut ws).await;
        let relay_msg: serde_json::Value = serde_json::from_str(&resp).unwrap();
        // Expect ["CLOSED", "sub-overflow", "error: too many subscriptions ..."]
        assert_eq!(relay_msg[0], "CLOSED");
        assert_eq!(relay_msg[1], "sub-overflow");
        assert!(
            relay_msg[2]
                .as_str()
                .unwrap()
                .contains("too many subscriptions"),
            "expected sub limit rejection, got: {resp}"
        );
    }

    #[tokio::test]
    async fn public_ws_enforces_filter_limit() {
        let addr = start_test_server().await;
        let mut ws = connect_public(addr).await;

        // Build a REQ with more filters than MAX_PUBLIC_FILTERS_PER_REQ (3).
        // Uses kind:40 (local-only) so no upstream dependency.
        let filters: Vec<String> = (0..=MAX_PUBLIC_FILTERS_PER_REQ)
            .map(|_| r#"{"kinds":[40],"limit":0}"#.to_string())
            .collect();
        let req = format!(r#"["REQ","too-many-filters",{}]"#, filters.join(","));
        send_text(&mut ws, &req).await;

        let resp = read_text(&mut ws).await;
        let relay_msg: serde_json::Value = serde_json::from_str(&resp).unwrap();
        // Expect ["CLOSED", "too-many-filters", "error: too many filters ..."]
        assert_eq!(relay_msg[0], "CLOSED");
        assert_eq!(relay_msg[1], "too-many-filters");
        assert!(
            relay_msg[2].as_str().unwrap().contains("too many filters"),
            "expected filter limit rejection, got: {resp}"
        );
    }

    #[tokio::test]
    async fn public_ws_close_frees_subscription_slot() {
        let addr = start_test_server().await;
        let mut ws = connect_public(addr).await;

        // Fill all subscription slots with kind:40 (local-only, no upstream needed).
        for i in 0..MAX_PUBLIC_SUBS_PER_CONN {
            let req = format!(r#"["REQ","sub-{i}",{{"kinds":[40],"limit":0}}]"#);
            send_text(&mut ws, &req).await;
            let eose = read_text(&mut ws).await;
            assert!(eose.contains("EOSE"), "expected EOSE, got: {eose}");
        }

        // Close one subscription.
        send_text(&mut ws, r#"["CLOSE","sub-0"]"#).await;

        // Small delay to let the server process the CLOSE.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Now a new subscription should succeed.
        let req = r#"["REQ","sub-replacement",{"kinds":[40],"limit":0}]"#;
        send_text(&mut ws, req).await;

        let resp = read_text(&mut ws).await;
        // Should get EOSE, not CLOSED error.
        assert!(
            resp.contains("EOSE"),
            "expected EOSE after freeing a slot, got: {resp}"
        );
    }

    #[tokio::test]
    async fn public_ws_invalid_json_returns_notice() {
        let addr = start_test_server().await;
        let mut ws = connect_public(addr).await;

        send_text(&mut ws, "this is not json").await;

        let resp = read_text(&mut ws).await;
        let relay_msg: serde_json::Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(relay_msg[0], "NOTICE");
        assert!(
            relay_msg[1].as_str().unwrap().contains("invalid"),
            "expected invalid message notice, got: {resp}"
        );
    }

    // ── Regression test infrastructure ──────────────────────────────────

    /// Start a test server and return the address, broadcast sender, and
    /// upstream client. The broadcast sender lets tests inject upstream relay
    /// messages (EVENT, EOSE, CLOSED). The upstream client lets tests discover
    /// prefixed subscription IDs via `active_sub_ids()`.
    async fn start_test_server_with_upstream() -> (
        std::net::SocketAddr,
        broadcast::Sender<String>,
        Arc<crate::upstream::UpstreamClient>,
    ) {
        let (state, _uuid) = make_state_with_public_channel();
        let events_tx = state.upstream_events.clone();
        let upstream = state.upstream.clone();
        let app = router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind to random port");
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.ok();
        });
        (addr, events_tx, upstream)
    }

    /// Regression test for the upstream CLOSED subscription slot leak.
    ///
    /// Bug: when upstream sends CLOSED for a subscription, the handler forwarded
    /// it to the client but didn't remove the sub from `active_subs` or
    /// `upstream_subs`. The dead sub permanently counted against the 5-slot limit.
    ///
    /// Fix: remove from both tracking sets before forwarding CLOSED to client.
    #[tokio::test]
    async fn public_ws_upstream_closed_frees_subscription_slot() {
        let (addr, events_tx, upstream) = start_test_server_with_upstream().await;
        let mut ws = connect_public(addr).await;

        // Fill 4 of 5 slots with local-only subs (kind:40 → immediate EOSE).
        for i in 0..4 {
            let req = format!(r#"["REQ","local-{i}",{{"kinds":[40],"limit":0}}]"#);
            send_text(&mut ws, &req).await;
            let eose = read_text(&mut ws).await;
            assert!(
                eose.contains("EOSE"),
                "expected EOSE for local-{i}, got: {eose}"
            );
        }

        // Fill the 5th slot with an upstream sub (kind:42).
        // This goes to the upstream relay, so no EOSE comes back in tests.
        send_text(
            &mut ws,
            r#"["REQ","upstream-sub",{"kinds":[42],"limit":0}]"#,
        )
        .await;

        // Wait briefly for the server to process the REQ and register the upstream sub.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Discover the prefixed sub ID from the upstream client's tracking.
        let sub_ids = upstream.active_sub_ids();
        let prefixed = sub_ids
            .iter()
            .find(|id| id.ends_with(":upstream-sub"))
            .expect("upstream sub should be tracked")
            .clone();

        // Inject a CLOSED from "upstream" through the broadcast channel.
        let closed_msg = RelayMessage::closed(
            SubscriptionId::new(&prefixed),
            "subscription closed by relay",
        );
        events_tx
            .send(closed_msg.as_json())
            .expect("broadcast send");

        // The client should receive the CLOSED (with prefix stripped).
        let resp = read_text(&mut ws).await;
        let relay_msg: serde_json::Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(relay_msg[0], "CLOSED", "expected CLOSED, got: {resp}");
        assert_eq!(relay_msg[1], "upstream-sub");

        // Now the 5th slot should be freed. Open a new local sub — should succeed.
        send_text(
            &mut ws,
            r#"["REQ","replacement-sub",{"kinds":[40],"limit":0}]"#,
        )
        .await;
        let resp = read_text(&mut ws).await;
        assert!(
            resp.contains("EOSE"),
            "expected EOSE after upstream CLOSED freed the slot, got: {resp}"
        );
    }
}
