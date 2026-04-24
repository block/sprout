use std::collections::HashMap;

use reqwest::Method;
use serde_json::{Map, Value};
use sprout_core::PresenceStatus;
use tauri::State;

use crate::{
    app_state::AppState,
    events,
    models::{
        GetUserNotesQuery, GetUsersBatchBody, ProfileInfo, SearchUsersResponse, SetPresenceBody,
        SetPresenceResponse, UserNotesResponse, UsersBatchResponse,
    },
    relay::{api_path, build_authed_request, send_json_request, submit_event},
};

#[tauri::command]
pub async fn get_profile(state: State<'_, AppState>) -> Result<ProfileInfo, String> {
    let fallback_pubkey = current_pubkey_hex(&state)?;
    let request = build_authed_request(
        &state.http_client,
        Method::GET,
        "/api/users/me/profile",
        &state,
    )?;
    fetch_profile_info(request, &fallback_pubkey, true).await
}

#[tauri::command]
pub async fn update_profile(
    display_name: Option<String>,
    avatar_url: Option<String>,
    about: Option<String>,
    nip05_handle: Option<String>,
    state: State<'_, AppState>,
) -> Result<ProfileInfo, String> {
    // Read-merge-write: kind 0 is a full profile snapshot, so we must fetch
    // the current profile, merge the caller's changes, then sign the complete
    // profile as a Nostr event. Same pattern as MCP's set_profile.
    let current: serde_json::Value = {
        let request = build_authed_request(
            &state.http_client,
            Method::GET,
            "/api/users/me/profile",
            &state,
        )?;
        send_json_request(request).await.unwrap_or_default()
    };

    let dn = display_name
        .as_deref()
        .or_else(|| profile_field_str(&current, "display_name"));
    let name = profile_field_str(&current, "name");
    let picture = avatar_url
        .as_deref()
        .or_else(|| profile_field_str(&current, "avatar_url"));
    let ab = about
        .as_deref()
        .or_else(|| profile_field_str(&current, "about"));
    let nip05 = nip05_handle
        .as_deref()
        .or_else(|| profile_field_str(&current, "nip05_handle"));

    let builder = events::build_profile(dn, name, picture, ab, nip05)?;
    submit_event(builder, &state).await?;

    // Re-fetch to return the canonical profile the frontend expects.
    let fallback_pubkey = current_pubkey_hex(&state)?;
    let request = build_authed_request(
        &state.http_client,
        Method::GET,
        "/api/users/me/profile",
        &state,
    )?;
    fetch_profile_info(request, &fallback_pubkey, true).await
}

// ── Unchanged reads below ────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_user_profile(
    pubkey: Option<String>,
    state: State<'_, AppState>,
) -> Result<ProfileInfo, String> {
    let path = match pubkey.as_deref() {
        Some(pubkey) => api_path(&["users", pubkey, "profile"]),
        None => "/api/users/me/profile".to_string(),
    };
    let fallback_pubkey = match pubkey {
        Some(pubkey) => pubkey,
        None => current_pubkey_hex(&state)?,
    };
    let request = build_authed_request(&state.http_client, Method::GET, &path, &state)?;
    fetch_profile_info(request, &fallback_pubkey, false).await
}

#[tauri::command]
pub async fn get_users_batch(
    pubkeys: Vec<String>,
    state: State<'_, AppState>,
) -> Result<UsersBatchResponse, String> {
    let request =
        build_authed_request(&state.http_client, Method::POST, "/api/users/batch", &state)?.json(
            &GetUsersBatchBody {
                pubkeys: pubkeys.as_slice(),
            },
        );
    send_json_request(request).await
}

#[tauri::command]
pub async fn get_user_notes(
    pubkey: String,
    limit: Option<u32>,
    before: Option<i64>,
    before_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<UserNotesResponse, String> {
    let path = format!("/api/users/{pubkey}/notes");
    let request = build_authed_request(&state.http_client, Method::GET, &path, &state)?.query(
        &GetUserNotesQuery {
            limit,
            before,
            before_id: before_id.as_deref(),
        },
    );

    send_json_request(request).await
}

#[tauri::command]
pub async fn search_users(
    query: String,
    limit: Option<u32>,
    state: State<'_, AppState>,
) -> Result<SearchUsersResponse, String> {
    let limit = limit.unwrap_or(8);
    let limit_param = limit.to_string();
    let request =
        build_authed_request(&state.http_client, Method::GET, "/api/users/search", &state)?
            .query(&[("q", query.as_str()), ("limit", limit_param.as_str())]);

    send_json_request(request).await
}

#[tauri::command]
pub async fn get_presence(
    pubkeys: Vec<String>,
    state: State<'_, AppState>,
) -> Result<HashMap<String, PresenceStatus>, String> {
    if pubkeys.is_empty() {
        return Ok(HashMap::new());
    }

    let request = build_authed_request(&state.http_client, Method::GET, "/api/presence", &state)?
        .query(&[("pubkeys", pubkeys.join(","))]);
    send_json_request(request).await
}

#[tauri::command]
pub async fn set_presence(
    status: PresenceStatus,
    state: State<'_, AppState>,
) -> Result<SetPresenceResponse, String> {
    let request = build_authed_request(&state.http_client, Method::PUT, "/api/presence", &state)?
        .json(&SetPresenceBody { status });
    send_json_request(request).await
}

fn current_pubkey_hex(state: &AppState) -> Result<String, String> {
    let keys = state.keys.lock().map_err(|e| e.to_string())?;
    Ok(keys.public_key().to_hex())
}

fn empty_profile_info(pubkey: &str) -> ProfileInfo {
    ProfileInfo {
        pubkey: pubkey.to_string(),
        display_name: None,
        avatar_url: None,
        about: None,
        nip05_handle: None,
    }
}

fn is_missing_profile_error(error: &str) -> bool {
    error.starts_with("relay returned 404") && error.contains("user not found")
}

fn profile_object(value: &Value) -> Option<&Map<String, Value>> {
    value
        .get("profile")
        .and_then(Value::as_object)
        .or_else(|| value.as_object())
}

fn profile_field_str<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    profile_object(value)
        .and_then(|object| object.get(key))
        .and_then(Value::as_str)
}

fn optional_profile_string(
    object: &Map<String, Value>,
    key: &str,
) -> Result<Option<String>, String> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(format!("parse failed: invalid profile field `{key}`")),
    }
}

fn profile_info_from_value(value: Value, fallback_pubkey: &str) -> Result<ProfileInfo, String> {
    let object = profile_object(&value)
        .ok_or_else(|| "parse failed: expected profile object".to_string())?;

    let pubkey = match object.get("pubkey") {
        None | Some(Value::Null) => fallback_pubkey.to_string(),
        Some(Value::String(value)) => value.clone(),
        Some(_) => return Err("parse failed: invalid profile field `pubkey`".to_string()),
    };

    Ok(ProfileInfo {
        pubkey,
        display_name: optional_profile_string(object, "display_name")?,
        avatar_url: optional_profile_string(object, "avatar_url")?,
        about: optional_profile_string(object, "about")?,
        nip05_handle: optional_profile_string(object, "nip05_handle")?,
    })
}

async fn fetch_profile_info(
    request: reqwest::RequestBuilder,
    fallback_pubkey: &str,
    missing_profile_ok: bool,
) -> Result<ProfileInfo, String> {
    match send_json_request::<Value>(request).await {
        Ok(value) => profile_info_from_value(value, fallback_pubkey),
        Err(error) if missing_profile_ok && is_missing_profile_error(&error) => {
            Ok(empty_profile_info(fallback_pubkey))
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        empty_profile_info, is_missing_profile_error, profile_field_str, profile_info_from_value,
    };

    #[test]
    fn profile_info_from_value_accepts_top_level_shape() {
        let value = serde_json::json!({
            "pubkey": "abc123",
            "display_name": "Sprout User",
            "avatar_url": "https://example.com/avatar.png",
            "about": "Hello",
            "nip05_handle": "sprout@example.com"
        });

        let profile = profile_info_from_value(value, "fallback").expect("profile");

        assert_eq!(profile.pubkey, "abc123");
        assert_eq!(profile.display_name.as_deref(), Some("Sprout User"));
        assert_eq!(
            profile.avatar_url.as_deref(),
            Some("https://example.com/avatar.png")
        );
        assert_eq!(profile.about.as_deref(), Some("Hello"));
        assert_eq!(profile.nip05_handle.as_deref(), Some("sprout@example.com"));
    }

    #[test]
    fn profile_info_from_value_accepts_nested_profile_shape() {
        let value = serde_json::json!({
            "profile": {
                "display_name": "Nested User",
                "avatar_url": null,
                "about": null,
                "nip05_handle": null
            }
        });

        let profile = profile_info_from_value(value, "fallback-pubkey").expect("profile");

        assert_eq!(profile.pubkey, "fallback-pubkey");
        assert_eq!(profile.display_name.as_deref(), Some("Nested User"));
        assert_eq!(profile.avatar_url, None);
        assert_eq!(profile.about, None);
        assert_eq!(profile.nip05_handle, None);
    }

    #[test]
    fn missing_profile_errors_are_detected() {
        assert!(is_missing_profile_error(
            "relay returned 404: user not found"
        ));
        assert!(is_missing_profile_error(
            "relay returned 404 Not Found: user not found"
        ));
        assert!(!is_missing_profile_error(
            "relay returned 401: authentication failed"
        ));
    }

    #[test]
    fn profile_field_str_accepts_nested_profile_shape() {
        let value = serde_json::json!({
            "profile": {
                "display_name": "Nested User",
                "about": "Nested about"
            }
        });

        assert_eq!(
            profile_field_str(&value, "display_name"),
            Some("Nested User")
        );
        assert_eq!(profile_field_str(&value, "about"), Some("Nested about"));
        assert_eq!(profile_field_str(&value, "avatar_url"), None);
    }

    #[test]
    fn empty_profile_info_preserves_pubkey() {
        let profile = empty_profile_info("fallback-pubkey");

        assert_eq!(profile.pubkey, "fallback-pubkey");
        assert_eq!(profile.display_name, None);
        assert_eq!(profile.avatar_url, None);
        assert_eq!(profile.about, None);
        assert_eq!(profile.nip05_handle, None);
    }
}
