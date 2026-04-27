use nostr::EventId;
use reqwest::Method;
use tauri::State;

use crate::{
    app_state::AppState,
    events,
    models::{ContactEntry, ContactListResponse, UserNotesResponse},
    relay::{api_path, build_authed_request, send_json_request, submit_event, SubmitEventResponse},
};

/// Publish a global kind:1 text note (NIP-01).
#[tauri::command]
pub async fn publish_note(
    content: String,
    reply_to: Option<String>,
    mention_pubkeys: Option<Vec<String>>,
    media_tags: Option<Vec<Vec<String>>>,
    state: State<'_, AppState>,
) -> Result<SubmitEventResponse, String> {
    let reply_id = reply_to
        .map(|hex| EventId::from_hex(&hex).map_err(|e| format!("invalid reply_to event id: {e}")))
        .transpose()?;
    let mentions = mention_pubkeys.unwrap_or_default();
    let mention_refs: Vec<&str> = mentions.iter().map(|s| s.as_str()).collect();
    let media = media_tags.unwrap_or_default();
    let builder = events::build_note(&content, reply_id, &mention_refs, &media)?;
    submit_event(builder, &state).await
}

/// Fetch a user's NIP-02 contact list (kind:3).
#[tauri::command]
pub async fn get_contact_list(
    pubkey: String,
    state: State<'_, AppState>,
) -> Result<ContactListResponse, String> {
    let path = api_path(&["users", &pubkey, "contact-list"]);
    let request = build_authed_request(&state.http_client, Method::GET, &path, &state)?;
    send_json_request(request).await
}

/// Replace the full contact list (kind:3, NIP-02). Read-before-write required
/// for delta updates — the caller must merge with the existing list.
#[tauri::command]
pub async fn set_contact_list(
    contacts: Vec<ContactEntry>,
    state: State<'_, AppState>,
) -> Result<SubmitEventResponse, String> {
    let tuples: Vec<(&str, Option<&str>, Option<&str>)> = contacts
        .iter()
        .map(|c| {
            (
                c.pubkey.as_str(),
                c.relay_url.as_deref(),
                c.petname.as_deref(),
            )
        })
        .collect();

    let builder = events::build_contact_list(&tuples)?;
    submit_event(builder, &state).await
}

/// Maximum number of pubkeys per timeline request to prevent unbounded
/// sequential HTTP requests.
const MAX_TIMELINE_PUBKEYS: usize = 100;

/// Fetch notes for multiple pubkeys sequentially and return a merged, sorted timeline.
#[tauri::command]
pub async fn get_notes_timeline(
    pubkeys: Vec<String>,
    limit_per_user: Option<u32>,
    state: State<'_, AppState>,
) -> Result<UserNotesResponse, String> {
    if pubkeys.len() > MAX_TIMELINE_PUBKEYS {
        return Err(format!(
            "too many pubkeys (max {MAX_TIMELINE_PUBKEYS}, got {})",
            pubkeys.len()
        ));
    }

    let per_user = limit_per_user.unwrap_or(10).min(50);
    let mut all_notes = Vec::new();
    let mut errors = 0u32;

    for pk in &pubkeys {
        let path = api_path(&["users", pk, "notes"]);
        let req = build_authed_request(&state.http_client, Method::GET, &path, &state)
            .map(|r| r.query(&[("limit", per_user.to_string())]));
        match req {
            Ok(r) => match send_json_request::<UserNotesResponse>(r).await {
                Ok(resp) => all_notes.extend(resp.notes),
                Err(_) => errors += 1,
            },
            Err(_) => errors += 1,
        }
    }

    // If all requests failed, surface the error instead of returning empty.
    if errors > 0 && all_notes.is_empty() && !pubkeys.is_empty() {
        return Err("failed to fetch notes from any user".into());
    }

    // Sort newest-first.
    all_notes.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    all_notes.truncate(200);

    Ok(UserNotesResponse {
        notes: all_notes,
        next_cursor: None,
    })
}
