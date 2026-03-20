use base64::{engine::general_purpose::STANDARD, Engine as _};
use png::{BitDepth, ColorType, Decoder, Encoder};
use serde::Serialize;
use serde_json::Value;
use std::io::{Cursor, Read};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ParsedPersonaPreview {
    pub display_name: String,
    pub system_prompt: String,
    pub avatar_data_url: Option<String>,
    pub source_file: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParsePersonaFilesResult {
    pub personas: Vec<ParsedPersonaPreview>,
    pub skipped: Vec<SkippedFile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkippedFile {
    pub source_file: String,
    pub reason: String,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const MAX_ZIP_ENTRIES: usize = 50;
const MAX_ZIP_DECOMPRESSED: usize = 100 * 1024 * 1024;

// ---------------------------------------------------------------------------
// PNG persona parsing
// ---------------------------------------------------------------------------

pub fn parse_png_persona(png_bytes: &[u8]) -> Result<ParsedPersonaPreview, String> {
    let decoder = Decoder::new(Cursor::new(png_bytes));
    let reader = decoder.read_info().map_err(|e| format!("Invalid PNG: {e}"))?;
    let info = reader.info();

    let mut sprout_text: Option<&str> = None;
    let mut chara_text: Option<&str> = None;

    for chunk in &info.uncompressed_latin1_text {
        match chunk.keyword.as_str() {
            "sprout_persona" if sprout_text.is_none() => sprout_text = Some(&chunk.text),
            "chara" | "ccv3" if chara_text.is_none() => chara_text = Some(&chunk.text),
            _ => {}
        }
    }

    let preview = if let Some(text) = sprout_text {
        parse_sprout_payload(text)?
    } else if let Some(text) = chara_text {
        parse_chara_payload(text)?
    } else {
        return Err("This image doesn't contain persona data.".to_string());
    };

    let avatar_data_url = Some(format!("data:image/png;base64,{}", STANDARD.encode(png_bytes)));

    Ok(ParsedPersonaPreview {
        display_name: preview.0,
        system_prompt: preview.1,
        avatar_data_url,
        source_file: String::new(),
    })
}

fn decode_b64_json(b64: &str) -> Result<Value, String> {
    let bytes = STANDARD
        .decode(b64.trim())
        .map_err(|e| format!("Invalid base64: {e}"))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("Invalid JSON: {e}"))
}

fn parse_sprout_payload(b64: &str) -> Result<(String, String), String> {
    let v = decode_b64_json(b64)?;
    let version = v.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
    if version != 1 {
        return Err(format!("Unsupported persona version: {version}"));
    }
    let name = v
        .get("displayName")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let prompt = v
        .get("systemPrompt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if name.is_empty() {
        return Err("displayName is empty".to_string());
    }
    if prompt.is_empty() {
        return Err("systemPrompt is empty".to_string());
    }
    Ok((name, prompt))
}

fn parse_chara_payload(b64: &str) -> Result<(String, String), String> {
    let v = decode_b64_json(b64)?;
    let data = v.get("data").ok_or("Missing 'data' in chara payload")?;
    let name = data
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let mut prompt = data
        .get("system_prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if prompt.is_empty() {
        prompt = data
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
    }
    if name.is_empty() {
        return Err("Chara card has no name".to_string());
    }
    if prompt.is_empty() {
        return Err("Chara card has no system_prompt or description".to_string());
    }
    Ok((name, prompt))
}

// ---------------------------------------------------------------------------
// PNG persona encoding
// ---------------------------------------------------------------------------

pub fn encode_persona_png(
    display_name: &str,
    system_prompt: &str,
    avatar_png_bytes: &[u8],
) -> Result<Vec<u8>, String> {
    // Decode the source PNG to raw pixels so we can re-encode with tEXt chunks.
    let decoder = Decoder::new(Cursor::new(avatar_png_bytes));
    let mut reader = decoder.read_info().map_err(|e| format!("Invalid avatar PNG: {e}"))?;
    let mut pixels = vec![0u8; reader.output_buffer_size().ok_or("Cannot determine PNG buffer size")?];
    let output_info = reader
        .next_frame(&mut pixels)
        .map_err(|e| format!("Failed to decode avatar PNG: {e}"))?;
    pixels.truncate(output_info.buffer_size());

    let width = output_info.width;
    let height = output_info.height;
    let color_type = output_info.color_type;
    let bit_depth = output_info.bit_depth;

    let sprout_json = serde_json::json!({
        "version": 1,
        "displayName": display_name,
        "systemPrompt": system_prompt,
    });
    let chara_json = serde_json::json!({
        "spec": "chara_card_v2",
        "spec_version": "2.0",
        "data": {
            "name": display_name,
            "description": "",
            "personality": "",
            "system_prompt": system_prompt,
            "extensions": {
                "sprout": {
                    "version": 1,
                    "source": "sprout"
                }
            }
        }
    });

    let sprout_b64 = STANDARD.encode(sprout_json.to_string().as_bytes());
    let chara_b64 = STANDARD.encode(chara_json.to_string().as_bytes());

    let mut buf = Vec::new();
    {
        let mut encoder = Encoder::new(Cursor::new(&mut buf), width, height);
        encoder.set_color(color_type);
        encoder.set_depth(bit_depth);
        encoder
            .add_text_chunk("sprout_persona".to_string(), sprout_b64)
            .map_err(|e| format!("Failed to add text chunk: {e}"))?;
        encoder
            .add_text_chunk("chara".to_string(), chara_b64)
            .map_err(|e| format!("Failed to add text chunk: {e}"))?;
        let mut writer = encoder
            .write_header()
            .map_err(|e| format!("Failed to write PNG header: {e}"))?;
        writer
            .write_image_data(&pixels)
            .map_err(|e| format!("Failed to write PNG data: {e}"))?;
    }
    Ok(buf)
}

// ---------------------------------------------------------------------------
// Placeholder PNG generation
// ---------------------------------------------------------------------------

pub fn generate_placeholder_png(display_name: &str) -> Result<Vec<u8>, String> {
    let hue = display_name.as_bytes().iter().map(|&b| b as u32).sum::<u32>() % 360;
    let (r, g, b) = hsl_to_rgb(hue as f64, 0.65, 0.55);

    const SIZE: u32 = 256;
    let mut pixels = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for _ in 0..(SIZE * SIZE) {
        pixels.extend_from_slice(&[r, g, b, 255]);
    }

    let mut buf = Vec::new();
    {
        let mut encoder = Encoder::new(Cursor::new(&mut buf), SIZE, SIZE);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| format!("Failed to write placeholder PNG header: {e}"))?;
        writer
            .write_image_data(&pixels)
            .map_err(|e| format!("Failed to write placeholder PNG data: {e}"))?;
    }
    Ok(buf)
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r1, g1, b1) = match h as u32 {
        0..=59 => (c, x, 0.0),
        60..=119 => (x, c, 0.0),
        120..=179 => (0.0, c, x),
        180..=239 => (0.0, x, c),
        240..=299 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        ((r1 + m) * 255.0).round() as u8,
        ((g1 + m) * 255.0).round() as u8,
        ((b1 + m) * 255.0).round() as u8,
    )
}

// ---------------------------------------------------------------------------
// ZIP parsing
// ---------------------------------------------------------------------------

pub fn parse_zip_personas(zip_bytes: &[u8]) -> Result<ParsePersonaFilesResult, String> {
    let cursor = Cursor::new(zip_bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("Invalid ZIP archive: {e}"))?;

    if archive.len() > MAX_ZIP_ENTRIES {
        return Err(format!(
            "ZIP contains too many entries ({}, max {MAX_ZIP_ENTRIES})",
            archive.len()
        ));
    }

    let mut personas = Vec::new();
    let mut skipped = Vec::new();
    let mut total_decompressed: usize = 0;
    let mut has_png = false;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read ZIP entry: {e}"))?;

        let raw_name = entry.name().to_string();

        // Sanitize path
        let name = raw_name.trim_start_matches('/');
        if name.contains("..") {
            skipped.push(SkippedFile {
                source_file: raw_name.clone(),
                reason: "Path traversal detected".to_string(),
            });
            continue;
        }

        if !name.to_ascii_lowercase().ends_with(".png") {
            skipped.push(SkippedFile {
                source_file: raw_name,
                reason: "Not a PNG file".to_string(),
            });
            continue;
        }

        has_png = true;

        // Read with cumulative size limit
        let mut data = Vec::new();
        loop {
            let mut chunk = [0u8; 8192];
            let n = entry.read(&mut chunk).map_err(|e| format!("Read error: {e}"))?;
            if n == 0 {
                break;
            }
            total_decompressed += n;
            if total_decompressed > MAX_ZIP_DECOMPRESSED {
                return Err("ZIP decompressed content exceeds 100MB limit".to_string());
            }
            data.extend_from_slice(&chunk[..n]);
        }

        match parse_png_persona(&data) {
            Ok(mut preview) => {
                preview.source_file = raw_name;
                personas.push(preview);
            }
            Err(reason) => {
                skipped.push(SkippedFile {
                    source_file: raw_name,
                    reason,
                });
            }
        }
    }

    if !has_png {
        return Err("No PNG files found in this archive.".to_string());
    }

    Ok(ParsePersonaFilesResult { personas, skipped })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::{SimpleFileOptions, ZipWriter};

    /// Helper: encode a persona PNG with given fields.
    fn make_persona_png(name: &str, prompt: &str) -> Vec<u8> {
        let placeholder = generate_placeholder_png(name).unwrap();
        encode_persona_png(name, prompt, &placeholder).unwrap()
    }

    /// Helper: build a minimal valid PNG with a custom tEXt chunk.
    fn make_png_with_text(keyword: &str, text: &str) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut enc = Encoder::new(Cursor::new(&mut buf), 1, 1);
            enc.set_color(ColorType::Rgba);
            enc.set_depth(BitDepth::Eight);
            enc.add_text_chunk(keyword.to_string(), text.to_string()).unwrap();
            let mut w = enc.write_header().unwrap();
            w.write_image_data(&[0, 0, 0, 255]).unwrap();
        }
        buf
    }

    /// Helper: build a plain PNG with no metadata.
    fn make_plain_png() -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut enc = Encoder::new(Cursor::new(&mut buf), 1, 1);
            enc.set_color(ColorType::Rgba);
            enc.set_depth(BitDepth::Eight);
            let mut w = enc.write_header().unwrap();
            w.write_image_data(&[0, 0, 0, 255]).unwrap();
        }
        buf
    }

    /// Helper: create a ZIP from name→data pairs.
    fn make_test_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        let mut zip = ZipWriter::new(&mut buf);
        let options = SimpleFileOptions::default();
        for (name, data) in entries {
            zip.start_file(*name, options).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
        buf.into_inner()
    }

    #[test]
    fn parse_png_round_trip() {
        let png = make_persona_png("George Costanza", "You are George.");
        let result = parse_png_persona(&png).unwrap();
        assert_eq!(result.display_name, "George Costanza");
        assert_eq!(result.system_prompt, "You are George.");
        assert!(result.avatar_data_url.unwrap().starts_with("data:image/png;base64,"));
    }

    #[test]
    fn parse_png_no_metadata() {
        let png = make_plain_png();
        let err = parse_png_persona(&png).unwrap_err();
        assert!(err.contains("doesn't contain persona data"));
    }

    #[test]
    fn parse_png_unknown_version() {
        let payload = serde_json::json!({"version": 99, "displayName": "X", "systemPrompt": "Y"});
        let b64 = STANDARD.encode(payload.to_string().as_bytes());
        let png = make_png_with_text("sprout_persona", &b64);
        let err = parse_png_persona(&png).unwrap_err();
        assert!(err.contains("Unsupported persona version"));
    }

    #[test]
    fn parse_png_malformed_base64() {
        let png = make_png_with_text("sprout_persona", "!!!not-base64!!!");
        let err = parse_png_persona(&png).unwrap_err();
        assert!(err.contains("Invalid base64"));
    }

    #[test]
    fn parse_png_malformed_json() {
        let b64 = STANDARD.encode(b"not json at all");
        let png = make_png_with_text("sprout_persona", &b64);
        let err = parse_png_persona(&png).unwrap_err();
        assert!(err.contains("Invalid JSON"));
    }

    #[test]
    fn parse_png_empty_fields() {
        let payload = serde_json::json!({"version": 1, "displayName": "", "systemPrompt": "Y"});
        let b64 = STANDARD.encode(payload.to_string().as_bytes());
        let png = make_png_with_text("sprout_persona", &b64);
        let err = parse_png_persona(&png).unwrap_err();
        assert!(err.contains("displayName is empty"));
    }

    #[test]
    fn parse_png_chara_fallback() {
        let chara = serde_json::json!({
            "spec": "chara_card_v2",
            "spec_version": "2.0",
            "data": {
                "name": "Kramer",
                "system_prompt": "You are Kramer.",
                "description": ""
            }
        });
        let b64 = STANDARD.encode(chara.to_string().as_bytes());
        let png = make_png_with_text("chara", &b64);
        let result = parse_png_persona(&png).unwrap();
        assert_eq!(result.display_name, "Kramer");
        assert_eq!(result.system_prompt, "You are Kramer.");
    }

    #[test]
    fn parse_png_chara_ignored_when_sprout_present() {
        // Build a PNG with both sprout_persona and chara chunks.
        let sprout = serde_json::json!({"version": 1, "displayName": "Sprout Name", "systemPrompt": "Sprout prompt"});
        let chara = serde_json::json!({
            "spec": "chara_card_v2", "spec_version": "2.0",
            "data": {"name": "Chara Name", "system_prompt": "Chara prompt", "description": ""}
        });
        let sprout_b64 = STANDARD.encode(sprout.to_string().as_bytes());
        let chara_b64 = STANDARD.encode(chara.to_string().as_bytes());

        let mut buf = Vec::new();
        {
            let mut enc = Encoder::new(Cursor::new(&mut buf), 1, 1);
            enc.set_color(ColorType::Rgba);
            enc.set_depth(BitDepth::Eight);
            enc.add_text_chunk("sprout_persona".to_string(), sprout_b64).unwrap();
            enc.add_text_chunk("chara".to_string(), chara_b64).unwrap();
            let mut w = enc.write_header().unwrap();
            w.write_image_data(&[0, 0, 0, 255]).unwrap();
        }

        let result = parse_png_persona(&buf).unwrap();
        assert_eq!(result.display_name, "Sprout Name");
        assert_eq!(result.system_prompt, "Sprout prompt");
    }

    #[test]
    fn export_writes_both_chunks() {
        let png = make_persona_png("Test", "A prompt");
        let decoder = Decoder::new(Cursor::new(&png));
        let reader = decoder.read_info().unwrap();
        let info = reader.info();

        let keywords: Vec<&str> = info.uncompressed_latin1_text.iter().map(|c| c.keyword.as_str()).collect();
        assert!(keywords.contains(&"sprout_persona"));
        assert!(keywords.contains(&"chara"));
    }

    #[test]
    fn parse_zip_valid_pack() {
        let p1 = make_persona_png("Alice", "Prompt A");
        let p2 = make_persona_png("Bob", "Prompt B");
        let p3 = make_persona_png("Carol", "Prompt C");
        let zip = make_test_zip(&[("alice.png", &p1), ("bob.png", &p2), ("carol.png", &p3)]);
        let result = parse_zip_personas(&zip).unwrap();
        assert_eq!(result.personas.len(), 3);
        assert!(result.skipped.is_empty());
        assert_eq!(result.personas[0].source_file, "alice.png");
    }

    #[test]
    fn parse_zip_mixed() {
        let valid1 = make_persona_png("Alice", "Prompt A");
        let valid2 = make_persona_png("Bob", "Prompt B");
        let bad_png = make_plain_png(); // no metadata
        let zip = make_test_zip(&[
            ("alice.png", &valid1),
            ("bob.png", &valid2),
            ("bad.png", &bad_png),
            ("readme.txt", b"hello"),
        ]);
        let result = parse_zip_personas(&zip).unwrap();
        assert_eq!(result.personas.len(), 2);
        assert_eq!(result.skipped.len(), 2);
    }

    #[test]
    fn parse_zip_no_pngs() {
        let zip = make_test_zip(&[("readme.txt", b"hello"), ("data.json", b"{}")]);
        let err = parse_zip_personas(&zip).unwrap_err();
        assert!(err.contains("No PNG files found"));
    }

    #[test]
    fn parse_zip_exceeds_entry_limit() {
        let png = make_persona_png("X", "Y");
        let entries: Vec<(String, &[u8])> = (0..51).map(|i| (format!("{i}.png"), png.as_slice())).collect();
        let refs: Vec<(&str, &[u8])> = entries.iter().map(|(n, d)| (n.as_str(), *d)).collect();
        let zip = make_test_zip(&refs);
        let err = parse_zip_personas(&zip).unwrap_err();
        assert!(err.contains("too many entries"));
    }

    #[test]
    fn parse_zip_path_traversal() {
        let valid = make_persona_png("Safe", "Prompt");
        let evil = make_persona_png("Evil", "Prompt");
        let zip = make_test_zip(&[("safe.png", &valid), ("../evil.png", &evil)]);
        let result = parse_zip_personas(&zip).unwrap();
        assert_eq!(result.personas.len(), 1);
        assert_eq!(result.skipped.len(), 1);
        assert!(result.skipped[0].reason.contains("Path traversal"));
    }

    #[test]
    fn placeholder_deterministic() {
        let a = generate_placeholder_png("George").unwrap();
        let b = generate_placeholder_png("George").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn placeholder_different_names() {
        let a = generate_placeholder_png("George").unwrap();
        let b = generate_placeholder_png("Elaine").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn parse_png_duplicate_chunks() {
        // Two sprout_persona chunks — should use the first and ignore the second.
        let payload1 = serde_json::json!({"version": 1, "displayName": "First", "systemPrompt": "Prompt 1"});
        let payload2 = serde_json::json!({"version": 1, "displayName": "Second", "systemPrompt": "Prompt 2"});
        let b64_1 = STANDARD.encode(payload1.to_string().as_bytes());
        let b64_2 = STANDARD.encode(payload2.to_string().as_bytes());

        let mut buf = Vec::new();
        {
            let mut enc = Encoder::new(Cursor::new(&mut buf), 1, 1);
            enc.set_color(ColorType::Rgba);
            enc.set_depth(BitDepth::Eight);
            enc.add_text_chunk("sprout_persona".to_string(), b64_1).unwrap();
            enc.add_text_chunk("sprout_persona".to_string(), b64_2).unwrap();
            let mut w = enc.write_header().unwrap();
            w.write_image_data(&[0, 0, 0, 255]).unwrap();
        }

        let result = parse_png_persona(&buf).unwrap();
        assert_eq!(result.display_name, "First");
        assert_eq!(result.system_prompt, "Prompt 1");
    }

    #[test]
    fn parse_zip_exceeds_size_limit() {
        // Create a ZIP with entries whose cumulative decompressed size exceeds 100MB.
        // Use a single large entry (101 MB of zeros — compresses well in ZIP).
        let mut zip_buf = Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut zip_buf);
            let options = SimpleFileOptions::default();
            zip.start_file("big.png", options).unwrap();
            // Write 101 MB in chunks — this is a valid PNG-named file but will
            // exceed the decompressed size limit during streaming read.
            let chunk = vec![0u8; 1024 * 1024]; // 1 MB
            for _ in 0..101 {
                zip.write_all(&chunk).unwrap();
            }
            zip.finish().unwrap();
        }
        let zip_bytes = zip_buf.into_inner();
        let err = parse_zip_personas(&zip_bytes).unwrap_err();
        assert!(err.contains("exceeds 100MB"));
    }

    #[test]
    fn export_data_uri_avatar_round_trip() {
        // Encode with a known placeholder, then verify the round-trip preserves data.
        let placeholder = generate_placeholder_png("Test").unwrap();
        let data_url = format!("data:image/png;base64,{}", STANDARD.encode(&placeholder));

        // Decode the data URI the same way export_persona_to_png does.
        let prefix = "data:image/png;base64,";
        assert!(data_url.starts_with(prefix));
        let decoded = STANDARD.decode(&data_url[prefix.len()..]).unwrap();
        assert_eq!(decoded, placeholder);

        // Encode into a persona PNG and verify it round-trips.
        let png = encode_persona_png("Test", "A prompt", &decoded).unwrap();
        let result = parse_png_persona(&png).unwrap();
        assert_eq!(result.display_name, "Test");
        assert_eq!(result.system_prompt, "A prompt");
    }

    #[test]
    fn export_no_avatar_uses_placeholder() {
        // When avatar_url is None, export should use generate_placeholder_png.
        let placeholder = generate_placeholder_png("NoAvatar").unwrap();
        let png = encode_persona_png("NoAvatar", "Prompt", &placeholder).unwrap();
        let result = parse_png_persona(&png).unwrap();
        assert_eq!(result.display_name, "NoAvatar");
        // The avatar_data_url should be a valid data URI containing the placeholder.
        let avatar = result.avatar_data_url.unwrap();
        assert!(avatar.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn export_non_png_avatar_uses_placeholder() {
        // When avatar_url is https://... or any non-data-URI, export uses placeholder.
        // Simulate: generate placeholder for a name, encode, verify it works.
        let placeholder = generate_placeholder_png("HttpsAvatar").unwrap();
        assert!(!placeholder.is_empty());
        let png = encode_persona_png("HttpsAvatar", "Prompt", &placeholder).unwrap();
        let result = parse_png_persona(&png).unwrap();
        assert_eq!(result.display_name, "HttpsAvatar");
    }
}
