use crate::managed_agents::known_acp_provider;

#[test]
fn sprout_agent_has_mcp_hooks() {
    let p = known_acp_provider("sprout-agent").expect("should resolve");
    assert!(p.mcp_hooks);
    assert_eq!(p.mcp_command, Some("sprout-dev-mcp"));
}

#[test]
fn sprout_agent_resolved_via_path() {
    assert!(known_acp_provider("/usr/local/bin/sprout-agent").is_some_and(|p| p.mcp_hooks));
}

#[test]
fn goose_has_no_mcp_hooks() {
    let p = known_acp_provider("goose").expect("should resolve");
    assert!(!p.mcp_hooks);
    assert_eq!(p.mcp_command, None);
}

#[test]
fn unknown_command_returns_none() {
    assert!(known_acp_provider("custom-agent").is_none());
}

// ── build_respond_to_env tests ───────────────────────────────────────

use super::build_respond_to_env;
use crate::managed_agents::types::{ManagedAgentRecord, RespondTo};

/// Construct a minimal record fixture for env-building tests. Only the
/// fields read by `build_respond_to_env` matter here.
fn fixture(
    respond_to: RespondTo,
    allowlist: Vec<String>,
    auth_tag: Option<String>,
) -> ManagedAgentRecord {
    ManagedAgentRecord {
        pubkey: "p".into(),
        name: "n".into(),
        persona_id: None,
        private_key_nsec: "nsec1fake".into(),
        auth_tag,
        relay_url: "ws://localhost:3000".into(),
        acp_command: "sprout-acp".into(),
        agent_command: "goose".into(),
        agent_args: vec![],
        mcp_command: "sprout-mcp-server".into(),
        turn_timeout_seconds: 320,
        idle_timeout_seconds: None,
        max_turn_duration_seconds: None,
        parallelism: 1,
        system_prompt: None,
        model: None,
        mcp_toolsets: None,
        env_vars: std::collections::BTreeMap::new(),
        start_on_app_launch: false,
        runtime_pid: None,
        backend: Default::default(),
        backend_agent_id: None,
        provider_binary_path: None,
        persona_pack_path: None,
        persona_name_in_pack: None,
        created_at: "now".into(),
        updated_at: "now".into(),
        last_started_at: None,
        last_stopped_at: None,
        last_exit_code: None,
        last_error: None,
        respond_to,
        respond_to_allowlist: allowlist,
    }
}

#[test]
fn build_env_owner_only_sets_mode_and_removes_others() {
    let rec = fixture(RespondTo::OwnerOnly, vec![], Some("tag".into()));
    let (set, remove) = build_respond_to_env(&rec, Some("owner")).unwrap();
    let set_map: std::collections::HashMap<_, _> = set.into_iter().collect();
    assert_eq!(
        set_map.get("SPROUT_ACP_RESPOND_TO").map(String::as_str),
        Some("owner-only")
    );
    assert!(!set_map.contains_key("SPROUT_ACP_RESPOND_TO_ALLOWLIST"));
    assert!(remove.contains(&"SPROUT_ACP_RESPOND_TO_ALLOWLIST"));
    // auth_tag is present → no AGENT_OWNER fallback fires.
    assert!(remove.contains(&"SPROUT_ACP_AGENT_OWNER"));
}

#[test]
fn build_env_allowlist_sets_both_envs_and_joins() {
    let a = "a".repeat(64);
    let b = "b".repeat(64);
    let rec = fixture(
        RespondTo::Allowlist,
        vec![a.clone(), b.clone()],
        Some("tag".into()),
    );
    let (set, _remove) = build_respond_to_env(&rec, Some("owner")).unwrap();
    let set_map: std::collections::HashMap<_, _> = set.into_iter().collect();
    assert_eq!(
        set_map.get("SPROUT_ACP_RESPOND_TO").map(String::as_str),
        Some("allowlist")
    );
    assert_eq!(
        set_map
            .get("SPROUT_ACP_RESPOND_TO_ALLOWLIST")
            .map(String::as_str),
        Some(format!("{a},{b}").as_str()),
    );
}

#[test]
fn build_env_anyone_omits_allowlist_var() {
    let rec = fixture(RespondTo::Anyone, vec![], Some("tag".into()));
    let (set, remove) = build_respond_to_env(&rec, Some("owner")).unwrap();
    let set_map: std::collections::HashMap<_, _> = set.into_iter().collect();
    assert_eq!(
        set_map.get("SPROUT_ACP_RESPOND_TO").map(String::as_str),
        Some("anyone")
    );
    assert!(!set_map.contains_key("SPROUT_ACP_RESPOND_TO_ALLOWLIST"));
    assert!(remove.contains(&"SPROUT_ACP_RESPOND_TO_ALLOWLIST"));
}

#[test]
fn build_env_legacy_record_without_auth_tag_emits_agent_owner() {
    let rec = fixture(RespondTo::OwnerOnly, vec![], None);
    let (set, remove) = build_respond_to_env(&rec, Some("ownerhex")).unwrap();
    let set_map: std::collections::HashMap<_, _> = set.into_iter().collect();
    assert_eq!(
        set_map.get("SPROUT_ACP_AGENT_OWNER").map(String::as_str),
        Some("ownerhex")
    );
    assert!(!remove.contains(&"SPROUT_ACP_AGENT_OWNER"));
}

#[test]
fn build_env_legacy_record_without_owner_hex_removes_agent_owner() {
    // No owner available to forward → make sure we don't inherit a leaked
    // env var from the parent.
    let rec = fixture(RespondTo::OwnerOnly, vec![], None);
    let (_set, remove) = build_respond_to_env(&rec, None).unwrap();
    assert!(remove.contains(&"SPROUT_ACP_AGENT_OWNER"));
}

#[test]
fn build_env_rejects_corrupted_allowlist() {
    let rec = fixture(
        RespondTo::Allowlist,
        vec!["not-hex".into()],
        Some("tag".into()),
    );
    assert!(build_respond_to_env(&rec, Some("owner")).is_err());
}

#[test]
fn build_env_rejects_empty_allowlist_in_allowlist_mode() {
    let rec = fixture(RespondTo::Allowlist, vec![], Some("tag".into()));
    let err = build_respond_to_env(&rec, Some("owner")).unwrap_err();
    assert!(err.contains("at least one pubkey"));
}

// ── resolve_effective_prompt_and_model tests ─────────────────────────

use super::resolve_effective_prompt_and_model;
use crate::managed_agents::PersonaRecord;

fn persona_fixture(id: &str, prompt: &str, model: Option<&str>) -> PersonaRecord {
    PersonaRecord {
        id: id.to_string(),
        display_name: "Test".to_string(),
        avatar_url: None,
        system_prompt: prompt.to_string(),
        provider: None,
        model: model.map(|m| m.to_string()),
        name_pool: Vec::new(),
        is_builtin: false,
        is_active: true,
        source_pack: None,
        source_pack_persona_slug: None,
        env_vars: std::collections::BTreeMap::new(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

#[test]
fn resolve_prompt_agent_override_wins_over_persona() {
    // Option A: when the agent record has explicit values (set via Edit dialog),
    // they take priority over the persona's live values.
    let personas = vec![persona_fixture(
        "custom:bot",
        "Persona prompt",
        Some("gpt-5"),
    )];

    let (prompt, model) = resolve_effective_prompt_and_model(
        Some("custom:bot"),
        &personas,
        Some("Agent override prompt"),
        Some("gpt-4-override"),
    );

    assert_eq!(prompt.as_deref(), Some("Agent override prompt"));
    assert_eq!(model.as_deref(), Some("gpt-4-override"));
}

#[test]
fn resolve_prompt_reads_persona_when_no_agent_override() {
    // When agent record fields are None (the default for persona-backed agents
    // after creation), the persona's live values are used.
    let personas = vec![persona_fixture(
        "custom:bot",
        "Fresh persona prompt",
        Some("gpt-5"),
    )];

    let (prompt, model) =
        resolve_effective_prompt_and_model(Some("custom:bot"), &personas, None, None);

    assert_eq!(prompt.as_deref(), Some("Fresh persona prompt"));
    assert_eq!(model.as_deref(), Some("gpt-5"));
}

#[test]
fn resolve_prompt_falls_back_when_no_persona_id() {
    let personas = vec![persona_fixture("custom:bot", "Fresh prompt", Some("gpt-5"))];

    let (prompt, model) = resolve_effective_prompt_and_model(
        None,
        &personas,
        Some("Record prompt"),
        Some("record-model"),
    );

    assert_eq!(prompt.as_deref(), Some("Record prompt"));
    assert_eq!(model.as_deref(), Some("record-model"));
}

/// Tests prompt/model resolution **in isolation** when the persona is missing.
///
/// In a real spawn, `resolve_persona_env()` (called later in `spawn_agent_child`)
/// would **fail closed** if the persona cannot be found — because persona env_vars
/// may contain required API credentials. This test only validates the
/// `resolve_effective_prompt_and_model` helper's fallback behavior, NOT that a
/// full agent spawn would succeed with a missing persona.
#[test]
fn resolve_prompt_falls_back_when_persona_not_found() {
    // NOTE: This tests the prompt/model resolution layer only. A full spawn
    // with a missing persona_id would fail at resolve_persona_env() before
    // the process is ever created.
    let personas = vec![persona_fixture("custom:other", "Other prompt", None)];

    let (prompt, model) = resolve_effective_prompt_and_model(
        Some("custom:deleted"),
        &personas,
        Some("Stale snapshot"),
        Some("old-model"),
    );

    assert_eq!(prompt.as_deref(), Some("Stale snapshot"));
    assert_eq!(model.as_deref(), Some("old-model"));
}

#[test]
fn resolve_prompt_returns_none_when_no_persona_and_no_record_prompt() {
    let (prompt, model) = resolve_effective_prompt_and_model(None, &[], None, None);

    assert_eq!(prompt, None);
    assert_eq!(model, None);
}

#[test]
fn resolve_prompt_persona_model_none_falls_through_when_no_override() {
    // Persona exists but has no model set. Agent record also has no model
    // override (None). Result: model is None.
    let personas = vec![persona_fixture("custom:bot", "Persona prompt", None)];

    let (prompt, model) =
        resolve_effective_prompt_and_model(Some("custom:bot"), &personas, None, None);

    assert_eq!(prompt.as_deref(), Some("Persona prompt"));
    assert_eq!(model, None);
}

#[test]
fn resolve_prompt_agent_model_override_wins_even_when_persona_model_none() {
    // Persona has no model, but agent record has an explicit override.
    // Agent override wins.
    let personas = vec![persona_fixture("custom:bot", "Persona prompt", None)];

    let (prompt, model) = resolve_effective_prompt_and_model(
        Some("custom:bot"),
        &personas,
        None,
        Some("agent-model-override"),
    );

    // Prompt comes from persona (no agent override), model from agent override.
    assert_eq!(prompt.as_deref(), Some("Persona prompt"));
    assert_eq!(model.as_deref(), Some("agent-model-override"));
}
