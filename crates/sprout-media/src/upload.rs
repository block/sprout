//! Upload pipeline — validate, store, thumbnail, sidecar.

use bytes::Bytes;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::auth::verify_blossom_upload_auth;
use crate::config::MediaConfig;
use crate::error::MediaError;
use crate::storage::{BlobMeta, MediaStorage};
use crate::thumbnail::generate_image_metadata_sync;
use crate::types::BlobDescriptor;
use crate::validation::{mime_to_ext, validate_content, validate_video_file};

/// Process an upload end-to-end: validate, store, thumbnail, return descriptor.
///
/// This is the image path — body is already fully buffered in RAM. Do NOT use
/// this for video uploads; use [`process_video_upload`] instead.
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

/// Process a video upload end-to-end using a streaming pipeline.
///
/// Unlike [`process_upload`], this function:
/// 1. Streams the request body to a [`tempfile::NamedTempFile`] while computing
///    SHA-256 incrementally — the full body is never in RAM simultaneously.
/// 2. Verifies the Blossom auth event `x` tag against the computed hash.
/// 3. Runs full MP4 validation (codec, duration, resolution, moov placement).
/// 4. Stores the blob via [`MediaStorage::put_file`] (streaming read from disk).
/// 5. Writes a sidecar with `duration_secs` (no thumbnail — desktop handles that).
///
/// Returns a [`BlobDescriptor`] with the `duration` field populated.
pub async fn process_video_upload(
    storage: &MediaStorage,
    config: &MediaConfig,
    auth_event: &nostr::Event,
    body_stream: impl futures_core::Stream<Item = Result<Bytes, axum::Error>> + Send + 'static,
    content_length: Option<u64>,
) -> Result<BlobDescriptor, MediaError> {
    // --- 1. Stream body to temp file, compute SHA-256 incrementally ---
    let tmp = tempfile::NamedTempFile::new().map_err(|e| MediaError::Io(e.to_string()))?;
    let tmp_path = tmp.path().to_path_buf();

    let max_bytes = config.max_video_bytes;

    // Fast-fail: reject oversized uploads before streaming starts.
    if let Some(cl) = content_length {
        if cl > max_bytes {
            return Err(MediaError::FileTooLarge {
                size: cl,
                max: max_bytes,
            });
        }
    }

    let (sha256_hex, file_size, first_bytes) = {
        use tokio_util::io::StreamReader;

        // Convert axum::Error stream to std::io::Error stream for StreamReader.
        // Box::pin is required because StreamReader needs a pinned stream.
        let mapped = futures_util::StreamExt::map(body_stream, |r| {
            r.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
        });
        let mut reader = StreamReader::new(Box::pin(mapped));

        let mut file = tokio::fs::File::create(&tmp_path)
            .await
            .map_err(|e| MediaError::Io(e.to_string()))?;
        let mut hasher = Sha256::new();
        let mut total: u64 = 0;
        let mut first_chunk: Option<Vec<u8>> = None;
        let mut buf = vec![0u8; 64 * 1024]; // 64 KiB read buffer

        loop {
            use tokio::io::AsyncReadExt;
            let n = reader
                .read(&mut buf)
                .await
                .map_err(|e| MediaError::Io(e.to_string()))?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > max_bytes {
                return Err(MediaError::FileTooLarge {
                    size: total,
                    max: max_bytes,
                });
            }
            hasher.update(&buf[..n]);
            file.write_all(&buf[..n])
                .await
                .map_err(|e| MediaError::Io(e.to_string()))?;
            if first_chunk.is_none() {
                first_chunk = Some(buf[..n].to_vec());
            }
        }
        file.flush().await.map_err(|e| MediaError::Io(e.to_string()))?;

        let sha256_hex = hex::encode(hasher.finalize());
        let first = first_chunk.unwrap_or_default();
        (sha256_hex, total, first)
    };

    // --- 2. Magic-byte check (video/mp4 only) ---
    let mime = infer::get(&first_bytes)
        .map(|t| t.mime_type().to_string())
        .ok_or(MediaError::UnknownContentType)?;
    if mime != "video/mp4" {
        return Err(MediaError::DisallowedContentType(mime));
    }

    // --- 3. Verify Blossom auth: x tag must match computed SHA-256 ---
    let auth = auth_event.clone();
    let sha256_for_auth = sha256_hex.clone();
    let server_domain = config.server_domain.clone();
    tokio::task::spawn_blocking(move || {
        verify_blossom_upload_auth(&auth, &sha256_for_auth, server_domain.as_deref())
    })
    .await
    .map_err(|_| MediaError::Internal)??;

    // --- 4. Full MP4 validation on the temp file ---
    let tmp_path_clone = tmp_path.clone();
    let cfg = config.clone();
    let video_meta = tokio::task::spawn_blocking(move || {
        validate_video_file(&tmp_path_clone, &cfg)
    })
    .await
    .map_err(|_| MediaError::Internal)??;

    let ext = "mp4";
    let key = format!("{sha256_hex}.{ext}");
    let meta_key = format!("_meta/{sha256_hex}.json");

    // --- 5. Idempotency check ---
    let sidecar_exists = storage.head(&meta_key).await?;
    let blob_exists = storage.head(&key).await?;
    if sidecar_exists && blob_exists {
        let meta = storage.get_sidecar(&sha256_hex).await?;
        return Ok(build_descriptor(
            config,
            &sha256_hex,
            ext,
            &mime,
            file_size,
            Some(&meta),
            meta.uploaded_at,
        ));
    }

    let uploaded_at = chrono::Utc::now().timestamp();

    // --- 6. Stream blob from temp file to S3 ---
    storage.put_file(&key, &tmp_path, &mime).await?;

    // --- 7. Write sidecar (no thumbnail for video — desktop handles that) ---
    let meta = BlobMeta {
        dim: format!("{}x{}", video_meta.width, video_meta.height),
        blurhash: String::new(),
        thumb_url: String::new(),
        ext: ext.to_string(),
        mime_type: mime.clone(),
        size: file_size,
        uploaded_at,
        duration_secs: Some(video_meta.duration_secs),
    };
    let meta_json = serde_json::to_vec(&meta)?;
    storage
        .put(&meta_key, &meta_json, "application/json")
        .await?;

    Ok(build_descriptor(
        config,
        &sha256_hex,
        ext,
        &mime,
        file_size,
        Some(&meta),
        uploaded_at,
    ))
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
    let duration = meta.and_then(|m| m.duration_secs);
    BlobDescriptor {
        url: format!("{}/{sha256}.{ext}", config.public_base_url),
        sha256: sha256.to_string(),
        size,
        mime_type: mime.to_string(),
        uploaded: uploaded_at,
        dim: meta.and_then(|m| (!m.dim.is_empty()).then(|| m.dim.clone())),
        blurhash: meta.and_then(|m| (!m.blurhash.is_empty()).then(|| m.blurhash.clone())),
        thumb: meta.and_then(|m| (!m.thumb_url.is_empty()).then(|| m.thumb_url.clone())),
        duration,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> MediaConfig {
        MediaConfig {
            s3_endpoint: String::new(),
            s3_access_key: String::new(),
            s3_secret_key: String::new(),
            s3_bucket: String::new(),
            max_image_bytes: 50 * 1024 * 1024,
            max_gif_bytes: 10 * 1024 * 1024,
            max_video_bytes: 524_288_000,
            public_base_url: "https://media.example.com".to_string(),
            server_domain: None,
        }
    }

    #[test]
    fn test_build_descriptor_video_omits_empty_thumb_and_blurhash() {
        // Video uploads produce a BlobMeta with empty thumb_url and blurhash.
        // build_descriptor must convert these to None so they're omitted from JSON.
        let config = test_config();
        let meta = BlobMeta {
            dim: "320x240".to_string(),
            blurhash: String::new(), // empty — video has no blurhash
            thumb_url: String::new(), // empty — video has no thumbnail
            ext: "mp4".to_string(),
            mime_type: "video/mp4".to_string(),
            size: 5_000_000,
            uploaded_at: 1700000000,
            duration_secs: Some(29.5),
        };

        let desc = build_descriptor(&config, "abc123", "mp4", "video/mp4", 5_000_000, Some(&meta), 1700000000);

        // Empty strings must become None, not Some("")
        assert!(desc.blurhash.is_none(), "blurhash should be None for video, got {:?}", desc.blurhash);
        assert!(desc.thumb.is_none(), "thumb should be None for video, got {:?}", desc.thumb);
        // Non-empty fields should be present
        assert_eq!(desc.dim, Some("320x240".to_string()));
        assert_eq!(desc.duration, Some(29.5));

        // Verify JSON serialization omits the empty fields entirely
        let json = serde_json::to_value(&desc).unwrap();
        assert!(json.get("blurhash").is_none(), "blurhash should be absent from JSON");
        assert!(json.get("thumb").is_none(), "thumb should be absent from JSON");
        assert!(json.get("dim").is_some(), "dim should be present in JSON");
        assert!(json.get("duration").is_some(), "duration should be present in JSON");
    }

    #[test]
    fn test_build_descriptor_image_includes_thumb_and_blurhash() {
        // Image uploads produce a BlobMeta with populated thumb_url and blurhash.
        let config = test_config();
        let hash = "a".repeat(64);
        let meta = BlobMeta {
            dim: "800x600".to_string(),
            blurhash: "LEHV6nWB2yk8pyo0adR*.7kCMdnj".to_string(),
            thumb_url: format!("https://media.example.com/{hash}.thumb.jpg"),
            ext: "jpg".to_string(),
            mime_type: "image/jpeg".to_string(),
            size: 100_000,
            uploaded_at: 1700000000,
            duration_secs: None,
        };

        let desc = build_descriptor(&config, &hash, "jpg", "image/jpeg", 100_000, Some(&meta), 1700000000);

        assert_eq!(desc.blurhash, Some("LEHV6nWB2yk8pyo0adR*.7kCMdnj".to_string()));
        assert!(desc.thumb.is_some());
        assert!(desc.duration.is_none());

        // Verify JSON: duration should be absent, blurhash and thumb present
        let json = serde_json::to_value(&desc).unwrap();
        assert!(json.get("blurhash").is_some());
        assert!(json.get("thumb").is_some());
        assert!(json.get("duration").is_none(), "duration should be absent for images");
    }

    #[test]
    fn test_build_descriptor_no_meta() {
        // When meta is None, all optional fields should be None.
        let config = test_config();
        let desc = build_descriptor(&config, "abc123", "jpg", "image/jpeg", 100, None, 1700000000);

        assert!(desc.dim.is_none());
        assert!(desc.blurhash.is_none());
        assert!(desc.thumb.is_none());
        assert!(desc.duration.is_none());
    }
}
