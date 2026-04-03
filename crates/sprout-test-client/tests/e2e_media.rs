//! End-to-end media upload tests (Blossom / NIP-96 style).
//!
//! Requires: relay running at localhost:3000, MinIO running at localhost:9000.
//! All tests are `#[ignore]` so they don't run in CI by default.
//!
//! # Running
//!
//! ```text
//! cargo test -p sprout-test-client --test e2e_media -- --ignored --nocapture
//! ```
//!
//! Override the relay URL:
//!
//! ```text
//! RELAY_HTTP_URL=http://localhost:3000 cargo test -p sprout-test-client --test e2e_media -- --ignored
//! ```

mod helpers;
use helpers::*;

use nostr::Keys;
use sha2::{Digest, Sha256};

// ── URL helpers ───────────────────────────────────────────────────────────────

fn relay_http_url() -> String {
    std::env::var("RELAY_HTTP_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Upload a tiny JPEG, then GET it back and verify the bytes match.
/// Also checks BlobDescriptor fields and thumbnail endpoint.
#[tokio::test]
#[ignore]
async fn test_upload_and_get() {
    let client = http_client();
    let keys = Keys::generate();
    let jpeg = tiny_jpeg();
    let sha256 = hex::encode(Sha256::digest(&jpeg));

    println!("sha256: {sha256}");
    println!("relay: {}", relay_http_url());

    // PUT /media/upload
    let auth = sign_blossom_auth(&keys, &sha256);
    let resp = client
        .put(format!("{}/media/upload", relay_http_url()))
        .header("Authorization", blossom_auth_header(&auth))
        .header("Content-Type", "image/jpeg")
        .header("X-SHA-256", &sha256)
        .body(jpeg.clone())
        .send()
        .await
        .expect("upload PUT failed");

    let status = resp.status();
    let body_text = resp.text().await.unwrap_or_default();
    println!("PUT /media/upload → {status}: {body_text}");
    assert_eq!(status, 200, "upload should succeed");

    // Parse BlobDescriptor
    let descriptor: serde_json::Value =
        serde_json::from_str(&body_text).expect("BlobDescriptor JSON");
    println!("BlobDescriptor: {descriptor:#}");

    assert_eq!(
        descriptor["sha256"].as_str().unwrap(),
        sha256,
        "sha256 must match"
    );
    assert!(
        descriptor["url"].as_str().unwrap().contains(&sha256),
        "url must contain sha256"
    );
    assert!(
        descriptor["size"].as_u64().unwrap() > 0,
        "size must be positive"
    );
    assert!(
        descriptor["type"].as_str().is_some(),
        "mime type must be present"
    );
    // dim and blurhash are best-effort (image processing may not run on 1x1 JPEG)
    println!(
        "dim: {:?}, blurhash: {:?}",
        descriptor["dim"], descriptor["blurhash"]
    );

    // GET /media/{sha256}.jpg — bytes must match
    let get_url = format!("{}/media/{sha256}.jpg", relay_http_url());
    let get_resp = client
        .get(&get_url)
        .send()
        .await
        .expect("GET /media/{sha256}.jpg failed");
    assert_eq!(get_resp.status(), 200, "GET should return 200");
    let returned_bytes = get_resp.bytes().await.unwrap();
    assert_eq!(
        returned_bytes.as_ref(),
        jpeg.as_slice(),
        "GET must return original bytes"
    );

    // HEAD /media/{sha256}.jpg — must return 200 with content-type
    let head_resp = client
        .head(&get_url)
        .send()
        .await
        .expect("HEAD /media/{sha256}.jpg failed");
    assert_eq!(head_resp.status(), 200, "HEAD should return 200");
    assert!(
        head_resp.headers().get("content-type").is_some(),
        "HEAD must include content-type"
    );

    // GET thumbnail — /media/{sha256}.thumb.jpg
    let thumb_url = format!("{}/media/{sha256}.thumb.jpg", relay_http_url());
    let thumb_resp = client
        .get(&thumb_url)
        .send()
        .await
        .expect("GET thumbnail failed");
    println!("GET thumbnail → {}", thumb_resp.status());
    // Thumbnail may be same as original for 1x1 images — just check 200
    assert_eq!(thumb_resp.status(), 200, "thumbnail should return 200");
}

/// Idempotency: uploading the same file twice returns the same BlobDescriptor.
#[tokio::test]
#[ignore]
async fn test_upload_idempotent() {
    let client = http_client();
    let keys = Keys::generate();
    let jpeg = tiny_jpeg();
    let sha256 = hex::encode(Sha256::digest(&jpeg));

    let upload = |keys: &Keys| {
        let auth = sign_blossom_auth(keys, &sha256);
        client
            .put(format!("{}/media/upload", relay_http_url()))
            .header("Authorization", blossom_auth_header(&auth))
            .header("Content-Type", "image/jpeg")
            .header("X-SHA-256", sha256.clone())
            .body(jpeg.clone())
            .send()
    };

    let r1: serde_json::Value = upload(&keys)
        .await
        .expect("first upload failed")
        .json()
        .await
        .expect("first descriptor parse");

    // Second upload — different key, same content
    let keys2 = Keys::generate();
    let r2: serde_json::Value = upload(&keys2)
        .await
        .expect("second upload failed")
        .json()
        .await
        .expect("second descriptor parse");

    assert_eq!(
        r1["sha256"], r2["sha256"],
        "sha256 must be identical on re-upload"
    );
    assert_eq!(r1["url"], r2["url"], "url must be identical on re-upload");
}

/// Upload without an Authorization header must return 401.
#[tokio::test]
#[ignore]
async fn test_upload_no_auth_returns_401() {
    let client = http_client();
    let jpeg = tiny_jpeg();

    let resp = client
        .put(format!("{}/media/upload", relay_http_url()))
        .header("Content-Type", "image/jpeg")
        .body(jpeg)
        .send()
        .await
        .expect("request failed");

    println!("no-auth → {}", resp.status());
    assert_eq!(resp.status(), 401, "upload without auth must be 401");
}

/// Upload without X-SHA-256 header must return 401 (BUD-11: mandatory).
#[tokio::test]
#[ignore]
async fn test_upload_missing_x_sha256_returns_401() {
    let client = http_client();
    let keys = Keys::generate();
    let jpeg = tiny_jpeg();
    let sha256 = hex::encode(Sha256::digest(&jpeg));

    let auth = sign_blossom_auth(&keys, &sha256);
    let resp = client
        .put(format!("{}/media/upload", relay_http_url()))
        .header("Authorization", blossom_auth_header(&auth))
        .header("Content-Type", "image/jpeg")
        // Intentionally omit X-SHA-256
        .body(jpeg)
        .send()
        .await
        .expect("request failed");

    println!("missing-x-sha256 → {}", resp.status());
    assert_eq!(resp.status(), 401, "upload without X-SHA-256 must be 401");
}

/// Upload where the `x` tag sha256 doesn't match the actual body must return 401.
#[tokio::test]
#[ignore]
async fn test_upload_hash_mismatch_returns_400() {
    let client = http_client();
    let keys = Keys::generate();
    let jpeg = tiny_jpeg();
    let wrong_hash = "f".repeat(64); // definitely not the real sha256

    let auth = sign_blossom_auth(&keys, &wrong_hash);
    let resp = client
        .put(format!("{}/media/upload", relay_http_url()))
        .header("Authorization", blossom_auth_header(&auth))
        .header("Content-Type", "image/jpeg")
        .header("X-SHA-256", &wrong_hash)
        .body(jpeg)
        .send()
        .await
        .expect("request failed");

    println!("hash-mismatch → {}", resp.status());
    assert_eq!(resp.status(), 401, "hash mismatch must be 401");
}

/// GET a sha256 that was never uploaded must return 404.
#[tokio::test]
#[ignore]
async fn test_get_nonexistent_returns_404() {
    let client = http_client();
    let missing_sha256 = "0".repeat(64);
    let url = format!("{}/media/{missing_sha256}.jpg", relay_http_url());

    let resp = client.get(&url).send().await.expect("GET failed");
    println!("missing blob → {}", resp.status());
    assert_eq!(resp.status(), 404, "missing blob must be 404");
}

/// Upload a real image from the filesystem (set TEST_IMAGE_PATH env var).
/// Verifies the full round-trip: upload → BlobDescriptor → GET bytes match.
#[tokio::test]
#[ignore]
async fn test_upload_real_image() {
    let image_path = match std::env::var("TEST_IMAGE_PATH") {
        Ok(p) => p,
        Err(_) => {
            println!("Skipping: TEST_IMAGE_PATH not set");
            return;
        }
    };

    let client = http_client();
    let keys = Keys::generate();
    let bytes = std::fs::read(&image_path).expect("read image file");
    let sha256 = hex::encode(Sha256::digest(&bytes));
    let size = bytes.len();

    println!("image: {image_path}");
    println!("size:  {size} bytes");
    println!("sha256: {sha256}");

    let auth = sign_blossom_auth(&keys, &sha256);
    let resp = client
        .put(format!("{}/media/upload", relay_http_url()))
        .header("Authorization", blossom_auth_header(&auth))
        .header("Content-Type", "image/jpeg")
        .header("X-SHA-256", &sha256)
        .body(bytes.clone())
        .send()
        .await
        .expect("upload PUT failed");

    let status = resp.status();
    let body_text = resp.text().await.unwrap_or_default();
    println!("PUT /media/upload → {status}: {body_text}");
    assert_eq!(status, 200, "upload should succeed");

    let descriptor: serde_json::Value =
        serde_json::from_str(&body_text).expect("BlobDescriptor JSON");
    println!("BlobDescriptor: {descriptor:#}");

    assert_eq!(descriptor["sha256"].as_str().unwrap(), sha256);
    assert_eq!(descriptor["size"].as_u64().unwrap(), size as u64);
    assert!(descriptor["url"].as_str().unwrap().contains(&sha256));
    assert!(
        descriptor["dim"].as_str().is_some(),
        "real image should have dim"
    );
    assert!(
        descriptor["blurhash"].as_str().is_some(),
        "real image should have blurhash"
    );

    // GET bytes back and verify
    let get_url = descriptor["url"].as_str().unwrap();
    let get_resp = client.get(get_url).send().await.expect("GET failed");
    assert_eq!(get_resp.status(), 200);
    let returned = get_resp.bytes().await.unwrap();
    assert_eq!(
        returned.as_ref(),
        bytes.as_slice(),
        "GET must return original bytes"
    );

    println!("✅ Real image upload round-trip passed");
}
