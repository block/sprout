//! Tests for the pure spawn-path branch added in plan v3 rev 3:
//! `resolve_target_profile`.
//!
//! `load_for_spawn` itself takes `AppHandle`/`AppState`, which need a real
//! Tauri test app. The wrapper→profile resolution + per-profile owner
//! cross-check are extracted into `resolve_target_profile` for direct unit
//! coverage; full IPC plumbing is exercised by Playwright.
//!
//! `LoadForSpawn` and `NamedProfile` intentionally don't implement `Debug`
//! (the latter holds an api_key). Tests destructure with explicit `match`
//! arms and a small classifier helper rather than printing variants.

use super::storage_profiles::new_profile_id;
use super::{
    NamedProfile, ProfilesPlaintext, StoredSettings, CURRENT_PLAINTEXT_SCHEMA, PROVIDER_ANTHROPIC,
};
use std::time::{SystemTime, UNIX_EPOCH};

// ── Helpers ─────────────────────────────────────────────────────────────────

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn stored_v2(owner_pubkey: &str, api_key: &str) -> StoredSettings {
    StoredSettings {
        schema_version: 2,
        owner_pubkey: owner_pubkey.into(),
        provider: PROVIDER_ANTHROPIC.into(),
        api_key: api_key.into(),
        base_url: "https://api.anthropic.com".into(),
        model: "claude-sonnet-4-5".into(),
        anthropic_api_version: Some("2023-06-01".into()),
        detected_provider_id: "anthropic".into(),
        detection_overridden: false,
        system_prompt: None,
        max_rounds: None,
        max_output_tokens: None,
        llm_timeout_secs: None,
        tool_timeout_secs: None,
        max_history_bytes: None,
    }
}

fn make_profile(label: &str, api_key: &str, owner_pubkey: &str) -> NamedProfile {
    let id = new_profile_id();
    let t = now();
    NamedProfile {
        id,
        label: label.into(),
        created_at: t,
        updated_at: t,
        settings: stored_v2(owner_pubkey, api_key),
    }
}

fn wrapper_with(
    profiles: Vec<NamedProfile>,
    default: Option<String>,
    owner: &str,
) -> ProfilesPlaintext {
    ProfilesPlaintext {
        schema_version: CURRENT_PLAINTEXT_SCHEMA,
        owner_pubkey: owner.into(),
        default_profile_id: default,
        profiles,
    }
}

/// Variant tag for assertion failure messages. Never prints contents.
fn load_tag(load: &super::spawn::LoadForSpawn) -> &'static str {
    match load {
        super::spawn::LoadForSpawn::Ok(_) => "Ok",
        super::spawn::LoadForSpawn::None => "None",
        super::spawn::LoadForSpawn::IdentityMismatch => "IdentityMismatch",
        super::spawn::LoadForSpawn::Error(_) => "Error",
    }
}

// ── resolve_target_profile ──────────────────────────────────────────────────

#[test]
fn resolve_target_profile_returns_explicit_id() {
    let owner = "a".repeat(64);
    let p1 = make_profile("Anthropic", "sk-ant-1", &owner);
    let p2 = make_profile("OpenAI", "sk-2", &owner);
    let want = p2.id.clone();
    let w = wrapper_with(vec![p1, p2], Some("ignored".into()), &owner);

    match super::spawn::resolve_target_profile(&w, Some(&want), &owner) {
        Ok(got) => {
            assert_eq!(got.id, want);
            assert_eq!(got.label, "OpenAI");
        }
        Err(load) => panic!("expected Ok, got Err({})", load_tag(&load)),
    }
}

#[test]
fn resolve_target_profile_unknown_id_errors_no_silent_default() {
    let owner = "a".repeat(64);
    let default = make_profile("Default", "sk-default", &owner);
    let default_id = default.id.clone();
    let w = wrapper_with(vec![default], Some(default_id), &owner);

    match super::spawn::resolve_target_profile(&w, Some("not-real"), &owner) {
        Err(super::spawn::LoadForSpawn::Error(e)) => {
            assert!(e.contains("not-real"), "want id in error, got: {e}");
            assert!(
                e.contains("not in saved settings"),
                "want fail-closed phrasing, got: {e}",
            );
        }
        Err(load) => panic!(
            "expected Error for unknown id, got Err({})",
            load_tag(&load)
        ),
        Ok(_) => panic!("expected Error for unknown id, got Ok"),
    }
}

#[test]
fn resolve_target_profile_uses_default_when_id_none() {
    let owner = "a".repeat(64);
    let p1 = make_profile("Other", "sk-x", &owner);
    let want = make_profile("Default", "sk-default", &owner);
    let want_id = want.id.clone();
    let w = wrapper_with(vec![p1, want], Some(want_id.clone()), &owner);

    match super::spawn::resolve_target_profile(&w, None, &owner) {
        Ok(got) => {
            assert_eq!(got.id, want_id);
            assert_eq!(got.label, "Default");
        }
        Err(load) => panic!("expected Ok (default), got Err({})", load_tag(&load)),
    }
}

#[test]
fn resolve_target_profile_no_default_errors() {
    let owner = "a".repeat(64);
    let p = make_profile("Only", "sk-only", &owner);
    let w = wrapper_with(vec![p], None, &owner);

    match super::spawn::resolve_target_profile(&w, None, &owner) {
        Err(super::spawn::LoadForSpawn::Error(e)) => {
            assert!(
                e.contains("no default") || e.contains("default"),
                "want default-missing phrasing, got: {e}",
            );
        }
        Err(load) => panic!(
            "expected Error when default is unset, got Err({})",
            load_tag(&load)
        ),
        Ok(_) => panic!("expected Error when default is unset, got Ok"),
    }
}

#[test]
fn resolve_target_profile_default_points_at_missing_errors() {
    let owner = "a".repeat(64);
    let p = make_profile("Real", "sk-real", &owner);
    let w = wrapper_with(vec![p], Some("ghost".into()), &owner);

    match super::spawn::resolve_target_profile(&w, None, &owner) {
        Err(super::spawn::LoadForSpawn::Error(e)) => {
            assert!(
                e.contains("default_profile_id"),
                "want default_profile_id error phrasing, got: {e}",
            );
        }
        Err(load) => panic!(
            "expected Error for dangling default, got Err({})",
            load_tag(&load)
        ),
        Ok(_) => panic!("expected Error for dangling default, got Ok"),
    }
}

#[test]
fn resolve_target_profile_empty_profiles_errors() {
    let owner = "a".repeat(64);
    let w = wrapper_with(vec![], None, &owner);

    match super::spawn::resolve_target_profile(&w, None, &owner) {
        Err(super::spawn::LoadForSpawn::Error(_)) => {}
        Err(load) => panic!(
            "expected Error for empty profile list, got Err({})",
            load_tag(&load)
        ),
        Ok(_) => panic!("expected Error for empty profile list, got Ok"),
    }

    match super::spawn::resolve_target_profile(&w, Some("anything"), &owner) {
        Err(super::spawn::LoadForSpawn::Error(e)) => {
            assert!(e.contains("anything"), "want id in error, got: {e}");
        }
        Err(load) => panic!(
            "expected Error for unknown id, got Err({})",
            load_tag(&load)
        ),
        Ok(_) => panic!("expected Error for unknown id, got Ok"),
    }
}

#[test]
fn resolve_target_profile_per_profile_owner_mismatch_is_identity_mismatch() {
    // Wrapper owner_pubkey matches current_pubkey, but the chosen
    // NamedProfile's inner owner_pubkey is stale (different identity). This
    // is the tamper case where someone reshuffled a NamedProfile from an
    // older wrapper into a freshly-written current-identity wrapper.
    let current = "a".repeat(64);
    let stale = "b".repeat(64);

    let stale_profile = make_profile("Stale", "sk-stale", &stale);
    let stale_id = stale_profile.id.clone();
    let w = wrapper_with(vec![stale_profile], Some(stale_id.clone()), &current);

    match super::spawn::resolve_target_profile(&w, None, &current) {
        Err(super::spawn::LoadForSpawn::IdentityMismatch) => {}
        Err(load) => panic!(
            "expected IdentityMismatch (default-pick), got Err({})",
            load_tag(&load)
        ),
        Ok(_) => panic!("expected IdentityMismatch for stale per-profile owner, got Ok"),
    }

    match super::spawn::resolve_target_profile(&w, Some(&stale_id), &current) {
        Err(super::spawn::LoadForSpawn::IdentityMismatch) => {}
        Err(load) => panic!(
            "expected IdentityMismatch (explicit pick), got Err({})",
            load_tag(&load)
        ),
        Ok(_) => panic!("expected IdentityMismatch when selected explicitly, got Ok"),
    }
}

#[test]
fn resolve_target_profile_skip_owner_check_when_profile_schema_v1() {
    // schema_version 1 profiles predate the owner-stamping in v2. The
    // per-profile cross-check is gated on schema_version >= 2, so a v1
    // legacy slot is allowed through (the wrapper-level owner_pubkey is the
    // authoritative integrity check at that point).
    let owner = "a".repeat(64);
    let mut p = make_profile("LegacyShape", "sk-legacy", &owner);
    p.settings.schema_version = 1;
    p.settings.owner_pubkey = String::new(); // v1 didn't carry one
    let id = p.id.clone();
    let w = wrapper_with(vec![p], Some(id.clone()), &owner);

    match super::spawn::resolve_target_profile(&w, None, &owner) {
        Ok(got) => assert_eq!(got.id, id),
        Err(load) => panic!(
            "v1-shaped profile should bypass per-profile owner check; got Err({})",
            load_tag(&load)
        ),
    }
}
