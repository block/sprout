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

/// Global event kinds that require `UsersRead` scope.
pub(crate) const GLOBAL_USER_DATA_KINDS: [u32; 2] = [KIND_PROFILE, KIND_CONTACT_LIST];
/// Global event kinds that require `MessagesRead` scope.
pub(crate) const GLOBAL_MESSAGE_KINDS: [u32; 2] = [KIND_TEXT_NOTE, KIND_LONG_FORM];

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
        // Channel-restricted tokens must not access global events — they are
        // scoped to specific channels and global events fall outside that scope.
        if ctx.channel_ids.is_some() {
            return Err(not_found("event not found"));
        }

        // Global event — scope-aware allowlist.
        let event_kind = event_kind_u32(&stored_event.event);

        let scope_ok = if GLOBAL_USER_DATA_KINDS.contains(&event_kind) {
            sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::UsersRead).is_ok()
        } else if GLOBAL_MESSAGE_KINDS.contains(&event_kind) {
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

    use super::{GLOBAL_MESSAGE_KINDS, GLOBAL_USER_DATA_KINDS};

    /// Reproduce the scope-check routing logic from `get_event` so we can
    /// unit-test it without standing up a full HTTP server.
    fn scope_check_for_global_event(event_kind: u32, scopes: &[sprout_auth::Scope]) -> bool {
        if GLOBAL_USER_DATA_KINDS.contains(&event_kind) {
            sprout_auth::require_scope(scopes, sprout_auth::Scope::UsersRead).is_ok()
        } else if GLOBAL_MESSAGE_KINDS.contains(&event_kind) {
            sprout_auth::require_scope(scopes, sprout_auth::Scope::MessagesRead).is_ok()
        } else {
            false
        }
    }

    // ── Positive cases: correct scope grants access ──────────────────────

    #[test]
    fn kind0_profile_allowed_with_users_read() {
        assert!(scope_check_for_global_event(
            KIND_PROFILE,
            &[sprout_auth::Scope::UsersRead],
        ));
    }

    #[test]
    fn kind3_contact_list_allowed_with_users_read() {
        assert!(scope_check_for_global_event(
            KIND_CONTACT_LIST,
            &[sprout_auth::Scope::UsersRead],
        ));
    }

    #[test]
    fn kind1_text_note_allowed_with_messages_read() {
        assert!(scope_check_for_global_event(
            KIND_TEXT_NOTE,
            &[sprout_auth::Scope::MessagesRead],
        ));
    }

    #[test]
    fn kind30023_long_form_allowed_with_messages_read() {
        assert!(scope_check_for_global_event(
            KIND_LONG_FORM,
            &[sprout_auth::Scope::MessagesRead],
        ));
    }

    // ── Negative cases: wrong scope is denied ────────────────────────────

    #[test]
    fn kind0_profile_denied_with_only_messages_read() {
        assert!(!scope_check_for_global_event(
            KIND_PROFILE,
            &[sprout_auth::Scope::MessagesRead],
        ));
    }

    #[test]
    fn kind3_contact_list_denied_with_only_messages_read() {
        assert!(!scope_check_for_global_event(
            KIND_CONTACT_LIST,
            &[sprout_auth::Scope::MessagesRead],
        ));
    }

    #[test]
    fn kind1_text_note_denied_with_only_users_read() {
        assert!(!scope_check_for_global_event(
            KIND_TEXT_NOTE,
            &[sprout_auth::Scope::UsersRead],
        ));
    }

    #[test]
    fn kind30023_long_form_denied_with_only_users_read() {
        assert!(!scope_check_for_global_event(
            KIND_LONG_FORM,
            &[sprout_auth::Scope::UsersRead],
        ));
    }

    // ── Closed-default: unknown kinds are always denied ──────────────────

    #[test]
    fn unknown_kind_denied_even_with_all_scopes() {
        let all_scopes = vec![
            sprout_auth::Scope::UsersRead,
            sprout_auth::Scope::MessagesRead,
        ];
        // kind:1059 (gift wrap), kind:5 (delete), kind:7 (reaction), kind:9 (stream msg)
        for kind in [1059, 5, 7, 9, 9002, 40003, 45001] {
            assert!(
                !scope_check_for_global_event(kind, &all_scopes),
                "kind:{kind} must be denied by the closed-default allowlist"
            );
        }
    }

    // ── Edge case: empty scopes deny everything ──────────────────────────

    #[test]
    fn empty_scopes_deny_all_allowed_kinds() {
        let no_scopes: &[sprout_auth::Scope] = &[];
        assert!(!scope_check_for_global_event(KIND_PROFILE, no_scopes));
        assert!(!scope_check_for_global_event(KIND_CONTACT_LIST, no_scopes));
        assert!(!scope_check_for_global_event(KIND_TEXT_NOTE, no_scopes));
        assert!(!scope_check_for_global_event(KIND_LONG_FORM, no_scopes));
    }

    // ── Both scopes together grant access to all allowed kinds ───────────

    #[test]
    fn both_scopes_grant_all_allowed_kinds() {
        let both = vec![
            sprout_auth::Scope::UsersRead,
            sprout_auth::Scope::MessagesRead,
        ];
        assert!(scope_check_for_global_event(KIND_PROFILE, &both));
        assert!(scope_check_for_global_event(KIND_CONTACT_LIST, &both));
        assert!(scope_check_for_global_event(KIND_TEXT_NOTE, &both));
        assert!(scope_check_for_global_event(KIND_LONG_FORM, &both));
    }
}
