use std::{fs, path::PathBuf};

use tauri::AppHandle;

use crate::{
    managed_agents::{managed_agents_base_dir, PersonaRecord},
    util::now_iso,
};

struct BuiltInPersona {
    id: &'static str,
    display_name: &'static str,
    system_prompt: &'static str,
}

const BUILT_IN_PERSONAS: &[BuiltInPersona] = &[
    BuiltInPersona {
        id: "builtin:orchestrator",
        display_name: "Orchestrator",
        system_prompt: "You are an orchestration agent. Coordinate multi-step work across specialized agents, keep the overall plan moving, and synthesize results into a clear final outcome. When another agent should take a task, @mention them explicitly with the assignment, expected deliverable, and any relevant constraints or deadlines.",
    },
    BuiltInPersona {
        id: "builtin:researcher",
        display_name: "Researcher",
        system_prompt: "You are a research agent. Gather relevant information, compare sources, call out uncertainty, and return concise findings with evidence.",
    },
    BuiltInPersona {
        id: "builtin:planner",
        display_name: "Planner",
        system_prompt: "You are a planning agent. Turn ambiguous requests into structured plans with milestones, dependencies, risks, and clear next actions. Do not implement the work yourself unless asked.",
    },
    BuiltInPersona {
        id: "builtin:implementer",
        display_name: "Builder",
        system_prompt: "You are a builder agent. Execute tasks directly, make code and configuration changes carefully, validate the result, and explain important decisions and follow-up items.",
    },
    BuiltInPersona {
        id: "builtin:refactor",
        display_name: "Refactor",
        system_prompt: "You are a refactoring agent. Improve structure, naming, duplication, and module boundaries without changing externally observable behavior. Keep changes incremental, preserve compatibility, and add or update validation when behavior could drift.",
    },
    BuiltInPersona {
        id: "builtin:reviewer",
        display_name: "Reviewer",
        system_prompt: "You are a review agent. Inspect plans, code, and outputs for bugs, regressions, edge cases, security issues, and missing tests. Prioritize findings by severity, cite concrete evidence, and keep summaries secondary to the actual review.",
    },
];

fn personas_store_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(managed_agents_base_dir(app)?.join("personas.json"))
}

fn built_in_persona_records(now: &str) -> Vec<PersonaRecord> {
    BUILT_IN_PERSONAS
        .iter()
        .map(|persona| PersonaRecord {
            id: persona.id.to_string(),
            display_name: persona.display_name.to_string(),
            avatar_url: None,
            system_prompt: persona.system_prompt.to_string(),
            provider: None,
            model: None,
            is_builtin: true,
            created_at: now.to_string(),
            updated_at: now.to_string(),
        })
        .collect()
}

fn built_in_order(id: &str) -> Option<usize> {
    BUILT_IN_PERSONAS.iter().position(|persona| persona.id == id)
}

fn sort_personas(records: &mut [PersonaRecord]) {
    records.sort_by(|left, right| {
        let left_builtin = if left.is_builtin { 0 } else { 1 };
        let right_builtin = if right.is_builtin { 0 } else { 1 };

        left_builtin
            .cmp(&right_builtin)
            .then_with(|| match (built_in_order(&left.id), built_in_order(&right.id)) {
                (Some(left_order), Some(right_order)) => left_order.cmp(&right_order),
                _ => std::cmp::Ordering::Equal,
            })
            .then_with(|| {
                left.display_name
                    .to_lowercase()
                    .cmp(&right.display_name.to_lowercase())
            })
            .then_with(|| left.id.cmp(&right.id))
    });
}

fn merge_personas(mut stored: Vec<PersonaRecord>, now: &str) -> (Vec<PersonaRecord>, bool) {
    let mut changed = false;

    for built_in in built_in_persona_records(now) {
        if let Some(existing) = stored.iter_mut().find(|record| record.id == built_in.id) {
            let created_at = existing.created_at.clone();
            let updated_at = existing.updated_at.clone();
            if existing.display_name != built_in.display_name
                || existing.avatar_url.is_some()
                || existing.system_prompt != built_in.system_prompt
                || existing.provider.is_some()
                || existing.model.is_some()
                || !existing.is_builtin
            {
                *existing = PersonaRecord {
                    created_at,
                    updated_at,
                    ..built_in
                };
                changed = true;
            }
        } else {
            stored.push(built_in);
            changed = true;
        }
    }

    sort_personas(&mut stored);
    (stored, changed)
}

pub fn load_personas(app: &AppHandle) -> Result<Vec<PersonaRecord>, String> {
    let path = personas_store_path(app)?;
    let now = now_iso();

    let records = if path.exists() {
        let content = fs::read_to_string(&path)
            .map_err(|error| format!("failed to read persona store: {error}"))?;
        serde_json::from_str::<Vec<PersonaRecord>>(&content)
            .map_err(|error| format!("failed to parse persona store: {error}"))?
    } else {
        Vec::new()
    };

    let (records, changed) = merge_personas(records, &now);
    if changed || !path.exists() {
        save_personas(app, &records)?;
    }

    Ok(records)
}

pub fn save_personas(app: &AppHandle, records: &[PersonaRecord]) -> Result<(), String> {
    let mut sorted = records.to_vec();
    sort_personas(&mut sorted);

    let path = personas_store_path(app)?;
    let payload = serde_json::to_vec_pretty(&sorted)
        .map_err(|error| format!("failed to serialize persona store: {error}"))?;
    fs::write(&path, payload).map_err(|error| format!("failed to write persona store: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{merge_personas, BUILT_IN_PERSONAS};
    use crate::managed_agents::PersonaRecord;

    fn custom_persona(id: &str, display_name: &str) -> PersonaRecord {
        PersonaRecord {
            id: id.to_string(),
            display_name: display_name.to_string(),
            avatar_url: Some("https://example.com/avatar.png".to_string()),
            system_prompt: "Custom prompt".to_string(),
            provider: None,
            model: None,
            is_builtin: false,
            created_at: "2026-03-19T00:00:00Z".to_string(),
            updated_at: "2026-03-19T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn merge_personas_adds_missing_built_ins() {
        let (records, changed) = merge_personas(Vec::new(), "2026-03-19T00:00:00Z");

        assert!(changed);
        assert_eq!(records.len(), BUILT_IN_PERSONAS.len());
        assert!(records.iter().all(|record| record.is_builtin));
        let display_names: Vec<&str> = records.iter().map(|record| record.display_name.as_str()).collect();
        assert_eq!(
            display_names,
            vec!["Orchestrator", "Researcher", "Planner", "Builder", "Refactor", "Reviewer"]
        );
    }

    #[test]
    fn merge_personas_preserves_custom_records() {
        let custom = custom_persona("custom:test", "Custom");
        let (records, changed) = merge_personas(vec![custom.clone()], "2026-03-19T00:00:00Z");

        assert!(changed);
        assert!(records.iter().any(|record| record.id == custom.id));
    }

    #[test]
    fn merge_personas_restores_builtin_defaults() {
        let mut edited_builtin = custom_persona("builtin:researcher", "My Researcher");
        edited_builtin.is_builtin = true;
        let original_created_at = edited_builtin.created_at.clone();
        let original_updated_at = edited_builtin.updated_at.clone();

        let (records, changed) = merge_personas(vec![edited_builtin], "2026-03-19T00:00:00Z");

        assert!(changed);
        let researcher = records
            .iter()
            .find(|record| record.id == "builtin:researcher")
            .expect("researcher built-in should exist");
        assert_eq!(researcher.display_name, "Researcher");
        assert_eq!(researcher.avatar_url, None);
        assert_eq!(researcher.created_at, original_created_at);
        assert_eq!(researcher.updated_at, original_updated_at);
    }

    #[test]
    fn merge_personas_backfills_new_builtins_for_existing_store() {
        let mut legacy_builtins = vec![
            custom_persona("builtin:researcher", "Researcher"),
            custom_persona("builtin:planner", "Planner"),
            custom_persona("builtin:implementer", "Implementer"),
        ];
        for persona in &mut legacy_builtins {
            persona.is_builtin = true;
            persona.avatar_url = None;
        }

        let (records, changed) = merge_personas(legacy_builtins, "2026-03-19T00:00:00Z");

        assert!(changed);
        assert!(
            records
                .iter()
                .any(|record| record.id == "builtin:implementer" && record.display_name == "Builder")
        );
        assert!(records.iter().any(|record| record.id == "builtin:orchestrator"));
        assert!(records.iter().any(|record| record.id == "builtin:refactor"));
        assert!(records.iter().any(|record| record.id == "builtin:reviewer"));
    }
}
