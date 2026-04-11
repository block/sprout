use tauri::{AppHandle, State};
use uuid::Uuid;

use super::export_util::save_json_with_dialog;
use crate::{
    app_state::AppState,
    managed_agents::{
        encode_team_json, ensure_persona_ids_are_active, load_personas, load_teams,
        parse_team_json, save_teams, CreateTeamRequest, ParsedTeamPreview, TeamRecord,
        UpdateTeamRequest,
    },
    util::now_iso,
};

fn trim_required(value: &str, label: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{label} is required"));
    }
    Ok(trimmed.to_string())
}

fn trim_optional(value: Option<String>) -> Option<String> {
    value.and_then(|candidate| {
        let trimmed = candidate.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

#[tauri::command]
pub fn list_teams(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<TeamRecord>, String> {
    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|error| error.to_string())?;
    load_teams(&app)
}

#[tauri::command]
pub fn create_team(
    input: CreateTeamRequest,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<TeamRecord, String> {
    let name = trim_required(&input.name, "Team name")?;
    let description = trim_optional(input.description);
    let now = now_iso();

    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|error| error.to_string())?;
    let personas = load_personas(&app)?;
    ensure_persona_ids_are_active(&personas, &input.persona_ids)?;
    let mut teams = load_teams(&app)?;
    let team = TeamRecord {
        id: Uuid::new_v4().to_string(),
        name,
        description,
        persona_ids: input.persona_ids,
        created_at: now.clone(),
        updated_at: now,
    };
    teams.push(team.clone());
    save_teams(&app, &teams)?;
    Ok(team)
}

#[tauri::command]
pub fn update_team(
    input: UpdateTeamRequest,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<TeamRecord, String> {
    let name = trim_required(&input.name, "Team name")?;
    let description = trim_optional(input.description);

    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|error| error.to_string())?;
    let personas = load_personas(&app)?;
    ensure_persona_ids_are_active(&personas, &input.persona_ids)?;
    let mut teams = load_teams(&app)?;
    let team = teams
        .iter_mut()
        .find(|record| record.id == input.id)
        .ok_or_else(|| format!("team {} not found", input.id))?;

    team.name = name;
    team.description = description;
    team.persona_ids = input.persona_ids;
    team.updated_at = now_iso();

    let updated = team.clone();
    save_teams(&app, &teams)?;
    Ok(updated)
}

#[tauri::command]
pub fn delete_team(id: String, app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|error| error.to_string())?;
    let mut teams = load_teams(&app)?;
    let original_len = teams.len();
    teams.retain(|record| record.id != id);
    if teams.len() == original_len {
        return Err(format!("team {id} not found"));
    }
    save_teams(&app, &teams)
}

// ---------------------------------------------------------------------------
// Import / Export
// ---------------------------------------------------------------------------

const MAX_TEAM_JSON_BYTES: usize = 5 * 1024 * 1024;

#[tauri::command]
pub async fn export_team_to_json(
    id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    // Load team and personas under lock, then drop lock before dialog.
    let (team, personas) = {
        let _store_guard = state
            .managed_agents_store_lock
            .lock()
            .map_err(|e| e.to_string())?;
        let teams = load_teams(&app)?;
        let team = teams
            .into_iter()
            .find(|t| t.id == id)
            .ok_or_else(|| format!("team {id} not found"))?;
        let personas = load_personas(&app)?;
        (team, personas)
    };

    let json_bytes = encode_team_json(&team, &personas)?;

    let slug = crate::util::slugify(&team.name, "team", 50);
    let filename = format!("{slug}.team.json");
    save_json_with_dialog(&app, &filename, &json_bytes).await
}

#[tauri::command]
pub fn parse_team_file(
    file_bytes: Vec<u8>,
    _file_name: String,
) -> Result<ParsedTeamPreview, String> {
    if file_bytes.is_empty() {
        return Err("File is empty.".to_string());
    }
    if file_bytes.len() > MAX_TEAM_JSON_BYTES {
        return Err("File is too large (max 5 MB).".to_string());
    }

    // Detect zip files (persona packs) by magic bytes.
    if file_bytes.len() >= 4 && file_bytes[..4] == [0x50, 0x4B, 0x03, 0x04] {
        return parse_team_from_pack_zip(&file_bytes);
    }

    parse_team_json(&file_bytes)
}

/// Parse a persona pack zip as a team: pack name → team name, personas → members.
fn parse_team_from_pack_zip(zip_bytes: &[u8]) -> Result<ParsedTeamPreview, String> {
    use crate::managed_agents::{parse_zip_pack, TeamPersonaPreview};

    let result = parse_zip_pack(zip_bytes)?;
    if result.personas.is_empty() {
        return Err("Pack contains no personas.".to_string());
    }

    // Extract pack name from source_file format: "persona_name (Pack Name)"
    let pack_name = result.personas[0]
        .source_file
        .rsplit_once('(')
        .and_then(|(_, rest)| rest.strip_suffix(')'))
        .unwrap_or("Imported Pack")
        .to_string();

    Ok(ParsedTeamPreview {
        name: pack_name,
        description: None,
        personas: result
            .personas
            .into_iter()
            .map(|p| TeamPersonaPreview {
                display_name: p.display_name,
                system_prompt: p.system_prompt,
                avatar_url: p.avatar_data_url,
            })
            .collect(),
    })
}
