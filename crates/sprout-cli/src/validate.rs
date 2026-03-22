use crate::error::CliError;

/// Maximum content size in bytes (64 KiB).
pub const MAX_CONTENT_BYTES: usize = 65_536;

/// Maximum diff size in bytes (60 KiB).
pub const MAX_DIFF_BYTES: usize = 61_440;

/// Validate UUID string. Returns CliError::Usage on failure.
pub fn validate_uuid(s: &str) -> Result<(), CliError> {
    uuid::Uuid::parse_str(s).map_err(|_| CliError::Usage(format!("invalid UUID: {s}")))?;
    Ok(())
}

/// Validate 64-character lowercase hex string (event_id, pubkey).
pub fn validate_hex64(s: &str) -> Result<(), CliError> {
    if s.len() != 64 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(CliError::Usage(format!(
            "must be a 64-character hex string: {s}"
        )));
    }
    Ok(())
}

/// Validate content does not exceed MAX_CONTENT_BYTES (65,536).
pub fn validate_content_size(content: &str) -> Result<(), CliError> {
    if content.len() > MAX_CONTENT_BYTES {
        return Err(CliError::Usage(format!(
            "content exceeds maximum size ({} > {} bytes)",
            content.len(),
            MAX_CONTENT_BYTES
        )));
    }
    Ok(())
}

/// Percent-encode for URL path segments and query parameter values.
/// Encodes all bytes except RFC 3986 unreserved: A-Z a-z 0-9 - _ . ~
pub fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                let hi = char::from_digit((byte >> 4) as u32, 16)
                    .unwrap()
                    .to_ascii_uppercase();
                let lo = char::from_digit((byte & 0xf) as u32, 16)
                    .unwrap()
                    .to_ascii_uppercase();
                out.push('%');
                out.push(hi);
                out.push(lo);
            }
        }
    }
    out
}

/// Truncate diff at hunk boundary within max_bytes (60 KiB for send-diff-message).
/// Returns (truncated_string, was_truncated).
pub fn truncate_diff(diff: &str, max_bytes: usize) -> (String, bool) {
    const TRUNCATION_NOTICE: &str = "\n\n[diff truncated — exceeded size limit]";
    if diff.len() <= max_bytes {
        return (diff.to_string(), false);
    }
    let effective_limit = max_bytes.saturating_sub(TRUNCATION_NOTICE.len());
    let utf8_boundary = diff
        .char_indices()
        .map(|(i, _)| i)
        .take_while(|&i| i <= effective_limit)
        .last()
        .unwrap_or(0);
    let safe_prefix = &diff[..utf8_boundary];
    let cut_point = safe_prefix
        .rfind("\n@@")
        .filter(|&p| p > 0)
        .unwrap_or_else(|| safe_prefix.rfind('\n').unwrap_or(utf8_boundary));
    let mut result = diff[..cut_point].to_string();
    result.push_str(TRUNCATION_NOTICE);
    (result, true)
}

/// Infer syntax-highlight language from file extension.
pub fn infer_language(file_path: &str) -> Option<String> {
    let ext = file_path.rsplit('.').next()?;
    let lang = match ext {
        "rs" => "rust",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" => "javascript",
        "py" => "python",
        "go" => "go",
        "java" => "java",
        "rb" => "ruby",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" => "cpp",
        "cs" => "csharp",
        "swift" => "swift",
        "kt" | "kts" => "kotlin",
        "scala" => "scala",
        "sh" | "bash" | "zsh" => "bash",
        "sql" => "sql",
        "html" | "htm" => "html",
        "css" | "scss" | "sass" => "css",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "xml" => "xml",
        "md" | "markdown" => "markdown",
        "dockerfile" => "dockerfile",
        _ => return None,
    };
    Some(lang.to_string())
}

/// Normalize mention pubkeys: lowercase, deduplicate, remove sender's own pubkey.
pub fn normalize_mention_pubkeys(pubkeys: &[String], sender_pubkey: &str) -> Vec<String> {
    let sender = sender_pubkey.to_ascii_lowercase();
    let mut seen = std::collections::HashSet::new();
    pubkeys
        .iter()
        .map(|pk| pk.to_ascii_lowercase())
        .filter(|pk| pk != &sender)
        .filter(|pk| seen.insert(pk.clone()))
        .collect()
}

/// Read content from a string value or stdin if the value is "-".
pub fn read_or_stdin(value: &str) -> Result<String, CliError> {
    if value == "-" {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| CliError::Other(format!("failed to read stdin: {e}")))?;
        Ok(buf)
    } else {
        Ok(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- validate_uuid ---

    #[test]
    fn validate_uuid_valid() {
        assert!(validate_uuid("550e8400-e29b-41d4-a716-446655440000").is_ok());
    }

    #[test]
    fn validate_uuid_malformed() {
        let err = validate_uuid("not-a-uuid").unwrap_err();
        assert!(matches!(err, CliError::Usage(_)));
    }

    #[test]
    fn validate_uuid_empty() {
        let err = validate_uuid("").unwrap_err();
        assert!(matches!(err, CliError::Usage(_)));
    }

    // --- validate_hex64 ---

    #[test]
    fn validate_hex64_valid() {
        let hex = "a".repeat(64);
        assert!(validate_hex64(&hex).is_ok());
    }

    #[test]
    fn validate_hex64_all_digits() {
        let hex = "0123456789abcdef".repeat(4);
        assert!(validate_hex64(&hex).is_ok());
    }

    #[test]
    fn validate_hex64_too_short() {
        let hex = "a".repeat(63);
        let err = validate_hex64(&hex).unwrap_err();
        assert!(matches!(err, CliError::Usage(_)));
    }

    #[test]
    fn validate_hex64_too_long() {
        let hex = "a".repeat(65);
        let err = validate_hex64(&hex).unwrap_err();
        assert!(matches!(err, CliError::Usage(_)));
    }

    #[test]
    fn validate_hex64_non_hex_char() {
        let mut hex = "a".repeat(63);
        hex.push('z'); // 'z' is not a hex digit
        let err = validate_hex64(&hex).unwrap_err();
        assert!(matches!(err, CliError::Usage(_)));
    }

    // --- validate_content_size ---

    #[test]
    fn validate_content_size_at_limit() {
        let content = "x".repeat(MAX_CONTENT_BYTES);
        assert!(validate_content_size(&content).is_ok());
    }

    #[test]
    fn validate_content_size_over_limit() {
        let content = "x".repeat(MAX_CONTENT_BYTES + 1);
        let err = validate_content_size(&content).unwrap_err();
        assert!(matches!(err, CliError::Usage(_)));
    }

    #[test]
    fn validate_content_size_empty() {
        assert!(validate_content_size("").is_ok());
    }

    // --- percent_encode ---

    #[test]
    fn percent_encode_unreserved_unchanged() {
        let input = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.~";
        assert_eq!(percent_encode(input), input);
    }

    #[test]
    fn percent_encode_space() {
        assert_eq!(percent_encode("hello world"), "hello%20world");
    }

    #[test]
    fn percent_encode_slash() {
        assert_eq!(percent_encode("a/b"), "a%2Fb");
    }

    #[test]
    fn percent_encode_unicode_multibyte() {
        // '€' is U+20AC, encoded as 3 UTF-8 bytes: 0xE2 0x82 0xAC
        assert_eq!(percent_encode("€"), "%E2%82%AC");
    }

    #[test]
    fn percent_encode_empty() {
        assert_eq!(percent_encode(""), "");
    }

    // --- truncate_diff ---

    #[test]
    fn truncate_diff_under_limit_noop() {
        let diff = "small diff";
        let (result, was_truncated) = truncate_diff(diff, 1000);
        assert_eq!(result, diff);
        assert!(!was_truncated);
    }

    #[test]
    fn truncate_diff_at_limit_noop() {
        let diff = "x".repeat(100);
        let (result, was_truncated) = truncate_diff(&diff, 100);
        assert_eq!(result, diff);
        assert!(!was_truncated);
    }

    #[test]
    fn truncate_diff_cuts_at_hunk_boundary() {
        // Build a diff with a @@ hunk marker after the limit
        let hunk1 = "@@ -1,3 +1,3 @@\n line1\n line2\n line3\n";
        let hunk2 = "@@ -5,3 +5,3 @@\n line4\n line5\n line6\n";
        let diff = format!("{}{}", hunk1, hunk2);
        // Limit to just past hunk1 but before hunk2 completes
        let limit = hunk1.len() + 5;
        let (result, was_truncated) = truncate_diff(&diff, limit);
        assert!(was_truncated);
        assert!(result.contains("[diff truncated — exceeded size limit]"));
        // Should cut at the \n@@ boundary before hunk2
        assert!(!result.contains("line4"));
    }

    #[test]
    fn truncate_diff_falls_back_to_newline() {
        // No @@ marker — should fall back to last newline
        let diff = "line one\nline two\nline three extra long content here";
        let limit = 20;
        let (result, was_truncated) = truncate_diff(diff, limit);
        assert!(was_truncated);
        assert!(result.contains("[diff truncated — exceeded size limit]"));
    }

    #[test]
    fn truncate_diff_appends_notice() {
        let diff = "x".repeat(200);
        let (result, was_truncated) = truncate_diff(&diff, 50);
        assert!(was_truncated);
        assert!(result.ends_with("[diff truncated — exceeded size limit]"));
    }

    // --- infer_language ---

    #[test]
    fn infer_language_rust() {
        assert_eq!(infer_language("main.rs"), Some("rust".to_string()));
    }

    #[test]
    fn infer_language_tsx() {
        assert_eq!(infer_language("App.tsx"), Some("typescript".to_string()));
    }

    #[test]
    fn infer_language_ts() {
        assert_eq!(infer_language("index.ts"), Some("typescript".to_string()));
    }

    #[test]
    fn infer_language_unknown_ext() {
        assert_eq!(infer_language("file.xyz"), None);
    }

    #[test]
    fn infer_language_no_ext() {
        assert_eq!(infer_language("Makefile"), None);
    }

    #[test]
    fn infer_language_path_with_dirs() {
        assert_eq!(
            infer_language("src/lib/utils.py"),
            Some("python".to_string())
        );
    }

    // --- normalize_mention_pubkeys ---

    #[test]
    fn normalize_mention_pubkeys_lowercases() {
        // 64 chars total
        let pk = "AABBCC".repeat(10) + "aabb";
        assert_eq!(pk.len(), 64);
        let result = normalize_mention_pubkeys(std::slice::from_ref(&pk), "sender");
        assert_eq!(result, vec![pk.to_ascii_lowercase()]);
    }

    #[test]
    fn normalize_mention_pubkeys_removes_sender() {
        let sender = "a".repeat(64);
        let other = "b".repeat(64);
        let pubkeys = vec![sender.clone(), other.clone()];
        let result = normalize_mention_pubkeys(&pubkeys, &sender);
        assert_eq!(result, vec![other]);
    }

    #[test]
    fn normalize_mention_pubkeys_deduplicates() {
        let pk = "c".repeat(64);
        let pubkeys = vec![pk.clone(), pk.clone(), pk.clone()];
        let result = normalize_mention_pubkeys(&pubkeys, "sender");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], pk);
    }

    #[test]
    fn normalize_mention_pubkeys_removes_sender_case_insensitive() {
        let sender_lower = "d".repeat(64);
        let sender_upper = sender_lower.to_ascii_uppercase();
        let other = "e".repeat(64);
        let pubkeys = vec![sender_upper, other.clone()];
        let result = normalize_mention_pubkeys(&pubkeys, &sender_lower);
        assert_eq!(result, vec![other]);
    }

    #[test]
    fn normalize_mention_pubkeys_empty_input() {
        let result = normalize_mention_pubkeys(&[], "sender");
        assert!(result.is_empty());
    }
}
