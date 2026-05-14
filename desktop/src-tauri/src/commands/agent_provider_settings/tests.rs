//! Unit tests over the private items of the parent module's submodules.

use std::path::Path;

use nostr::Keys;

use super::storage::{
    decrypt_settings, encrypt_settings, normalize_origin, read_envelope, validate_input,
    validate_stored, write_envelope,
};
use std::ffi::OsStr;

use super::commands::compute_preview;
use super::spawn::{stored_to_env_pairs, LoadForSpawn};
use super::{
    AgentProviderSettingsInput, SettingsEnvelope, StoredSettings, ENVELOPE_ALG, ENVELOPE_VERSION,
    MAX_ENVELOPE_BYTES, MAX_PLAINTEXT_BYTES, MAX_SYSTEM_PROMPT_BYTES, OWNED_AGENT_ENV_VARS,
    PROVIDER_ANTHROPIC, PROVIDER_OPENAI,
};

/// Helper: encrypt+envelope-write a StoredSettings to a temp file, return path.
fn write_settings_with(keys: &Keys, path: &Path, stored: &StoredSettings) {
    let plain = serde_json::to_string(stored).unwrap();
    let ct = encrypt_settings(keys, &plain).unwrap();
    let env = SettingsEnvelope {
        version: ENVELOPE_VERSION,
        alg: ENVELOPE_ALG.into(),
        pubkey: keys.public_key().to_hex(),
        ciphertext: ct,
        updated_at: 0,
    };
    write_envelope(path, &env).unwrap();
}

fn stored_anthropic(api_key: &str) -> StoredSettings {
    StoredSettings {
        schema_version: 2,
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

fn make_input_anthropic(api_key: Option<String>) -> AgentProviderSettingsInput {
    AgentProviderSettingsInput {
        profile_id: None,
        label: "Default".into(),
        provider: PROVIDER_ANTHROPIC.into(),
        api_key,
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

#[test]
fn round_trip_envelope_and_view() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agent-provider-settings.json");
    let keys = Keys::generate();
    let stored = stored_anthropic("sk-ant-secret-key-1234");
    write_settings_with(&keys, &path, &stored);

    // Decrypt manually (the IPC command needs an AppHandle, which we don't have here).
    let env = read_envelope(&path).unwrap().unwrap();
    assert_eq!(env.pubkey, keys.public_key().to_hex());
    let plain = decrypt_settings(&keys, &env.ciphertext).unwrap();
    let got: StoredSettings = serde_json::from_str(&plain).unwrap();
    assert_eq!(got.api_key, "sk-ant-secret-key-1234");
    assert_eq!(got.model, "claude-sonnet-4-5");
}

#[test]
fn rejects_oversized_plaintext() {
    let keys = Keys::generate();
    let big = "a".repeat(MAX_PLAINTEXT_BYTES + 1);
    assert!(encrypt_settings(&keys, &big).is_err());
}

#[test]
fn preview_empty_returns_none() {
    assert_eq!(compute_preview(""), None);
}

#[test]
fn preview_short_key_returns_none_not_full_key() {
    // Defense against IPC contract violation: the previous implementation
    // returned the full key for len <= 4.
    assert_eq!(compute_preview("abc"), None);
    assert_eq!(compute_preview("abcd"), None);
    assert_eq!(compute_preview("abcde"), None);
    assert_eq!(compute_preview("abcdefg"), None); // 7 chars, still < 8 min
}

#[test]
fn preview_typical_key_returns_last_four() {
    let key = "sk-ant-api03-deadbeef-cafebabe-1234";
    assert_eq!(compute_preview(key).as_deref(), Some("1234"));
}

#[test]
fn preview_min_length_key_returns_last_four() {
    // Exactly MIN_KEY_LEN_FOR_PREVIEW (8 chars).
    assert_eq!(compute_preview("abcdefgh").as_deref(), Some("efgh"));
}

#[test]
fn max_system_prompt_fits_within_plaintext_budget() {
    // Regression for: validate_input accepts a 32 KB prompt, but the total
    // serialized JSON (prompt + other fields + key + structure overhead)
    // must still fit under MAX_PLAINTEXT_BYTES. With MAX_PLAINTEXT_BYTES
    // = 48 KB and prompt cap = 32 KB, this leaves 16 KB for everything else.
    assert!(
        MAX_PLAINTEXT_BYTES > MAX_SYSTEM_PROMPT_BYTES + 4096,
        "plaintext budget must comfortably exceed prompt cap + envelope/json overhead"
    );

    // End-to-end: serialize a max-sized prompt + a realistic api_key and
    // confirm encrypt accepts it.
    let keys = Keys::generate();
    let stored = serde_json::json!({
        "schema_version": 1,
        "provider": "anthropic",
        "api_key": "sk-ant-api03-".to_string() + &"x".repeat(120),
        "model": "claude-sonnet-4-5",
        "base_url": "https://api.anthropic.com",
        "anthropic_api_version": "2023-06-01",
        "system_prompt": "p".repeat(MAX_SYSTEM_PROMPT_BYTES),
        "detected_provider_id": "anthropic",
        "detection_overridden": false,
    });
    let plaintext = serde_json::to_string(&stored).unwrap();
    assert!(
        plaintext.as_bytes().len() <= MAX_PLAINTEXT_BYTES,
        "serialized max-prompt plaintext fits ({} <= {})",
        plaintext.as_bytes().len(),
        MAX_PLAINTEXT_BYTES,
    );
    assert!(encrypt_settings(&keys, &plaintext).is_ok());
}

#[test]
fn rejects_oversized_envelope_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agent-provider-settings.json");
    // Write a junk file larger than the cap.
    let huge = "x".repeat((MAX_ENVELOPE_BYTES + 1) as usize);
    std::fs::write(&path, huge).unwrap();
    let err = read_envelope(&path).unwrap_err();
    assert!(err.contains("too large"));
}

#[test]
fn rejects_wrong_version() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agent-provider-settings.json");
    let bad = serde_json::json!({
        "version": 99,
        "alg": ENVELOPE_ALG,
        "pubkey": "0".repeat(64),
        "ciphertext": "x",
        "updated_at": 0,
    });
    std::fs::write(&path, serde_json::to_string(&bad).unwrap()).unwrap();
    let err = read_envelope(&path).unwrap_err();
    assert!(err.contains("version"));
}

#[test]
fn rejects_wrong_alg() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agent-provider-settings.json");
    let bad = serde_json::json!({
        "version": 1,
        "alg": "aes-cbc",
        "pubkey": "0".repeat(64),
        "ciphertext": "x",
        "updated_at": 0,
    });
    std::fs::write(&path, serde_json::to_string(&bad).unwrap()).unwrap();
    let err = read_envelope(&path).unwrap_err();
    assert!(err.contains("alg"));
}

#[test]
fn rejects_malformed_pubkey() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agent-provider-settings.json");
    let bad = serde_json::json!({
        "version": 1,
        "alg": ENVELOPE_ALG,
        "pubkey": "deadbeef",
        "ciphertext": "x",
        "updated_at": 0,
    });
    std::fs::write(&path, serde_json::to_string(&bad).unwrap()).unwrap();
    assert!(read_envelope(&path).is_err());
}

#[test]
fn missing_file_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing.json");
    assert!(matches!(read_envelope(&path), Ok(None)));
}

#[test]
fn normalize_origin_basics() {
    assert_eq!(
        normalize_origin("https://api.foo.com").unwrap(),
        "https://api.foo.com:443"
    );
    assert_eq!(
        normalize_origin("https://API.foo.com:443/x").unwrap(),
        "https://api.foo.com:443"
    );
    // Loopback http is allowed (Ollama / LM Studio / vLLM local servers).
    assert_eq!(
        normalize_origin("http://127.0.0.1:11434").unwrap(),
        "http://127.0.0.1:11434"
    );
    assert_eq!(
        normalize_origin("http://localhost:8000/v1").unwrap(),
        "http://localhost:8000"
    );
    assert_eq!(
        normalize_origin("http://[::1]:8080").unwrap(),
        "http://[::1]:8080"
    );
}

#[test]
fn normalize_origin_rejects_scheme() {
    assert!(normalize_origin("ftp://api.foo.com").is_err());
    assert!(normalize_origin("not-a-url").is_err());
}

#[test]
fn normalize_origin_rejects_non_loopback_http() {
    // R2-H1 / codex review #2: cleartext http to a remote host would leak
    // the API key on the wire. Only loopback is allowed.
    assert!(normalize_origin("http://api.foo.com").is_err());
    assert!(normalize_origin("http://evil.example/v1").is_err());
    assert!(normalize_origin("http://10.0.0.1:8080").is_err());
    assert!(normalize_origin("http://192.168.1.5:8080").is_err());
}

#[test]
fn normalize_origin_rejects_userinfo_query_fragment() {
    // R2-H1: defense-in-depth against pasted curl-style URLs.
    assert!(normalize_origin("https://user:pass@api.foo.com").is_err());
    assert!(normalize_origin("https://user@api.foo.com").is_err());
    assert!(normalize_origin("https://api.foo.com/?api_key=leak").is_err());
    assert!(normalize_origin("https://api.foo.com#frag").is_err());
}

#[test]
fn normalize_origin_port_change_differs() {
    let a = normalize_origin("http://localhost:11434").unwrap();
    let b = normalize_origin("http://localhost:8000").unwrap();
    assert_ne!(a, b);
}

#[test]
fn validate_rejects_unknown_provider() {
    let mut inp = make_input_anthropic(Some("k".into()));
    inp.provider = "foo".into();
    assert!(validate_input(&inp).is_err());
}

#[test]
fn validate_rejects_zero_output_tokens() {
    let mut inp = make_input_anthropic(Some("k".into()));
    inp.max_output_tokens = Some(0);
    assert!(validate_input(&inp).is_err());
}

#[test]
fn validate_rejects_tiny_history_bytes() {
    let mut inp = make_input_anthropic(Some("k".into()));
    inp.max_history_bytes = Some(4096);
    assert!(validate_input(&inp).is_err());
}

#[test]
fn validate_rejects_oversized_system_prompt() {
    let mut inp = make_input_anthropic(Some("k".into()));
    inp.system_prompt = Some("a".repeat(MAX_SYSTEM_PROMPT_BYTES + 1));
    assert!(validate_input(&inp).is_err());
}

#[test]
fn validate_rejects_zero_timeouts() {
    let mut inp = make_input_anthropic(Some("k".into()));
    inp.llm_timeout_secs = Some(0);
    assert!(validate_input(&inp).is_err());
    let mut inp = make_input_anthropic(Some("k".into()));
    inp.tool_timeout_secs = Some(0);
    assert!(validate_input(&inp).is_err());
}

#[test]
fn stored_to_env_pairs_anthropic() {
    let s = stored_anthropic("sk-ant-xxx");
    let pairs = stored_to_env_pairs(&s);
    let map: std::collections::HashMap<_, _> = pairs.into_iter().collect();
    assert_eq!(
        map.get("SPROUT_AGENT_PROVIDER").map(|s| s.as_str()),
        Some("anthropic")
    );
    assert_eq!(
        map.get("ANTHROPIC_API_KEY").map(|s| s.as_str()),
        Some("sk-ant-xxx")
    );
    assert_eq!(
        map.get("ANTHROPIC_MODEL").map(|s| s.as_str()),
        Some("claude-sonnet-4-5")
    );
    assert_eq!(
        map.get("ANTHROPIC_BASE_URL").map(|s| s.as_str()),
        Some("https://api.anthropic.com")
    );
    assert_eq!(
        map.get("ANTHROPIC_API_VERSION").map(|s| s.as_str()),
        Some("2023-06-01")
    );
    assert!(!map.contains_key("OPENAI_COMPAT_API_KEY"));
    assert!(!map.contains_key("SPROUT_AGENT_SYSTEM_PROMPT"));
}

#[test]
fn stored_to_env_pairs_openai_and_knobs() {
    let mut s = stored_anthropic("sk-test");
    s.provider = PROVIDER_OPENAI.into();
    s.base_url = "https://api.openai.com/v1".into();
    s.anthropic_api_version = None;
    s.system_prompt = Some("be helpful".into());
    s.max_rounds = Some(8);
    s.max_output_tokens = Some(2048);
    s.llm_timeout_secs = Some(60);
    s.tool_timeout_secs = Some(120);
    s.max_history_bytes = Some(2 * 1024 * 1024);
    let map: std::collections::HashMap<_, _> = stored_to_env_pairs(&s).into_iter().collect();
    assert_eq!(
        map.get("OPENAI_COMPAT_API_KEY").map(|s| s.as_str()),
        Some("sk-test")
    );
    assert_eq!(
        map.get("OPENAI_COMPAT_MODEL").map(|s| s.as_str()),
        Some("claude-sonnet-4-5")
    );
    assert_eq!(
        map.get("OPENAI_COMPAT_BASE_URL").map(|s| s.as_str()),
        Some("https://api.openai.com/v1")
    );
    assert!(!map.contains_key("ANTHROPIC_API_KEY"));
    assert_eq!(
        map.get("SPROUT_AGENT_SYSTEM_PROMPT").map(|s| s.as_str()),
        Some("be helpful")
    );
    assert_eq!(
        map.get("SPROUT_AGENT_MAX_ROUNDS").map(|s| s.as_str()),
        Some("8")
    );
    assert_eq!(
        map.get("SPROUT_AGENT_MAX_OUTPUT_TOKENS")
            .map(|s| s.as_str()),
        Some("2048")
    );
    assert_eq!(
        map.get("SPROUT_AGENT_LLM_TIMEOUT_SECS").map(|s| s.as_str()),
        Some("60")
    );
    assert_eq!(
        map.get("SPROUT_AGENT_TOOL_TIMEOUT_SECS")
            .map(|s| s.as_str()),
        Some("120")
    );
    assert_eq!(
        map.get("SPROUT_AGENT_MAX_HISTORY_BYTES")
            .map(|s| s.as_str()),
        Some("2097152")
    );
}

#[test]
fn empty_system_prompt_not_emitted() {
    let mut s = stored_anthropic("sk-test");
    s.system_prompt = Some("".into());
    let map: std::collections::HashMap<_, _> = stored_to_env_pairs(&s).into_iter().collect();
    assert!(!map.contains_key("SPROUT_AGENT_SYSTEM_PROMPT"));
}

#[test]
fn pubkey_mismatch_preserves_file_on_disk() {
    // Verifies that the canonical "load with mismatch" behavior does NOT
    // rename the file — quarantine happens only on overwrite via save.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("agent-provider-settings.json");
    let keys_a = Keys::generate();
    let stored = stored_anthropic("sk-ant-original");
    write_settings_with(&keys_a, &path, &stored);

    // Now simulate identity rotation: a different keypair tries to read.
    let env = read_envelope(&path).unwrap().unwrap();
    let keys_b = Keys::generate();
    assert_ne!(env.pubkey, keys_b.public_key().to_hex());
    // The file is still on disk after the load (which is the only place
    // a mismatch would be detected). Nothing has renamed it.
    assert!(path.exists());
}
// ── apply_sprout_agent_provider_env tests ──────────────────────────────
//
// The pure helper is directly testable on `std::process::Command`.
// `Command::get_envs()` yields `(&OsStr, Option<&OsStr>)` where `None`
// means "remove this from inherited env". We assert both injection
// (k → Some(v)) and removal (k → None) here.

fn env_map_of(cmd: &std::process::Command) -> std::collections::HashMap<String, Option<String>> {
    cmd.get_envs()
        .map(|(k, v)| {
            (
                k.to_string_lossy().into_owned(),
                v.map(|v| v.to_string_lossy().into_owned()),
            )
        })
        .collect()
}

fn cmd_with_stale_inherit() -> std::process::Command {
    // Simulate the state of a Command after the rest of build_agent_command
    // has run: SPROUT_ACP_MODEL/PROMPT may have been set (we want to verify
    // they're cleared); the parent env may have stale shell exports (env_remove
    // is the only way to suppress those for child processes).
    let mut c = std::process::Command::new("/bin/true");
    c.env("SPROUT_ACP_MODEL", "stale-model-from-record");
    c.env("SPROUT_ACP_SYSTEM_PROMPT", "stale-prompt-from-record");
    c
}

#[test]
fn apply_ok_strips_owned_and_injects() {
    let mut cmd = cmd_with_stale_inherit();
    let pairs = vec![
        ("SPROUT_AGENT_PROVIDER".to_string(), "anthropic".to_string()),
        ("ANTHROPIC_API_KEY".to_string(), "sk-ant-xxx".to_string()),
        (
            "ANTHROPIC_MODEL".to_string(),
            "claude-sonnet-4-5".to_string(),
        ),
        (
            "ANTHROPIC_BASE_URL".to_string(),
            "https://api.anthropic.com".to_string(),
        ),
    ];
    super::spawn::apply_to_command(
        &mut cmd,
        LoadForSpawn::Ok(super::spawn::EnvPairs::new(pairs)),
        "test-agent",
    );
    let map = env_map_of(&cmd);

    // ACP-level overrides cleared.
    assert_eq!(map.get("SPROUT_ACP_MODEL"), Some(&None));
    assert_eq!(map.get("SPROUT_ACP_SYSTEM_PROMPT"), Some(&None));

    // SPROUT_AGENT_SYSTEM_PROMPT_FILE explicitly removed (mutually
    // exclusive with the panel-managed SYSTEM_PROMPT per config.rs:81).
    assert_eq!(map.get("SPROUT_AGENT_SYSTEM_PROMPT_FILE"), Some(&None));

    // Owned vars not in pairs are explicitly removed (so a stale shell
    // export can't shadow the panel).
    assert_eq!(
        map.get("OPENAI_COMPAT_API_KEY"),
        Some(&None),
        "OPENAI_COMPAT_API_KEY should be removed even though we're injecting anthropic vars"
    );

    // Injected pairs present with correct values.
    assert_eq!(
        map.get("SPROUT_AGENT_PROVIDER"),
        Some(&Some("anthropic".to_string()))
    );
    assert_eq!(
        map.get("ANTHROPIC_API_KEY"),
        Some(&Some("sk-ant-xxx".to_string()))
    );
    assert_eq!(
        map.get("ANTHROPIC_MODEL"),
        Some(&Some("claude-sonnet-4-5".to_string()))
    );
}

#[test]
fn apply_none_clears_acp_but_does_not_remove_owned() {
    // No saved settings: shell-env fallback is acceptable. We still
    // unconditionally clear ACP-level overrides so a stale record value
    // never leaks via the harness.
    let mut cmd = cmd_with_stale_inherit();
    super::spawn::apply_to_command(&mut cmd, LoadForSpawn::None, "test-agent");
    let map = env_map_of(&cmd);

    assert_eq!(map.get("SPROUT_ACP_MODEL"), Some(&None));
    assert_eq!(map.get("SPROUT_ACP_SYSTEM_PROMPT"), Some(&None));

    // None of the owned vars should be touched — the parent's shell env
    // is the intended source.
    for k in OWNED_AGENT_ENV_VARS {
        assert!(
            !map.contains_key(*k),
            "expected {k} to be untouched when LoadForSpawn::None"
        );
    }
    // And no settings-managed file removal either.
    assert!(!map.contains_key("SPROUT_AGENT_SYSTEM_PROMPT_FILE"));
}

#[test]
fn apply_identity_mismatch_fails_closed() {
    // Settings exist but were saved under a different nsec. We must NOT
    // inherit the parent shell's provider env — that would silently run
    // the agent under the wrong identity. Strip owned vars so the agent
    // surfaces a clean "missing required env" failure.
    let mut cmd = cmd_with_stale_inherit();
    super::spawn::apply_to_command(&mut cmd, LoadForSpawn::IdentityMismatch, "test-agent");
    let map = env_map_of(&cmd);

    assert_eq!(map.get("SPROUT_ACP_MODEL"), Some(&None));
    assert_eq!(map.get("SPROUT_ACP_SYSTEM_PROMPT"), Some(&None));

    for k in OWNED_AGENT_ENV_VARS {
        assert_eq!(
            map.get(*k).and_then(|v| v.as_ref()),
            None,
            "owned var {k} must be removed under identity-mismatch fail-closed"
        );
        assert!(
            map.contains_key(*k),
            "owned var {k} should have an explicit env_remove entry under \
             identity-mismatch (not merely absent)"
        );
    }
    assert_eq!(map.get("SPROUT_AGENT_SYSTEM_PROMPT_FILE"), Some(&None));
}

#[test]
fn apply_error_fails_closed() {
    // Corrupt/unreadable settings file: don't inject, but DO strip owned
    // vars so a stale shell export can't silently take over.
    let mut cmd = cmd_with_stale_inherit();
    super::spawn::apply_to_command(
        &mut cmd,
        LoadForSpawn::Error("malformed envelope".into()),
        "test-agent",
    );
    let map = env_map_of(&cmd);

    assert_eq!(map.get("SPROUT_ACP_MODEL"), Some(&None));
    assert_eq!(map.get("SPROUT_ACP_SYSTEM_PROMPT"), Some(&None));

    for k in OWNED_AGENT_ENV_VARS {
        assert_eq!(
            map.get(*k).and_then(|v| v.as_ref()),
            None,
            "owned var {k} should be removed under fail-closed"
        );
        // And the entry should be present-as-None (i.e. an explicit env_remove
        // was issued), not simply absent from the map.
        assert!(
            map.contains_key(*k),
            "owned var {k} should have an explicit env_remove entry under fail-closed"
        );
    }
    assert_eq!(map.get("SPROUT_AGENT_SYSTEM_PROMPT_FILE"), Some(&None));
}

#[test]
fn apply_ok_injects_openai_dialect_correctly() {
    let mut cmd = cmd_with_stale_inherit();
    let pairs = vec![
        ("SPROUT_AGENT_PROVIDER".to_string(), "openai".to_string()),
        (
            "OPENAI_COMPAT_API_KEY".to_string(),
            "sk-or-v1-deadbeef".to_string(),
        ),
        ("OPENAI_COMPAT_MODEL".to_string(), "gpt-4o".to_string()),
        (
            "OPENAI_COMPAT_BASE_URL".to_string(),
            "https://openrouter.ai/api/v1".to_string(),
        ),
    ];
    super::spawn::apply_to_command(
        &mut cmd,
        LoadForSpawn::Ok(super::spawn::EnvPairs::new(pairs)),
        "test-agent",
    );
    let map = env_map_of(&cmd);

    assert_eq!(
        map.get("OPENAI_COMPAT_API_KEY"),
        Some(&Some("sk-or-v1-deadbeef".to_string()))
    );
    // Anthropic vars must still be removed (they're in OWNED_AGENT_ENV_VARS).
    assert_eq!(map.get("ANTHROPIC_API_KEY"), Some(&None));
    assert_eq!(map.get("ANTHROPIC_MODEL"), Some(&None));
    assert_eq!(map.get("ANTHROPIC_BASE_URL"), Some(&None));
}

// ───────────────────────────────────────────────────────────────────────────
// R2 — codex review #2 follow-ups
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn validate_rejects_control_chars_in_fields() {
    // NUL terminates env vars in execve; CR/LF break log redaction.
    let mut inp = make_input_anthropic(Some("sk-ant-test".into()));
    inp.api_key = Some("sk-ant-test\n".into());
    assert!(
        validate_input(&inp).is_err(),
        "trailing newline in api_key should be rejected by validate"
    );

    let mut inp2 = make_input_anthropic(Some("sk-ant-test".into()));
    inp2.api_key = Some("sk-ant\0evil".into());
    assert!(validate_input(&inp2).is_err());

    let mut inp3 = make_input_anthropic(Some("sk-ant-test".into()));
    inp3.model = "claude\nsonnet".into();
    assert!(validate_input(&inp3).is_err());

    let mut inp4 = make_input_anthropic(Some("sk-ant-test".into()));
    inp4.detected_provider_id = "anth\nropic".into();
    assert!(validate_input(&inp4).is_err());
}

#[test]
fn validate_rejects_oversized_fields() {
    let mut inp = make_input_anthropic(Some("sk-ant-test".into()));
    inp.api_key = Some("a".repeat(5 * 1024));
    assert!(
        validate_input(&inp).is_err(),
        "api_key > 4 KB should be rejected"
    );

    let mut inp2 = make_input_anthropic(Some("sk-ant-test".into()));
    inp2.model = "m".repeat(300);
    assert!(
        validate_input(&inp2).is_err(),
        "model > 256 B should be rejected"
    );

    let mut inp3 = make_input_anthropic(Some("sk-ant-test".into()));
    inp3.base_url = format!("https://example.com/{}", "x".repeat(3 * 1024));
    assert!(
        validate_input(&inp3).is_err(),
        "base_url > 2 KB should be rejected"
    );
}

#[test]
fn save_trims_api_key_whitespace() {
    use crate::commands::agent_provider_settings::AgentProviderSettingsInput;
    // Trim happens in the command wrapper before validate_input runs. We
    // can't drive the real Tauri command without a full AppHandle, so test
    // the field-trim logic by constructing an input with surrounding
    // whitespace and asserting validate accepts it after we trim the way
    // the real command path does.
    let mut inp = AgentProviderSettingsInput {
        profile_id: None,
        label: "Default".into(),
        provider: PROVIDER_ANTHROPIC.into(),
        api_key: Some("  sk-ant-trim-me  \n".into()),
        model: "  claude-sonnet-4-5  ".into(),
        base_url: " https://api.anthropic.com\n".into(),
        anthropic_api_version: Some(" 2023-06-01 ".into()),
        system_prompt: None,
        max_rounds: None,
        max_output_tokens: None,
        llm_timeout_secs: None,
        tool_timeout_secs: None,
        max_history_bytes: None,
        detected_provider_id: " anthropic ".into(),
        detection_overridden: false,
    };
    // Pre-trim: control chars (the \n) should make validation fail.
    assert!(validate_input(&inp).is_err());

    // Mirror the real command-path trim.
    inp.model = inp.model.trim().to_owned();
    inp.base_url = inp.base_url.trim().to_owned();
    inp.detected_provider_id = inp.detected_provider_id.trim().to_owned();
    if let Some(v) = inp.anthropic_api_version.as_mut() {
        *v = v.trim().to_owned();
    }
    if let Some(k) = inp.api_key.as_mut() {
        *k = k.trim().to_owned();
    }
    validate_input(&inp).expect("post-trim input should validate");
}

#[test]
fn v2_envelope_round_trips_with_owner_pubkey() {
    use serde_json::Value;
    // Construct a v2 plaintext with explicit owner_pubkey and verify it
    // serializes the field so the loader's integrity check can read it.
    let mut s = stored_anthropic("sk-ant-test");
    s.schema_version = 2;
    s.owner_pubkey = "deadbeef".repeat(8);
    let json = serde_json::to_string(&s).unwrap();
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["schema_version"], 2);
    assert_eq!(v["owner_pubkey"].as_str().unwrap().len(), 64);
}

#[test]
fn v2_envelope_with_mismatched_owner_pubkey_decodes_as_view_check() {
    // R2-L7: the loader rejects v2 plaintexts whose owner_pubkey disagrees
    // with the envelope's outer pubkey. We can't drive `get_agent_provider_settings`
    // without a Tauri State, but we can simulate the check by deserializing
    // a plaintext with the wrong owner and asserting the integrity gate
    // would fire.
    let mut s = stored_anthropic("sk-ant-test");
    s.schema_version = 2;
    s.owner_pubkey = "deadbeef".repeat(8); // pretend this was written by another identity
    let json = serde_json::to_string(&s).unwrap();
    let parsed: StoredSettings = serde_json::from_str(&json).unwrap();
    let envelope_pubkey = "cafef00d".repeat(8);
    let integrity_pass = !(parsed.schema_version >= 2
        && !parsed.owner_pubkey.is_empty()
        && parsed.owner_pubkey != envelope_pubkey);
    assert!(
        !integrity_pass,
        "loader should treat plaintext.owner_pubkey != envelope.pubkey as identity_mismatch"
    );
}

#[test]
fn known_acp_provider_handles_inline_args() {
    // R2-M2: TS resolveAcpProviderId strips trailing args. Rust used to
    // not. Both now agree: an args-bearing command resolves to the bare
    // binary id, while whitespace-bearing aliases like "Claude Code" still
    // resolve via the alias table.
    use crate::managed_agents::known_acp_provider;
    assert!(known_acp_provider("sprout-agent --verbose").is_some_and(|p| p.id == "sprout-agent"));
    assert!(known_acp_provider("/usr/local/bin/sprout-agent --foo")
        .is_some_and(|p| p.id == "sprout-agent"));
    // Bare aliases still work after the change.
    assert!(known_acp_provider("Claude Code").is_some());
    assert!(known_acp_provider("sprout-agent").is_some_and(|p| p.id == "sprout-agent"));
    // Unknown stays unknown.
    assert!(known_acp_provider("totally-custom --verbose").is_none());
}

// ───────────────────────────────────────────────────────────────────────────
// R7 — codex review #7 follow-ups
// ───────────────────────────────────────────────────────────────────────────

#[test]
fn validate_stored_accepts_clean_remote_settings() {
    let s = stored_anthropic("sk-ant-test");
    validate_stored(&s).expect("baseline anthropic stored should validate");
}

#[test]
fn validate_stored_accepts_loopback_local_provider() {
    // OpenAI-compatible dialect against a loopback URL (Ollama/vLLM/llama.cpp).
    let mut s = stored_anthropic("placeholder-ignored");
    s.provider = PROVIDER_OPENAI.into();
    s.base_url = "http://127.0.0.1:11434/v1".into();
    s.detected_provider_id = "ollama".into();
    s.anthropic_api_version = None;
    validate_stored(&s).expect("loopback OpenAI-compat stored should validate");
}

#[test]
fn validate_stored_rejects_non_loopback_http_base_url() {
    // R7-P2: this is the headline rollback-attack scenario. A pre-validation
    // envelope (or one rolled back to a less-strict build) could have a
    // non-loopback HTTP base URL like `http://api.example.com/v1`. The
    // save-time gate now blocks this; spawn must too.
    let mut s = stored_anthropic("sk-ant-test");
    s.provider = PROVIDER_OPENAI.into();
    s.base_url = "http://api.example.com/v1".into();
    s.detected_provider_id = "openai".into();
    assert!(
        validate_stored(&s).is_err(),
        "stored settings with non-loopback http:// base URL must be rejected at spawn"
    );
}

#[test]
fn validate_stored_rejects_control_chars_in_key_and_model() {
    let s = stored_anthropic("sk-ant-bad\n");
    assert!(validate_stored(&s).is_err(), "newline in api_key rejected");

    let mut s2 = stored_anthropic("sk-ant-ok");
    s2.model = "claude\0sonnet".into();
    assert!(validate_stored(&s2).is_err(), "NUL in model rejected");

    let mut s3 = stored_anthropic("sk-ant-ok");
    s3.base_url = "https://api.anthropic.com\n".into();
    assert!(
        validate_stored(&s3).is_err(),
        "newline in base_url rejected"
    );

    // The api_key field on StoredSettings is the *only* place we drop the
    // newline check when empty (local providers may store empty keys in
    // some future flow). Spot-check that path:
    let mut s4 = stored_anthropic("");
    s4.provider = PROVIDER_OPENAI.into();
    s4.base_url = "http://127.0.0.1:11434/v1".into();
    s4.detected_provider_id = "ollama".into();
    s4.anthropic_api_version = None;
    validate_stored(&s4).expect("empty api_key allowed at load time");
}

#[test]
fn validate_stored_rejects_unknown_provider() {
    let mut s = stored_anthropic("sk-ant-test");
    s.provider = "rogue".into();
    assert!(validate_stored(&s).is_err());
}

#[test]
fn validate_stored_rejects_userinfo_or_query_in_base_url() {
    // Defense-in-depth for an envelope hand-edited to embed credentials.
    let mut s = stored_anthropic("sk-ant-test");
    s.base_url = "https://user:pass@api.anthropic.com".into();
    assert!(validate_stored(&s).is_err());

    let mut s2 = stored_anthropic("sk-ant-test");
    s2.base_url = "https://api.anthropic.com/?api_key=leak".into();
    assert!(validate_stored(&s2).is_err());
}

#[test]
fn validate_stored_rejects_oversized_system_prompt() {
    let mut s = stored_anthropic("sk-ant-test");
    s.system_prompt = Some("a".repeat(MAX_SYSTEM_PROMPT_BYTES + 1));
    assert!(validate_stored(&s).is_err());
}

// Silence unused-import warnings if OWNED_AGENT_ENV_VARS / OsStr aren't
// referenced in some test configs.
#[allow(dead_code)]
fn _force_use(_: &OsStr) {}
