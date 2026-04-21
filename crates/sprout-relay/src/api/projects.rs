//! Project REST API.
//!
//! Endpoints:
//!   GET /api/projects                        — list all projects
//!   GET /api/projects/:project_id            — get a single project
//!   GET /api/projects/:project_id/channels   — list channels in a project

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};

use crate::state::AppState;

use super::{api_error, extract_auth_context, internal_error, not_found, scope_error};

// ── GET /api/projects ────────────────────────────────────────────────────────

/// List all projects accessible to the authenticated user.
pub async fn list_projects_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::ChannelsRead)
        .map_err(scope_error)?;

    let records = state
        .db
        .list_projects(None)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    let projects: Vec<serde_json::Value> =
        records.into_iter().map(|r| project_to_json(&r)).collect();

    Ok(Json(serde_json::json!({ "projects": projects })))
}

// ── GET /api/projects/:project_id ────────────────────────────────────────────

/// Get a single project by ID.
pub async fn get_project_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(project_id_str): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::ChannelsRead)
        .map_err(scope_error)?;

    let project_id = uuid::Uuid::parse_str(&project_id_str)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid project UUID"))?;

    let record = state
        .db
        .get_project(project_id)
        .await
        .map_err(|e| match e {
            sprout_db::DbError::NotFound(_) => not_found("project not found"),
            other => internal_error(&format!("db error: {other}")),
        })?;

    Ok(Json(project_to_json(&record)))
}

// ── GET /api/projects/:project_id/channels ───────────────────────────────────

/// List channels belonging to a project.
pub async fn list_project_channels_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(project_id_str): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::ChannelsRead)
        .map_err(scope_error)?;

    let project_id = uuid::Uuid::parse_str(&project_id_str)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid project UUID"))?;

    let records = state
        .db
        .list_project_channels(project_id)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    let channels: Vec<serde_json::Value> = records
        .into_iter()
        .map(|ch| {
            serde_json::json!({
                "id": ch.id,
                "name": ch.name,
                "channel_type": ch.channel_type,
                "visibility": ch.visibility,
                "description": ch.description,
                "created_by": hex::encode(&ch.created_by),
                "created_at": ch.created_at.to_rfc3339(),
                "updated_at": ch.updated_at.to_rfc3339(),
                "archived_at": ch.archived_at.map(|t| t.to_rfc3339()),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "channels": channels })))
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn project_to_json(r: &sprout_db::project::ProjectRecord) -> serde_json::Value {
    serde_json::json!({
        "id": r.id,
        "name": r.name,
        "description": r.description,
        "prompt": r.prompt,
        "icon": r.icon,
        "color": r.color,
        "environment": r.environment,
        "repo_urls": r.repo_urls,
        "created_by": hex::encode(&r.created_by),
        "created_at": r.created_at.to_rfc3339(),
        "updated_at": r.updated_at.to_rfc3339(),
        "archived_at": r.archived_at.map(|t| t.to_rfc3339()),
    })
}
