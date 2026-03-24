use tauri::{AppHandle, State};
use uuid::Uuid;

use super::export_util::save_json_with_dialog;
use crate::{
    app_state::AppState,
    managed_agents::{
        encode_persona_json, load_managed_agents, load_personas, parse_json_persona,
        parse_png_persona, parse_zip_personas, save_managed_agents, save_personas,
        CreatePersonaRequest, ParsePersonaFilesResult, PersonaRecord, UpdatePersonaRequest,
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
pub fn list_personas(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<PersonaRecord>, String> {
    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|error| error.to_string())?;
    load_personas(&app)
}

#[tauri::command]
pub fn create_persona(
    input: CreatePersonaRequest,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<PersonaRecord, String> {
    let display_name = trim_required(&input.display_name, "Display name")?;
    let system_prompt = trim_required(&input.system_prompt, "System prompt")?;
    let avatar_url = trim_optional(input.avatar_url);
    let provider = trim_optional(input.provider);
    let model = trim_optional(input.model);
    let now = now_iso();

    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|error| error.to_string())?;
    let mut personas = load_personas(&app)?;
    let persona = PersonaRecord {
        id: Uuid::new_v4().to_string(),
        display_name,
        avatar_url,
        system_prompt,
        provider,
        model,
        is_builtin: false,
        created_at: now.clone(),
        updated_at: now,
    };
    personas.push(persona.clone());
    save_personas(&app, &personas)?;
    Ok(persona)
}

#[tauri::command]
pub fn update_persona(
    input: UpdatePersonaRequest,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<PersonaRecord, String> {
    let display_name = trim_required(&input.display_name, "Display name")?;
    let system_prompt = trim_required(&input.system_prompt, "System prompt")?;
    let avatar_url = trim_optional(input.avatar_url);
    let provider = trim_optional(input.provider);
    let model = trim_optional(input.model);

    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|error| error.to_string())?;
    let mut personas = load_personas(&app)?;
    let persona = personas
        .iter_mut()
        .find(|record| record.id == input.id)
        .ok_or_else(|| format!("persona {} not found", input.id))?;

    if persona.is_builtin {
        return Err("Built-in personas cannot be edited.".to_string());
    }

    persona.display_name = display_name;
    persona.avatar_url = avatar_url;
    persona.system_prompt = system_prompt;
    persona.provider = provider;
    persona.model = model;
    persona.updated_at = now_iso();

    save_personas(&app, &personas)?;
    personas
        .into_iter()
        .find(|record| record.id == input.id)
        .ok_or_else(|| format!("persona {} disappeared unexpectedly", input.id))
}

#[tauri::command]
pub fn delete_persona(
    id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let _store_guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|error| error.to_string())?;
    let mut personas = load_personas(&app)?;
    let persona = personas
        .iter()
        .find(|record| record.id == id)
        .ok_or_else(|| format!("persona {id} not found"))?;
    if persona.is_builtin {
        return Err("Built-in personas cannot be deleted.".to_string());
    }

    let original_len = personas.len();
    personas.retain(|record| record.id != id);
    if personas.len() == original_len {
        return Err(format!("persona {id} not found"));
    }
    save_personas(&app, &personas)?;

    let mut agents = load_managed_agents(&app)?;
    let mut changed_agents = false;
    let now = now_iso();
    for agent in &mut agents {
        if agent.persona_id.as_deref() == Some(id.as_str()) {
            agent.persona_id = None;
            agent.updated_at = now.clone();
            changed_agents = true;
        }
    }
    if changed_agents {
        save_managed_agents(&app, &agents)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Import / Export
// ---------------------------------------------------------------------------

const MAX_PNG_BYTES: usize = 10 * 1024 * 1024;
const MAX_JSON_BYTES: usize = 5 * 1024 * 1024;
const MAX_ZIP_BYTES: usize = 100 * 1024 * 1024;

const PNG_MAGIC: [u8; 4] = [0x89, 0x50, 0x4E, 0x47];
const ZIP_MAGIC: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];
const JSON_OPEN_BRACE: u8 = 0x7B;

#[tauri::command]
pub fn parse_persona_files(
    file_bytes: Vec<u8>,
    file_name: String,
) -> Result<ParsePersonaFilesResult, String> {
    if file_bytes.len() > MAX_ZIP_BYTES {
        return Err("File is too large (max 100 MB).".to_string());
    }
    if file_bytes.is_empty() {
        return Err("File is empty.".to_string());
    }

    let first_byte = file_bytes[0];

    if file_bytes.len() >= 4 {
        let magic: [u8; 4] = file_bytes[..4]
            .try_into()
            .map_err(|_| "Failed to read file header".to_string())?;

        if magic == PNG_MAGIC {
            if file_bytes.len() > MAX_PNG_BYTES {
                return Err("PNG file is too large (max 10 MB).".to_string());
            }
            let mut preview = parse_png_persona(&file_bytes)?;
            preview.source_file = file_name;
            return Ok(ParsePersonaFilesResult {
                personas: vec![preview],
                skipped: vec![],
            });
        }

        if magic == ZIP_MAGIC {
            return parse_zip_personas(&file_bytes);
        }
    }

    if first_byte == JSON_OPEN_BRACE {
        if file_bytes.len() > MAX_JSON_BYTES {
            return Err("JSON file is too large (max 5 MB).".to_string());
        }
        let mut preview = parse_json_persona(&file_bytes)?;
        preview.source_file = file_name;
        return Ok(ParsePersonaFilesResult {
            personas: vec![preview],
            skipped: vec![],
        });
    }

    Err("Unsupported file format. Expected .persona.png, .persona.json, or .zip".to_string())
}

#[tauri::command]
pub async fn export_persona_to_json(
    id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    // Load persona data under lock, then drop lock before dialog.
    let (display_name, system_prompt, avatar_url, provider, model) = {
        let _store_guard = state
            .managed_agents_store_lock
            .lock()
            .map_err(|e| e.to_string())?;
        let personas = load_personas(&app)?;
        let persona = personas
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| format!("persona {id} not found"))?;
        (
            persona.display_name.clone(),
            persona.system_prompt.clone(),
            persona.avatar_url.clone(),
            persona.provider.clone(),
            persona.model.clone(),
        )
    };

    let json_bytes = encode_persona_json(
        &display_name,
        &system_prompt,
        avatar_url.as_deref(),
        provider.as_deref(),
        model.as_deref(),
    )?;

    let slug = crate::util::slugify(&display_name, "persona", 50);
    let filename = format!("{slug}.persona.json");
    save_json_with_dialog(&app, &filename, &json_bytes).await
}
