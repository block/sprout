//! Event endpoints.
//!
//! Endpoints:
//!   GET  /api/events/:id — fetch a single stored event by ID
//!   POST /api/events     — submit a signed Nostr event for ingestion

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};

use crate::handlers::ingest::{HttpAuthMethod, IngestAuth, IngestError};
use crate::state::AppState;

use super::{
    api_error, check_channel_access, check_token_channel_access, extract_auth_context,
    internal_error, not_found, RestAuthMethod,
};

use sprout_core::kind::{
    event_kind_u32, KIND_CONTACT_LIST, KIND_LONG_FORM, KIND_PROFILE, KIND_TEXT_NOTE,
};

/// Fetch a single stored event by its 64-char hex ID.
pub async fn get_event(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(event_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // Step 1: authenticate (no scope check yet)
    let ctx = extract_auth_context(&headers, &state).await?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();

    // Step 2: parse event ID
    let id_bytes = hex::decode(&event_id)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid event ID"))?;
    if id_bytes.len() != 32 {
        return Err(api_error(StatusCode::BAD_REQUEST, "invalid event ID"));
    }

    // Step 3: load the event (no scope check yet)
    let stored_event = state
        .db
        .get_event_by_id(&id_bytes)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?
        .ok_or_else(|| not_found("event not found"))?;

    // Step 4: scope check depends on whether this is a channel event or a global event
    if let Some(channel_id) = stored_event.channel_id {
        // Channel event: MessagesRead + membership check.
        // All failures return 404 (not 403) to avoid leaking event existence.
        sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::MessagesRead)
            .map_err(|_| not_found("event not found"))?;
        check_token_channel_access(&ctx, &channel_id).map_err(|_| not_found("event not found"))?;
        check_channel_access(&state, channel_id, &pubkey_bytes)
            .await
            .map_err(|_| not_found("event not found"))?;
    } else {
        // Global event — scope-aware allowlist.
        let event_kind = event_kind_u32(&stored_event.event);

        const USER_DATA_KINDS: [u32; 2] = [KIND_PROFILE, KIND_CONTACT_LIST];
        const MESSAGE_KINDS: [u32; 2] = [KIND_TEXT_NOTE, KIND_LONG_FORM];

        let scope_ok = if USER_DATA_KINDS.contains(&event_kind) {
            sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::UsersRead).is_ok()
        } else if MESSAGE_KINDS.contains(&event_kind) {
            sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::MessagesRead).is_ok()
        } else {
            false
        };

        if !scope_ok {
            // Return 404 (not 403) to avoid leaking event existence
            // when the caller lacks the required scope.
            return Err(not_found("event not found"));
        }
    }

    let tags = serde_json::to_value(&stored_event.event.tags)
        .map_err(|e| internal_error(&format!("tag serialization error: {e}")))?;

    Ok(Json(serde_json::json!({
        "id":         stored_event.event.id.to_hex(),
        "pubkey":     stored_event.event.pubkey.to_hex(),
        "created_at": stored_event.event.created_at.as_u64(),
        "kind":       stored_event.event.kind.as_u16(),
        "tags":       tags,
        "content":    stored_event.event.content,
        "sig":        stored_event.event.sig.to_string(),
    })))
}

// ── POST /api/events ─────────────────────────────────────────────────────────

/// Submit a signed Nostr event for ingestion.
///
/// Accepts the same 18 persistent kinds as the WebSocket `["EVENT", ...]` path.
/// WS-only kinds (1059 gift-wrap, 20001 presence) are rejected.
///
/// Auth: API token, Okta JWT, or dev X-Pubkey — mapped to [`IngestAuth::Http`].
pub async fn submit_event(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;

    let event: nostr::Event = serde_json::from_slice(&body)
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, &format!("invalid event JSON: {e}")))?;

    let auth = IngestAuth::Http {
        pubkey: ctx.pubkey,
        scopes: ctx.scopes,
        auth_method: match ctx.auth_method {
            RestAuthMethod::ApiToken => HttpAuthMethod::ApiToken,
            RestAuthMethod::OktaJwt => HttpAuthMethod::OktaJwt,
            RestAuthMethod::DevPubkey => HttpAuthMethod::DevPubkey,
            RestAuthMethod::Nip98 => {
                return Err(api_error(
                    StatusCode::BAD_REQUEST,
                    "NIP-98 auth is not supported for event submission",
                ));
            }
        },
        token_id: ctx.token_id,
        channel_ids: ctx.channel_ids,
    };

    match crate::handlers::ingest::ingest_event(&state, event, auth).await {
        Ok(result) => Ok(Json(serde_json::json!({
            "event_id": result.event_id,
            "accepted": result.accepted,
            "message": result.message,
        }))),
        Err(e) => match e {
            IngestError::Rejected(msg) => Err(api_error(StatusCode::BAD_REQUEST, &msg)),
            IngestError::AuthFailed(msg) => Err(api_error(StatusCode::FORBIDDEN, &msg)),
            IngestError::Internal(msg) => Err(internal_error(&msg)),
        },
    }
}

#[cfg(test)]
mod tests {
    use sprout_core::kind::{KIND_CONTACT_LIST, KIND_LONG_FORM, KIND_PROFILE, KIND_TEXT_NOTE};

    const USER_DATA_KINDS: [u32; 2] = [KIND_PROFILE, KIND_CONTACT_LIST];
    const MESSAGE_KINDS: [u32; 2] = [KIND_TEXT_NOTE, KIND_LONG_FORM];

    /// MessagesRead must NOT satisfy UsersRead requirement.
    #[test]
    fn kind0_profile_requires_users_read() {
        let scopes = vec![sprout_auth::Scope::MessagesRead];
        assert!(
            sprout_auth::require_scope(&scopes, sprout_auth::Scope::UsersRead).is_err(),
            "MessagesRead must not satisfy UsersRead"
        );
    }

    /// kind:3 (contact list) also requires UsersRead.
    #[test]
    fn kind3_contact_list_requires_users_read() {
        let scopes = vec![sprout_auth::Scope::MessagesRead];
        assert!(
            sprout_auth::require_scope(&scopes, sprout_auth::Scope::UsersRead).is_err(),
            "MessagesRead must not satisfy UsersRead for contact lists"
        );
    }

    /// kind:1 (text note) requires MessagesRead.
    #[test]
    fn kind1_text_note_requires_messages_read() {
        let scopes = vec![sprout_auth::Scope::MessagesRead];
        assert!(
            sprout_auth::require_scope(&scopes, sprout_auth::Scope::MessagesRead).is_ok(),
            "MessagesRead should satisfy MessagesRead"
        );
    }

    /// UsersRead must NOT satisfy MessagesRead requirement.
    #[test]
    fn kind1_with_only_users_read_is_denied() {
        let scopes = vec![sprout_auth::Scope::UsersRead];
        assert!(
            sprout_auth::require_scope(&scopes, sprout_auth::Scope::MessagesRead).is_err(),
            "UsersRead must not satisfy MessagesRead"
        );
    }

    /// Unknown global kinds (e.g. kind:1059 gift wrap) are denied regardless of scopes.
    #[test]
    fn unknown_global_kind_denied() {
        let gift_wrap_kind: u32 = 1059;
        let scope_ok = if USER_DATA_KINDS.contains(&gift_wrap_kind) {
            true
        } else if MESSAGE_KINDS.contains(&gift_wrap_kind) {
            true
        } else {
            false
        };
        assert!(!scope_ok, "kind:1059 must be denied by the allowlist");
    }

    // ── Positive-path tests ──────────────────────────────────────────────

    /// UsersRead grants access to kind:0 (profile).
    #[test]
    fn kind0_profile_allowed_with_users_read() {
        let scopes = vec![sprout_auth::Scope::UsersRead];
        assert!(
            USER_DATA_KINDS.contains(&KIND_PROFILE)
                && sprout_auth::require_scope(&scopes, sprout_auth::Scope::UsersRead).is_ok(),
            "UsersRead should grant access to kind:0 profile events"
        );
    }

    /// UsersRead grants access to kind:3 (contact list).
    #[test]
    fn kind3_contact_list_allowed_with_users_read() {
        let scopes = vec![sprout_auth::Scope::UsersRead];
        assert!(
            USER_DATA_KINDS.contains(&KIND_CONTACT_LIST)
                && sprout_auth::require_scope(&scopes, sprout_auth::Scope::UsersRead).is_ok(),
            "UsersRead should grant access to kind:3 contact list events"
        );
    }

    /// MessagesRead grants access to kind:30023 (long-form content).
    #[test]
    fn kind30023_long_form_allowed_with_messages_read() {
        let scopes = vec![sprout_auth::Scope::MessagesRead];
        assert!(
            MESSAGE_KINDS.contains(&KIND_LONG_FORM)
                && sprout_auth::require_scope(&scopes, sprout_auth::Scope::MessagesRead).is_ok(),
            "MessagesRead should grant access to kind:30023 long-form events"
        );
    }

    /// Full routing: each allowed kind maps to exactly one scope.
    #[test]
    fn scope_routing_exhaustive() {
        let users_read = vec![sprout_auth::Scope::UsersRead];
        let messages_read = vec![sprout_auth::Scope::MessagesRead];

        // Helper: mirrors the get_event scope-check logic
        let check = |kind: u32, scopes: &[sprout_auth::Scope]| -> bool {
            if USER_DATA_KINDS.contains(&kind) {
                sprout_auth::require_scope(scopes, sprout_auth::Scope::UsersRead).is_ok()
            } else if MESSAGE_KINDS.contains(&kind) {
                sprout_auth::require_scope(scopes, sprout_auth::Scope::MessagesRead).is_ok()
            } else {
                false
            }
        };

        // UsersRead grants user-data kinds, denies message kinds
        assert!(check(KIND_PROFILE, &users_read));
        assert!(check(KIND_CONTACT_LIST, &users_read));
        assert!(!check(KIND_TEXT_NOTE, &users_read));
        assert!(!check(KIND_LONG_FORM, &users_read));

        // MessagesRead grants message kinds, denies user-data kinds
        assert!(!check(KIND_PROFILE, &messages_read));
        assert!(!check(KIND_CONTACT_LIST, &messages_read));
        assert!(check(KIND_TEXT_NOTE, &messages_read));
        assert!(check(KIND_LONG_FORM, &messages_read));

        // Neither scope grants unknown kinds
        assert!(!check(1059, &users_read));
        assert!(!check(1059, &messages_read));
        assert!(!check(9, &users_read)); // kind:9 stream message
        assert!(!check(9, &messages_read));
    }
}
