//! Content validation — magic bytes, allowlist, size, image bomb protection.

use crate::config::MediaConfig;
use crate::error::MediaError;

/// V1: images only. Video deferred to v2.
const ALLOWED_MIME_TYPES: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/webp",
];

/// Validate uploaded bytes: magic bytes, allowlist, size, pixel dimensions.
pub fn validate_content(bytes: &[u8], config: &MediaConfig) -> Result<String, MediaError> {
    // 1. Magic bytes — never trust Content-Type header
    let mime = infer::get(bytes)
        .map(|t| t.mime_type().to_string())
        .ok_or(MediaError::UnknownContentType)?;

    // 2. Allowlist (SVG, PDF, executables all rejected)
    if !ALLOWED_MIME_TYPES.contains(&mime.as_str()) {
        return Err(MediaError::DisallowedContentType(mime));
    }

    // 3. Size — GIF-specific cap (animated GIFs are CPU-intensive)
    let max = if mime == "image/gif" {
        config.max_gif_bytes
    } else {
        config.max_image_bytes
    };
    if bytes.len() as u64 > max {
        return Err(MediaError::FileTooLarge {
            size: bytes.len() as u64,
            max,
        });
    }

    // 4. Image bomb — check pixel dimensions before full decode.
    //    Fail closed for all accepted types: imagesize supports JPEG, PNG, GIF, WebP.
    //    If dimensions can't be parsed, reject — don't let unknown-geometry images
    //    reach the full decoder in thumbnail generation.
    const MAX_PIXELS: u64 = 25_000_000; // 25 megapixels — 100MB max RGBA decode
    let size = imagesize::blob_size(bytes).map_err(|_| MediaError::InvalidImage)?;
    if (size.width as u64) * (size.height as u64) > MAX_PIXELS {
        return Err(MediaError::ImageTooLarge);
    }

    Ok(mime)
}

/// Map MIME type to file extension.
pub fn mime_to_ext(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => "bin",
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
            public_base_url: String::new(),
            server_domain: None,
        }
    }

    // Minimal valid JPEG: SOI + APP0 + SOF0 (1x1px).
    // SOF0 is required for imagesize to parse dimensions (fail-closed check).
    const TINY_JPEG: &[u8] = &[
        // SOI
        0xFF, 0xD8,
        // APP0 (JFIF marker)
        0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00,
        0x01, 0x01, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00,
        // SOF0: precision=8, height=1, width=1, components=1
        0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01, 0x00, 0x01, 0x01, 0x01, 0x11, 0x00,
        // EOI
        0xFF, 0xD9,
    ];

    // Minimal PNG header
    const TINY_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // 1x1
        0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, 0xDE,
    ];

    #[test]
    fn test_validate_jpeg() {
        let config = test_config();
        let result = validate_content(TINY_JPEG, &config);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "image/jpeg");
    }

    #[test]
    fn test_validate_png() {
        let config = test_config();
        let result = validate_content(TINY_PNG, &config);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "image/png");
    }

    #[test]
    fn test_validate_svg_rejected() {
        let config = test_config();
        // SVG starts with XML declaration — infer won't detect it as image
        let svg = b"<?xml version=\"1.0\"?><svg xmlns=\"http://www.w3.org/2000/svg\"></svg>";
        let result = validate_content(svg, &config);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_oversized() {
        let mut config = test_config();
        config.max_image_bytes = 10; // 10 bytes max
        let result = validate_content(TINY_JPEG, &config);
        assert!(matches!(result, Err(MediaError::FileTooLarge { .. })));
    }

    // Minimal valid GIF89a (1x1 pixel) — full logical screen descriptor so imagesize can parse.
    const TINY_GIF: &[u8] = &[
        // Header
        0x47, 0x49, 0x46, 0x38, 0x39, 0x61,
        // Logical Screen Descriptor: width=1, height=1, flags, bgcolor, aspect
        0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00,
        // Global Color Table (2 colors: white, black)
        0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00,
        // Image Descriptor
        0x2C, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00,
        // Image Data
        0x02, 0x02, 0x4C, 0x01, 0x00,
        // Trailer
        0x3B,
    ];

    #[test]
    fn test_validate_gif_cap() {
        let mut config = test_config();
        config.max_gif_bytes = 5; // tiny cap
        config.max_image_bytes = 50 * 1024 * 1024;
        let result = validate_content(TINY_GIF, &config);
        assert!(matches!(result, Err(MediaError::FileTooLarge { .. })));
    }

    #[test]
    fn test_validate_gif_ok() {
        let config = test_config();
        let result = validate_content(TINY_GIF, &config);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "image/gif");
    }

    #[test]
    fn test_mime_to_ext() {
        assert_eq!(mime_to_ext("image/jpeg"), "jpg");
        assert_eq!(mime_to_ext("image/png"), "png");
        assert_eq!(mime_to_ext("image/gif"), "gif");
        assert_eq!(mime_to_ext("image/webp"), "webp");
        assert_eq!(mime_to_ext("application/pdf"), "bin");
    }
}
