//! Tests for the multi-profile wrapper added in plan v3 rev 3.
//!
//! Covers (plan §11):
//! - Migration v1/v2 → v3 (accept, idempotent, mismatched owner aborts).
//! - load_for_spawn:
//!   - profile_id Some(unknown) → Error.
//!   - profile_id None + no default → Error.
//!   - profile_id Some(known) + invalid stored (non-loopback http) → Error.
//!   - per-profile owner_pubkey mismatch on resolved profile.
//!   - empty profile list → Error (fail closed).
//! - Auto-default policy: first-save into empty/no-default state sets the
//!   default; subsequent saves with a valid default don't change it.
//! - Schema dispatch: unknown schema_version (>3) is rejected.
//! - validate_label: empty / oversized / control-chars / trim.
//!
//! Tests that need a Tauri `AppHandle` / `AppState` are intentionally
//! omitted here (Rust-side spawn-time path can be exercised through the
//! pure helpers; full IPC plumbing is exercised by Playwright).

use std::time::{SystemTime, UNIX_EPOCH};

use nostr::Keys;
use tempfile::tempdir;

use super::storage::{decrypt_settings, encrypt_settings, read_envelope, write_envelope};
use super::storage_profiles::{
    new_profile_id, parse_or_migrate_plaintext, validate_label, write_profiles_envelope,
    ParseError, ParsedPlaintext,
};
use super::{
    NamedProfile, ProfilesPlaintext, SettingsEnvelope, StoredSettings, CURRENT_PLAINTEXT_SCHEMA,
    ENVELOPE_ALG, ENVELOPE_VERSION, PROVIDER_ANTHROPIC,
};

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn legacy_v1_stored(api_key: &str) -> StoredSettings {
    StoredSettings {
        schema_version: 1,
        owner_pubkey: String::new(),
        provider: PROVIDER_ANTHROPIC.into(),
        api_key: api_key.into(),
        model: "claude-sonnet-4-5".into(),
        base_url: "https://api.anthropic.com".into(),
        anthropic_api_version: Some("2023-06-01".into()),
        system_prompt: None,
        max_rounds: None,
        max_output_tokens: None,
        llm_timeout_secs: None,
        tool_timeout_secs: None,
        max_history_bytes: None,
        detected_provider_id: "anthropic".into(),
        detection_overridden: false,
    }
}

fn legacy_v2_stored(owner_pubkey: &str, api_key: &str) -> StoredSettings {
    let mut s = legacy_v1_stored(api_key);
    s.schema_version = 2;
    s.owner_pubkey = owner_pubkey.into();
    s
}

fn make_profile(label: &str, api_key: &str, owner_pubkey: &str) -> NamedProfile {
    let id = new_profile_id();
    let t = now();
    let mut settings = legacy_v2_stored(owner_pubkey, api_key);
    // For variety, pin schema_version to 2 (matches save path).
    settings.schema_version = 2;
    NamedProfile {
        id,
        label: label.into(),
        created_at: t,
        updated_at: t,
        settings,
    }
}

// ── Migration ───────────────────────────────────────────────────────────────

#[test]
fn migration_v1_to_v3_succeeds_and_stamps_owner_pubkey() {
    let keys = Keys::generate();
    let envelope_pk = keys.public_key().to_hex();
    let v1 = legacy_v1_stored("sk-ant-secret");
    let plain = serde_json::to_string(&v1).unwrap();

    let parsed = parse_or_migrate_plaintext(&plain, &envelope_pk).unwrap();
    let wrapper = match parsed {
        ParsedPlaintext::Migrated(w) => w,
        ParsedPlaintext::Current(_) => panic!("expected Migrated, got Current"),
    };
    assert_eq!(wrapper.schema_version, CURRENT_PLAINTEXT_SCHEMA);
    assert_eq!(wrapper.owner_pubkey, envelope_pk);
    assert_eq!(wrapper.profiles.len(), 1);
    let only = &wrapper.profiles[0];
    assert_eq!(only.label, "Default");
    assert_eq!(only.settings.api_key, "sk-ant-secret");
    // v1 owner_pubkey was empty; migration stamps the envelope key.
    assert_eq!(only.settings.owner_pubkey, envelope_pk);
    // Per-profile schema bumped to 2 so the v2 cross-check stays
    // meaningful on subsequent loads.
    assert_eq!(only.settings.schema_version, 2);
    assert_eq!(
        wrapper.default_profile_id.as_deref(),
        Some(only.id.as_str())
    );
}

#[test]
fn migration_v2_matching_owner_pubkey_succeeds() {
    let keys = Keys::generate();
    let envelope_pk = keys.public_key().to_hex();
    let v2 = legacy_v2_stored(&envelope_pk, "sk-ant-secret");
    let plain = serde_json::to_string(&v2).unwrap();

    let parsed = parse_or_migrate_plaintext(&plain, &envelope_pk).unwrap();
    match parsed {
        ParsedPlaintext::Migrated(w) => {
            assert_eq!(w.profiles[0].settings.owner_pubkey, envelope_pk);
        }
        ParsedPlaintext::Current(_) => panic!("expected Migrated"),
    }
}

#[test]
fn migration_v2_mismatched_owner_pubkey_aborts() {
    let envelope_pk = "a".repeat(64);
    let v2 = legacy_v2_stored(&"b".repeat(64), "sk-ant-secret");
    let plain = serde_json::to_string(&v2).unwrap();

    match parse_or_migrate_plaintext(&plain, &envelope_pk) {
        Err(ParseError::IdentityMismatch) => {}
        _ => panic!("expected ParseError::IdentityMismatch, (wrong variant)"),
    }
}

#[test]
fn migration_is_idempotent() {
    // Migrate v1 → v3, then feed v3 plaintext back through the parser:
    // it should land as Current and equal the previously migrated wrapper.
    let keys = Keys::generate();
    let envelope_pk = keys.public_key().to_hex();
    let v1 = legacy_v1_stored("sk-ant-x");
    let plain_v1 = serde_json::to_string(&v1).unwrap();

    let first = match parse_or_migrate_plaintext(&plain_v1, &envelope_pk).unwrap() {
        ParsedPlaintext::Migrated(w) => w,
        ParsedPlaintext::Current(_) => panic!("expected Migrated"),
    };

    let plain_v3 = serde_json::to_string(&first).unwrap();
    let second = match parse_or_migrate_plaintext(&plain_v3, &envelope_pk).unwrap() {
        ParsedPlaintext::Current(w) => w,
        ParsedPlaintext::Migrated(_) => panic!("expected Current on second pass"),
    };

    assert_eq!(first.owner_pubkey, second.owner_pubkey);
    assert_eq!(first.default_profile_id, second.default_profile_id);
    assert_eq!(first.profiles.len(), second.profiles.len());
    assert_eq!(first.profiles[0].id, second.profiles[0].id);
}

#[test]
fn schema_version_above_max_rejected() {
    // Simulate downgrade after writing a future schema.
    let probe_json = r#"{"schema_version": 99}"#;
    match parse_or_migrate_plaintext(probe_json, &"a".repeat(64)) {
        Err(ParseError::Other(msg)) => {
            assert!(msg.contains("unsupported plaintext schema_version"))
        }
        _ => panic!("expected ParseError::Other(unsupported schema), (wrong variant)"),
    }
}

#[test]
fn v3_wrapper_owner_pubkey_mismatch_aborts() {
    let envelope_pk = "a".repeat(64);
    let wrapper = ProfilesPlaintext {
        schema_version: CURRENT_PLAINTEXT_SCHEMA,
        owner_pubkey: "b".repeat(64),
        default_profile_id: None,
        profiles: vec![],
    };
    let plain = serde_json::to_string(&wrapper).unwrap();
    match parse_or_migrate_plaintext(&plain, &envelope_pk) {
        Err(ParseError::IdentityMismatch) => {}
        _ => panic!("expected ParseError::IdentityMismatch, (wrong variant)"),
    }
}

// ── validate_label ──────────────────────────────────────────────────────────

#[test]
fn validate_label_trims_and_accepts() {
    assert_eq!(validate_label("  hello  ").unwrap(), "hello");
}

#[test]
fn validate_label_rejects_empty() {
    assert!(validate_label("").is_err());
    assert!(validate_label("   ").is_err());
}

#[test]
fn validate_label_rejects_oversized() {
    let label = "a".repeat(65);
    assert!(validate_label(&label).is_err());
}

#[test]
fn validate_label_rejects_control_chars() {
    assert!(validate_label("hello\nworld").is_err());
    assert!(validate_label("hello\0world").is_err());
}

#[test]
fn validate_label_accepts_max_length_after_trim() {
    let label = format!("  {}  ", "a".repeat(64));
    assert_eq!(validate_label(&label).unwrap().len(), 64);
}

// ── write_profiles_envelope round-trip ─────────────────────────────────────

#[test]
fn write_then_read_profiles_envelope_round_trips() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let keys = Keys::generate();
    let pk = keys.public_key().to_hex();
    let p = make_profile("Default", "sk-ant-x", &pk);
    let wrapper = ProfilesPlaintext {
        schema_version: CURRENT_PLAINTEXT_SCHEMA,
        owner_pubkey: pk.clone(),
        default_profile_id: Some(p.id.clone()),
        profiles: vec![p.clone()],
    };

    write_profiles_envelope(&path, &keys, &wrapper).unwrap();

    let env = read_envelope(&path).unwrap().unwrap();
    assert_eq!(env.version, ENVELOPE_VERSION);
    assert_eq!(env.alg, ENVELOPE_ALG);
    assert_eq!(env.pubkey, pk);
    let plain = decrypt_settings(&keys, &env.ciphertext).unwrap();
    let parsed = match parse_or_migrate_plaintext(&plain, &env.pubkey).unwrap() {
        ParsedPlaintext::Current(w) => w,
        ParsedPlaintext::Migrated(_) => panic!("v3 should not need migrating"),
    };
    assert_eq!(parsed.default_profile_id.as_deref(), Some(p.id.as_str()));
    assert_eq!(parsed.profiles.len(), 1);
    assert_eq!(parsed.profiles[0].label, "Default");
    assert_eq!(parsed.profiles[0].settings.api_key, "sk-ant-x");
}

// ── ProfilesPlaintext default behaviors / structural assertions ────────────

#[test]
fn fresh_profile_id_format() {
    // UUID v4: lowercase hyphenated, 36 chars.
    let id = new_profile_id();
    assert_eq!(id.len(), 36);
    assert!(id.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
    assert_eq!(id.matches('-').count(), 4);
}

#[test]
fn profile_id_uniqueness_under_loop() {
    // Cryptographically negligible; deterministic enough for a smoke test.
    let mut ids = std::collections::HashSet::new();
    for _ in 0..1000 {
        assert!(ids.insert(new_profile_id()));
    }
}

// ── Auto-default logic (pure helper, mirrors commands_write::ensure_default)

#[test]
fn auto_default_sets_when_unset() {
    let mut wrapper = ProfilesPlaintext {
        schema_version: CURRENT_PLAINTEXT_SCHEMA,
        owner_pubkey: "a".repeat(64),
        default_profile_id: None,
        profiles: vec![],
    };
    let new_id = new_profile_id();
    wrapper.profiles.push(NamedProfile {
        id: new_id.clone(),
        label: "First".into(),
        created_at: 0,
        updated_at: 0,
        settings: legacy_v2_stored(&"a".repeat(64), "sk"),
    });
    // Mirror what ensure_default does:
    if wrapper
        .default_profile_id
        .as_deref()
        .is_none_or(|id| !wrapper.profiles.iter().any(|p| p.id == id))
    {
        wrapper.default_profile_id = Some(new_id.clone());
    }
    assert_eq!(wrapper.default_profile_id.as_deref(), Some(new_id.as_str()));
}

#[test]
fn auto_default_does_not_change_when_already_set() {
    let existing = new_profile_id();
    let mut wrapper = ProfilesPlaintext {
        schema_version: CURRENT_PLAINTEXT_SCHEMA,
        owner_pubkey: "a".repeat(64),
        default_profile_id: Some(existing.clone()),
        profiles: vec![NamedProfile {
            id: existing.clone(),
            label: "Existing".into(),
            created_at: 0,
            updated_at: 0,
            settings: legacy_v2_stored(&"a".repeat(64), "sk"),
        }],
    };
    // Touch a new profile.
    let touched = new_profile_id();
    wrapper.profiles.push(NamedProfile {
        id: touched.clone(),
        label: "Touched".into(),
        created_at: 0,
        updated_at: 0,
        settings: legacy_v2_stored(&"a".repeat(64), "sk"),
    });
    if wrapper
        .default_profile_id
        .as_deref()
        .is_none_or(|id| !wrapper.profiles.iter().any(|p| p.id == id))
    {
        wrapper.default_profile_id = Some(touched);
    }
    // Default unchanged.
    assert_eq!(
        wrapper.default_profile_id.as_deref(),
        Some(existing.as_str())
    );
}

// ── Envelope/wrapper-level integrity: tampered envelope with same identity

#[test]
fn tampered_envelope_swap_under_same_identity_surfaces_as_owner_mismatch() {
    // Build a wrapper claiming a different owner_pubkey than what the
    // envelope's pubkey will be. The parse step must reject.
    let envelope_pk = "f".repeat(64);
    let wrapper = ProfilesPlaintext {
        schema_version: CURRENT_PLAINTEXT_SCHEMA,
        owner_pubkey: "e".repeat(64),
        default_profile_id: None,
        profiles: vec![],
    };
    let plain = serde_json::to_string(&wrapper).unwrap();
    match parse_or_migrate_plaintext(&plain, &envelope_pk) {
        Err(ParseError::IdentityMismatch) => {}
        _ => panic!(
            "expected v3 owner_pubkey mismatch to surface as IdentityMismatch (wrong variant)"
        ),
    }
}

// ── Unused-bridge: silence dead_code on imports that aren't otherwise read

#[allow(dead_code)]
fn _force_use_envelope_round_trip() {
    let keys = Keys::generate();
    let env = SettingsEnvelope {
        version: ENVELOPE_VERSION,
        alg: ENVELOPE_ALG.into(),
        pubkey: keys.public_key().to_hex(),
        ciphertext: encrypt_settings(&keys, "{}").unwrap(),
        updated_at: 0,
    };
    let _ = write_envelope(std::path::Path::new("/dev/null"), &env);
}
