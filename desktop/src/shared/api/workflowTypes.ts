export type WorkflowStatus = "active" | "disabled" | "archived";

export type Workflow = {
  id: string;
  name: string;
  description: string | null;
  ownerPubkey: string;
  channelId: string;
  definition: string;
  definitionHash: string;
  status: WorkflowStatus;
  enabled: boolean;
  createdAt: string;
  updatedAt: string;
};

export type WorkflowRunStatus =
  | "pending"
  | "running"
  | "completed"
  | "failed"
  | "cancelled";

export type TraceEntry = {
  stepId: string;
  status: string;
  output: string | null;
  startedAt: string | null;
  completedAt: string | null;
  error: string | null;
};

export type WorkflowRun = {
  id: string;
  workflowId: string;
  status: WorkflowRunStatus;
  currentStep: string | null;
  executionTrace: TraceEntry[];
  triggerContext: Record<string, unknown> | null;
  startedAt: string | null;
  completedAt: string | null;
  errorMessage: string | null;
  createdAt: string;
};

export type WorkflowApprovalStatus =
  | "pending"
  | "granted"
  | "denied"
  | "expired";

export type WorkflowApproval = {
  token: string;
  workflowId: string;
  runId: string;
  stepId: string;
  stepIndex: number;
  approverSpec: string;
  status: WorkflowApprovalStatus;
  approverPubkey: string | null;
  note: string | null;
  expiresAt: string | null;
  createdAt: string;
};
