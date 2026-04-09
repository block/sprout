//! End-to-end video upload tests (Blossom protocol, MP4/H.264).
//!
//! Requires: relay running at localhost:3000, MinIO running at localhost:9000.
//! All tests are `#[ignore]` so they don't run in CI by default.
//!
//! # Running
//!
//! ```text
//! cargo test -p sprout-test-client --test e2e_media_video -- --ignored --nocapture
//! ```

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use nostr::{EventBuilder, JsonUtil, Keys, Kind, Tag, Timestamp};
use reqwest::{Client, StatusCode};
use sha2::{Digest, Sha256};
use std::time::Duration;

// ── URL helpers ───────────────────────────────────────────────────────────────

fn relay_http_url() -> String {
    std::env::var("RELAY_HTTP_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
}

fn http_client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("failed to build HTTP client")
}

// ── Blossom auth helpers ──────────────────────────────────────────────────────

fn sign_blossom_auth(keys: &Keys, sha256: &str) -> nostr::Event {
    let now = Timestamp::now().as_u64();
    let exp_str = (now + 300).to_string();
    let tags = vec![
        Tag::parse(&["t", "upload"]).expect("t tag"),
        Tag::parse(&["x", sha256]).expect("x tag"),
        Tag::parse(&["expiration", &exp_str]).expect("expiration tag"),
    ];
    EventBuilder::new(Kind::from(24242), "Upload test", tags)
        .sign_with_keys(keys)
        .expect("sign blossom auth")
}

fn blossom_auth_header(event: &nostr::Event) -> String {
    format!(
        "Nostr {}",
        URL_SAFE_NO_PAD.encode(event.as_json().as_bytes())
    )
}

// ── Minimal MP4 builder ───────────────────────────────────────────────────────

/// Build a minimal but structurally valid fast-start MP4 (H.264, 1s, 320×240).
///
/// Layout: ftyp | moov(mvhd + trak(tkhd + mdia(mdhd + hdlr + minf(vmhd + dinf + stbl)))) | mdat
/// This is enough for `infer` to detect video/mp4 and for the `mp4` crate to parse.
fn build_test_mp4() -> Vec<u8> {
    fn box_wrap(fourcc: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let size = (8 + payload.len()) as u32;
        let mut b = Vec::new();
        b.extend_from_slice(&size.to_be_bytes());
        b.extend_from_slice(fourcc);
        b.extend_from_slice(payload);
        b
    }

    // ftyp
    let ftyp = {
        let mut b = Vec::new();
        b.extend_from_slice(&20u32.to_be_bytes());
        b.extend_from_slice(b"ftyp");
        b.extend_from_slice(b"isom");
        b.extend_from_slice(&0u32.to_be_bytes());
        b.extend_from_slice(b"isom");
        b
    };

    // mvhd (version 0, timescale=1000, duration=1000ms)
    let mvhd_payload = {
        let mut b = vec![0u8; 4]; // version=0, flags=0
        b.extend_from_slice(&0u32.to_be_bytes()); // creation_time
        b.extend_from_slice(&0u32.to_be_bytes()); // modification_time
        b.extend_from_slice(&1000u32.to_be_bytes()); // timescale
        b.extend_from_slice(&1000u32.to_be_bytes()); // duration
        b.extend_from_slice(&0x00010000u32.to_be_bytes()); // rate
        b.extend_from_slice(&0x0100u16.to_be_bytes()); // volume
        b.extend_from_slice(&[0u8; 10]); // reserved
                                         // identity matrix (9 × u32)
        for &v in &[0x00010000u32, 0, 0, 0, 0x00010000, 0, 0, 0, 0x40000000] {
            b.extend_from_slice(&v.to_be_bytes());
        }
        b.extend_from_slice(&[0u8; 24]); // pre_defined
        b.extend_from_slice(&2u32.to_be_bytes()); // next_track_id
        b
    };
    let mvhd = box_wrap(b"mvhd", &mvhd_payload);

    // tkhd
    let tkhd_payload = {
        let mut b = vec![0u8, 0, 0, 3]; // version=0, flags=3
        b.extend_from_slice(&0u32.to_be_bytes()); // creation
        b.extend_from_slice(&0u32.to_be_bytes()); // modification
        b.extend_from_slice(&1u32.to_be_bytes()); // track_id
        b.extend_from_slice(&0u32.to_be_bytes()); // reserved
        b.extend_from_slice(&1000u32.to_be_bytes()); // duration
        b.extend_from_slice(&[0u8; 8]); // reserved
        b.extend_from_slice(&0i16.to_be_bytes()); // layer
        b.extend_from_slice(&0i16.to_be_bytes()); // alternate_group
        b.extend_from_slice(&0u16.to_be_bytes()); // volume
        b.extend_from_slice(&0u16.to_be_bytes()); // reserved
        for &v in &[0x00010000u32, 0, 0, 0, 0x00010000, 0, 0, 0, 0x40000000] {
            b.extend_from_slice(&v.to_be_bytes());
        }
        b.extend_from_slice(&(320u32 << 16).to_be_bytes()); // width 16.16
        b.extend_from_slice(&(240u32 << 16).to_be_bytes()); // height 16.16
        b
    };
    let tkhd = box_wrap(b"tkhd", &tkhd_payload);

    // mdhd
    let mdhd_payload = {
        let mut b = vec![0u8; 4];
        b.extend_from_slice(&0u32.to_be_bytes());
        b.extend_from_slice(&0u32.to_be_bytes());
        b.extend_from_slice(&1000u32.to_be_bytes()); // timescale
        b.extend_from_slice(&1000u32.to_be_bytes()); // duration
        b.extend_from_slice(&0u16.to_be_bytes()); // language
        b.extend_from_slice(&0u16.to_be_bytes()); // pre_defined
        b
    };
    let mdhd = box_wrap(b"mdhd", &mdhd_payload);

    // hdlr (video)
    let hdlr_payload = {
        let mut b = vec![0u8; 4];
        b.extend_from_slice(&0u32.to_be_bytes()); // pre_defined
        b.extend_from_slice(b"vide");
        b.extend_from_slice(&[0u8; 12]); // reserved
        b.extend_from_slice(b"VideoHandler\0");
        b
    };
    let hdlr = box_wrap(b"hdlr", &hdlr_payload);

    // vmhd
    let vmhd_payload = {
        let mut b = vec![0u8, 0, 0, 1]; // flags=1
        b.extend_from_slice(&0u16.to_be_bytes());
        b.extend_from_slice(&[0u8; 6]);
        b
    };
    let vmhd = box_wrap(b"vmhd", &vmhd_payload);

    // dinf -> dref -> url
    let url_box = box_wrap(b"url ", &[0, 0, 0, 1]);
    let dref_payload = {
        let mut b = vec![0u8; 4];
        b.extend_from_slice(&1u32.to_be_bytes());
        b.extend_from_slice(&url_box);
        b
    };
    let dref = box_wrap(b"dref", &dref_payload);
    let dinf = box_wrap(b"dinf", &dref);

    // stsd -> avc1 (H.264)
    let avc1_entry = {
        let mut b = vec![0u8; 6]; // reserved
        b.extend_from_slice(&1u16.to_be_bytes()); // data_ref_idx
        b.extend_from_slice(&[0u8; 2]); // pre_defined
        b.extend_from_slice(&[0u8; 2]); // reserved
        b.extend_from_slice(&[0u8; 12]); // pre_defined
        b.extend_from_slice(&320u16.to_be_bytes()); // width
        b.extend_from_slice(&240u16.to_be_bytes()); // height
        b.extend_from_slice(&0x00480000u32.to_be_bytes()); // horiz_res
        b.extend_from_slice(&0x00480000u32.to_be_bytes()); // vert_res
        b.extend_from_slice(&0u32.to_be_bytes()); // reserved
        b.extend_from_slice(&1u16.to_be_bytes()); // frame_count
        b.extend_from_slice(&[0u8; 32]); // compressorname
        b.extend_from_slice(&0x0018u16.to_be_bytes()); // depth
        b.extend_from_slice(&(-1i16).to_be_bytes()); // pre_defined
                                                     // avcC
        let avcc = vec![
            0x01, 0x42, 0x00, 0x1E, 0xFF, 0xE1, 0x00, 0x00, 0x01, 0x00, 0x00,
        ];
        b.extend_from_slice(&box_wrap(b"avcC", &avcc));
        b
    };
    let avc1 = box_wrap(b"avc1", &avc1_entry);
    let stsd_payload = {
        let mut b = vec![0u8; 4];
        b.extend_from_slice(&1u32.to_be_bytes());
        b.extend_from_slice(&avc1);
        b
    };
    let stsd = box_wrap(b"stsd", &stsd_payload);

    // Minimal sample tables
    let stts = box_wrap(b"stts", &{
        let mut b = vec![0u8; 4];
        b.extend_from_slice(&1u32.to_be_bytes());
        b.extend_from_slice(&1u32.to_be_bytes());
        b.extend_from_slice(&1000u32.to_be_bytes());
        b
    });
    let stsc = box_wrap(b"stsc", &{
        let mut b = vec![0u8; 4];
        b.extend_from_slice(&1u32.to_be_bytes());
        b.extend_from_slice(&1u32.to_be_bytes());
        b.extend_from_slice(&1u32.to_be_bytes());
        b.extend_from_slice(&1u32.to_be_bytes());
        b
    });
    let stsz = box_wrap(b"stsz", &{
        let mut b = vec![0u8; 4];
        b.extend_from_slice(&0u32.to_be_bytes());
        b.extend_from_slice(&1u32.to_be_bytes());
        b.extend_from_slice(&0u32.to_be_bytes());
        b
    });
    let stco = box_wrap(b"stco", &{
        let mut b = vec![0u8; 4];
        b.extend_from_slice(&1u32.to_be_bytes());
        b.extend_from_slice(&28u32.to_be_bytes());
        b
    });

    let stbl_payload = [&stsd[..], &stts, &stsc, &stsz, &stco].concat();
    let stbl = box_wrap(b"stbl", &stbl_payload);
    let minf_payload = [&vmhd[..], &dinf, &stbl].concat();
    let minf = box_wrap(b"minf", &minf_payload);
    let mdia_payload = [&mdhd[..], &hdlr, &minf].concat();
    let mdia = box_wrap(b"mdia", &mdia_payload);
    let trak_payload = [&tkhd[..], &mdia].concat();
    let trak = box_wrap(b"trak", &trak_payload);
    let moov_payload = [&mvhd[..], &trak].concat();
    let moov = box_wrap(b"moov", &moov_payload);
    let mdat = box_wrap(b"mdat", &[]);

    [ftyp, moov, mdat].concat()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Upload a valid MP4 video via Blossom, verify the BlobDescriptor includes
/// video-specific fields (duration, dim) and the blob is retrievable.
#[tokio::test]
#[ignore]
async fn test_video_upload_and_get() {
    let client = http_client();
    let keys = Keys::generate();
    let mp4 = build_test_mp4();
    let sha256 = hex::encode(Sha256::digest(&mp4));

    let auth = sign_blossom_auth(&keys, &sha256);
    let url = format!("{}/media/upload", relay_http_url());

    let resp = client
        .put(&url)
        .header("Authorization", blossom_auth_header(&auth))
        .header("Content-Type", "video/mp4")
        .body(mp4.clone())
        .send()
        .await
        .expect("upload request");

    assert_eq!(resp.status(), StatusCode::OK, "upload should succeed");

    let desc: serde_json::Value = resp.json().await.expect("json body");
    assert_eq!(desc["sha256"].as_str().unwrap(), sha256);
    assert_eq!(desc["type"].as_str().unwrap(), "video/mp4");
    assert!(desc["size"].as_u64().unwrap() > 0);
    // Video descriptor should have duration
    assert!(
        desc.get("duration").is_some(),
        "video descriptor should include duration"
    );

    // GET the blob back
    let get_url = desc["url"].as_str().unwrap();
    let get_resp = client.get(get_url).send().await.expect("GET blob");
    assert_eq!(get_resp.status(), StatusCode::OK);
    let body = get_resp.bytes().await.expect("body bytes");
    assert_eq!(body.len(), mp4.len());
}

/// Upload an MP4 as Content-Type: image/jpeg — should be rejected.
/// This tests the Content-Type spoofing fix: validate_content() rejects
/// video/mp4 from the image path.
#[tokio::test]
#[ignore]
async fn test_video_content_type_spoofing_rejected() {
    let client = http_client();
    let keys = Keys::generate();
    let mp4 = build_test_mp4();
    let sha256 = hex::encode(Sha256::digest(&mp4));

    let auth = sign_blossom_auth(&keys, &sha256);
    let url = format!("{}/media/upload", relay_http_url());

    // Upload MP4 bytes but claim it's image/jpeg
    let resp = client
        .put(&url)
        .header("Authorization", blossom_auth_header(&auth))
        .header("Content-Type", "image/jpeg")
        .body(mp4)
        .send()
        .await
        .expect("upload request");

    // Should be rejected — either 415 (DisallowedContentType) or 400
    assert!(
        resp.status() == StatusCode::UNSUPPORTED_MEDIA_TYPE
            || resp.status() == StatusCode::BAD_REQUEST,
        "MP4 uploaded as image/jpeg should be rejected, got {}",
        resp.status()
    );
}

/// Range request on a video blob should return 206 Partial Content.
#[tokio::test]
#[ignore]
async fn test_video_range_request_206() {
    let client = http_client();
    let keys = Keys::generate();
    let mp4 = build_test_mp4();
    let sha256 = hex::encode(Sha256::digest(&mp4));

    // Upload first
    let auth = sign_blossom_auth(&keys, &sha256);
    let url = format!("{}/media/upload", relay_http_url());
    let resp = client
        .put(&url)
        .header("Authorization", blossom_auth_header(&auth))
        .header("Content-Type", "video/mp4")
        .body(mp4.clone())
        .send()
        .await
        .expect("upload");
    assert_eq!(resp.status(), StatusCode::OK);
    let desc: serde_json::Value = resp.json().await.unwrap();
    let blob_url = desc["url"].as_str().unwrap();

    // Range request: first 100 bytes
    let range_resp = client
        .get(blob_url)
        .header("Range", "bytes=0-99")
        .send()
        .await
        .expect("range GET");

    assert_eq!(range_resp.status(), StatusCode::PARTIAL_CONTENT);
    assert!(range_resp.headers().get("content-range").is_some());
    assert!(range_resp
        .headers()
        .get("accept-ranges")
        .map_or(false, |v| v == "bytes"));
    let body = range_resp.bytes().await.unwrap();
    assert_eq!(body.len(), 100);
    assert_eq!(&body[..], &mp4[..100]);
}

/// Unsatisfiable range request should return 416.
#[tokio::test]
#[ignore]
async fn test_video_range_request_416() {
    let client = http_client();
    let keys = Keys::generate();
    let mp4 = build_test_mp4();
    let sha256 = hex::encode(Sha256::digest(&mp4));

    // Upload first
    let auth = sign_blossom_auth(&keys, &sha256);
    let url = format!("{}/media/upload", relay_http_url());
    let resp = client
        .put(&url)
        .header("Authorization", blossom_auth_header(&auth))
        .header("Content-Type", "video/mp4")
        .body(mp4.clone())
        .send()
        .await
        .expect("upload");
    assert_eq!(resp.status(), StatusCode::OK);
    let desc: serde_json::Value = resp.json().await.unwrap();
    let blob_url = desc["url"].as_str().unwrap();

    // Request a range beyond the file size
    let range_resp = client
        .get(blob_url)
        .header(
            "Range",
            format!("bytes={}-{}", mp4.len() + 1000, mp4.len() + 2000),
        )
        .send()
        .await
        .expect("range GET");

    assert_eq!(
        range_resp.status(),
        StatusCode::RANGE_NOT_SATISFIABLE,
        "out-of-range request should return 416"
    );
}

/// Upload without auth should return 401.
#[tokio::test]
#[ignore]
async fn test_video_upload_no_auth_returns_401() {
    let client = http_client();
    let mp4 = build_test_mp4();
    let url = format!("{}/media/upload", relay_http_url());

    let resp = client
        .put(&url)
        .header("Content-Type", "video/mp4")
        .body(mp4)
        .send()
        .await
        .expect("upload request");

    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "upload without auth should return 401"
    );
}
