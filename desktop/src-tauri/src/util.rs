use chrono::Utc;

pub fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

/// Turn a human-readable name into a filesystem-safe slug.
///
/// Non-alphanumeric characters become hyphens, leading/trailing hyphens are
/// stripped, and the result is capped at `max_len` characters (on a hyphen
/// boundary when possible). Returns `fallback` when the input produces an
/// empty slug.
pub fn slugify(name: &str, fallback: &str, max_len: usize) -> String {
    let raw: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    let raw = if raw.is_empty() { fallback } else { &raw };
    let raw = if raw.len() > max_len {
        &raw[..max_len]
    } else {
        raw
    };
    raw.trim_end_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::slugify;

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify("My Cool Team", "team", 50), "my-cool-team");
    }

    #[test]
    fn slugify_special_chars() {
        assert_eq!(slugify("héllo wörld!", "fallback", 50), "h-llo-w-rld");
    }

    #[test]
    fn slugify_empty_uses_fallback() {
        assert_eq!(slugify("   ", "persona", 50), "persona");
        assert_eq!(slugify("", "team", 50), "team");
    }

    #[test]
    fn slugify_truncates_at_max_len() {
        let long_name = "a]".repeat(60);
        let result = slugify(&long_name, "fallback", 10);
        assert!(result.len() <= 10);
        assert!(!result.ends_with('-'));
    }

    #[test]
    fn slugify_trims_trailing_hyphens_after_truncation() {
        // "abcde-----fghij" truncated at 10 → "abcde-----" → trimmed → "abcde"
        assert_eq!(slugify("abcde     fghij", "x", 10), "abcde");
    }
}

pub fn percent_encode(input: &str) -> String {
    let mut encoded = String::with_capacity(input.len());

    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                let high = char::from_digit((byte >> 4) as u32, 16)
                    .expect("nibble 0-15 is always a valid hex digit")
                    .to_ascii_uppercase();
                let low = char::from_digit((byte & 0x0f) as u32, 16)
                    .expect("nibble 0-15 is always a valid hex digit")
                    .to_ascii_uppercase();
                encoded.push('%');
                encoded.push(high);
                encoded.push(low);
            }
        }
    }

    encoded
}
