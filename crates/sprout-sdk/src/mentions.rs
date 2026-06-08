//! `@name` mention resolution helpers for Sprout chat messages.
//!
//! These helpers are **pure** — no network calls, no async. Callers query
//! channel membership (kind 39002) and profile (kind 0) events themselves,
//! then hand the profile JSON to [`match_names_to_profiles`].
//!
//! ## Pipeline
//!
//! ```text
//! body text ──► extract_at_names ──► names: Vec<String>
//!                                       │
//! members + profiles (queried by caller) │
//!                                       ▼
//!                            match_names_to_profiles
//!                                       │
//! explicit mentions ──► normalize ──► merge_mentions ──► p-tags
//! ```
//!
//! For callers that have the set of known member display names available
//! upfront, [`extract_at_mentions_with_known`] provides a two-pass approach
//! that correctly handles multi-word display names (e.g. "Will Pfleger"):
//!
//! ```text
//! body text + known_names ──► extract_at_mentions_with_known ──► names: Vec<String>
//!                                                                      │
//!                                                    direct name→pubkey lookup
//!                                                                      │
//!                                                              p-tags (merged)
//! ```
//!
//! See [`crate::mentions::MENTION_CAP`] for the hard upper bound on tags.

use std::collections::HashSet;

/// Maximum number of mention p-tags allowed on a single message.
///
/// Matches the cap enforced by Sprout message builders and the legacy MCP
/// inline implementation.
pub const MENTION_CAP: usize = 50;

/// A channel-member profile, as needed for name matching.
///
/// `pubkey` is the lowercase hex public key. `content_json` is the raw
/// kind 0 event content (a JSON object). Borrowing the content avoids
/// cloning what can be a sizable string.
#[derive(Debug, Clone, Copy)]
pub struct MentionProfile<'a> {
    /// Lowercase hex public key.
    pub pubkey: &'a str,
    /// Raw kind 0 event `content` field (a JSON object).
    pub content_json: &'a str,
}

/// Extract `@mention` names from message content.
///
/// Returns lowercased names found after `@` tokens. An `@name` only matches
/// when the `@` is at start-of-string or preceded by an ASCII whitespace
/// character — this excludes things like email addresses (`user@host`).
///
/// Allowed name characters: ASCII alphanumerics, `.`, `-`, `_`.
/// Duplicates are removed; first-seen order is preserved.
pub fn extract_at_names(content: &str) -> Vec<String> {
    if content.is_empty() || !content.contains('@') {
        return vec![];
    }
    let mut names: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    let chars: Vec<char> = content.chars().collect();
    let len = chars.len();
    let mut i = 0;
    while i < len {
        if chars[i] == '@' {
            let preceded_by_ws = i == 0 || chars[i - 1].is_ascii_whitespace();
            if preceded_by_ws && i + 1 < len {
                let start = i + 1;
                let mut end = start;
                while end < len {
                    let c = chars[end];
                    if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                        end += 1;
                    } else {
                        break;
                    }
                }
                if end > start {
                    let name: String = chars[start..end].iter().collect();
                    let lower = name.to_ascii_lowercase();
                    if seen.insert(lower.clone()) {
                        names.push(lower);
                    }
                }
            }
        }
        i += 1;
    }
    names
}

/// Extract `@mention` names from message content when the set of known member
/// display names is available upfront.
///
/// Uses a two-pass approach to correctly handle multi-word display names
/// (e.g. "Will Pfleger"):
///
/// **Pass 1 — known-name matching:** At each `@` token (preceded by
/// start-of-string or ASCII whitespace), try each known name longest-first,
/// case-insensitively. A match is accepted only when the name is followed by a
/// word boundary (whitespace, common punctuation, or end-of-string). When a
/// known name matches, the lowercased name is emitted and the scan advances
/// past the entire matched name.
///
/// **Pass 2 — single-word fallback:** If no known name matches at a given `@`,
/// falls back to the existing single-word tokenizer (alphanumeric + `.` `-`
/// `_`) so that `@alice` still works even when Alice's profile hasn't been
/// fetched yet.
///
/// `known_names` should be the display names (or `name` fallbacks) of all
/// channel members. Duplicates and empty strings are ignored. The function
/// does **not** require `known_names` to be pre-sorted — it sorts
/// longest-first internally.
///
/// Returns lowercased names in first-seen order, deduplicated.
pub fn extract_at_mentions_with_known(content: &str, known_names: &[&str]) -> Vec<String> {
    if content.is_empty() || !content.contains('@') {
        return vec![];
    }

    // Sort known names longest-first so multi-word names beat their prefixes.
    let mut sorted_known: Vec<&str> = known_names
        .iter()
        .copied()
        .filter(|n| !n.trim().is_empty())
        .collect();
    sorted_known.sort_by_key(|k| std::cmp::Reverse(k.len()));

    let mut names: Vec<String> = Vec::new();
    let mut seen = HashSet::new();
    let chars: Vec<char> = content.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if chars[i] == '@' {
            let preceded_by_ws = i == 0 || chars[i - 1].is_ascii_whitespace();
            if preceded_by_ws && i + 1 < len {
                // Build the remaining content after '@' as a &str for prefix matching.
                let after_at: String = chars[i + 1..].iter().collect();

                // Pass 1: try each known name (longest-first).
                let mut matched: Option<(String, usize)> = None;
                for known in &sorted_known {
                    if after_at.len() < known.len() {
                        continue;
                    }
                    // Use get() to safely handle byte boundaries — known.len()
                    // may land mid-character when content contains multi-byte
                    // UTF-8 (e.g. CJK, emoji). If the slice isn't on a char
                    // boundary, skip this candidate.
                    let candidate = match after_at.get(..known.len()) {
                        Some(s) => s,
                        None => continue,
                    };
                    if !candidate.eq_ignore_ascii_case(known) {
                        continue;
                    }
                    // Word-boundary check: must be followed by whitespace,
                    // common punctuation, or end-of-string.
                    let after_name = &after_at[known.len()..];
                    let boundary = after_name.is_empty()
                        || after_name
                            .chars()
                            .next()
                            .map(|c| {
                                c.is_ascii_whitespace()
                                    || matches!(
                                        c,
                                        ',' | ';' | '.' | '!' | '?' | ':' | ')' | ']' | '}'
                                    )
                            })
                            .unwrap_or(true);
                    if boundary {
                        // Advance i past '@' + matched name length (in chars).
                        let name_char_len = known.chars().count();
                        matched = Some((known.to_ascii_lowercase(), name_char_len));
                        break;
                    }
                }

                if let Some((lower, char_len)) = matched {
                    if seen.insert(lower.clone()) {
                        names.push(lower);
                    }
                    // Skip past '@' + the matched name chars.
                    i += 1 + char_len;
                    continue;
                }

                // Pass 2: single-word fallback (alphanumeric + . - _).
                let start = i + 1;
                let mut end = start;
                while end < len {
                    let c = chars[end];
                    if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                        end += 1;
                    } else {
                        break;
                    }
                }
                if end > start {
                    let name: String = chars[start..end].iter().collect();
                    let lower = name.to_ascii_lowercase();
                    if seen.insert(lower.clone()) {
                        names.push(lower);
                    }
                }
            }
        }
        i += 1;
    }
    names
}

/// Match extracted `@names` against channel-member profiles.
///
/// For each profile, parses its `content_json` and reads the
/// `display_name` field (falling back to `name` **only if `display_name`
/// is absent**, preserving the legacy MCP behavior). If the resulting
/// name matches any extracted `@name` case-insensitively, the profile's
/// pubkey is included.
///
/// Output order is **profile-input order**, not name-input order. When
/// the [`MENTION_CAP`] is later applied during merging, this means the
/// matched-pubkey set is stable with respect to query result ordering
/// rather than text-position ordering.
///
/// Profiles whose `content_json` does not parse, or whose `display_name`
/// (and `name`) are absent or non-string, are silently skipped.
///
/// Duplicate display names within a channel will produce multiple matches
/// for a single `@name` — this is by design; resolution is bounded to
/// channel members, so ambiguity is local to that channel.
pub fn match_names_to_profiles(names: &[String], profiles: &[MentionProfile<'_>]) -> Vec<String> {
    if names.is_empty() {
        return vec![];
    }
    let mut out = Vec::new();
    for p in profiles {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(p.content_json) else {
            continue;
        };
        let name = value
            .get("display_name")
            .or_else(|| value.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if name.is_empty() {
            continue;
        }
        if names.iter().any(|n| n.eq_ignore_ascii_case(name)) {
            out.push(p.pubkey.to_string());
        }
    }
    out
}

/// Merge auto-resolved pubkeys into an explicit mention list, up to `cap`.
///
/// Explicit mentions have priority; auto-resolved entries are appended
/// only if not already present (case-sensitive contains check — callers
/// should normalize beforehand). Stops adding once `cap` is reached.
pub fn merge_mentions(explicit: &mut Vec<String>, auto_resolved: &[String], cap: usize) {
    let budget = cap.saturating_sub(explicit.len());
    let mut added = 0usize;
    for pk in auto_resolved {
        if added >= budget {
            break;
        }
        if !explicit.contains(pk) {
            explicit.push(pk.clone());
            added += 1;
        }
    }
}

/// Normalize a list of mention pubkeys.
///
/// - Lowercases every entry.
/// - Removes duplicates, preserving first-seen order.
/// - When `sender_pubkey` is `Some(pk)`, removes any case-insensitive match
///   against the sender's own pubkey (you don't @mention yourself).
pub fn normalize_mention_pubkeys(pubkeys: &[String], sender_pubkey: Option<&str>) -> Vec<String> {
    let sender = sender_pubkey.map(|s| s.to_ascii_lowercase());
    let mut seen = HashSet::new();
    pubkeys
        .iter()
        .map(|pk| pk.to_ascii_lowercase())
        .filter(|pk| sender.as_deref() != Some(pk.as_str()))
        .filter(|pk| seen.insert(pk.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── extract_at_names ────────────────────────────────────────────────

    #[test]
    fn extract_at_names_matches_basic() {
        assert_eq!(extract_at_names("hello @alice"), vec!["alice"]);
        assert_eq!(extract_at_names("@bob hello"), vec!["bob"]);
    }

    #[test]
    fn extract_at_names_lowercases_and_dedups() {
        assert_eq!(
            extract_at_names("@Alice and @alice, meet @Bob"),
            vec!["alice", "bob"]
        );
    }

    #[test]
    fn extract_at_names_allows_newline_prefix() {
        assert_eq!(extract_at_names("line1\n@tyler line2"), vec!["tyler"]);
    }

    #[test]
    fn extract_at_names_allows_punctuation_in_names() {
        assert_eq!(
            extract_at_names("@john.doe @mary_jane @bob-smith"),
            vec!["john.doe", "mary_jane", "bob-smith"]
        );
    }

    #[test]
    fn extract_at_names_rejects_email_and_empty() {
        assert!(extract_at_names("").is_empty());
        assert!(extract_at_names("no mentions").is_empty());
        assert!(extract_at_names("user@example.com").is_empty());
        assert!(extract_at_names("hello @ world").is_empty());
        assert!(extract_at_names("hello @").is_empty());
    }

    // ── extract_at_mentions_with_known ──────────────────────────────────

    #[test]
    fn known_multiword_name_matches_fully() {
        // "Will Pfleger" should match @Will Pfleger, not just @Will.
        let result = extract_at_mentions_with_known("hello @Will Pfleger!", &["Will Pfleger"]);
        assert_eq!(result, vec!["will pfleger"]);
    }

    #[test]
    fn partial_first_word_does_not_match_multiword_name() {
        // @Will alone must NOT match "Will Pfleger" — partial matches are rejected.
        let result = extract_at_mentions_with_known("hey @Will how are you", &["Will Pfleger"]);
        // No known name matches @Will (boundary check: 'Will' is followed by ' h'
        // which would match "Will Pfleger" only if the full name follows).
        // Falls back to single-word tokenizer → emits "will".
        assert_eq!(result, vec!["will"]);
    }

    #[test]
    fn longest_first_wins_over_prefix() {
        // With both "Will" and "Will Pfleger" known, "@Will Pfleger" should
        // match the longer name, not just "Will".
        let result = extract_at_mentions_with_known(
            "@Will Pfleger sent a message",
            &["Will", "Will Pfleger"],
        );
        assert_eq!(result, vec!["will pfleger"]);
    }

    #[test]
    fn single_word_known_name_matches() {
        let result = extract_at_mentions_with_known("ping @alice please", &["Alice"]);
        assert_eq!(result, vec!["alice"]);
    }

    #[test]
    fn unknown_name_falls_back_to_single_word() {
        // @alice is not in known_names but single-word fallback still emits it.
        let result = extract_at_mentions_with_known("hey @alice", &["Bob"]);
        assert_eq!(result, vec!["alice"]);
    }

    #[test]
    fn multiple_mentions_mixed_known_and_unknown() {
        let result = extract_at_mentions_with_known(
            "@Will Pfleger and @alice should review",
            &["Will Pfleger"],
        );
        assert_eq!(result, vec!["will pfleger", "alice"]);
    }

    #[test]
    fn deduplicates_case_insensitively() {
        let result = extract_at_mentions_with_known(
            "@Will Pfleger and @will pfleger again",
            &["Will Pfleger"],
        );
        assert_eq!(result, vec!["will pfleger"]);
    }

    #[test]
    fn multiword_name_at_end_of_string() {
        let result = extract_at_mentions_with_known("cc @Will Pfleger", &["Will Pfleger"]);
        assert_eq!(result, vec!["will pfleger"]);
    }

    #[test]
    fn multiword_name_followed_by_punctuation() {
        let result =
            extract_at_mentions_with_known("thanks @Will Pfleger, great work", &["Will Pfleger"]);
        assert_eq!(result, vec!["will pfleger"]);
    }

    #[test]
    fn email_address_not_matched() {
        let result = extract_at_mentions_with_known("user@example.com", &["example.com"]);
        assert!(result.is_empty());
    }

    #[test]
    fn empty_content_returns_empty() {
        let result = extract_at_mentions_with_known("", &["Alice"]);
        assert!(result.is_empty());
    }

    #[test]
    fn empty_known_names_uses_single_word_fallback() {
        let result = extract_at_mentions_with_known("hey @alice", &[]);
        assert_eq!(result, vec!["alice"]);
    }

    #[test]
    fn unicode_content_does_not_panic() {
        // Known name byte-length may land mid-character in multi-byte content.
        // e.g. known "ab" (2 bytes) vs content starting with 日 (3 bytes) —
        // byte offset 2 is not a char boundary. Must not panic; gracefully
        // skips the candidate via get() returning None.
        let result = extract_at_mentions_with_known("@日本語 hello", &["ab"]);
        // "ab" doesn't match — falls through to single-word fallback which
        // stops at non-ASCII, so no match. The key assertion: no panic.
        assert!(result.is_empty());
    }

    #[test]
    fn unicode_known_name_matches_with_boundary() {
        // Multi-byte known name followed by a space (valid boundary).
        let result = extract_at_mentions_with_known("@日本 hello", &["日本"]);
        assert_eq!(result, vec!["日本"]);
    }

    #[test]
    fn unicode_known_name_with_ascii_content_no_panic() {
        // Reverse case: multi-byte known name against ASCII content.
        let result = extract_at_mentions_with_known("@alice hello", &["日本語"]);
        assert_eq!(result, vec!["alice"]);
    }

    // ── match_names_to_profiles ─────────────────────────────────────────

    fn profile<'a>(pk: &'a str, json: &'a str) -> MentionProfile<'a> {
        MentionProfile {
            pubkey: pk,
            content_json: json,
        }
    }

    #[test]
    fn match_uses_display_name_case_insensitive() {
        let names = vec!["alice".to_string()];
        let profiles = vec![profile("pk1", r#"{"display_name":"Alice"}"#)];
        assert_eq!(match_names_to_profiles(&names, &profiles), vec!["pk1"]);
    }

    #[test]
    fn match_falls_back_to_name_only_if_display_name_absent() {
        let names = vec!["bob".to_string()];
        // display_name present but empty → skipped (no fallback to `name`).
        let p1 = profile("pk1", r#"{"display_name":"","name":"Bob"}"#);
        // display_name absent → falls back to `name`.
        let p2 = profile("pk2", r#"{"name":"Bob"}"#);
        let out = match_names_to_profiles(&names, &[p1, p2]);
        assert_eq!(out, vec!["pk2"]);
    }

    #[test]
    fn match_preserves_profile_input_order() {
        let names = vec!["alice".to_string(), "bob".to_string()];
        let profiles = vec![
            profile("pkB", r#"{"display_name":"Bob"}"#),
            profile("pkA", r#"{"display_name":"Alice"}"#),
        ];
        // Output order tracks the profile slice, not the name slice.
        assert_eq!(
            match_names_to_profiles(&names, &profiles),
            vec!["pkB", "pkA"]
        );
    }

    #[test]
    fn match_returns_all_pubkeys_for_duplicate_display_names() {
        // Ambiguity is intentional and bounded to channel members.
        let names = vec!["alice".to_string()];
        let profiles = vec![
            profile("pk1", r#"{"display_name":"Alice"}"#),
            profile("pk2", r#"{"display_name":"alice"}"#),
        ];
        assert_eq!(
            match_names_to_profiles(&names, &profiles),
            vec!["pk1", "pk2"]
        );
    }

    #[test]
    fn match_skips_unparseable_and_missing_fields() {
        let names = vec!["alice".to_string()];
        let profiles = vec![
            profile("pk1", "not json"),
            profile("pk2", "{}"),
            profile("pk3", r#"{"display_name":42}"#),
            profile("pk4", r#"{"display_name":"Alice"}"#),
        ];
        assert_eq!(match_names_to_profiles(&names, &profiles), vec!["pk4"]);
    }

    #[test]
    fn match_empty_names_returns_empty() {
        let profiles = vec![profile("pk1", r#"{"display_name":"Alice"}"#)];
        assert!(match_names_to_profiles(&[], &profiles).is_empty());
    }

    // ── merge_mentions ──────────────────────────────────────────────────

    #[test]
    fn merge_appends_new_and_skips_dupes() {
        let mut m = vec!["a".to_string()];
        merge_mentions(&mut m, &["a".into(), "b".into()], MENTION_CAP);
        assert_eq!(m, vec!["a", "b"]);
    }

    #[test]
    fn merge_respects_cap() {
        let mut m: Vec<String> = (0..49).map(|i| format!("pk{i}")).collect();
        merge_mentions(&mut m, &["x".into(), "y".into()], MENTION_CAP);
        assert_eq!(m.len(), MENTION_CAP);
        assert_eq!(m.last().unwrap(), "x");
    }

    #[test]
    fn merge_noop_when_explicit_at_cap() {
        let mut m: Vec<String> = (0..MENTION_CAP).map(|i| format!("pk{i}")).collect();
        merge_mentions(&mut m, &["extra".into()], MENTION_CAP);
        assert_eq!(m.len(), MENTION_CAP);
        assert!(!m.contains(&"extra".to_string()));
    }

    // ── normalize_mention_pubkeys ───────────────────────────────────────

    #[test]
    fn normalize_lowercases_and_dedups() {
        let pks = vec!["ABC".to_string(), "abc".to_string(), "DEF".to_string()];
        assert_eq!(normalize_mention_pubkeys(&pks, None), vec!["abc", "def"]);
    }

    #[test]
    fn normalize_removes_sender_case_insensitive() {
        let pks = vec!["ABC".to_string(), "DEF".to_string()];
        assert_eq!(normalize_mention_pubkeys(&pks, Some("abc")), vec!["def"]);
    }

    #[test]
    fn normalize_with_none_sender_keeps_everything() {
        let pks = vec!["abc".to_string()];
        assert_eq!(normalize_mention_pubkeys(&pks, None), vec!["abc"]);
    }

    #[test]
    fn normalize_empty_input() {
        assert!(normalize_mention_pubkeys(&[], Some("anything")).is_empty());
    }
}
