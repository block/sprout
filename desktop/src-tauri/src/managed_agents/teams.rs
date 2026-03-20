use std::{fs, path::PathBuf};

use tauri::AppHandle;

use crate::managed_agents::{managed_agents_base_dir, TeamRecord};

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

    let content =
        fs::read_to_string(&path).map_err(|error| format!("failed to read teams store: {error}"))?;
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

#[cfg(test)]
mod tests {
    use super::sort_teams;
    use crate::managed_agents::TeamRecord;

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
        let mut teams = vec![
            team("3", "Zulu"),
            team("1", "alpha"),
            team("2", "Bravo"),
        ];
        sort_teams(&mut teams);

        let names: Vec<&str> = teams.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "Bravo", "Zulu"]);
    }

    #[test]
    fn sort_teams_breaks_ties_by_id() {
        let mut teams = vec![
            team("b", "same"),
            team("a", "same"),
        ];
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
}
