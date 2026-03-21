use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use uuid::Uuid;

use crate::{
    app_state::AppState,
    managed_agents::{
        encode_team_json, load_personas, load_teams, parse_team_json, save_teams,
        CreateTeamRequest, ParsedTeamPreview, TeamRecord, UpdateTeamRequest,
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
pub fn delete_team(
    id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
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

    // Slugify team name for filename.
    let slug: String = team
        .name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    let slug = if slug.is_empty() { "team" } else { &slug };
    let slug = if slug.len() > 50 { &slug[..50] } else { slug };
    let slug = slug.trim_end_matches('-');

    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .add_filter("JSON", &["json"])
        .set_file_name(&format!("{slug}.team.json"))
        .save_file(move |path| {
            let _ = tx.send(path);
        });

    let selected = rx.await.map_err(|_| "dialog cancelled".to_string())?;
    let file_path = match selected {
        Some(p) => p,
        None => return Ok(false),
    };

    let dest = file_path
        .as_path()
        .ok_or_else(|| "Save dialog returned an invalid path".to_string())?;
    std::fs::write(dest, &json_bytes)
        .map_err(|e| format!("Failed to write file: {e}"))?;

    Ok(true)
}

#[tauri::command]
pub fn parse_team_file(
    file_bytes: Vec<u8>,
    _file_name: String,
) -> Result<ParsedTeamPreview, String> {
    if file_bytes.is_empty() {
        return Err("File is empty.".to_string());
    }
    if file_bytes.len() > 5 * 1024 * 1024 {
        return Err("File is too large (max 5 MB).".to_string());
    }
    parse_team_json(&file_bytes)
}
