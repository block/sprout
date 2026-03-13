//! Workflow CRUD endpoints and execution triggers.
//!
//! Endpoints:
//!   GET    /api/channels/:channel_id/workflows — list workflows in a channel
//!   POST   /api/channels/:channel_id/workflows — create workflow
//!   GET    /api/workflows/:id                  — get workflow
//!   PUT    /api/workflows/:id                  — update workflow
//!   DELETE /api/workflows/:id                  — delete workflow
//!   GET    /api/workflows/:id/runs             — list workflow runs
//!   POST   /api/workflows/:id/trigger          — manually trigger workflow
//!   POST   /api/workflows/:id/webhook          — webhook trigger (no auth)

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use serde::Deserialize;

use crate::state::AppState;

use super::workflow_helpers::{
    definition_hash, ensure_webhook_secret, run_record_to_json, spawn_workflow_execution,
    validate_webhook_urls, workflow_record_to_json,
};
use super::{
    api_error, check_channel_access, check_token_channel_access, extract_auth_context, forbidden,
    internal_error, not_found, scope_error,
};

// ── GET /api/channels/:channel_id/workflows ───────────────────────────────────

/// List all workflows in a channel.
pub async fn list_channel_workflows(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel_id_str): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::ChannelsRead)
        .map_err(scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();

    let channel_id = uuid::Uuid::parse_str(&channel_id_str)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid channel UUID"))?;

    check_token_channel_access(&ctx, &channel_id)?;
    check_channel_access(&state, channel_id, &pubkey_bytes).await?;

    let workflows = state
        .db
        .list_channel_workflows(channel_id)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    let result: Vec<serde_json::Value> = workflows.iter().map(workflow_record_to_json).collect();
    Ok(Json(serde_json::json!(result)))
}

// ── POST /api/channels/:channel_id/workflows ──────────────────────────────────

/// Request body for creating a new workflow.
#[derive(Debug, Deserialize)]
pub struct CreateWorkflowBody {
    /// YAML workflow definition string.
    pub yaml_definition: String,
}

/// Create a new workflow in a channel.
///
/// Parses and validates the YAML definition, generates a webhook secret if needed,
/// and stores the workflow. Returns the webhook secret in the response (only time it's visible).
pub async fn create_workflow(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(channel_id_str): Path<String>,
    Json(body): Json<CreateWorkflowBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::ChannelsWrite)
        .map_err(scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();

    let channel_id = uuid::Uuid::parse_str(&channel_id_str)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid channel UUID"))?;

    check_token_channel_access(&ctx, &channel_id)?;
    check_channel_access(&state, channel_id, &pubkey_bytes).await?;

    let (def, definition_json_str) =
        sprout_workflow::WorkflowEngine::parse_yaml(&body.yaml_definition).map_err(|e| {
            api_error(
                StatusCode::BAD_REQUEST,
                &format!("invalid workflow YAML: {e}"),
            )
        })?;

    validate_webhook_urls(&def)
        .await
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, &e))?;

    let mut definition_json: serde_json::Value = serde_json::from_str(&definition_json_str)
        .map_err(|e| internal_error(&format!("json parse error: {e}")))?;

    // I5: Generate a webhook secret if this workflow uses a Webhook trigger.
    let webhook_secret = if matches!(def.trigger, sprout_workflow::TriggerDef::Webhook) {
        Some(ensure_webhook_secret(&mut definition_json, None))
    } else {
        None
    };

    // C5: Compute SHA-256 hash AFTER secret injection so hash matches stored definition.
    let definition_json_final = serde_json::to_string(&definition_json)
        .map_err(|e| internal_error(&format!("json serialize error: {e}")))?;
    let hash = definition_hash(&definition_json_final);

    let workflow_id = state
        .db
        .create_workflow(
            Some(channel_id),
            &pubkey_bytes,
            &def.name,
            &definition_json_final,
            &hash,
        )
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    let workflow = state
        .db
        .get_workflow(workflow_id)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    let mut resp = workflow_record_to_json(&workflow);
    // Return the webhook secret in the creation response (only time it's visible).
    if let Some(secret) = &webhook_secret {
        resp["webhook_secret"] = serde_json::Value::String(secret.clone());
    }
    Ok(Json(resp))
}

// ── GET /api/workflows/:id ────────────────────────────────────────────────────

/// Get a single workflow by ID.
pub async fn get_workflow(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id_str): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::ChannelsRead)
        .map_err(scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();

    let id = uuid::Uuid::parse_str(&id_str)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid workflow UUID"))?;

    let workflow = state
        .db
        .get_workflow(id)
        .await
        .map_err(|_| not_found("workflow not found"))?;

    if let Some(channel_id) = workflow.channel_id {
        check_token_channel_access(&ctx, &channel_id)?;
        check_channel_access(&state, channel_id, &pubkey_bytes).await?;
    } else if workflow.owner_pubkey != pubkey_bytes {
        return Err(forbidden("not authorized to access this workflow"));
    }

    Ok(Json(workflow_record_to_json(&workflow)))
}

// ── PUT /api/workflows/:id ────────────────────────────────────────────────────

/// Request body for updating an existing workflow.
#[derive(Debug, Deserialize)]
pub struct UpdateWorkflowBody {
    /// Replacement YAML workflow definition string.
    pub yaml_definition: String,
}

/// Update an existing workflow's definition.
///
/// Preserves the webhook secret across updates if the trigger type remains Webhook.
/// If the trigger changes TO Webhook, a new secret is generated and returned.
pub async fn update_workflow(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id_str): Path<String>,
    Json(body): Json<UpdateWorkflowBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::ChannelsWrite)
        .map_err(scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();

    let id = uuid::Uuid::parse_str(&id_str)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid workflow UUID"))?;

    let existing = state
        .db
        .get_workflow(id)
        .await
        .map_err(|_| not_found("workflow not found"))?;

    if let Some(channel_id) = existing.channel_id {
        check_token_channel_access(&ctx, &channel_id)?;
        check_channel_access(&state, channel_id, &pubkey_bytes).await?;
    } else if existing.owner_pubkey != pubkey_bytes {
        return Err(forbidden("not authorized to access this workflow"));
    }

    let (def, definition_json_str) =
        sprout_workflow::WorkflowEngine::parse_yaml(&body.yaml_definition).map_err(|e| {
            api_error(
                StatusCode::BAD_REQUEST,
                &format!("invalid workflow YAML: {e}"),
            )
        })?;

    validate_webhook_urls(&def)
        .await
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, &e))?;

    let mut definition_json: serde_json::Value = serde_json::from_str(&definition_json_str)
        .map_err(|e| internal_error(&format!("json parse error: {e}")))?;

    // N3: Preserve (or regenerate) the webhook secret across updates.
    let is_webhook_now = matches!(def.trigger, sprout_workflow::TriggerDef::Webhook);
    let new_secret: Option<String> = if is_webhook_now {
        let had_existing = crate::webhook_secret::extract_secret(&existing.definition).is_some();
        let secret = ensure_webhook_secret(&mut definition_json, Some(&existing.definition));
        // Only return the secret in the response if it was newly generated.
        if had_existing {
            None
        } else {
            Some(secret)
        }
    } else {
        None
    };

    let definition_json_str_final = serde_json::to_string(&definition_json)
        .map_err(|e| internal_error(&format!("json serialize error: {e}")))?;
    let hash = definition_hash(&definition_json_str_final);

    state
        .db
        .update_workflow(id, &def.name, &definition_json_str_final, &hash)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    let updated = state
        .db
        .get_workflow(id)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    let mut resp = workflow_record_to_json(&updated);
    // M4: If a new webhook secret was generated during this update, include it in the
    // response so the caller can store it. It will not be retrievable again.
    if let Some(secret) = new_secret {
        resp["webhook_secret"] = serde_json::Value::String(secret);
    }
    Ok(Json(resp))
}

// ── DELETE /api/workflows/:id ─────────────────────────────────────────────────

/// Delete a workflow. Only the owner or a channel member may delete.
pub async fn delete_workflow(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id_str): Path<String>,
) -> Result<axum::response::Response, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::ChannelsWrite)
        .map_err(scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();

    let id = uuid::Uuid::parse_str(&id_str)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid workflow UUID"))?;

    let workflow = state
        .db
        .get_workflow(id)
        .await
        .map_err(|_| not_found("workflow not found"))?;

    if workflow.owner_pubkey != pubkey_bytes {
        if let Some(channel_id) = workflow.channel_id {
            check_token_channel_access(&ctx, &channel_id)?;
            check_channel_access(&state, channel_id, &pubkey_bytes)
                .await
                .map_err(|_| forbidden("not authorized to delete this workflow"))?;
        } else {
            return Err(forbidden("not authorized to delete this workflow"));
        }
    }

    state
        .db
        .delete_workflow(id)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    use axum::response::IntoResponse;
    Ok(StatusCode::NO_CONTENT.into_response())
}

// ── GET /api/workflows/:id/runs ───────────────────────────────────────────────

/// Query parameters for the workflow runs list endpoint.
#[derive(Debug, Deserialize)]
pub struct RunsParams {
    /// Maximum number of runs to return. Defaults to 20.
    pub limit: Option<u32>,
}

/// List recent runs for a workflow.
pub async fn list_workflow_runs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id_str): Path<String>,
    Query(params): Query<RunsParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::ChannelsRead)
        .map_err(scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();

    let id = uuid::Uuid::parse_str(&id_str)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid workflow UUID"))?;

    let workflow = state
        .db
        .get_workflow(id)
        .await
        .map_err(|_| not_found("workflow not found"))?;

    if let Some(channel_id) = workflow.channel_id {
        check_token_channel_access(&ctx, &channel_id)?;
        check_channel_access(&state, channel_id, &pubkey_bytes).await?;
    }

    let limit = params.limit.unwrap_or(20).min(100) as i64;
    let runs = state
        .db
        .list_workflow_runs(id, limit)
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    let result: Vec<serde_json::Value> = runs.iter().map(run_record_to_json).collect();
    Ok(Json(serde_json::json!(result)))
}

// ── POST /api/workflows/:id/trigger ──────────────────────────────────────────

/// Manually trigger a workflow. Returns 202 Accepted; execution is async.
pub async fn trigger_workflow(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id_str): Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let ctx = extract_auth_context(&headers, &state).await?;
    sprout_auth::require_scope(&ctx.scopes, sprout_auth::Scope::ChannelsWrite)
        .map_err(scope_error)?;
    let pubkey_bytes = ctx.pubkey_bytes.clone();

    let id = uuid::Uuid::parse_str(&id_str)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid workflow UUID"))?;

    let workflow = state
        .db
        .get_workflow(id)
        .await
        .map_err(|_| not_found("workflow not found"))?;

    if let Some(channel_id) = workflow.channel_id {
        check_token_channel_access(&ctx, &channel_id)?;
        check_channel_access(&state, channel_id, &pubkey_bytes).await?;
    }

    let trigger_ctx = sprout_workflow::executor::TriggerContext::default();
    let trigger_ctx_json = serde_json::to_value(&trigger_ctx).ok();

    let run_id = state
        .db
        .create_workflow_run(id, None, trigger_ctx_json.as_ref())
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    spawn_workflow_execution(
        Arc::clone(&state.workflow_engine),
        state.db.clone(),
        run_id,
        workflow.definition.clone(),
        trigger_ctx,
    );

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "run_id": run_id.to_string(),
            "workflow_id": id.to_string(),
            "status": "pending",
        })),
    ))
}

// ── POST /api/workflows/:id/webhook ──────────────────────────────────────────

/// Query parameters for the webhook trigger endpoint.
#[derive(Debug, Deserialize)]
pub struct WebhookQuery {
    /// Webhook secret for authentication. Prefer the `X-Webhook-Secret` header instead.
    pub secret: Option<String>,
}

/// Webhook trigger endpoint. No user auth — the webhook secret authenticates the caller.
///
/// Prefers `X-Webhook-Secret` header over `?secret=` query param (headers aren't logged
/// by most proxies). Returns 202 Accepted; execution is async.
pub async fn workflow_webhook(
    State(state): State<Arc<AppState>>,
    Path(id_str): Path<String>,
    Query(query): Query<WebhookQuery>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let id = uuid::Uuid::parse_str(&id_str)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid workflow UUID"))?;

    let workflow = state
        .db
        .get_workflow(id)
        .await
        .map_err(|_| not_found("workflow not found"))?;

    let def: sprout_workflow::WorkflowDef = serde_json::from_value(workflow.definition.clone())
        .map_err(|e| internal_error(&format!("corrupt workflow definition: {e}")))?;

    if !matches!(def.trigger, sprout_workflow::TriggerDef::Webhook) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "workflow does not have a webhook trigger",
        ));
    }

    // I5: Verify webhook secret. Prefer header (not logged by proxies); fall back to query param.
    let stored_secret = crate::webhook_secret::extract_secret(&workflow.definition);
    let provided_secret = headers
        .get("x-webhook-secret")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| query.secret.clone())
        .unwrap_or_default();

    match &stored_secret {
        Some(secret) => {
            if !crate::webhook_secret::verify_secret(&provided_secret, secret) {
                tracing::warn!("webhook: invalid secret for workflow {id}");
                return Err(api_error(StatusCode::UNAUTHORIZED, "authentication failed"));
            }
        }
        None => {
            return Err(api_error(
                StatusCode::UNAUTHORIZED,
                "webhook secret required but not configured — re-save the workflow to generate one",
            ));
        }
    }

    // Parse optional JSON body as trigger context. Return 400 if the body is
    // non-empty but not valid JSON so callers get actionable error feedback.
    let body_json: Option<serde_json::Value> =
        if body.is_empty() {
            None
        } else {
            Some(serde_json::from_slice(&body).map_err(|e| {
                api_error(StatusCode::BAD_REQUEST, &format!("invalid JSON body: {e}"))
            })?)
        };

    // Build trigger context from webhook body fields before creating the run so
    // we can persist it immediately (needed for post-approval resume).
    let mut trigger_ctx = sprout_workflow::executor::TriggerContext::default();
    if let Some(serde_json::Value::Object(ref map)) = body_json {
        for (k, v) in map {
            let val_str = match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            trigger_ctx.webhook_fields.insert(k.clone(), val_str);
        }
    }
    let trigger_ctx_json = serde_json::to_value(&trigger_ctx).ok();

    let run_id = state
        .db
        .create_workflow_run(id, None, trigger_ctx_json.as_ref())
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    spawn_workflow_execution(
        Arc::clone(&state.workflow_engine),
        state.db.clone(),
        run_id,
        workflow.definition.clone(),
        trigger_ctx,
    );

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "run_id": run_id.to_string(),
            "workflow_id": id.to_string(),
            "status": "pending",
        })),
    ))
}
