use crate::client::SproutClient;
use crate::error::CliError;
use crate::validate::{read_or_stdin, validate_uuid};

pub async fn cmd_list_workflows(client: &SproutClient, channel_id: &str) -> Result<(), CliError> {
    validate_uuid(channel_id)?;
    client
        .run_get(&format!("/api/channels/{}/workflows", channel_id))
        .await
}

pub async fn cmd_create_workflow(
    client: &SproutClient,
    channel_id: &str,
    yaml: &str,
) -> Result<(), CliError> {
    validate_uuid(channel_id)?;
    let yaml_definition = read_or_stdin(yaml)?;
    client
        .run_post(
            &format!("/api/channels/{}/workflows", channel_id),
            &serde_json::json!({ "yaml_definition": yaml_definition }),
        )
        .await
}

pub async fn cmd_update_workflow(
    client: &SproutClient,
    workflow_id: &str,
    yaml: &str,
) -> Result<(), CliError> {
    validate_uuid(workflow_id)?;
    let yaml_definition = read_or_stdin(yaml)?;
    client
        .run_put(
            &format!("/api/workflows/{}", workflow_id),
            &serde_json::json!({ "yaml_definition": yaml_definition }),
        )
        .await
}

pub async fn cmd_delete_workflow(client: &SproutClient, workflow_id: &str) -> Result<(), CliError> {
    validate_uuid(workflow_id)?;
    client
        .run_delete(&format!("/api/workflows/{}", workflow_id))
        .await
}

pub async fn cmd_trigger_workflow(
    client: &SproutClient,
    workflow_id: &str,
) -> Result<(), CliError> {
    validate_uuid(workflow_id)?;
    client
        .run_post(
            &format!("/api/workflows/{}/trigger", workflow_id),
            &serde_json::json!({}),
        )
        .await
}

pub async fn cmd_get_workflow_runs(
    client: &SproutClient,
    workflow_id: &str,
    limit: Option<u32>,
) -> Result<(), CliError> {
    validate_uuid(workflow_id)?;
    let limit = limit.unwrap_or(20).min(100);
    let path = format!("/api/workflows/{}/runs?limit={}", workflow_id, limit);
    client.run_get(&path).await
}

pub async fn cmd_get_workflow(client: &SproutClient, workflow_id: &str) -> Result<(), CliError> {
    validate_uuid(workflow_id)?;
    client
        .run_get(&format!("/api/workflows/{}", workflow_id))
        .await
}

/// Route is /grant or /deny based on the `approved` flag.
pub async fn cmd_approve_step(
    client: &SproutClient,
    approval_token: &str,
    approved: bool,
    note: Option<&str>,
) -> Result<(), CliError> {
    validate_uuid(approval_token)?;
    let route = if approved { "grant" } else { "deny" };
    let mut body = serde_json::json!({});
    if let Some(n) = note {
        body["note"] = n.into();
    }
    client
        .run_post(
            &format!("/api/approvals/{}/{}", approval_token, route),
            &body,
        )
        .await
}
