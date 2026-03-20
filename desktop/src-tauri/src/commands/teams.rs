use tauri::{AppHandle, State};
use uuid::Uuid;

use crate::{
    app_state::AppState,
    managed_agents::{load_teams, save_teams, CreateTeamRequest, TeamRecord, UpdateTeamRequest},
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
