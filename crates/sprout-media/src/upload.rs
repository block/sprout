//! Upload pipeline — validate, store, thumbnail, sidecar.

use bytes::Bytes;
use sha2::{Digest, Sha256};

use crate::auth::verify_blossom_upload_auth;
use crate::config::MediaConfig;
use crate::error::MediaError;
use crate::storage::{BlobMeta, MediaStorage};
use crate::thumbnail::generate_image_metadata_sync;
use crate::types::BlobDescriptor;
use crate::validation::{mime_to_ext, validate_content};

/// Process an upload end-to-end: validate, store, thumbnail, return descriptor.
pub async fn process_upload(
    storage: &MediaStorage,
    config: &MediaConfig,
    auth_event: &nostr::Event,
    body: Bytes,
) -> Result<BlobDescriptor, MediaError> {
    // CPU-bound: validate content, compute hash, verify auth
    let auth = auth_event.clone();
    let bytes = body.clone();
    let cfg = config.clone();
    let (mime, sha256, ext) = tokio::task::spawn_blocking(move || -> Result<_, MediaError> {
        let mime = validate_content(&bytes, &cfg)?;
        let sha256 = hex::encode(Sha256::digest(&bytes));
        let ext = mime_to_ext(&mime).to_string();
        verify_blossom_upload_auth(&auth, &sha256, cfg.server_domain.as_deref())?;
        Ok((mime, sha256, ext))
    })
    .await
    .map_err(|_| MediaError::Internal)??;

    let key = format!("{sha256}.{ext}");
    let meta_key = format!("_meta/{sha256}.json"); // used in idempotency check below

    // Idempotent: check BOTH sidecar AND blob exist before short-circuiting.
    // If sidecar exists but blob is missing, fall through to re-upload.
    let sidecar_exists = storage.head(&meta_key).await?;
    let blob_exists = storage.head(&key).await?;
    if sidecar_exists && blob_exists {
        let meta = storage.get_sidecar(&sha256).await?;
        return Ok(build_descriptor(
            config,
            &sha256,
            &ext,
            &mime,
            body.len() as u64,
            Some(&meta),
            meta.uploaded_at,
        ));
    }

    // Compute uploaded_at once — single source of truth for sidecar and response.
    let uploaded_at = chrono::Utc::now().timestamp();

    // Store blob first, then generate metadata.
    // On failure we intentionally do NOT delete the orphan blob — concurrent
    // uploads of the same hash could race and delete a blob that another
    // request is about to reference via its sidecar. Orphan blobs are
    // content-addressed and bounded by the upload size limit, so the storage
    // cost is negligible. A V2 background GC job can sweep blobs with no
    // matching sidecar after a grace period.
    storage.put(&key, &body, &mime).await?;

    match generate_and_store_metadata(storage, config, &sha256, &ext, &mime, &body, uploaded_at)
        .await
    {
        Ok(meta) => Ok(build_descriptor(
            config,
            &sha256,
            &ext,
            &mime,
            body.len() as u64,
            Some(&meta),
            uploaded_at,
        )),
        Err(e) => {
            tracing::warn!(sha256 = %sha256, "metadata generation failed; orphan blob left for GC");
            Err(e)
        }
    }
}

/// Generate thumbnail, blurhash, and sidecar metadata, then store them.
/// Returns the completed [`BlobMeta`] on success.
async fn generate_and_store_metadata(
    storage: &MediaStorage,
    config: &MediaConfig,
    sha256: &str,
    ext: &str,
    mime: &str,
    body: &Bytes,
    uploaded_at: i64,
) -> Result<BlobMeta, MediaError> {
    let body_ref = body.clone();
    let mime_ref = mime.to_string();
    let ext_ref = ext.to_string();
    let sha256_ref = sha256.to_string();
    let cfg_ref = config.clone();
    let (mut meta, thumb_bytes) = tokio::task::spawn_blocking(move || {
        generate_image_metadata_sync(&cfg_ref, &sha256_ref, &body_ref, &mime_ref, &ext_ref)
    })
    .await
    .map_err(|_| MediaError::Internal)??;

    meta.uploaded_at = uploaded_at;

    if let Some(ref tb) = thumb_bytes {
        let thumb_key = format!("{sha256}.thumb.jpg");
        storage.put(&thumb_key, tb, "image/jpeg").await?;
    }

    let meta_key = format!("_meta/{sha256}.json");
    let meta_json = serde_json::to_vec(&meta)?;
    storage
        .put(&meta_key, &meta_json, "application/json")
        .await?;
    Ok(meta)
}

fn build_descriptor(
    config: &MediaConfig,
    sha256: &str,
    ext: &str,
    mime: &str,
    size: u64,
    meta: Option<&BlobMeta>,
    uploaded_at: i64,
) -> BlobDescriptor {
    BlobDescriptor {
        url: format!("{}/{sha256}.{ext}", config.public_base_url),
        sha256: sha256.to_string(),
        size,
        mime_type: mime.to_string(),
        uploaded: uploaded_at,
        dim: meta.map(|m| m.dim.clone()),
        blurhash: meta.map(|m| m.blurhash.clone()),
        thumb: meta.map(|m| m.thumb_url.clone()),
    }
}
