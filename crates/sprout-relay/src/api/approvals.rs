//! Approval grant/deny endpoints.
//!
//! Endpoints:
//!   POST /api/approvals/:token/grant — grant a pending approval
//!   POST /api/approvals/:token/deny  — deny a pending approval

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use chrono::Utc;
use serde::Deserialize;

use crate::state::AppState;

use super::{api_error, extract_auth_pubkey, forbidden, internal_error, not_found};

// ── Request body ──────────────────────────────────────────────────────────────

/// Request body for approval grant/deny endpoints.
#[derive(Debug, Deserialize)]
pub struct ApprovalBody {
    /// Optional human-readable note explaining the approval decision.
    pub note: Option<String>,
}

// ── Shared approver-spec enforcement ─────────────────────────────────────────

/// Enforce the approver_spec field against the requesting pubkey.
///
/// Accepted specs:
/// - `""` or `"any"` — any authenticated user may approve.
/// - 64-char lowercase hex string — only that exact pubkey may approve.
///
/// All other formats (role strings such as `@release-manager`, group specs, etc.)
/// are **rejected** (fail-closed). They are not yet implemented; allowing them
/// silently would let any user approve a gate the workflow author intended to restrict.
fn check_approver_spec(
    approver_spec: &str,
    requester_hex: &str,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    let spec = approver_spec.trim();

    // Empty or "any" — anyone may approve.
    if spec.is_empty() || spec == "any" {
        return Ok(());
    }

    // Exact pubkey match (64-char lowercase hex).
    if spec.len() == 64 && spec.chars().all(|c| c.is_ascii_hexdigit()) {
        if requester_hex == spec {
            return Ok(());
        }
        return Err(forbidden(
            "you are not the designated approver for this request",
        ));
    }

    // Role-based specs (e.g., "@release-manager") and any other unrecognised format:
    // fail closed until role resolution is implemented.
    Err(forbidden(&format!(
        "approver spec '{}' is not yet supported — only 'any' or a specific pubkey hex are currently accepted",
        spec
    )))
}

// ── Resume workflow after approval ───────────────────────────────────────────

/// Resume a suspended workflow run after an approval gate has been granted.
///
/// Extracted from `grant_approval` to keep the handler lean and allow independent testing.
async fn resume_workflow_after_approval(
    engine: Arc<sprout_workflow::WorkflowEngine>,
    db: sprout_db::Db,
    run_id: uuid::Uuid,
    workflow_id: uuid::Uuid,
    resume_index: usize,
) {
    let run = match db.get_workflow_run(run_id).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("grant_approval: failed to fetch run {run_id}: {e}");
            return;
        }
    };

    let workflow = match db.get_workflow(workflow_id).await {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("grant_approval: failed to fetch workflow {workflow_id}: {e}");
            return;
        }
    };

    let def: sprout_workflow::WorkflowDef =
        match serde_json::from_value(workflow.definition.clone()) {
            Ok(d) => d,
            Err(e) => {
                tracing::error!("grant_approval: failed to parse workflow definition: {e}");
                if let Err(db_err) = db
                    .update_workflow_run(
                        run_id,
                        sprout_db::workflow::RunStatus::Failed,
                        run.current_step,
                        &run.execution_trace,
                        Some(&format!("definition parse error: {e}")),
                    )
                    .await
                {
                    tracing::error!(
                        "grant_approval: failed to set Failed status for run {run_id}: {db_err}"
                    );
                }
                return;
            }
        };

    // Reconstruct step_outputs from the execution trace so that steps after
    // the resume point can reference {{steps.PREV_STEP.output.X}}.
    let mut initial_outputs: std::collections::HashMap<String, serde_json::Value> =
        std::collections::HashMap::new();
    if let Some(trace_arr) = run.execution_trace.as_array() {
        for entry in trace_arr {
            if let (Some(step_id), Some(output)) = (
                entry.get("step_id").and_then(|v| v.as_str()),
                entry.get("output"),
            ) {
                initial_outputs.insert(step_id.to_string(), output.clone());
            }
        }
    }

    // Restore the original trigger context so that {{trigger.*}} templates
    // in post-approval steps resolve correctly. Fall back to default (empty)
    // for runs created before the trigger_context column was added.
    let trigger_ctx: sprout_workflow::executor::TriggerContext = run
        .trigger_context
        .as_ref()
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    match sprout_workflow::executor::execute_from_step(
        &engine,
        run_id,
        &def,
        &trigger_ctx,
        resume_index,
        Some(initial_outputs),
    )
    .await
    {
        Ok(result) if result.approval_token.is_none() => {
            let mut full_trace = run.execution_trace.as_array().cloned().unwrap_or_default();
            full_trace.extend(result.trace);
            let trace_json = serde_json::Value::Array(full_trace);
            if let Err(e) = db
                .update_workflow_run(
                    run_id,
                    sprout_db::workflow::RunStatus::Completed,
                    result.step_index as i32,
                    &trace_json,
                    None,
                )
                .await
            {
                tracing::error!(
                    "grant_approval: failed to set Completed status for run {run_id}: {e}"
                );
            }
        }
        Ok(result) => {
            // Chained approval gates are not yet fully implemented (see WF-08).
            // Mark the run as Failed rather than silently creating a new approval
            // record that nothing will ever resolve.
            let mut full_trace = run.execution_trace.as_array().cloned().unwrap_or_default();
            full_trace.extend(result.trace);
            let trace_json = serde_json::Value::Array(full_trace);
            if let Err(e) = db
                .update_workflow_run(
                    run_id,
                    sprout_db::workflow::RunStatus::Failed,
                    result.step_index as i32,
                    &trace_json,
                    Some("approval gates not yet fully implemented — see WF-08"),
                )
                .await
            {
                tracing::error!(
                    "grant_approval: failed to set Failed status for run {run_id}: {e}"
                );
            }
        }
        Err(e) => {
            tracing::error!(
                "grant_approval: resume of run {run_id} failed at step >= {resume_index}: {e}"
            );
            // Note: partial trace from steps executed after resume is lost on error.
            // The executor error type does not carry partial results.
            // TODO(WF-08): Consider returning partial trace in WorkflowError.
            if let Err(db_err) = db
                .update_workflow_run(
                    run_id,
                    sprout_db::workflow::RunStatus::Failed,
                    resume_index as i32,
                    &run.execution_trace, // preserve existing trace
                    Some(&format!("execution failed after approval resume: {e}")),
                )
                .await
            {
                tracing::error!(
                    "grant_approval: failed to set Failed status for run {run_id}: {db_err}"
                );
            }
        }
    }
}

// ── POST /api/approvals/:token/grant ─────────────────────────────────────────

/// Grant a pending approval and resume the suspended workflow run.
///
/// Uses `AND status = 'pending'` in the DB update to prevent TOCTOU races.
pub async fn grant_approval(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(token): Path<String>,
    body: Option<Json<ApprovalBody>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (pubkey, pubkey_bytes) = extract_auth_pubkey(&headers, &state).await?;

    let approval = state
        .db
        .get_approval(&token)
        .await
        .map_err(|_| not_found("approval not found"))?;

    if approval.status != sprout_db::workflow::ApprovalStatus::Pending {
        return Err(api_error(
            StatusCode::CONFLICT,
            &format!("approval already {}", approval.status),
        ));
    }

    if Utc::now() > approval.expires_at {
        return Err(api_error(StatusCode::GONE, "approval token has expired"));
    }

    check_approver_spec(&approval.approver_spec, &pubkey.to_hex())?;

    let note = body.as_ref().and_then(|b| b.note.as_deref());

    let updated = state
        .db
        .update_approval(
            &token,
            sprout_db::workflow::ApprovalStatus::Granted,
            Some(&pubkey_bytes),
            note,
        )
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    if !updated {
        return Err(api_error(StatusCode::CONFLICT, "approval already acted on"));
    }

    // Resume workflow execution from the step after the approval gate.
    let run_id = approval.run_id;
    let workflow_id = approval.workflow_id;
    let resume_index = approval.step_index as usize + 1;

    let engine = Arc::clone(&state.workflow_engine);
    let db = state.db.clone();

    tokio::spawn(async move {
        resume_workflow_after_approval(engine, db, run_id, workflow_id, resume_index).await;
    });

    Ok(Json(serde_json::json!({
        "token": token,
        "status": "granted",
        "run_id": approval.run_id.to_string(),
        "workflow_id": approval.workflow_id.to_string(),
    })))
}

// ── POST /api/approvals/:token/deny ──────────────────────────────────────────

/// Deny a pending approval and cancel the suspended workflow run.
///
/// Uses `AND status = 'pending'` in the DB update to prevent TOCTOU races.
pub async fn deny_approval(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(token): Path<String>,
    body: Option<Json<ApprovalBody>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (pubkey, pubkey_bytes) = extract_auth_pubkey(&headers, &state).await?;

    let approval = state
        .db
        .get_approval(&token)
        .await
        .map_err(|_| not_found("approval not found"))?;

    if approval.status != sprout_db::workflow::ApprovalStatus::Pending {
        return Err(api_error(
            StatusCode::CONFLICT,
            &format!("approval already {}", approval.status),
        ));
    }

    if Utc::now() > approval.expires_at {
        return Err(api_error(StatusCode::GONE, "approval token has expired"));
    }

    check_approver_spec(&approval.approver_spec, &pubkey.to_hex())?;

    let note = body.as_ref().and_then(|b| b.note.as_deref());

    let updated = state
        .db
        .update_approval(
            &token,
            sprout_db::workflow::ApprovalStatus::Denied,
            Some(&pubkey_bytes),
            note,
        )
        .await
        .map_err(|e| internal_error(&format!("db error: {e}")))?;

    if !updated {
        return Err(api_error(StatusCode::CONFLICT, "approval already acted on"));
    }

    // Mark the workflow run as Cancelled.
    let run_id = approval.run_id;
    let pubkey_for_msg = pubkey.to_hex();
    let db = state.db.clone();
    tokio::spawn(async move {
        let (current_step, trace) = match db.get_workflow_run(run_id).await {
            Ok(r) => (r.current_step, r.execution_trace),
            Err(e) => {
                tracing::error!("deny_approval: failed to fetch run {run_id}: {e}");
                (0, serde_json::Value::Array(vec![]))
            }
        };
        let cancel_msg = format!("workflow cancelled: approval denied by {pubkey_for_msg}");
        if let Err(e) = db
            .update_workflow_run(
                run_id,
                sprout_db::workflow::RunStatus::Cancelled,
                current_step,
                &trace,
                Some(&cancel_msg),
            )
            .await
        {
            tracing::error!("deny_approval: failed to set Cancelled status for run {run_id}: {e}");
        }
    });

    Ok(Json(serde_json::json!({
        "token": token,
        "status": "denied",
        "run_id": approval.run_id.to_string(),
        "workflow_id": approval.workflow_id.to_string(),
    })))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // A valid 64-char lowercase hex pubkey for testing.
    const ALICE_HEX: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const BOB_HEX: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    // ── Empty / "any" spec ────────────────────────────────────────────────────

    #[test]
    fn empty_spec_allows_any_requester() {
        assert!(check_approver_spec("", ALICE_HEX).is_ok());
        assert!(check_approver_spec("", BOB_HEX).is_ok());
    }

    #[test]
    fn any_spec_allows_any_requester() {
        assert!(check_approver_spec("any", ALICE_HEX).is_ok());
        assert!(check_approver_spec("any", BOB_HEX).is_ok());
    }

    #[test]
    fn any_spec_with_surrounding_whitespace_allows_any_requester() {
        assert!(check_approver_spec("  any  ", ALICE_HEX).is_ok());
    }

    // ── Exact pubkey spec ─────────────────────────────────────────────────────

    #[test]
    fn exact_pubkey_spec_allows_matching_requester() {
        assert!(check_approver_spec(ALICE_HEX, ALICE_HEX).is_ok());
    }

    #[test]
    fn exact_pubkey_spec_rejects_non_matching_requester() {
        let result = check_approver_spec(ALICE_HEX, BOB_HEX);
        assert!(result.is_err());
        let (status, _) = result.unwrap_err();
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[test]
    fn exact_pubkey_spec_rejects_empty_requester() {
        let result = check_approver_spec(ALICE_HEX, "");
        assert!(result.is_err());
        let (status, _) = result.unwrap_err();
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    // ── Role-based / unrecognised spec ────────────────────────────────────────

    #[test]
    fn role_spec_is_rejected_fail_closed() {
        // Role strings are not yet implemented — must fail closed regardless of requester.
        let result = check_approver_spec("@release-manager", ALICE_HEX);
        assert!(result.is_err());
        let (status, _) = result.unwrap_err();
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[test]
    fn group_spec_is_rejected_fail_closed() {
        let result = check_approver_spec("group:security-team", BOB_HEX);
        assert!(result.is_err());
        let (status, _) = result.unwrap_err();
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[test]
    fn short_hex_spec_is_rejected_as_unrecognised() {
        // A hex string shorter than 64 chars is not a valid pubkey spec — fail closed.
        let result = check_approver_spec("deadbeef", ALICE_HEX);
        assert!(result.is_err());
        let (status, _) = result.unwrap_err();
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[test]
    fn uppercase_hex_spec_is_rejected_as_unrecognised() {
        // Spec must be lowercase hex — uppercase fails the `is_ascii_hexdigit` path length check
        // (it IS hex digits, but the spec says 64-char lowercase; uppercase passes hexdigit but
        // won't match a lowercase requester_hex, so it falls through to the role branch).
        let upper = ALICE_HEX.to_uppercase();
        let result = check_approver_spec(&upper, &upper.to_lowercase());
        // Either forbidden (no match) or forbidden (unrecognised spec) — both are errors.
        assert!(result.is_err());
    }
}
