import { invokeTauri } from "@/shared/api/tauri";
import type {
  Workflow,
  WorkflowApproval,
  WorkflowRun,
} from "@/shared/api/types";

export async function getChannelWorkflows(
  channelId: string,
): Promise<Workflow[]> {
  return invokeTauri<Workflow[]>("get_channel_workflows", { channelId });
}

export async function getWorkflow(workflowId: string): Promise<Workflow> {
  return invokeTauri<Workflow>("get_workflow", { workflowId });
}

export async function createWorkflow(
  channelId: string,
  yamlDefinition: string,
): Promise<Workflow> {
  return invokeTauri<Workflow>("create_workflow", {
    channelId,
    yamlDefinition,
  });
}

export async function updateWorkflow(
  workflowId: string,
  yamlDefinition: string,
): Promise<Workflow> {
  return invokeTauri<Workflow>("update_workflow", {
    workflowId,
    yamlDefinition,
  });
}

export async function deleteWorkflow(workflowId: string): Promise<void> {
  await invokeTauri("delete_workflow", { workflowId });
}

export async function getWorkflowRuns(
  workflowId: string,
  limit?: number,
): Promise<WorkflowRun[]> {
  return invokeTauri<WorkflowRun[]>("get_workflow_runs", {
    workflowId,
    limit: limit ?? null,
  });
}

export async function triggerWorkflow(
  workflowId: string,
): Promise<WorkflowRun> {
  return invokeTauri<WorkflowRun>("trigger_workflow", { workflowId });
}

export async function grantApproval(
  token: string,
  note?: string,
): Promise<WorkflowApproval> {
  return invokeTauri<WorkflowApproval>("grant_approval", {
    token,
    note: note ?? null,
  });
}

export async function denyApproval(
  token: string,
  note?: string,
): Promise<WorkflowApproval> {
  return invokeTauri<WorkflowApproval>("deny_approval", {
    token,
    note: note ?? null,
  });
}
