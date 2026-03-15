//! Blossom-compatible media upload, retrieval, and existence check handlers.
//!
//! Routes:
//!   PUT  /media/upload          — BUD-02 upload (auth required)
//!   GET  /media/{sha256_ext}    — BUD-01 serve blob
//!   HEAD /media/{sha256_ext}    — BUD-01 existence check

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{FromRequestParts, Path, State},
    http::{request::Parts, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use base64::Engine;
use sha2::{Digest, Sha256};
use sprout_audit::{AuditAction, NewAuditEntry};
use sprout_auth::Scope;
use sprout_media::{BlobDescriptor, MediaError};

use crate::state::AppState;

// ── Upload ────────────────────────────────────────────────────────────────────

/// Axum extractor that validates Blossom auth + API token scopes from headers
/// BEFORE the request body is read. This prevents unauthenticated clients from
/// forcing the server to buffer up to 50MB of body data.
///
/// Axum processes `FromRequestParts` extractors before `FromRequest` (body)
/// extractors, so auth rejection happens before any body buffering.
pub(crate) struct AuthenticatedUpload {
    auth_event: nostr::Event,
    #[allow(dead_code)] // scopes validated in extractor; stored for future per-scope handler logic
    scopes: Vec<Scope>,
}

impl FromRequestParts<Arc<AppState>> for AuthenticatedUpload {
    type Rejection = MediaError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let headers = &parts.headers;

        // 1. Extract and validate Blossom auth event
        let auth_event = extract_blossom_auth(headers)?;
        sprout_media::auth::verify_blossom_auth_event(
            &auth_event,
            state.config.media.server_domain.as_deref(),
        )?;

        // 2. Require X-SHA-256 header (BUD-11: mandatory for PUT /upload)
        let claimed_hash = headers
            .get("x-sha-256")
            .and_then(|v| v.to_str().ok())
            .ok_or(MediaError::MissingTag("x-sha-256"))?;

        // Validate format: exactly 64 lowercase hex characters
        if claimed_hash.len() != 64
            || !claimed_hash
                .chars()
                .all(|c| matches!(c, '0'..='9' | 'a'..='f'))
        {
            return Err(MediaError::HashMismatch);
        }

        // 3. Validate X-SHA-256 matches at least one x tag in the auth event
        let has_matching_x = auth_event.tags.iter().any(|tag| {
            tag.kind().to_string() == "x" && tag.content().map_or(false, |v| v == claimed_hash)
        });
        if !has_matching_x {
            return Err(MediaError::HashMismatch);
        }

        // 4. Resolve scopes (API token or dev mode)
        let scopes = resolve_upload_scopes(headers, state, &auth_event.pubkey).await?;
        sprout_auth::require_scope(&scopes, Scope::FilesWrite)
            .map_err(|_| MediaError::InsufficientScope)?;

        Ok(AuthenticatedUpload { auth_event, scopes })
    }
}

/// PUT /media/upload — Blossom BUD-02 upload.
///
/// Auth is validated via the [`AuthenticatedUpload`] extractor BEFORE the body
/// is read, preventing unauthenticated clients from forcing body buffering.
// AuthenticatedUpload is pub(crate) — it's an internal extractor type, never
// exposed outside this crate. The warning is benign: axum resolves it at
// compile time via trait bounds, not by name.
#[allow(private_interfaces)]
///
/// Expects:
///   - `Authorization: Nostr <base64(kind:24242 event)>` — Blossom auth
///   - `X-SHA-256: <hex>` — Required per BUD-11
///   - `X-Auth-Token: sprout_*` — API token for scope resolution (optional in dev mode)
///   - Raw binary body (the file bytes)
///
/// Returns a [`BlobDescriptor`] JSON on success.
// TODO(v2): Add per-pubkey upload rate limiting and storage quotas to prevent
// bandwidth/storage exhaustion from authenticated callers. Currently mitigated by
// auth requirement (API token + Blossom signature) and body size limit.
pub async fn upload_blob(
    State(state): State<Arc<AppState>>,
    auth: AuthenticatedUpload,
    body: Bytes,
) -> Result<Json<BlobDescriptor>, MediaError> {
    let descriptor = sprout_media::process_upload(
        &state.media_storage,
        &state.config.media,
        &auth.auth_event,
        body,
    )
    .await?;

    // Fire-and-forget audit — never block the response on audit I/O.
    let audit = state.audit.clone();
    let desc = descriptor.clone();
    let uploader = auth.auth_event.pubkey.to_hex();
    tokio::spawn(async move {
        let _ = audit
            .log(NewAuditEntry {
                event_id: desc.sha256.clone(),
                event_kind: sprout_core::kind::KIND_MEDIA_UPLOAD,
                actor_pubkey: uploader,
                action: AuditAction::MediaUploaded,
                channel_id: None,
                metadata: serde_json::json!({
                    "sha256": desc.sha256,
                    "size": desc.size,
                    "mime": desc.mime_type,
                }),
            })
            .await;
    });

    Ok(Json(descriptor))
}

// ── Serve ─────────────────────────────────────────────────────────────────────

/// Validate that sha256_ext is a safe path segment.
///
/// Accepted forms (max 3 segments):
///   - `{sha256}`                   — bare 64-char lowercase hex
///   - `{sha256}.{ext}`             — hash + extension
///   - `{sha256}.thumb.jpg`          — hash + thumb variant (always JPEG)
///
/// Where `{ext}` ∈ {"jpg", "png", "gif", "webp"} for primary blobs (uploads canonicalize to .jpg, not .jpeg).
/// Rejects path traversal, leading underscores, and any non-hex first segment.
fn validate_media_path(sha256_ext: &str) -> Result<(), MediaError> {
    const ALLOWED_EXTS: &[&str] = &["jpg", "png", "gif", "webp"];

    let segments: Vec<&str> = sha256_ext.split('.').collect();

    // 1–3 segments only (hash, optional thumb, optional ext)
    if segments.is_empty() || segments.len() > 3 {
        return Err(MediaError::NotFound);
    }

    // First segment must be exactly 64 lowercase hex chars (SHA-256)
    let hash = segments[0];
    if hash.len() != 64 || !hash.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')) {
        return Err(MediaError::NotFound);
    }

    // Validate remaining segments
    match segments.len() {
        1 => {} // bare hash — ok
        2 => {
            // {hash}.{ext}
            if !ALLOWED_EXTS.contains(&segments[1]) {
                return Err(MediaError::NotFound);
            }
        }
        3 => {
            // {hash}.thumb.jpg — thumbnails are always JPEG
            if segments[1] != "thumb" || segments[2] != "jpg" {
                return Err(MediaError::NotFound);
            }
        }
        _ => return Err(MediaError::NotFound),
    }

    Ok(())
}

/// GET /media/{sha256_ext} — Blossom BUD-01 serve blob.
///
/// `sha256_ext` is either:
///   - `<sha256>.<ext>` — direct key (e.g. `abc123.jpg`)
///   - `<sha256>` — bare hash; extension resolved from sidecar
///   - `<sha256>.thumb.jpg` — thumbnail variant
pub async fn get_blob(
    State(state): State<Arc<AppState>>,
    Path(sha256_ext): Path<String>,
) -> Result<Response, MediaError> {
    validate_media_path(&sha256_ext)?;

    // Sidecar gate FIRST — reject before any blob I/O. Storage is not authoritative.
    let content_type = if sha256_ext.ends_with(".thumb.jpg") {
        let parent_hash = sha256_ext.strip_suffix(".thumb.jpg").unwrap_or(&sha256_ext);
        let _ = state
            .media_storage
            .read_sidecar_mime(parent_hash)
            .await
            .ok_or(MediaError::NotFound)?;
        "image/jpeg".to_string()
    } else {
        // For explicit paths (hash.ext), verify the requested extension matches
        // the sidecar's canonical extension — sidecar is authoritative.
        let sidecar_mime = state
            .media_storage
            .read_sidecar_mime(&sha256_ext)
            .await
            .ok_or(MediaError::NotFound)?;
        if sha256_ext.contains('.') {
            let requested_ext = sha256_ext.rsplit('.').next().unwrap_or("");
            let sidecar = state.media_storage.get_sidecar(
                sha256_ext.split('.').next().unwrap_or(&sha256_ext)
            ).await.map_err(|_| MediaError::NotFound)?;
            if requested_ext != sidecar.ext {
                return Err(MediaError::NotFound);
            }
        }
        sidecar_mime
    };

    let key = resolve_s3_key(&state.media_storage, &sha256_ext).await?;
    let bytes = state.media_storage.get(&key).await?;

    Ok((
        [
            ("content-type", content_type.as_str()),
            ("cache-control", "public, max-age=31536000, immutable"),
            ("content-disposition", "inline"),
            ("content-security-policy", "default-src 'none'"),
            ("x-content-type-options", "nosniff"),
        ],
        bytes,
    )
        .into_response())
}

/// HEAD /media/{sha256_ext} — Blossom BUD-01 existence check.
///
/// Content-type is derived from the validated sidecar only — never from raw S3
/// object metadata — to prevent MIME spoofing via tampered storage. If the sidecar
/// is missing, we return 404 rather than fall back to untrusted metadata.
pub async fn head_blob(
    State(state): State<Arc<AppState>>,
    Path(sha256_ext): Path<String>,
) -> Result<Response, MediaError> {
    validate_media_path(&sha256_ext)?;

    // Sidecar gate FIRST — reject before any blob I/O.
    let content_type = if sha256_ext.ends_with(".thumb.jpg") {
        let parent_hash = sha256_ext.strip_suffix(".thumb.jpg").unwrap_or(&sha256_ext);
        let _ = state
            .media_storage
            .read_sidecar_mime(parent_hash)
            .await
            .ok_or(MediaError::NotFound)?;
        "image/jpeg".to_string()
    } else {
        let sidecar_mime = state
            .media_storage
            .read_sidecar_mime(&sha256_ext)
            .await
            .ok_or(MediaError::NotFound)?;
        if sha256_ext.contains('.') {
            let requested_ext = sha256_ext.rsplit('.').next().unwrap_or("");
            let sidecar = state.media_storage.get_sidecar(
                sha256_ext.split('.').next().unwrap_or(&sha256_ext)
            ).await.map_err(|_| MediaError::NotFound)?;
            if requested_ext != sidecar.ext {
                return Err(MediaError::NotFound);
            }
        }
        sidecar_mime
    };

    let key = resolve_s3_key(&state.media_storage, &sha256_ext).await?;
    match state.media_storage.head_with_metadata(&key).await? {
        Some(meta) => {
            let size_str = meta.size.to_string();
            Ok((
                StatusCode::OK,
                [
                    ("content-type", content_type.as_str()),
                    ("content-length", size_str.as_str()),
                    ("cache-control", "public, max-age=31536000, immutable"),
                ],
            )
                .into_response())
        }
        None => Ok(StatusCode::NOT_FOUND.into_response()),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Resolve the S3 key from a URL path segment.
///
/// - `sha256.ext`       → used as-is (already validated by `validate_media_path`)
/// - `sha256` (no dot)  → read sidecar to get extension, return `sha256.ext`
///
/// Sidecar-derived extensions are validated against the allowlist to prevent
/// object-key confusion if sidecar data is ever tampered with.
async fn resolve_s3_key(
    storage: &sprout_media::MediaStorage,
    sha256_ext: &str,
) -> Result<String, MediaError> {
    const ALLOWED_EXTS: &[&str] = &["jpg", "png", "gif", "webp"];

    if sha256_ext.contains('.') {
        Ok(sha256_ext.to_string())
    } else {
        let sidecar = storage
            .get_sidecar(sha256_ext)
            .await
            .map_err(|_| MediaError::NotFound)?;
        // Validate sidecar ext — never trust storage as authoritative for path construction
        if !ALLOWED_EXTS.contains(&sidecar.ext.as_str()) {
            return Err(MediaError::NotFound);
        }
        Ok(format!("{}.{}", sha256_ext, sidecar.ext))
    }
}

/// Extract and verify a kind:24242 Blossom auth event from the `Authorization` header.
///
/// Accepts both base64url (BUD-11 spec) and standard base64 (nostr-tools compat).
fn extract_blossom_auth(headers: &HeaderMap) -> Result<nostr::Event, MediaError> {
    use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};

    let header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(MediaError::MissingAuth)?;

    let token = header
        .strip_prefix("Nostr ")
        .ok_or(MediaError::InvalidAuthScheme)?;

    let json_bytes = URL_SAFE_NO_PAD
        .decode(token)
        .or_else(|_| STANDARD.decode(token))
        .map_err(|_| MediaError::InvalidBase64)?;

    let event: nostr::Event =
        serde_json::from_slice(&json_bytes).map_err(|_| MediaError::InvalidAuthEvent)?;

    Ok(event)
}

/// Resolve permission scopes for an upload caller.
///
/// Resolution order:
/// 1. `X-Auth-Token: sprout_*` header — API token path (validates owner matches Blossom signer)
/// 2. If `require_auth_token` is false (dev mode) — check pubkey allowlist, then grant file scopes
async fn resolve_upload_scopes(
    headers: &HeaderMap,
    state: &AppState,
    blossom_pubkey: &nostr::PublicKey,
) -> Result<Vec<Scope>, MediaError> {
    // 1. API token path — desktop sends Blossom auth in Authorization + token in X-Auth-Token.
    if let Some(token) = headers
        .get("x-auth-token")
        .and_then(|v| v.to_str().ok())
        .filter(|t| t.starts_with("sprout_"))
    {
        let hash: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        let record = state
            .db
            .get_api_token_by_hash_including_revoked(&hash)
            .await
            .map_err(|_| MediaError::Unauthorized)?
            .ok_or(MediaError::Unauthorized)?;

        if record.revoked_at.is_some() {
            return Err(MediaError::TokenRevoked);
        }
        if let Some(expires_at) = record.expires_at {
            if expires_at < chrono::Utc::now() {
                return Err(MediaError::TokenExpired);
            }
        }

        // Token owner must match the Blossom signer — prevents token theft attacks.
        let blossom_bytes = blossom_pubkey.serialize().to_vec();
        if record.owner_pubkey != blossom_bytes {
            return Err(MediaError::PubkeyMismatch);
        }

        return Ok(record
            .scopes
            .iter()
            .filter_map(|s| s.parse::<Scope>().ok())
            .collect());
    }

    // 2. Dev mode: no API token required.
    if state.config.require_auth_token {
        return Err(MediaError::Unauthorized);
    }

    // Dev mode is active — any valid Blossom signer can upload.
    // This must never be enabled in production.
    tracing::warn!("dev mode upload: no API token required — ensure require_auth_token=true in production");

    // 3. Pubkey allowlist check (dev mode only).
    if state.config.pubkey_allowlist_enabled {
        let pubkey_bytes = blossom_pubkey.serialize().to_vec();
        if !state
            .db
            .is_pubkey_allowed(&pubkey_bytes)
            .await
            .unwrap_or(false)
        {
            return Err(MediaError::Unauthorized);
        }
    }

    // Dev mode: grant file scopes.
    Ok(vec![Scope::FilesRead, Scope::FilesWrite])
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_HASH: &str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";

    #[test]
    fn test_validate_media_path_bare_hash() {
        assert!(validate_media_path(VALID_HASH).is_ok());
    }

    #[test]
    fn test_validate_media_path_hash_ext() {
        for ext in &["jpg", "png", "gif", "webp"] {
            assert!(validate_media_path(&format!("{VALID_HASH}.{ext}")).is_ok());
        }
    }

    #[test]
    fn test_validate_media_path_thumb_jpg_only() {
        assert!(validate_media_path(&format!("{VALID_HASH}.thumb.jpg")).is_ok());
        // Other thumb extensions rejected — thumbnails are always JPEG
        assert!(validate_media_path(&format!("{VALID_HASH}.thumb.png")).is_err());
        assert!(validate_media_path(&format!("{VALID_HASH}.thumb.webp")).is_err());
    }

    #[test]
    fn test_validate_media_path_rejects_bad_ext() {
        assert!(validate_media_path(&format!("{VALID_HASH}.svg")).is_err());
        assert!(validate_media_path(&format!("{VALID_HASH}.exe")).is_err());
        assert!(validate_media_path(&format!("{VALID_HASH}.pdf")).is_err());
    }

    #[test]
    fn test_validate_media_path_rejects_short_hash() {
        assert!(validate_media_path("abc123.jpg").is_err());
    }

    #[test]
    fn test_validate_media_path_rejects_uppercase_hash() {
        let upper = "ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789";
        assert!(validate_media_path(&format!("{upper}.jpg")).is_err());
    }

    #[test]
    fn test_validate_media_path_rejects_traversal() {
        assert!(validate_media_path("../etc/passwd").is_err());
        assert!(validate_media_path(&format!("../{VALID_HASH}.jpg")).is_err());
    }

    #[test]
    fn test_validate_media_path_rejects_too_many_segments() {
        assert!(validate_media_path(&format!("{VALID_HASH}.thumb.jpg.extra")).is_err());
    }

    #[test]
    fn test_validate_media_path_rejects_empty() {
        assert!(validate_media_path("").is_err());
    }
}
