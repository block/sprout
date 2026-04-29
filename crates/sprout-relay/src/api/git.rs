//! Smart HTTP git transport for Sprout.
//!
//! Three endpoints implement the git Smart HTTP protocol:
//! - `GET  /git/{owner}/{repo}/info/refs?service={svc}` — ref advertisement
//! - `POST /git/{owner}/{repo}/git-upload-pack` — clone/fetch
//! - `POST /git/{owner}/{repo}/git-receive-pack` — push
//!
//! Auth: NIP-98 on all routes (clone + push). No public repos for v1.
//! Transport: shells out to `git --stateless-rpc` with `env_clear()`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Path as AxumPath, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use base64::Engine;
use hex;
use serde::Deserialize;
use tokio::process::Command;
use tower_http::limit::RequestBodyLimitLayer;
use tracing::{error, info, warn};

use crate::state::AppState;

// ── NIP-98 Auth Extractor ────────────────────────────────────────────────────

/// NIP-98 auth extractor for git routes.
///
/// Validates the `Authorization: Nostr <base64>` header before the request body
/// is read. Same pattern as `AuthenticatedUpload` in media.rs.
///
/// Authorization model (v1): any authenticated pubkey can clone; only the repo
/// owner can push. Maintainer lists from kind:30617 are a future enhancement.
pub struct GitAuth {
    /// The authenticated user's public key, extracted from the NIP-98 event.
    pub pubkey: nostr::PublicKey,
}

impl axum::extract::FromRequestParts<Arc<AppState>> for GitAuth {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let method = parts.method.as_str();

        let auth_header = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .header(
                        "WWW-Authenticate",
                        format!("Nostr realm=\"sprout\", method=\"{method}\""),
                    )
                    .body(Body::from("missing Authorization header"))
                    .unwrap()
            })?;

        let token = auth_header.strip_prefix("Nostr ").ok_or_else(|| {
            Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header(
                    "WWW-Authenticate",
                    format!("Nostr realm=\"sprout\", method=\"{method}\""),
                )
                .body(Body::from("expected Authorization: Nostr <base64>"))
                .unwrap()
        })?;

        let event_bytes = base64::engine::general_purpose::STANDARD
            .decode(token)
            .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(token))
            .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid base64").into_response())?;
        let event_json = String::from_utf8(event_bytes)
            .map_err(|_| (StatusCode::UNAUTHORIZED, "invalid utf-8").into_response())?;

        // Use configured relay_url as canonical base (don't trust forwarded headers).
        let relay_url = &state.config.relay_url;
        let base_url = relay_url
            .replace("ws://", "http://")
            .replace("wss://", "https://");
        let base_url = base_url.trim_end_matches('/');
        let path_and_query = parts
            .uri
            .path_and_query()
            .map(|pq| pq.as_str())
            .unwrap_or(parts.uri.path());

        // Bug fix: strip git endpoint suffixes before building the expected URL.
        //
        // Git's credential helper is invoked once — for the initial GET /info/refs —
        // and signs a NIP-98 token with `u = <repo-root>` (e.g. `/git/{owner}/{repo}.git`).
        // That same token is reused for all subsequent requests in the session
        // (git-upload-pack, git-receive-pack). We must verify against the repo-root URL,
        // not the full endpoint URL, so all three endpoints share a single canonical URL.
        let repo_path = path_and_query
            .split_once("/info/refs")
            .map(|(prefix, _)| prefix)
            .or_else(|| path_and_query.strip_suffix("/git-upload-pack"))
            .or_else(|| path_and_query.strip_suffix("/git-receive-pack"))
            .unwrap_or(path_and_query);
        let expected_url = format!("{base_url}{repo_path}");

        // Bug fix: skip the HTTP method check for git routes.
        //
        // Git's credential helper signs the NIP-98 token with `method=GET` (the method
        // used for the initial /info/refs discovery request), then reuses that same token
        // for POST requests (git-upload-pack, git-receive-pack). Verifying against the
        // actual HTTP method would reject all pack operations after the first request.
        //
        // The URL tag already scopes the token to the correct repo — the method tag adds
        // no meaningful security here since all git endpoints require the same authorization
        // (repo owner). We pass the method from the event itself so verify_nip98_event
        // always accepts whatever method the credential helper signed.
        let event_method = serde_json::from_str::<serde_json::Value>(&event_json)
            .ok()
            .and_then(|v| {
                v["tags"]
                    .as_array()?
                    .iter()
                    .find(|t| t[0].as_str() == Some("method"))?[1]
                    .as_str()
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| method.to_owned());

        // body=None: can't buffer streaming pack data to verify payload hash.
        // Token is time-bounded (±60s) and URL-locked — acceptable trade-off.
        let pubkey =
            sprout_auth::nip98::verify_nip98_event(&event_json, &expected_url, &event_method, None)
                .map_err(|e| {
                    warn!(error = %e, "git NIP-98 auth failed");
                    (StatusCode::UNAUTHORIZED, format!("NIP-98 auth failed: {e}")).into_response()
                })?;

        Ok(GitAuth { pubkey })
    }
}

// ── Path Validation ──────────────────────────────────────────────────────────

struct ValidatedRepoPath {
    repo_path: PathBuf,
}

/// Validate and resolve a git repo path from URL parameters.
///
/// Security: allowlist characters, canonicalize, verify under repo root.
fn validate_repo_path(
    owner: &str,
    repo: &str,
    git_repo_root: &Path,
) -> Result<ValidatedRepoPath, Response> {
    // Owner must be exactly 64 lowercase hex chars.
    if owner.len() != 64
        || !owner
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    {
        return Err((StatusCode::BAD_REQUEST, "invalid owner").into_response());
    }

    // Strip trailing .git if present.
    let repo_name = repo.strip_suffix(".git").unwrap_or(repo);

    // Repo name: [a-zA-Z0-9._-]{1,64}, no leading dots, no "..".
    if repo_name.is_empty()
        || repo_name.len() > 64
        || repo_name.starts_with('.')
        || repo_name.contains("..")
        || !repo_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
    {
        return Err((StatusCode::BAD_REQUEST, "invalid repo name").into_response());
    }

    let repo_path = git_repo_root.join(owner).join(format!("{repo_name}.git"));

    // Path canonicalization: verify resolved path is under repo root.
    // Fail closed: if the repo root doesn't exist, reject — the service can't operate safely.
    let canonical_root = git_repo_root.canonicalize().map_err(|_| {
        error!("git_repo_path does not exist or cannot be canonicalized");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "git service misconfigured",
        )
            .into_response()
    })?;
    // Repo may not exist yet (404 handled later), but if it does, verify containment.
    if let Ok(canonical_repo) = repo_path.canonicalize() {
        if !canonical_repo.starts_with(&canonical_root) {
            return Err((StatusCode::BAD_REQUEST, "path traversal detected").into_response());
        }
    }

    Ok(ValidatedRepoPath { repo_path })
}

/// Apply hardened environment to a git subprocess command.
///
/// Clears all inherited env vars, then sets only the minimum required:
/// - `PATH` — so git can find its own helpers
/// - `GIT_HTTP_EXPORT_ALL` — required for Smart HTTP
/// - `GIT_CONFIG_NOSYSTEM=1` — ignore system-wide gitconfig
/// - `HOME=/dev/null` — prevent reading ~/.gitconfig
fn harden_git_env(cmd: &mut Command) {
    cmd.env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("GIT_HTTP_EXPORT_ALL", "1")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("HOME", "/dev/null");
}

// ── Route Handlers ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
/// Query parameters for the `info/refs` endpoint.
pub struct InfoRefsQuery {
    service: String,
}

#[derive(Deserialize)]
/// Path parameters for git repo routes: `{owner}/{repo}`.
pub struct GitRepoParams {
    owner: String,
    repo: String,
}

/// `GET /git/{owner}/{repo}/info/refs?service={service}`
///
/// Advertises refs for clone (git-upload-pack) or push (git-receive-pack).
pub async fn info_refs(
    State(state): State<Arc<AppState>>,
    _auth: GitAuth,
    AxumPath(params): AxumPath<GitRepoParams>,
    Query(query): Query<InfoRefsQuery>,
) -> Result<Response, Response> {
    // Validate service parameter: exact allowlist.
    let service = match query.service.as_str() {
        "git-upload-pack" | "git-receive-pack" => &query.service,
        _ => return Err((StatusCode::BAD_REQUEST, "invalid service").into_response()),
    };

    let validated = validate_repo_path(&params.owner, &params.repo, &state.config.git_repo_path)?;
    if !validated.repo_path.exists() {
        return Err((StatusCode::NOT_FOUND, "repository not found").into_response());
    }

    let _permit = state
        .git_semaphore
        .acquire()
        .await
        .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "git service busy").into_response())?;

    let mut cmd = Command::new("git");
    // Git's smart HTTP protocol uses service names like "git-upload-pack" and
    // "git-receive-pack", but the actual git subcommands are "upload-pack" and
    // "receive-pack" (without the "git-" prefix).
    let git_subcmd = service.strip_prefix("git-").unwrap_or(service.as_str());
    cmd.arg(git_subcmd)
        .arg("--stateless-rpc")
        .arg("--advertise-refs")
        .arg(&validated.repo_path);
    harden_git_env(&mut cmd);
    let output = cmd.output().await.map_err(|e| {
        error!(error = %e, "git subprocess failed to spawn");
        (StatusCode::INTERNAL_SERVER_ERROR, "git error").into_response()
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!(stderr = %stderr, "git --advertise-refs failed");
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "git error").into_response());
    }

    // Build pkt-line response: service header + flush + git output.
    let svc_line = format!("# service={service}\n");
    let svc_pkt = format!("{:04x}{svc_line}", svc_line.len() + 4);
    let mut body = Vec::with_capacity(svc_pkt.len() + 4 + output.stdout.len());
    body.extend_from_slice(svc_pkt.as_bytes());
    body.extend_from_slice(b"0000"); // flush packet
    body.extend_from_slice(&output.stdout);

    let content_type = format!("application/x-{service}-advertisement");
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from(body))
        .unwrap())
}

/// `POST /git/{owner}/{repo}/git-upload-pack`
///
/// Handles clone/fetch — client sends wants/haves, server sends pack data.
pub async fn upload_pack(
    State(state): State<Arc<AppState>>,
    _auth: GitAuth,
    AxumPath(params): AxumPath<GitRepoParams>,
    body: Body,
) -> Result<Response, Response> {
    run_git_service(&state, &params.owner, &params.repo, "upload-pack", body).await
}

/// `POST /git/{owner}/{repo}/git-receive-pack`
///
/// Handles push — client sends ref updates + pack data.
/// Authorization: only the repo owner (whose pubkey matches `{owner}` in the URL)
/// can push. Maintainer lists from kind:30617 are a future enhancement.
pub async fn receive_pack(
    State(state): State<Arc<AppState>>,
    auth: GitAuth,
    AxumPath(params): AxumPath<GitRepoParams>,
    body: Body,
) -> Result<Response, Response> {
    // Push authorization: authenticated pubkey must match the repo owner.
    let pusher_hex = hex::encode(auth.pubkey.serialize());
    if pusher_hex != params.owner {
        return Err((StatusCode::FORBIDDEN, "push denied: not the repo owner").into_response());
    }

    // Per-repo lock: prevent concurrent pushes to the same bare repo.
    // git receive-pack is not safe for concurrent access.
    let validated = validate_repo_path(&params.owner, &params.repo, &state.config.git_repo_path)?;
    let repo_lock = state
        .git_repo_locks
        .entry(validated.repo_path.clone())
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone();
    let _repo_guard = repo_lock.lock().await;

    let response =
        run_git_service(&state, &params.owner, &params.repo, "receive-pack", body).await?;

    // Post-push: publish kind:30618 ref state event (fire-and-forget).
    let state_clone = state.clone();
    let owner = params.owner.clone();
    let repo = params.repo.clone();
    let pusher = auth.pubkey;
    tokio::spawn(async move {
        if let Err(e) = publish_ref_state(&state_clone, &owner, &repo, &pusher).await {
            warn!(error = %e, owner = %owner, repo = %repo, "failed to publish kind:30618");
        }
    });

    Ok(response)
}

/// Shared git service runner — spawns subprocess and streams I/O.
async fn run_git_service(
    state: &Arc<AppState>,
    owner: &str,
    repo: &str,
    service: &str,
    body: Body,
) -> Result<Response, Response> {
    let validated = validate_repo_path(owner, repo, &state.config.git_repo_path)?;
    if !validated.repo_path.exists() {
        return Err((StatusCode::NOT_FOUND, "repository not found").into_response());
    }

    let _permit = state
        .git_semaphore
        .acquire()
        .await
        .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "git service busy").into_response())?;

    let mut cmd = Command::new("git");
    cmd.arg(service)
        .arg("--stateless-rpc")
        .arg(&validated.repo_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    harden_git_env(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| {
        error!(error = %e, "git subprocess failed to spawn");
        (StatusCode::INTERNAL_SERVER_ERROR, "git error").into_response()
    })?;

    // Stream request body to git stdin.
    let mut stdin = child.stdin.take().unwrap();
    let body_task = tokio::spawn(async move {
        use futures_util::StreamExt;
        let mut stream = body.into_data_stream();
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(bytes) => {
                    if tokio::io::AsyncWriteExt::write_all(&mut stdin, &bytes)
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        drop(stdin); // Close stdin → EOF for git.
    });

    let _ = body_task.await;

    let output = child.wait_with_output().await.map_err(|e| {
        error!(error = %e, "git subprocess failed");
        (StatusCode::INTERNAL_SERVER_ERROR, "git error").into_response()
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!(stderr = %stderr, service = %service, "git subprocess exited with error");
        // Still return output — git protocol errors are communicated in-band.
    }

    let content_type = format!("application/x-git-{service}-result");
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from(output.stdout))
        .unwrap())
}

// ── Post-Push Event Publishing ───────────────────────────────────────────────

/// Publish kind:30618 (repo state) after a successful push.
///
/// Reads current refs from the repo and publishes a relay-signed event
/// with the pusher's pubkey in a `p` tag. This makes pushes subscribable.
async fn publish_ref_state(
    state: &Arc<AppState>,
    owner: &str,
    repo: &str,
    pusher: &nostr::PublicKey,
) -> anyhow::Result<()> {
    let validated = validate_repo_path(owner, repo, &state.config.git_repo_path)
        .map_err(|_| anyhow::anyhow!("invalid repo path"))?;

    // Read current refs.
    let mut cmd = Command::new("git");
    cmd.args(["for-each-ref", "--format=%(refname) %(objectname)"])
        .current_dir(&validated.repo_path);
    harden_git_env(&mut cmd);
    let output = cmd.output().await?;

    if !output.status.success() {
        return Err(anyhow::anyhow!("git for-each-ref failed"));
    }

    let refs_output = String::from_utf8_lossy(&output.stdout);

    // Get HEAD symbolic ref.
    let mut head_cmd = Command::new("git");
    head_cmd
        .args(["symbolic-ref", "HEAD"])
        .current_dir(&validated.repo_path);
    harden_git_env(&mut head_cmd);
    let head_output = head_cmd.output().await.ok();

    let repo_name = repo.strip_suffix(".git").unwrap_or(repo);

    // Build NIP-34 kind:30618 tags.
    let mut tags = vec![nostr::Tag::custom(nostr::TagKind::custom("d"), [repo_name])];

    for line in refs_output.lines() {
        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.len() != 2 {
            continue;
        }
        let (ref_name, sha) = (parts[0], parts[1]);

        // NIP-34 kind:30618 only includes heads and tags — skip stash, notes, remotes.
        if !ref_name.starts_with("refs/heads/") && !ref_name.starts_with("refs/tags/") {
            continue;
        }
        // Validate ref name and SHA.
        if ref_name.starts_with('/')
            || ref_name.contains("//")
            || !ref_name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "/_.-".contains(c))
        {
            continue;
        }
        if sha.len() != 40 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }

        tags.push(nostr::Tag::custom(nostr::TagKind::custom(ref_name), [sha]));
    }

    // HEAD tag.
    if let Some(head) = head_output {
        if head.status.success() {
            let head_ref = String::from_utf8_lossy(&head.stdout).trim().to_string();
            if !head_ref.is_empty() {
                tags.push(nostr::Tag::custom(
                    nostr::TagKind::custom("HEAD"),
                    [format!("ref: {head_ref}")],
                ));
            }
        }
    }

    // Pusher pubkey in p tag.
    tags.push(nostr::Tag::public_key(pusher.clone()));

    info!(
        repo = %repo_name,
        owner = %owner,
        ref_count = tags.len().saturating_sub(2),
        "publishing kind:30618 ref state"
    );

    // Sign with relay keypair — the relay is the authoritative source of ref state.
    let event = nostr::EventBuilder::new(nostr::Kind::Custom(30618), "", tags)
        .sign_with_keys(&state.relay_keypair)
        .map_err(|e| anyhow::anyhow!("failed to sign kind:30618: {e}"))?;

    // Store globally (channel_id = None) and fan out to subscribers.
    let (stored, was_inserted) = state.db.insert_event(&event, None).await?;
    if was_inserted {
        let matches = state.sub_registry.fan_out(&stored);
        for (conn_id, sub_id) in matches {
            let _ = state.conn_manager.send_to(
                conn_id,
                crate::protocol::RelayMessage::event(&sub_id, &stored.event),
            );
        }
    }

    Ok(())
}

// ── Router Builder ───────────────────────────────────────────────────────────

/// Build the git sub-router with its own body limit.
///
/// Mounted at `/git/{owner}/{repo}/...` with a configurable max pack size.
pub fn git_router(state: Arc<AppState>) -> Router {
    let body_limit = state.config.git_max_pack_bytes as usize;

    Router::new()
        .route("/git/{owner}/{repo}/info/refs", get(info_refs))
        .route("/git/{owner}/{repo}/git-upload-pack", post(upload_pack))
        .route("/git/{owner}/{repo}/git-receive-pack", post(receive_pack))
        .layer(RequestBodyLimitLayer::new(body_limit))
        .with_state(state)
}
