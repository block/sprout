//! Integration tests for the PKCE OAuth token source.
//!
//! No browser dance — we cover the silent-refresh and cache-hit paths
//! against a stubbed OIDC server (axum). The interactive browser flow is
//! exercised manually in `sprout-agent auth databricks` and on Tyler's
//! laptop via the smoke test in the PR description.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::Form;
use axum::{routing::get, routing::post, Json, Router};
use serde::Deserialize;
use serde_json::json;
use sprout_agent::auth::{PkceOAuthConfig, PkceOAuthTokenSource, TokenSource};
use tempfile::TempDir;

#[derive(Deserialize)]
struct TokenForm {
    grant_type: String,
    #[allow(dead_code)]
    refresh_token: Option<String>,
}

/// Boot a stub OIDC server that:
///   - serves discovery at `/.well-known/oauth-authorization-server`
///   - issues a fresh access token for every `refresh_token` request
///   - counts how many refresh hits it gets
async fn spawn_oidc() -> (String, Arc<AtomicU64>) {
    let counter = Arc::new(AtomicU64::new(0));
    let counter_for_token = counter.clone();

    // Bind first so we know our own base URL before building the router.
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let base = format!("http://{addr}");
    let base_for_discovery = base.clone();

    let app = Router::new()
        .route(
            "/.well-known/oauth-authorization-server",
            get(move || {
                let base = base_for_discovery.clone();
                async move {
                    Json(json!({
                        "authorization_endpoint": format!("{base}/authorize"),
                        "token_endpoint": format!("{base}/token"),
                    }))
                }
            }),
        )
        .route(
            "/token",
            post(move |Form(form): Form<TokenForm>| {
                let counter = counter_for_token.clone();
                async move {
                    let n = counter.fetch_add(1, Ordering::SeqCst) + 1;
                    assert_eq!(form.grant_type, "refresh_token");
                    Json(json!({
                        "access_token": format!("fresh-token-{n}"),
                        "refresh_token": "rotated-refresh",
                        "expires_in": 3600,
                    }))
                }
            }),
        );

    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (base, counter)
}

/// Cache key construction matches the auth module: sha256(discovery|client|scopes).
fn cache_path_for(cache_dir: &std::path::Path, cfg: &PkceOAuthConfig) -> std::path::PathBuf {
    use sha2::Digest;
    let mut h = sha2::Sha256::new();
    h.update(cfg.discovery_url.as_bytes());
    h.update(b"|");
    h.update(cfg.client_id.as_bytes());
    h.update(b"|");
    h.update(cfg.scopes.join(",").as_bytes());
    let hash = hex::encode(h.finalize());
    cache_dir
        .join(&cfg.cache_namespace)
        .join(format!("{hash}.json"))
}

/// Write a token file the engine should pick up on construction.
fn seed_cache(path: &std::path::Path, body: serde_json::Value) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_vec(&body).unwrap()).unwrap();
}

#[tokio::test]
async fn cache_hit_short_circuits_network() {
    let tmp = TempDir::new().unwrap();

    let (base, refresh_counter) = spawn_oidc().await;
    let cfg = PkceOAuthConfig {
        discovery_url: format!("{base}/.well-known/oauth-authorization-server"),
        client_id: "test-client".into(),
        scopes: vec!["a".into(), "b".into()],
        cache_namespace: "databricks".into(),
        cache_dir_override: Some(tmp.path().to_path_buf()),
    };

    // Seed an unexpired token in the cache.
    let future = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    let path = cache_path_for(tmp.path(), &cfg);
    seed_cache(
        &path,
        json!({
            "access_token": "cached-token",
            "refresh_token": "rt",
            "expires_at": future,
        }),
    );

    let src = PkceOAuthTokenSource::new(cfg).unwrap();
    let bearer = src.bearer().await.unwrap();
    assert_eq!(bearer, "cached-token");
    assert_eq!(refresh_counter.load(Ordering::SeqCst), 0, "no refresh should fire");
}

#[tokio::test]
async fn expired_cache_silently_refreshes() {
    let tmp = TempDir::new().unwrap();

    let (base, refresh_counter) = spawn_oidc().await;
    let cfg = PkceOAuthConfig {
        discovery_url: format!("{base}/.well-known/oauth-authorization-server"),
        client_id: "test-client".into(),
        scopes: vec!["a".into()],
        cache_namespace: "databricks".into(),
        cache_dir_override: Some(tmp.path().to_path_buf()),
    };

    // Seed an already-expired token with a refresh_token.
    let path = cache_path_for(tmp.path(), &cfg);
    seed_cache(
        &path,
        json!({
            "access_token": "stale",
            "refresh_token": "valid-refresh",
            "expires_at": 1u64, // way in the past
        }),
    );

    let src = PkceOAuthTokenSource::new(cfg).unwrap();
    let bearer = src.bearer().await.unwrap();
    assert_eq!(bearer, "fresh-token-1");
    assert_eq!(refresh_counter.load(Ordering::SeqCst), 1);

    // A second call should hit the in-memory cache and skip the network.
    let bearer2 = src.bearer().await.unwrap();
    assert_eq!(bearer2, "fresh-token-1");
    assert_eq!(refresh_counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn refreshed_token_is_persisted_to_disk() {
    let tmp = TempDir::new().unwrap();

    let (base, _) = spawn_oidc().await;
    let cfg = PkceOAuthConfig {
        discovery_url: format!("{base}/.well-known/oauth-authorization-server"),
        client_id: "test-client".into(),
        scopes: vec!["a".into()],
        cache_namespace: "databricks".into(),
        cache_dir_override: Some(tmp.path().to_path_buf()),
    };

    let path = cache_path_for(tmp.path(), &cfg);
    seed_cache(
        &path,
        json!({
            "access_token": "stale",
            "refresh_token": "valid-refresh",
            "expires_at": 1u64,
        }),
    );

    let src = PkceOAuthTokenSource::new(cfg).unwrap();
    let _ = src.bearer().await.unwrap();

    let on_disk: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(on_disk["access_token"], "fresh-token-1");
    assert_eq!(on_disk["refresh_token"], "rotated-refresh");
    assert!(on_disk["expires_at"].is_u64());
}
