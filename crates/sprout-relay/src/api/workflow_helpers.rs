//! Shared helpers for workflow endpoints: serialization, SSRF validation, and async execution.

use std::sync::Arc;

use nostr::util::hex as nostr_hex;
use sha2::{Digest, Sha256};

// ── Serialization ─────────────────────────────────────────────────────────────

/// Strip `_webhook_secret` from a workflow definition before returning it to clients.
///
/// The secret is an internal field used only for webhook authentication; it must never
/// be exposed via GET responses.
fn sanitize_definition(def: &serde_json::Value) -> serde_json::Value {
    crate::webhook_secret::strip_secret(def)
}

/// Serialize a [`WorkflowRecord`] to a JSON value safe for API responses.
pub(crate) fn workflow_record_to_json(
    w: &sprout_db::workflow::WorkflowRecord,
) -> serde_json::Value {
    serde_json::json!({
        "id": w.id.to_string(),
        "name": w.name,
        "owner_pubkey": nostr_hex::encode(&w.owner_pubkey),
        "channel_id": w.channel_id.map(|id| id.to_string()),
        "definition": sanitize_definition(&w.definition),
        "status": w.status,
        "created_at": w.created_at.timestamp(),
        "updated_at": w.updated_at.timestamp(),
    })
}

/// Serialize a [`WorkflowRunRecord`] to a JSON value.
pub(crate) fn run_record_to_json(r: &sprout_db::workflow::WorkflowRunRecord) -> serde_json::Value {
    serde_json::json!({
        "id": r.id.to_string(),
        "workflow_id": r.workflow_id.to_string(),
        "status": r.status,
        "current_step": r.current_step,
        "execution_trace": r.execution_trace,
        "started_at": r.started_at.map(|t| t.timestamp()),
        "completed_at": r.completed_at.map(|t| t.timestamp()),
        "error_message": r.error_message,
        "created_at": r.created_at.timestamp(),
    })
}

/// Serialize an [`ApprovalRecord`] to a JSON value.
pub(crate) fn approval_record_to_json(
    a: &sprout_db::workflow::ApprovalRecord,
) -> serde_json::Value {
    serde_json::json!({
        "token": a.token,
        "workflow_id": a.workflow_id.to_string(),
        "run_id": a.run_id.to_string(),
        "step_id": a.step_id,
        "step_index": a.step_index,
        "approver_spec": a.approver_spec,
        "status": a.status.to_string(),
        "approver_pubkey": a.approver_pubkey.as_ref().map(nostr_hex::encode),
        "note": a.note,
        "expires_at": a.expires_at.to_rfc3339(),
        "created_at": a.created_at.timestamp(),
    })
}

// ── SSRF prevention ───────────────────────────────────────────────────────────

/// Validate all CallWebhook URLs in a workflow definition.
///
/// Rejects non-http(s) schemes, known metadata endpoints, literal private IPs,
/// and hostnames that resolve to private/loopback/link-local addresses (SSRF via DNS).
///
/// Uses `tokio::net::lookup_host` for async DNS resolution to avoid blocking the executor.
pub(crate) async fn validate_webhook_urls(
    def: &sprout_workflow::WorkflowDef,
) -> Result<(), String> {
    for step in &def.steps {
        if let sprout_workflow::ActionDef::CallWebhook { url, .. } = &step.action {
            let parsed = url::Url::parse(url)
                .map_err(|e| format!("invalid webhook URL in step '{}': {e}", step.id))?;

            match parsed.scheme() {
                "http" | "https" => {}
                s => {
                    return Err(format!(
                        "webhook URL scheme '{}' not allowed in step '{}' (only http/https)",
                        s, step.id
                    ))
                }
            }

            if let Some(host) = parsed.host_str() {
                // Block loopback hostnames and cloud metadata endpoints.
                if matches!(host, "localhost" | "127.0.0.1" | "::1" | "[::1]") {
                    return Err(format!(
                        "webhook URL in step '{}' targets loopback address",
                        step.id
                    ));
                }
                if matches!(host, "169.254.169.254" | "metadata.google.internal") {
                    return Err(format!(
                        "webhook URL in step '{}' targets cloud metadata endpoint",
                        step.id
                    ));
                }

                if let Ok(ip) = host.parse::<std::net::IpAddr>() {
                    // Literal IP — check directly.
                    if sprout_core::network::is_private_ip(&ip) {
                        return Err(format!(
                            "webhook URL in step '{}' targets private/internal network",
                            step.id
                        ));
                    }
                } else {
                    // Hostname — resolve DNS asynchronously and check all resolved IPs (SSRF via DNS).
                    match tokio::net::lookup_host(format!("{}:80", host)).await {
                        Ok(addrs) => {
                            for addr in addrs {
                                if sprout_core::network::is_private_ip(&addr.ip()) {
                                    return Err(format!(
                                        "webhook URL in step '{}' resolves to private/internal address",
                                        step.id
                                    ));
                                }
                            }
                        }
                        Err(e) => {
                            // DNS resolution failed — reject to be safe (fail-closed).
                            tracing::warn!(
                                step_id = %step.id,
                                host = %host,
                                "webhook URL hostname DNS resolution failed: {e}"
                            );
                            return Err(format!(
                                "webhook URL in step '{}' hostname could not be resolved",
                                step.id
                            ));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

// ── Webhook secret helpers ────────────────────────────────────────────────────

/// Inject or preserve webhook secret in a definition JSON value, returning the secret used.
///
/// If the existing definition already has a secret, it is preserved and returned.
/// Otherwise a new secret is generated, injected, and returned.
pub(crate) fn ensure_webhook_secret(
    definition_json: &mut serde_json::Value,
    existing_definition: Option<&serde_json::Value>,
) -> String {
    if let Some(existing) = existing_definition {
        if let Some(s) = crate::webhook_secret::extract_secret(existing) {
            crate::webhook_secret::inject_secret(definition_json, &s);
            return s;
        }
    }
    let secret = crate::webhook_secret::generate_webhook_secret();
    crate::webhook_secret::inject_secret(definition_json, &secret);
    secret
}

/// Compute SHA-256 hash of a JSON string for storage.
pub(crate) fn definition_hash(json_str: &str) -> Vec<u8> {
    Sha256::digest(json_str.as_bytes()).to_vec()
}

// ── Async workflow execution ──────────────────────────────────────────────────

/// Spawn an async workflow execution task.
///
/// Handles the full lifecycle: Pending → (executor sets Running) → Completed / Failed.
/// Uses [`WorkflowEngine::finalize_run`] for the result→DB-status mapping.
/// Used by trigger and webhook paths to avoid code duplication.
pub(crate) fn spawn_workflow_execution(
    engine: Arc<sprout_workflow::WorkflowEngine>,
    db: sprout_db::Db,
    run_id: uuid::Uuid,
    workflow_def_value: serde_json::Value,
    trigger_ctx: sprout_workflow::executor::TriggerContext,
) {
    tokio::spawn(async move {
        let def: sprout_workflow::WorkflowDef = match serde_json::from_value(workflow_def_value) {
            Ok(d) => d,
            Err(e) => {
                tracing::error!("workflow run {run_id}: failed to parse definition: {e}");
                if let Err(db_err) = db
                    .update_workflow_run(
                        run_id,
                        sprout_db::workflow::RunStatus::Failed,
                        0,
                        &serde_json::json!([]),
                        Some(&format!("definition parse error: {e}")),
                    )
                    .await
                {
                    tracing::error!("workflow run {run_id}: failed to set Failed status: {db_err}");
                }
                return;
            }
        };

        let result =
            sprout_workflow::executor::execute_run(&engine, run_id, &def, &trigger_ctx).await;
        engine.finalize_run(run_id, result, None).await;
    });
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use sprout_workflow::{ActionDef, Step, TriggerDef, WorkflowDef};

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn make_workflow(steps: Vec<Step>) -> WorkflowDef {
        WorkflowDef {
            name: "test-workflow".to_string(),
            description: None,
            trigger: TriggerDef::Webhook,
            steps,
            enabled: true,
        }
    }

    fn webhook_step(id: &str, url: &str) -> Step {
        Step {
            id: id.to_string(),
            name: None,
            if_expr: None,
            timeout_secs: None,
            action: ActionDef::CallWebhook {
                url: url.to_string(),
                method: None,
                headers: None,
                body: None,
            },
        }
    }

    fn send_message_step(id: &str) -> Step {
        Step {
            id: id.to_string(),
            name: None,
            if_expr: None,
            timeout_secs: None,
            action: ActionDef::SendMessage {
                text: "hello".to_string(),
                channel: None,
            },
        }
    }

    // ── No webhook steps ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn empty_workflow_passes_validation() {
        let def = make_workflow(vec![]);
        assert!(validate_webhook_urls(&def).await.is_ok());
    }

    #[tokio::test]
    async fn non_webhook_steps_pass_validation() {
        let def = make_workflow(vec![send_message_step("step1"), send_message_step("step2")]);
        assert!(validate_webhook_urls(&def).await.is_ok());
    }

    // ── Valid public URLs ─────────────────────────────────────────────────────
    //
    // Use literal public IPs to avoid DNS resolution in the test environment.
    // `validate_webhook_urls` is fail-closed: unresolvable hostnames are rejected.
    // 8.8.8.8 (Google Public DNS) is a well-known public IP that is never private.

    #[tokio::test]
    async fn valid_https_literal_public_ip_passes() {
        let def = make_workflow(vec![webhook_step("s1", "https://8.8.8.8/notify")]);
        assert!(validate_webhook_urls(&def).await.is_ok());
    }

    #[tokio::test]
    async fn valid_http_literal_public_ip_passes() {
        let def = make_workflow(vec![webhook_step("s1", "http://8.8.8.8/webhook")]);
        assert!(validate_webhook_urls(&def).await.is_ok());
    }

    // ── Loopback / private literal IPs ───────────────────────────────────────

    #[tokio::test]
    async fn loopback_127_0_0_1_is_rejected() {
        let def = make_workflow(vec![webhook_step("s1", "http://127.0.0.1/evil")]);
        let err = validate_webhook_urls(&def).await.unwrap_err();
        assert!(
            err.contains("loopback") || err.contains("private"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn loopback_localhost_is_rejected() {
        let def = make_workflow(vec![webhook_step("s1", "http://localhost/evil")]);
        let err = validate_webhook_urls(&def).await.unwrap_err();
        assert!(
            err.contains("loopback") || err.contains("private"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn private_10_network_is_rejected() {
        let def = make_workflow(vec![webhook_step("s1", "http://10.0.0.1/internal")]);
        let err = validate_webhook_urls(&def).await.unwrap_err();
        assert!(
            err.contains("private") || err.contains("internal"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn private_192_168_network_is_rejected() {
        let def = make_workflow(vec![webhook_step("s1", "http://192.168.1.100/internal")]);
        let err = validate_webhook_urls(&def).await.unwrap_err();
        assert!(
            err.contains("private") || err.contains("internal"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn cloud_metadata_endpoint_is_rejected() {
        let def = make_workflow(vec![webhook_step(
            "s1",
            "http://169.254.169.254/latest/meta-data/",
        )]);
        let err = validate_webhook_urls(&def).await.unwrap_err();
        assert!(
            err.contains("metadata") || err.contains("loopback") || err.contains("private"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn ipv6_loopback_is_rejected() {
        let def = make_workflow(vec![webhook_step("s1", "http://[::1]/evil")]);
        let err = validate_webhook_urls(&def).await.unwrap_err();
        assert!(
            err.contains("loopback") || err.contains("private"),
            "unexpected error: {err}"
        );
    }

    // ── Non-http(s) schemes ───────────────────────────────────────────────────

    #[tokio::test]
    async fn ftp_scheme_is_rejected() {
        let def = make_workflow(vec![webhook_step("s1", "ftp://files.example.com/data")]);
        let err = validate_webhook_urls(&def).await.unwrap_err();
        assert!(
            err.contains("scheme") || err.contains("not allowed"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn file_scheme_is_rejected() {
        let def = make_workflow(vec![webhook_step("s1", "file:///etc/passwd")]);
        let err = validate_webhook_urls(&def).await.unwrap_err();
        assert!(
            err.contains("scheme") || err.contains("not allowed"),
            "unexpected error: {err}"
        );
    }

    // ── Multiple steps — one invalid ──────────────────────────────────────────

    #[tokio::test]
    async fn multiple_steps_one_invalid_is_rejected() {
        // First step is a valid public IP, third step is a private IP — must reject.
        let def = make_workflow(vec![
            webhook_step("s1", "https://8.8.8.8/ok"),
            send_message_step("s2"),
            webhook_step("s3", "http://10.0.0.1/bad"),
        ]);
        let err = validate_webhook_urls(&def).await.unwrap_err();
        assert!(
            err.contains("private") || err.contains("internal"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn multiple_valid_webhook_steps_all_pass() {
        // Both steps use literal public IPs — no DNS resolution needed.
        let def = make_workflow(vec![
            webhook_step("s1", "https://8.8.8.8/first"),
            webhook_step("s2", "https://1.1.1.1/second"),
        ]);
        assert!(validate_webhook_urls(&def).await.is_ok());
    }

    // ── Invalid URL format ────────────────────────────────────────────────────

    #[tokio::test]
    async fn malformed_url_is_rejected() {
        let def = make_workflow(vec![webhook_step("s1", "not a url at all")]);
        let err = validate_webhook_urls(&def).await.unwrap_err();
        assert!(
            err.contains("invalid webhook URL"),
            "unexpected error: {err}"
        );
    }
}
