//! axum router — WebSocket, NIP-11, NIP-05, health.

use std::sync::Arc;

use axum::{
    extract::{ConnectInfo, FromRequest, State, WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

use crate::api;
use crate::connection::handle_connection;
use crate::nip11::{relay_info_handler, RelayInfo};
use crate::state::AppState;

/// Build the axum [`Router`] with all relay routes, middleware, and CORS configuration.
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(nip11_or_ws_handler))
        .route("/info", get(relay_info_handler))
        .route("/.well-known/nostr.json", get(nip05_handler))
        .route("/health", get(health_handler))
        .route(
            "/api/channels",
            get(api::channels_handler).post(api::create_channel),
        )
        .route("/api/search", get(api::search_handler))
        .route("/api/agents", get(api::agents_handler))
        .route("/api/presence", get(api::presence_handler))
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
        .route("/api/workflows/{id}/trigger", post(api::trigger_workflow))
        .route("/api/workflows/{id}/webhook", post(api::workflow_webhook))
        .route("/api/approvals/{token}/grant", post(api::grant_approval))
        .route("/api/approvals/{token}/deny", post(api::deny_approval))
        // Feed route
        .route("/api/feed", get(api::feed_handler))
        .layer(TraceLayer::new_for_http())
        .layer(build_cors_layer(&state.config.cors_origins))
        // Reject request bodies larger than 1 MB to prevent resource exhaustion.
        .layer(RequestBodyLimitLayer::new(1024 * 1024))
        .with_state(state)
}

/// Content-negotiated: NIP-11 JSON for plain HTTP, WebSocket upgrade otherwise.
///
/// Uses `axum::extract::Request` to manually attempt WS upgrade, so non-WS
/// requests aren't rejected by the extractor.
async fn nip11_or_ws_handler(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    req: axum::extract::Request,
) -> impl IntoResponse {
    let accept = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if accept.contains("application/nostr+json") {
        let info = RelayInfo::from_config(&state.config);
        return Json(info).into_response();
    }

    match WebSocketUpgrade::from_request(req, &state).await {
        Ok(ws) => ws
            .on_upgrade(move |socket| handle_connection(socket, state, addr))
            .into_response(),
        Err(_) => {
            // Not a WS request and not asking for nostr+json — serve NIP-11 as fallback.
            let info = RelayInfo::from_config(&state.config);
            Json(info).into_response()
        }
    }
}

// NIP-05 stub: returns empty names/relays. Full NIP-05 verification is planned.
async fn nip05_handler() -> impl IntoResponse {
    Json(serde_json::json!({
        "names": {},
        "relays": {}
    }))
}

async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, "ok")
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
