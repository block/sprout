//! Legacy JSON persona format adapter.
//!
//! Reads the flat `.persona.json` format used by the Sprout desktop app
//! and converts to `.persona.md` (YAML frontmatter + markdown body).
//!
//! This is a read-only compatibility shim — the JSON format is deprecated.
//! The ACP harness only consumes `.persona.md`; the desktop converts at
//! deploy time via this adapter.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::persona::PersonaConfig;
use crate::validate::ValidationDiagnostic;

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum LegacyError {
    #[error("failed to read file: {0}")]
    Io(#[from] std::io::Error),

    #[error("failed to parse JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("migration error: {0}")]
    Migration(String),
}

// ── Legacy persona record (desktop JSON format) ──────────────────────────────

/// The flat JSON persona format stored by the Sprout desktop app.
///
/// Source: `desktop/src-tauri/src/managed_agents/types.rs` → `PersonaRecord`.
/// This struct mirrors that layout for deserialization — we don't depend on
/// the desktop crate directly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyPersonaRecord {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    pub system_prompt: String,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub name_pool: Vec<String>,
    #[serde(default)]
    pub is_builtin: bool,
    #[serde(default = "default_true")]
    pub is_active: bool,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

fn default_true() -> bool {
    true
}

// ── Loading ──────────────────────────────────────────────────────────────────

/// Load legacy persona records from a JSON file.
///
/// Accepts either a single `LegacyPersonaRecord` object or an array of them
/// (the desktop stores an array in `personas.json`).
pub fn load_legacy_json(path: &Path) -> Result<Vec<LegacyPersonaRecord>, LegacyError> {
    const MAX_LEGACY_FILE_SIZE: u64 = 10 * 1024 * 1024; // 10MB
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > MAX_LEGACY_FILE_SIZE {
        return Err(LegacyError::Migration(format!(
            "file too large: {} bytes (max {})",
            metadata.len(),
            MAX_LEGACY_FILE_SIZE
        )));
    }
    let content = std::fs::read_to_string(path)?;
    let trimmed = content.trim();

    if trimmed.starts_with('[') {
        Ok(serde_json::from_str::<Vec<LegacyPersonaRecord>>(trimmed)?)
    } else if trimmed.starts_with('{') {
        Ok(vec![serde_json::from_str::<LegacyPersonaRecord>(trimmed)?])
    } else {
        Err(LegacyError::Migration(
            "file is not a JSON object or array".into(),
        ))
    }
}

// ── Field mapping ────────────────────────────────────────────────────────────

/// Derive the V7 `name` field from the legacy `id`.
///
/// Strips the `builtin:` prefix if present, lowercases, and replaces
/// spaces/special chars with hyphens.
pub fn derive_name(id: &str) -> String {
    let stripped = id
        .strip_prefix("builtin:")
        .or_else(|| id.strip_prefix("custom:"))
        .unwrap_or(id);

    stripped
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

/// Merge `provider` and `model` into the V7 `"provider:model-id"` format.
///
/// Cases:
/// 1. Both present → `"provider:model-id"`
/// 2. Only model   → `"model-id"` (no colon)
/// 3. Only provider → `None` (provider alone is not useful)
/// 4. Neither      → `None`
pub fn merge_provider_model(
    provider: Option<&str>,
    model: Option<&str>,
) -> Option<String> {
    match (provider, model) {
        (Some(p), Some(m)) if !p.is_empty() && !m.is_empty() => {
            Some(format!("{p}:{m}"))
        }
        (_, Some(m)) if !m.is_empty() => Some(m.to_string()),
        _ => None,
    }
}

/// Extract a one-line description from the system prompt.
///
/// Takes the first non-empty line, truncated to at most 120 bytes of UTF-8.
/// Uses char boundaries to avoid slicing through multibyte codepoints.
/// Falls back to "Migrated persona" if the prompt is empty.
pub fn derive_description(system_prompt: &str) -> String {
    system_prompt
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| {
            if line.len() > 119 {
                // Find the last char boundary ending at or before byte 119,
                // so the result (content + '…') is at most 120 chars.
                let truncate_at = line
                    .char_indices()
                    .take_while(|(i, _)| *i < 119)
                    .last()
                    .map(|(i, c)| i + c.len_utf8())
                    .unwrap_or(119);
                format!("{}…", &line[..truncate_at])
            } else {
                line.to_string()
            }
        })
        .unwrap_or_else(|| "Migrated persona".to_string())
}

// ── Migration: JSON → .persona.md ────────────────────────────────────────────

/// Convert a single legacy record to `.persona.md` content (YAML frontmatter
/// + markdown body).
///
/// Returns `(filename, content)` where filename is `{name}.persona.md`.
pub fn legacy_to_persona_md(
    record: &LegacyPersonaRecord,
) -> Result<(String, String), LegacyError> {
    let name = derive_name(&record.id);
    if name.is_empty() {
        return Err(LegacyError::Migration(format!(
            "could not derive a valid name from id {:?}",
            record.id
        )));
    }
    if record.display_name.trim().is_empty() {
        return Err(LegacyError::Migration("display_name is empty".into()));
    }

    let description = derive_description(&record.system_prompt);
    let model = merge_provider_model(
        record.provider.as_deref(),
        record.model.as_deref(),
    );

    // Build YAML frontmatter using serde_yaml for correct escaping.
    // BTreeMap keeps keys in alphabetical order for deterministic output.
    let mut fm = serde_json::Map::new();
    fm.insert("name".into(), serde_json::Value::String(name.clone()));
    fm.insert("display_name".into(), serde_json::Value::String(record.display_name.clone()));
    fm.insert("description".into(), serde_json::Value::String(description));

    if let Some(ref avatar) = record.avatar_url {
        // Skip data: URIs — they're inline base64, not pack-relative paths.
        if !avatar.starts_with("data:") && !avatar.is_empty() {
            fm.insert("avatar".into(), serde_json::Value::String(avatar.clone()));
        }
    }

    if let Some(ref m) = model {
        fm.insert("model".into(), serde_json::Value::String(m.clone()));
    }

    let yaml_value = serde_json::Value::Object(fm);
    let frontmatter = serde_yaml::to_string(&yaml_value)
        .map_err(|e| LegacyError::Migration(format!("failed to serialize frontmatter: {e}")))?;
    // serde_yaml emits a trailing newline; trim it since we add our own.
    let frontmatter = frontmatter.trim_end();
    let body = &record.system_prompt;

    let content = format!("---\n{frontmatter}\n---\n\n{body}");
    let filename = format!("{name}.persona.md");

    Ok((filename, content))
}

/// Migrate a legacy JSON file to a directory of `.persona.md` files.
///
/// Creates the output directory if it doesn't exist. Skips inactive and
/// built-in personas by default (controlled by `include_builtin` and
/// `include_inactive`).
pub fn migrate_json_to_md(
    input_path: &Path,
    output_dir: &Path,
    include_builtin: bool,
    include_inactive: bool,
) -> Result<MigrationReport, LegacyError> {
    let records = load_legacy_json(input_path)?;
    std::fs::create_dir_all(output_dir)?;

    let mut report = MigrationReport::default();

    for record in &records {
        if !include_builtin && record.is_builtin {
            report.skipped_builtin += 1;
            continue;
        }
        if !include_inactive && !record.is_active {
            report.skipped_inactive += 1;
            continue;
        }

        match legacy_to_persona_md(record) {
            Ok((filename, content)) => {
                let out_path = output_dir.join(&filename);
                if out_path.exists() {
                    report.diagnostics.push(ValidationDiagnostic::Warning(
                        format!("output file already exists, skipping: {}", out_path.display()),
                    ));
                    continue;
                }
                std::fs::write(&out_path, &content)?;
                report.migrated.push(filename);
            }
            Err(e) => {
                report.diagnostics.push(ValidationDiagnostic::Error(
                    format!("failed to migrate {:?}: {e}", record.id),
                ));
            }
        }
    }

    Ok(report)
}

/// Summary of a migration run.
#[derive(Debug, Default)]
pub struct MigrationReport {
    pub migrated: Vec<String>,
    pub skipped_builtin: usize,
    pub skipped_inactive: usize,
    pub diagnostics: Vec<ValidationDiagnostic>,
}

impl std::fmt::Display for MigrationReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Migration complete:")?;
        writeln!(f, "  Migrated: {}", self.migrated.len())?;
        for name in &self.migrated {
            writeln!(f, "    ✓ {name}")?;
        }
        if self.skipped_builtin > 0 {
            writeln!(f, "  Skipped (built-in): {}", self.skipped_builtin)?;
        }
        if self.skipped_inactive > 0 {
            writeln!(f, "  Skipped (inactive): {}", self.skipped_inactive)?;
        }
        for w in &self.diagnostics {
            writeln!(f, "  {w}")?;
        }
        Ok(())
    }
}

// ── Adapter: LegacyPersonaRecord → PersonaConfig ─────────────────────────────

/// Convert a legacy JSON persona record into a typed `PersonaConfig`.
///
/// This is the bridge between the old flat JSON format and the new V7 struct.
/// Fields that don't exist in the legacy format (`skills`, `mcp_servers`,
/// `subscribe`, `respond_to`, `hooks`, etc.) get their default/empty values.
pub fn legacy_to_persona_config(record: &LegacyPersonaRecord) -> Result<PersonaConfig, LegacyError> {
    let name = derive_name(&record.id);
    if name.is_empty() {
        return Err(LegacyError::Migration(format!(
            "could not derive a valid name from id {:?}",
            record.id
        )));
    }
    if record.display_name.trim().is_empty() {
        return Err(LegacyError::Migration("display_name is empty".into()));
    }

    Ok(PersonaConfig {
        name,
        display_name: record.display_name.clone(),
        avatar: record.avatar_url.as_ref()
            .filter(|a| !a.starts_with("data:") && !a.is_empty())
            .cloned(),
        description: derive_description(&record.system_prompt),
        version: None,
        author: None,
        skills: Vec::new(),
        mcp_servers: Vec::new(),
        subscribe: None,
        triggers: None,
        model: merge_provider_model(
            record.provider.as_deref(),
            record.model.as_deref(),
        ),
        temperature: None,
        max_context_tokens: None,
        thread_replies: None,
        broadcast_replies: None,
        hooks: None,
        prompt: record.system_prompt.clone(),
    })
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── derive_name ──────────────────────────────────────────────────────

    #[test]
    fn derive_name_strips_builtin_prefix() {
        assert_eq!(derive_name("builtin:solo"), "solo");
    }

    #[test]
    fn derive_name_strips_custom_prefix() {
        assert_eq!(derive_name("custom:my-agent"), "my-agent");
    }

    #[test]
    fn derive_name_lowercases() {
        assert_eq!(derive_name("MyAgent"), "myagent");
    }

    #[test]
    fn derive_name_replaces_spaces() {
        assert_eq!(derive_name("My Cool Agent"), "my-cool-agent");
    }

    #[test]
    fn derive_name_plain_id() {
        assert_eq!(derive_name("lep"), "lep");
    }

    // ── merge_provider_model ─────────────────────────────────────────────

    #[test]
    fn merge_both_present() {
        assert_eq!(
            merge_provider_model(Some("anthropic"), Some("claude-sonnet-4-20250514")),
            Some("anthropic:claude-sonnet-4-20250514".into())
        );
    }

    #[test]
    fn merge_model_only() {
        assert_eq!(
            merge_provider_model(None, Some("gpt-4o")),
            Some("gpt-4o".into())
        );
    }

    #[test]
    fn merge_provider_only() {
        assert_eq!(merge_provider_model(Some("anthropic"), None), None);
    }

    #[test]
    fn merge_neither() {
        assert_eq!(merge_provider_model(None, None), None);
    }

    #[test]
    fn merge_empty_strings() {
        assert_eq!(merge_provider_model(Some(""), Some("")), None);
    }

    #[test]
    fn merge_empty_provider_with_model() {
        assert_eq!(
            merge_provider_model(Some(""), Some("gpt-4o")),
            Some("gpt-4o".into())
        );
    }

    // ── derive_description ───────────────────────────────────────────────

    #[test]
    fn description_from_first_line() {
        assert_eq!(
            derive_description("You are a security bot.\nMore details here."),
            "You are a security bot."
        );
    }

    #[test]
    fn description_skips_empty_lines() {
        assert_eq!(
            derive_description("\n\n  \nActual content here."),
            "Actual content here."
        );
    }

    #[test]
    fn description_empty_prompt() {
        assert_eq!(derive_description(""), "Migrated persona");
        assert_eq!(derive_description("   \n  \n  "), "Migrated persona");
    }

    #[test]
    fn description_truncates_long_line() {
        let long = "x".repeat(200);
        let desc = derive_description(&long);
        assert!(desc.chars().count() <= 120, "should be at most 120 chars (119 + ellipsis)");
        assert!(desc.ends_with('…'));
    }

    #[test]
    fn description_truncates_multibyte_utf8() {
        // 200 emoji chars — each is 4 bytes. Old code would panic on byte slice.
        let long: String = "🔒".repeat(200);
        let desc = derive_description(&long);
        assert!(desc.chars().count() <= 120);
        assert!(desc.ends_with('…'));
        // Verify it's valid UTF-8 (would panic on construction if not).
        assert!(desc.len() > 0);
    }

    #[test]
    fn description_truncates_multibyte_safely() {
        // 40 CJK characters = 120 bytes (3 bytes each), then more
        let long = "你".repeat(50); // 150 bytes
        let desc = derive_description(&long);
        assert!(!desc.is_empty());
        // Should not panic, and should end with …
        assert!(desc.ends_with('…'));
    }

    // ── legacy_to_persona_md ─────────────────────────────────────────────

    #[test]
    fn basic_conversion() {
        let record = LegacyPersonaRecord {
            id: "custom:lep".into(),
            display_name: "Lep 🍀".into(),
            avatar_url: Some("./avatars/lep.png".into()),
            system_prompt: "You are Lep, a security reviewer.".into(),
            provider: Some("anthropic".into()),
            model: Some("claude-sonnet-4-20250514".into()),
            name_pool: vec!["clover".into()],
            is_builtin: false,
            is_active: true,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        };

        let (filename, content) = legacy_to_persona_md(&record).unwrap();
        assert_eq!(filename, "lep.persona.md");
        assert!(content.starts_with("---\n"), "should start with ---");
        // serde_yaml may or may not quote simple strings; check key: value pairs
        assert!(content.contains("name:") && content.contains("lep"), "should contain name: lep");
        assert!(content.contains("display_name:"), "should contain display_name");
        assert!(content.contains("model:") && content.contains("anthropic:claude-sonnet-4-20250514"), "should contain model");
        assert!(content.contains("avatar:") && content.contains("./avatars/lep.png"), "should contain avatar");
        assert!(content.contains("You are Lep, a security reviewer."), "should contain system prompt");
        // name_pool should NOT appear
        assert!(!content.contains("name_pool"), "name_pool should not appear");
        assert!(!content.contains("clover"), "name_pool values should not appear");
    }

    #[test]
    fn conversion_skips_data_uri_avatar() {
        let record = LegacyPersonaRecord {
            id: "builtin:solo".into(),
            display_name: "Solo".into(),
            avatar_url: Some("data:image/png;base64,iVBOR...".into()),
            system_prompt: "You are Solo.".into(),
            provider: None,
            model: None,
            name_pool: vec![],
            is_builtin: true,
            is_active: true,
            created_at: String::new(),
            updated_at: String::new(),
        };

        let (_, content) = legacy_to_persona_md(&record).unwrap();
        assert!(!content.contains("avatar:"));
        assert!(!content.contains("data:image"));
    }

    #[test]
    fn conversion_no_model() {
        let record = LegacyPersonaRecord {
            id: "custom:plain".into(),
            display_name: "Plain".into(),
            avatar_url: None,
            system_prompt: "A simple agent.".into(),
            provider: None,
            model: None,
            name_pool: vec![],
            is_builtin: false,
            is_active: true,
            created_at: String::new(),
            updated_at: String::new(),
        };

        let (_, content) = legacy_to_persona_md(&record).unwrap();
        assert!(!content.contains("model:"));
    }

    #[test]
    fn conversion_preserves_prompt_whitespace() {
        // Fix #4: migration must not trim leading/trailing whitespace from prompts.
        let prompt = "\n  You are Lep.\n\nBe helpful.\n  ";
        let record = LegacyPersonaRecord {
            id: "custom:lep".into(),
            display_name: "Lep".into(),
            avatar_url: None,
            system_prompt: prompt.into(),
            provider: None,
            model: None,
            name_pool: vec![],
            is_builtin: false,
            is_active: true,
            created_at: String::new(),
            updated_at: String::new(),
        };

        let (_, content) = legacy_to_persona_md(&record).unwrap();
        // The body section (after the closing ---\n\n) must contain the prompt verbatim.
        let body_start = content.find("---\n\n").unwrap() + 5;
        let body = &content[body_start..];
        assert_eq!(body, prompt, "prompt must be written as-is, not trimmed");
    }

    // ── load_legacy_json ─────────────────────────────────────────────────

    #[test]
    fn load_single_object() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("single.json");
        std::fs::write(
            &path,
            r#"{"id":"custom:test","display_name":"Test","system_prompt":"Hello","created_at":"","updated_at":""}"#,
        )
        .unwrap();

        let records = load_legacy_json(&path).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, "custom:test");
    }

    #[test]
    fn load_array() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("array.json");
        std::fs::write(
            &path,
            r#"[{"id":"a","display_name":"A","system_prompt":"","created_at":"","updated_at":""},{"id":"b","display_name":"B","system_prompt":"","created_at":"","updated_at":""}]"#,
        )
        .unwrap();

        let records = load_legacy_json(&path).unwrap();
        assert_eq!(records.len(), 2);
    }

    // ── legacy_to_persona_config ─────────────────────────────────────────

    fn make_record(id: &str, prompt: &str) -> LegacyPersonaRecord {
        LegacyPersonaRecord {
            id: id.into(),
            display_name: "Test Agent".into(),
            avatar_url: Some("./avatars/test.png".into()),
            system_prompt: prompt.into(),
            provider: Some("anthropic".into()),
            model: Some("claude-sonnet-4-20250514".into()),
            name_pool: vec!["alpha".into()],
            is_builtin: false,
            is_active: true,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn adapter_basic() {
        let record = make_record("custom:lep", "You are Lep.");
        let config = legacy_to_persona_config(&record).unwrap();
        assert_eq!(config.name, "lep");
        assert_eq!(config.display_name, "Test Agent");
        assert_eq!(config.avatar.as_deref(), Some("./avatars/test.png"));
        assert_eq!(config.model.as_deref(), Some("anthropic:claude-sonnet-4-20250514"));
        assert_eq!(config.prompt, "You are Lep.");
        assert!(config.skills.is_empty());
        assert!(config.mcp_servers.is_empty());
        assert!(config.subscribe.is_none());
        assert!(config.triggers.is_none());
        assert!(config.hooks.is_none());
    }

    #[test]
    fn adapter_skips_data_uri_avatar() {
        let mut record = make_record("builtin:solo", "Solo prompt.");
        record.avatar_url = Some("data:image/png;base64,abc".into());
        let config = legacy_to_persona_config(&record).unwrap();
        assert!(config.avatar.is_none());
    }

    #[test]
    fn adapter_no_model() {
        let mut record = make_record("custom:plain", "Plain.");
        record.provider = None;
        record.model = None;
        let config = legacy_to_persona_config(&record).unwrap();
        assert!(config.model.is_none());
    }

    #[test]
    fn adapter_preserves_prompt_whitespace() {
        // Fix #4: legacy_to_persona_config must not trim the system prompt.
        let prompt = "\n  You are Lep.\n\nBe helpful.\n  ";
        let record = make_record("custom:lep", prompt);
        let config = legacy_to_persona_config(&record).unwrap();
        assert_eq!(config.prompt, prompt, "prompt must be stored as-is, not trimmed");
    }

    // ── empty display_name validation ────────────────────────────────────

    #[test]
    fn empty_display_name_is_rejected_by_md() {
        let mut record = make_record("custom:lep", "You are Lep.");
        record.display_name = "".into();
        let err = legacy_to_persona_md(&record).unwrap_err();
        assert!(
            matches!(err, LegacyError::Migration(ref msg) if msg.contains("display_name")),
            "expected Migration error mentioning display_name, got: {err}"
        );
    }

    #[test]
    fn whitespace_only_display_name_is_rejected_by_md() {
        let mut record = make_record("custom:lep", "You are Lep.");
        record.display_name = "   ".into();
        let err = legacy_to_persona_md(&record).unwrap_err();
        assert!(matches!(err, LegacyError::Migration(_)));
    }

    #[test]
    fn empty_display_name_is_rejected_by_config() {
        let mut record = make_record("custom:lep", "You are Lep.");
        record.display_name = "".into();
        let err = legacy_to_persona_config(&record).unwrap_err();
        assert!(
            matches!(err, LegacyError::Migration(ref msg) if msg.contains("display_name")),
            "expected Migration error mentioning display_name, got: {err}"
        );
    }

    #[test]
    fn whitespace_only_display_name_is_rejected_by_config() {
        let mut record = make_record("custom:lep", "You are Lep.");
        record.display_name = "   ".into();
        let err = legacy_to_persona_config(&record).unwrap_err();
        assert!(matches!(err, LegacyError::Migration(_)));
    }

    // ── Round-trip: JSON → .persona.md → parse back ──────────────────────

    #[test]
    fn round_trip_json_to_md_to_config() {
        let record = make_record("custom:roundtrip", "You are a round-trip test agent.\n\nBe helpful.");
        let (filename, md_content) = legacy_to_persona_md(&record).unwrap();
        assert_eq!(filename, "roundtrip.persona.md");

        // Parse the generated .persona.md back using the real parser.
        let parsed = crate::persona::parse_persona_md(&md_content).unwrap();

        assert_eq!(parsed.name, "roundtrip");
        assert_eq!(parsed.display_name, "Test Agent");
        assert_eq!(parsed.model.as_deref(), Some("anthropic:claude-sonnet-4-20250514"));
        assert_eq!(parsed.avatar.as_deref(), Some("./avatars/test.png"));
        // The prompt should survive the round-trip.
        assert!(parsed.prompt.contains("You are a round-trip test agent."));
        assert!(parsed.prompt.contains("Be helpful."));
    }
}
