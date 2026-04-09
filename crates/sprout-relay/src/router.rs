//! axum routers — app (WebSocket + REST), health (K8s probes), metrics (Prometheus).

use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::{
    extract::{ConnectInfo, FromRequest, State, WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    middleware,
    response::{IntoResponse, Json},
    routing::{delete, get, post, put},
    Router,
};
use serde_json::json;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

use crate::api;
use crate::api::tokens;
use crate::audio;
use crate::connection::handle_connection;
use crate::metrics::track_metrics;
use crate::nip11::{relay_info_handler, RelayInfo};
use crate::state::AppState;

/// Build the axum [`Router`] with all relay routes, middleware, and CORS configuration.
///
/// Uses a dual sub-router pattern so media routes can carry a 50 MB body limit
/// while all other routes remain capped at 1 MB.  Each sub-router attaches its
/// own `RequestBodyLimitLayer` before merging; the outer layer adds tracing and
/// CORS once over the combined router.
pub fn build_router(state: Arc<AppState>) -> Router {
    // ── Media routes: body limit covers both images and video ────────────────
    // Transport cap is the larger of image and video limits. Video uploads stream
    // to disk (never fully buffered); images collect to bytes within this limit.
    // Per-MIME app-level limits (GIF: 10 MB) are enforced in sprout-media
    // validation after MIME detection.
    let media_body_limit = state
        .config
        .media
        .max_image_bytes
        .max(state.config.media.max_video_bytes) as usize;
    let media_router = Router::new()
        .route("/media/upload", put(api::media::upload_blob))
        .route(
            "/media/{sha256_ext}",
            get(api::media::get_blob).head(api::media::head_blob),
        )
        .layer(RequestBodyLimitLayer::new(media_body_limit))
        .with_state(state.clone());

    // ── All other routes: 1 MB body limit ────────────────────────────────────
    let api_router = Router::new()
        .route("/", get(nip11_or_ws_handler))
        .route("/info", get(relay_info_handler))
        .route("/.well-known/nostr.json", get(api::nip05::nostr_nip05))
        // Health endpoints remain on the app router for backward compat (local dev).
        // In CAKE, probes hit the dedicated health port (8080) instead.
        .route("/health", get(health_handler))
        .route("/_liveness", get(liveness_handler))
        .route("/_readiness", get(readiness_handler))
        // Token self-service routes
        .route(
            "/api/tokens",
            post(tokens::post_tokens)
                .get(tokens::get_tokens)
                .delete(tokens::delete_all_tokens),
        )
        .route("/api/tokens/{id}", delete(tokens::delete_token))
        .route("/api/channels", get(api::channels_handler))
        .route("/api/events", post(api::events::submit_event))
        .route("/api/events/{id}", get(api::get_event))
        .route("/api/search", get(api::search_handler))
        .route("/api/agents", get(api::agents_handler))
        .route(
            "/api/presence",
            get(api::presence_handler).put(api::set_presence_handler),
        )
        // Workflow routes
        .route(
            "/api/channels/{channel_id}/workflows",
            get(api::list_channel_workflows).post(api::create_workflow),
        )
        .route(
            "/api/workflows/{id}",
            get(api::get_workflow)
                .put(api::update_workflow)
                .delete(api::delete_workflow),
        )
        .route("/api/workflows/{id}/runs", get(api::list_workflow_runs))
        .route(
            "/api/workflows/{id}/runs/{run_id}/approvals",
            get(api::list_run_approvals),
        )
        .route("/api/workflows/{id}/trigger", post(api::trigger_workflow))
        .route("/api/workflows/{id}/webhook", post(api::workflow_webhook))
        .route("/api/approvals/{token}/grant", post(api::grant_approval))
        .route("/api/approvals/{token}/deny", post(api::deny_approval))
        .route(
            "/api/approvals/by-hash/{hash}/grant",
            post(api::grant_approval_by_hash),
        )
        .route(
            "/api/approvals/by-hash/{hash}/deny",
            post(api::deny_approval_by_hash),
        )
        // Huddle audio WebSocket route
        .route(
            "/huddle/{channel_id}/audio",
            get(audio::handler::ws_audio_handler),
        )
        // Membership routes
        .route("/api/channels/{channel_id}/members", get(api::list_members))
        // Channel detail + metadata routes
        .route("/api/channels/{channel_id}", get(api::get_channel_handler))
        // Canvas routes
        .route("/api/channels/{channel_id}/canvas", get(api::get_canvas))
        // Message + thread routes
        .route(
            "/api/channels/{channel_id}/messages",
            get(api::list_messages),
        )
        .route(
            "/api/channels/{channel_id}/threads/{event_id}",
            get(api::get_thread),
        )
        // DM routes
        .route(
            "/api/dms",
            get(api::list_dms_handler).post(api::open_dm_handler),
        )
        .route(
            "/api/dms/{channel_id}/members",
            post(api::add_dm_member_handler),
        )
        .route("/api/dms/{channel_id}/hide", post(api::hide_dm_handler))
        // Reaction routes
        .route(
            "/api/messages/{event_id}/reactions",
            get(api::list_reactions_handler),
        )
        // User profile routes
        .route("/api/users/me/profile", get(api::get_profile))
        .route(
            "/api/users/me/channel-add-policy",
            put(api::put_channel_add_policy),
        )
        .route("/api/users/search", get(api::search_users))
        .route("/api/users/{pubkey}/profile", get(api::get_user_profile))
        .route("/api/users/{pubkey}/notes", get(api::get_user_notes))
        .route(
            "/api/users/{pubkey}/contact-list",
            get(api::get_contact_list),
        )
        .route("/api/users/batch", post(api::get_users_batch))
        // Feed route
        .route("/api/feed", get(api::feed_handler))
        // Identity bootstrap (proxy mode — returns derived signing key).
        // POST prevents intermediary caching of the secret key response.
        .route(
            "/api/identity/bootstrap",
            post(api::identity::identity_bootstrap),
        )
        // Reject request bodies larger than 1 MB to prevent resource exhaustion.
        .layer(RequestBodyLimitLayer::new(1024 * 1024))
        .with_state(state.clone());

    // Merge — each sub-router carries its own body limit.
    // Metrics → Trace → CORS applied once over the combined router.
    api_router
        .merge(media_router)
        .layer(middleware::from_fn(track_metrics))
        .layer(TraceLayer::new_for_http())
        .layer(build_cors_layer(&state.config.cors_origins))
}

/// Build the health-only router for K8s probes (port 8080 in CAKE).
///
/// No metrics middleware, no auth, no CORS, no body limit.
/// Separate from the app router so probes bypass Istio and don't pollute metrics.
pub fn build_health_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/_liveness", get(liveness_handler))
        .route("/_readiness", get(readiness_handler))
        .route("/_status", get(status_handler))
        .with_state(state)
}

/// Content-negotiated: NIP-11 JSON for plain HTTP, WebSocket upgrade otherwise.
///
/// Uses `axum::extract::Request` to manually attempt WS upgrade, so non-WS
/// requests aren't rejected by the extractor.
///
/// `ConnectInfo` is read from request extensions rather than as an extractor —
/// UDS connections have no `SocketAddr`, so the extractor would panic.
/// TCP connections populate it via `into_make_service_with_connect_info`; UDS
/// connections fall back to `0.0.0.0:0`.
///
/// In proxy identity mode, the `x-forwarded-identity-token` header is validated
/// at upgrade time and the connection is pre-authenticated (NIP-42 is skipped).
async fn nip11_or_ws_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    req: axum::extract::Request,
) -> impl IntoResponse {
    // Read peer address from extensions (set by TCP serve; absent for UDS).
    let addr = req
        .extensions()
        .get::<ConnectInfo<std::net::SocketAddr>>()
        .map(|ci| ci.0)
        .unwrap_or_else(|| std::net::SocketAddr::from(([0, 0, 0, 0], 0)));

    let accept = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if accept.contains("application/nostr+json") {
        let info = RelayInfo::from_config(&state.config);
        return Json(info).into_response();
    }

    // ── Proxy / hybrid identity: validate at upgrade time ──────────────
    //   - Proxy:  identity token mandatory — reject if missing.
    //   - Hybrid: identity token preferred — fall through to NIP-42 if missing.
    let identity_mode = &state.auth.identity_config().mode;
    let pre_auth = if identity_mode.is_proxy() {
        match headers
            .get("x-forwarded-identity-token")
            .and_then(|v| v.to_str().ok())
        {
            Some(jwt) => match state.auth.validate_identity_jwt(jwt).await {
                Ok((pubkey, scopes, username)) => {
                    let pubkey_bytes = pubkey.serialize().to_vec();
                    if let Err(e) = state
                        .db
                        .ensure_user_with_verified_name(&pubkey_bytes, &username)
                        .await
                    {
                        tracing::warn!("ws: ensure_user_with_verified_name failed: {e}");
                    }
                    Some(sprout_auth::AuthContext {
                        pubkey,
                        scopes,
                        auth_method: sprout_auth::AuthMethod::ProxyIdentity,
                    })
                }
                Err(e) => {
                    tracing::warn!("ws: proxy identity JWT validation failed: {e}");
                    return (StatusCode::UNAUTHORIZED, "identity token invalid").into_response();
                }
            },
            None if *identity_mode == sprout_auth::IdentityMode::Proxy => {
                tracing::warn!("ws: proxy mode enabled but x-forwarded-identity-token missing");
                return (StatusCode::UNAUTHORIZED, "identity token required").into_response();
            }
            // Hybrid: no identity token — proceed to NIP-42 auth.
            None => None,
        }
    } else {
        None
    };

    match WebSocketUpgrade::from_request(req, &state).await {
        Ok(ws) => ws
            .on_upgrade(move |socket| handle_connection(socket, state, addr, pre_auth))
            .into_response(),
        Err(_) => {
            // Not a WS request and not asking for nostr+json — serve NIP-11 as fallback.
            let info = RelayInfo::from_config(&state.config);
            Json(info).into_response()
        }
    }
}

async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

async fn liveness_handler() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

/// Readiness probe — checks shutdown flag, Postgres, and Redis connectivity.
///
/// Returns 503 immediately during graceful shutdown (SIGTERM received).
/// Otherwise returns 200 when both backends are reachable, or 503 with details.
async fn readiness_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    use std::time::Duration;

    // CAKE: readiness must return 503 after graceful shutdown begins.
    if state.shutting_down.load(Ordering::Relaxed) {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status": "shutting_down"})),
        )
            .into_response();
    }

    let check = async {
        let (pg_ok, redis_ok) = tokio::join!(state.db.ping(), async {
            state.redis_pool.get().await.is_ok()
        },);
        (pg_ok, redis_ok)
    };

    let (pg_ok, redis_ok) = tokio::time::timeout(Duration::from_secs(2), check)
        .await
        .unwrap_or((false, false));

    if pg_ok && redis_ok {
        (StatusCode::OK, Json(json!({"status": "ready"}))).into_response()
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status": "not_ready", "postgres": pg_ok, "redis": redis_ok})),
        )
            .into_response()
    }
}

/// Status endpoint — service name, version, uptime. Optional per CAKE contract.
async fn status_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let uptime_secs = state.started_at.elapsed().as_secs();
    Json(json!({
        "service": "sprout-relay",
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_seconds": uptime_secs,
    }))
}

/// Build a CORS layer from the configured origins list.
///
/// If `cors_origins` is empty (dev default), returns a permissive layer.
/// Otherwise, parses each entry as an `http::HeaderValue` and restricts
/// `Allow-Origin` to that exact set.
fn build_cors_layer(cors_origins: &[String]) -> CorsLayer {
    if cors_origins.is_empty() {
        return CorsLayer::permissive();
    }

    let origins: Vec<axum::http::HeaderValue> = cors_origins
        .iter()
        .filter_map(|o| o.parse::<axum::http::HeaderValue>().ok())
        .collect();

    if origins.is_empty() {
        tracing::error!(
            "SPROUT_CORS_ORIGINS set but no valid origins could be parsed — \
             refusing to fall back to permissive CORS. Fix the origins or unset \
             the variable for development mode."
        );
        // Deny all cross-origin requests rather than silently allowing all.
        return CorsLayer::new();
    }

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any)
}
