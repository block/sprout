use base64::{engine::general_purpose::STANDARD, Engine as _};
use png::Decoder;
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
// JSON persona parsing / encoding
// ---------------------------------------------------------------------------

pub fn parse_json_persona(json_bytes: &[u8]) -> Result<ParsedPersonaPreview, String> {
    let v: Value =
        serde_json::from_slice(json_bytes).map_err(|e| format!("Invalid JSON: {e}"))?;

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

    Ok(ParsedPersonaPreview {
        display_name: name,
        system_prompt: prompt,
        avatar_data_url: None,
        source_file: String::new(),
    })
}

pub fn encode_persona_json(
    display_name: &str,
    system_prompt: &str,
    avatar_url: Option<&str>,
) -> Result<Vec<u8>, String> {
    let mut map = serde_json::Map::new();
    map.insert("version".to_string(), serde_json::json!(1));
    map.insert(
        "displayName".to_string(),
        serde_json::json!(display_name),
    );
    map.insert(
        "systemPrompt".to_string(),
        serde_json::json!(system_prompt),
    );
    if let Some(url) = avatar_url {
        map.insert("avatarUrl".to_string(), serde_json::json!(url));
    }

    serde_json::to_vec_pretty(&map).map_err(|e| format!("Failed to serialize JSON: {e}"))
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
    let mut has_valid_file = false;

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

        let lower = name.to_ascii_lowercase();
        let is_png = lower.ends_with(".png");
        let is_json = lower.ends_with(".json");

        if !is_png && !is_json {
            skipped.push(SkippedFile {
                source_file: raw_name,
                reason: "Not a PNG or JSON file".to_string(),
            });
            continue;
        }

        has_valid_file = true;

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

        let parse_result = if is_json {
            parse_json_persona(&data)
        } else {
            parse_png_persona(&data)
        };

        match parse_result {
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

    if !has_valid_file {
        return Err("No persona files found (expected .png or .json).".to_string());
    }

    Ok(ParsePersonaFilesResult { personas, skipped })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use png::{BitDepth, ColorType, Encoder};
    use std::io::Write;
    use zip::write::{SimpleFileOptions, ZipWriter};

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

    /// Helper: build a PNG with a sprout_persona tEXt chunk for the given name/prompt.
    fn make_test_persona_png(name: &str, prompt: &str) -> Vec<u8> {
        let payload = serde_json::json!({
            "version": 1,
            "displayName": name,
            "systemPrompt": prompt,
        });
        let b64 = STANDARD.encode(payload.to_string().as_bytes());
        make_png_with_text("sprout_persona", &b64)
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
        let png = make_test_persona_png("George Costanza", "You are George.");
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
    fn parse_zip_valid_pack() {
        let p1 = make_test_persona_png("Alice", "Prompt A");
        let p2 = make_test_persona_png("Bob", "Prompt B");
        let p3 = make_test_persona_png("Carol", "Prompt C");
        let zip = make_test_zip(&[("alice.png", &p1), ("bob.png", &p2), ("carol.png", &p3)]);
        let result = parse_zip_personas(&zip).unwrap();
        assert_eq!(result.personas.len(), 3);
        assert!(result.skipped.is_empty());
        assert_eq!(result.personas[0].source_file, "alice.png");
    }

    #[test]
    fn parse_zip_mixed() {
        let valid1 = make_test_persona_png("Alice", "Prompt A");
        let valid2 = make_test_persona_png("Bob", "Prompt B");
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
        let zip = make_test_zip(&[("readme.txt", b"hello"), ("data.csv", b"a,b")]);
        let err = parse_zip_personas(&zip).unwrap_err();
        assert!(err.contains("No persona files found"));
    }

    #[test]
    fn parse_zip_exceeds_entry_limit() {
        let png = make_test_persona_png("X", "Y");
        let entries: Vec<(String, &[u8])> = (0..51).map(|i| (format!("{i}.png"), png.as_slice())).collect();
        let refs: Vec<(&str, &[u8])> = entries.iter().map(|(n, d)| (n.as_str(), *d)).collect();
        let zip = make_test_zip(&refs);
        let err = parse_zip_personas(&zip).unwrap_err();
        assert!(err.contains("too many entries"));
    }

    #[test]
    fn parse_zip_path_traversal() {
        let valid = make_test_persona_png("Safe", "Prompt");
        let evil = make_test_persona_png("Evil", "Prompt");
        let zip = make_test_zip(&[("safe.png", &valid), ("../evil.png", &evil)]);
        let result = parse_zip_personas(&zip).unwrap();
        assert_eq!(result.personas.len(), 1);
        assert_eq!(result.skipped.len(), 1);
        assert!(result.skipped[0].reason.contains("Path traversal"));
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
        let mut zip_buf = Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut zip_buf);
            let options = SimpleFileOptions::default();
            zip.start_file("big.png", options).unwrap();
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

    // --- JSON persona tests ---

    #[test]
    fn parse_json_round_trip() {
        let bytes =
            encode_persona_json("Ada Lovelace", "You are Ada.", Some("https://example.com/ada.png"))
                .unwrap();
        let result = parse_json_persona(&bytes).unwrap();
        assert_eq!(result.display_name, "Ada Lovelace");
        assert_eq!(result.system_prompt, "You are Ada.");
        assert!(result.avatar_data_url.is_none());
        assert!(result.source_file.is_empty());
    }

    #[test]
    fn parse_json_invalid_version() {
        let json = serde_json::json!({
            "version": 99,
            "displayName": "X",
            "systemPrompt": "Y"
        });
        let bytes = serde_json::to_vec(&json).unwrap();
        let err = parse_json_persona(&bytes).unwrap_err();
        assert!(err.contains("Unsupported persona version"));
    }

    #[test]
    fn parse_json_empty_fields() {
        let json_empty_name = serde_json::json!({
            "version": 1,
            "displayName": "",
            "systemPrompt": "Y"
        });
        let err = parse_json_persona(&serde_json::to_vec(&json_empty_name).unwrap()).unwrap_err();
        assert!(err.contains("displayName is empty"));

        let json_empty_prompt = serde_json::json!({
            "version": 1,
            "displayName": "X",
            "systemPrompt": ""
        });
        let err =
            parse_json_persona(&serde_json::to_vec(&json_empty_prompt).unwrap()).unwrap_err();
        assert!(err.contains("systemPrompt is empty"));
    }

    #[test]
    fn parse_json_malformed() {
        let err = parse_json_persona(b"not json at all").unwrap_err();
        assert!(err.contains("Invalid JSON"));
    }

    #[test]
    fn parse_zip_with_json() {
        let j1 = encode_persona_json("Alice", "Prompt A", None).unwrap();
        let j2 = encode_persona_json("Bob", "Prompt B", None).unwrap();
        let zip = make_test_zip(&[("alice.persona.json", &j1), ("bob.persona.json", &j2)]);
        let result = parse_zip_personas(&zip).unwrap();
        assert_eq!(result.personas.len(), 2);
        assert!(result.skipped.is_empty());
        assert_eq!(result.personas[0].display_name, "Alice");
        assert_eq!(result.personas[1].display_name, "Bob");
    }

    #[test]
    fn parse_zip_mixed_png_and_json() {
        let png = make_test_persona_png("PngPersona", "PNG prompt");
        let json = encode_persona_json("JsonPersona", "JSON prompt", None).unwrap();
        let zip = make_test_zip(&[
            ("persona.png", &png),
            ("persona.json", &json),
            ("readme.txt", b"hello"),
        ]);
        let result = parse_zip_personas(&zip).unwrap();
        assert_eq!(result.personas.len(), 2);
        // readme.txt should be skipped
        assert_eq!(result.skipped.len(), 1);
        assert!(result.skipped[0].reason.contains("Not a PNG or JSON file"));
    }
}
