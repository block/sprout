use std::{fs, path::PathBuf};

use tauri::AppHandle;

use crate::managed_agents::{managed_agents_base_dir, PersonaRecord, TeamRecord};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
pub struct TeamPersonaPreview {
    pub display_name: String,
    pub system_prompt: String,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ParsedTeamPreview {
    pub name: String,
    pub description: Option<String>,
    pub personas: Vec<TeamPersonaPreview>,
}

fn teams_store_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(managed_agents_base_dir(app)?.join("teams.json"))
}

fn sort_teams(records: &mut [TeamRecord]) {
    records.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });
}

pub fn load_teams(app: &AppHandle) -> Result<Vec<TeamRecord>, String> {
    let path = teams_store_path(app)?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("failed to read teams store: {error}"))?;
    let mut records: Vec<TeamRecord> = serde_json::from_str(&content)
        .map_err(|error| format!("failed to parse teams store: {error}"))?;
    sort_teams(&mut records);
    Ok(records)
}

pub fn save_teams(app: &AppHandle, records: &[TeamRecord]) -> Result<(), String> {
    let mut sorted = records.to_vec();
    sort_teams(&mut sorted);

    let path = teams_store_path(app)?;
    let payload = serde_json::to_vec_pretty(&sorted)
        .map_err(|error| format!("failed to serialize teams store: {error}"))?;
    fs::write(&path, payload).map_err(|error| format!("failed to write teams store: {error}"))
}

// ---------------------------------------------------------------------------
// Team JSON export / import
// ---------------------------------------------------------------------------

/// Encode a team as a JSON blob for export. The format includes the team's
/// name, description, and the full persona data for each member (so the
/// import side can recreate personas that don't exist locally).
pub fn encode_team_json(team: &TeamRecord, personas: &[PersonaRecord]) -> Result<Vec<u8>, String> {
    let mut missing_persona_ids = Vec::new();
    let mut resolved_personas = Vec::with_capacity(team.persona_ids.len());

    for persona_id in &team.persona_ids {
        let Some(persona) = personas
            .iter()
            .find(|candidate| candidate.id == *persona_id)
        else {
            missing_persona_ids.push(persona_id.clone());
            continue;
        };

        resolved_personas.push(serde_json::json!({
            "displayName": persona.display_name,
            "systemPrompt": persona.system_prompt,
            "avatarUrl": persona.avatar_url,
        }));
    }

    if !missing_persona_ids.is_empty() {
        return Err(format!(
            "Team {} references missing personas: {}. Repair the team before exporting.",
            team.name,
            missing_persona_ids.join(", ")
        ));
    }

    let map = serde_json::json!({
        "version": 1,
        "type": "team",
        "name": team.name,
        "description": team.description,
        "personas": resolved_personas,
    });

    serde_json::to_vec_pretty(&map).map_err(|e| format!("Failed to serialize team JSON: {e}"))
}

/// Parse a team JSON file. Returns the team name, description, and embedded persona previews.
pub fn parse_team_json(json_bytes: &[u8]) -> Result<ParsedTeamPreview, String> {
    let v: serde_json::Value =
        serde_json::from_slice(json_bytes).map_err(|e| format!("Invalid JSON: {e}"))?;

    let version = v.get("version").and_then(|v| v.as_u64()).unwrap_or(0);
    if version != 1 {
        return Err(format!("Unsupported team version: {version}"));
    }

    let file_type = v.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if file_type != "team" {
        return Err("Not a team export file".to_string());
    }

    let name = v
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if name.is_empty() {
        return Err("Team name is empty".to_string());
    }

    let description = v
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let personas = v
        .get("personas")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| {
                    let display_name = p
                        .get("displayName")
                        .and_then(|v| v.as_str())?
                        .trim()
                        .to_string();
                    let system_prompt = p
                        .get("systemPrompt")
                        .and_then(|v| v.as_str())?
                        .trim()
                        .to_string();
                    let avatar_url = p
                        .get("avatarUrl")
                        .and_then(|v| v.as_str())
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty());
                    if display_name.is_empty() || system_prompt.is_empty() {
                        return None;
                    }
                    Some(TeamPersonaPreview {
                        display_name,
                        system_prompt,
                        avatar_url,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(ParsedTeamPreview {
        name,
        description,
        personas,
    })
}

#[cfg(test)]
mod tests {
    use super::{encode_team_json, parse_team_json, sort_teams};
    use crate::managed_agents::{PersonaRecord, TeamRecord};

    fn team(id: &str, name: &str) -> TeamRecord {
        TeamRecord {
            id: id.to_string(),
            name: name.to_string(),
            description: None,
            persona_ids: Vec::new(),
            created_at: "2026-03-20T00:00:00Z".to_string(),
            updated_at: "2026-03-20T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn sort_teams_alphabetical_case_insensitive() {
        let mut teams = vec![team("3", "Zulu"), team("1", "alpha"), team("2", "Bravo")];
        sort_teams(&mut teams);

        let names: Vec<&str> = teams.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "Bravo", "Zulu"]);
    }

    #[test]
    fn sort_teams_breaks_ties_by_id() {
        let mut teams = vec![team("b", "same"), team("a", "same")];
        sort_teams(&mut teams);

        let ids: Vec<&str> = teams.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b"]);
    }

    #[test]
    fn sort_teams_empty_is_noop() {
        let mut teams: Vec<TeamRecord> = Vec::new();
        sort_teams(&mut teams);
        assert!(teams.is_empty());
    }

    // -----------------------------------------------------------------------
    // encode / parse round-trip tests
    // -----------------------------------------------------------------------

    fn persona(id: &str, name: &str, prompt: &str) -> PersonaRecord {
        PersonaRecord {
            id: id.to_string(),
            display_name: name.to_string(),
            avatar_url: None,
            system_prompt: prompt.to_string(),
            provider: None,
            model: None,
            name_pool: Vec::new(),
            is_builtin: false,
            is_active: true,
            created_at: "2026-03-20T00:00:00Z".to_string(),
            updated_at: "2026-03-20T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn encode_parse_round_trip() {
        let t = team("t1", "My Team");
        let t = TeamRecord {
            description: Some("A great team".to_string()),
            persona_ids: vec!["p1".to_string(), "p2".to_string()],
            ..t
        };
        let personas = vec![
            persona("p1", "Alice", "You are Alice"),
            persona("p2", "Bob", "You are Bob"),
        ];

        let bytes = encode_team_json(&t, &personas).unwrap();
        let parsed = parse_team_json(&bytes).unwrap();

        assert_eq!(parsed.name, "My Team");
        assert_eq!(parsed.description.as_deref(), Some("A great team"));
        assert_eq!(parsed.personas.len(), 2);
        assert_eq!(parsed.personas[0].display_name, "Alice");
        assert_eq!(parsed.personas[0].system_prompt, "You are Alice");
        assert_eq!(parsed.personas[1].display_name, "Bob");
        assert_eq!(parsed.personas[1].system_prompt, "You are Bob");
    }

    #[test]
    fn encode_errors_for_missing_personas() {
        let t = TeamRecord {
            persona_ids: vec!["p1".to_string(), "missing".to_string()],
            ..team("t1", "Team")
        };
        let personas = vec![persona("p1", "Alice", "prompt")];

        let err = encode_team_json(&t, &personas).unwrap_err();

        assert_eq!(
            err,
            "Team Team references missing personas: missing. Repair the team before exporting."
        );
    }

    #[test]
    fn parse_team_json_invalid_version() {
        let json = serde_json::json!({
            "version": 99,
            "type": "team",
            "name": "X",
        });
        let bytes = serde_json::to_vec(&json).unwrap();
        let err = parse_team_json(&bytes).unwrap_err();
        assert!(err.contains("Unsupported team version"), "{err}");
    }

    #[test]
    fn parse_team_json_wrong_type() {
        let json = serde_json::json!({
            "version": 1,
            "type": "persona",
            "name": "X",
        });
        let bytes = serde_json::to_vec(&json).unwrap();
        let err = parse_team_json(&bytes).unwrap_err();
        assert!(err.contains("Not a team export file"), "{err}");
    }

    #[test]
    fn parse_team_json_empty_name() {
        let json = serde_json::json!({
            "version": 1,
            "type": "team",
            "name": "  ",
        });
        let bytes = serde_json::to_vec(&json).unwrap();
        let err = parse_team_json(&bytes).unwrap_err();
        assert!(err.contains("Team name is empty"), "{err}");
    }

    #[test]
    fn parse_team_json_skips_invalid_personas() {
        let json = serde_json::json!({
            "version": 1,
            "type": "team",
            "name": "Team",
            "personas": [
                { "displayName": "Good", "systemPrompt": "prompt" },
                { "displayName": "", "systemPrompt": "prompt" },
                { "displayName": "NoPrompt" },
            ],
        });
        let bytes = serde_json::to_vec(&json).unwrap();
        let parsed = parse_team_json(&bytes).unwrap();
        assert_eq!(parsed.personas.len(), 1);
        assert_eq!(parsed.personas[0].display_name, "Good");
    }

    #[test]
    fn parse_team_json_no_personas_key() {
        let json = serde_json::json!({
            "version": 1,
            "type": "team",
            "name": "Solo",
        });
        let bytes = serde_json::to_vec(&json).unwrap();
        let parsed = parse_team_json(&bytes).unwrap();
        assert!(parsed.personas.is_empty());
        assert_eq!(parsed.name, "Solo");
    }
}
