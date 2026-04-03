import { Check, Clock, SkipForward, X } from "lucide-react";

import type { WorkflowApproval, WorkflowRun } from "@/shared/api/types";
import { WorkflowApprovalCard } from "@/features/workflows/ui/WorkflowApprovalCard";

type WorkflowRunTraceProps = {
  run: WorkflowRun;
  approvals?: WorkflowApproval[];
};

function StepStatusIcon({ status }: { status: string }) {
  switch (status) {
    case "completed":
      return <Check className="h-4 w-4 text-green-500" />;
    case "failed":
    case "error":
      return <X className="h-4 w-4 text-red-500" />;
    case "skipped":
      return <SkipForward className="h-4 w-4 text-muted-foreground" />;
    case "waiting_approval":
      return <Clock className="h-4 w-4 text-amber-500" />;
    default:
      return <Clock className="h-4 w-4 text-blue-500" />;
  }
}

function formatDuration(startedAt: string | null, completedAt: string | null) {
  if (!startedAt || !completedAt) return null;
  const ms = new Date(completedAt).getTime() - new Date(startedAt).getTime();
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

export function WorkflowRunTrace({
  run,
  approvals = [],
}: WorkflowRunTraceProps) {
  if (run.executionTrace.length === 0) {
    return (
      <p className="py-4 text-center text-sm text-muted-foreground">
        No steps recorded yet.
      </p>
    );
  }

  return (
    <div className="space-y-1" data-testid="workflow-run-trace">
      {run.executionTrace.map((step) => {
        const duration = formatDuration(step.startedAt, step.completedAt);
        const pendingApproval = approvals.find(
          (a) => a.stepId === step.stepId && a.status === "pending",
        );

        return (
          <div key={step.stepId}>
            <div className="flex items-center gap-2 rounded-md px-2 py-1.5 text-sm hover:bg-muted/50">
              <StepStatusIcon status={step.status} />
              <span className="flex-1 truncate font-mono text-xs">
                {step.stepId}
              </span>
              <span className="text-xs text-muted-foreground">
                {step.status}
              </span>
              {duration ? (
                <span className="text-xs text-muted-foreground">
                  {duration}
                </span>
              ) : null}
            </div>
            {step.output ? (
              <pre className="ml-8 max-h-24 overflow-auto rounded bg-muted/30 px-2 py-1 font-mono text-xs text-muted-foreground">
                {step.output}
              </pre>
            ) : null}
            {step.error ? (
              <pre className="ml-8 max-h-24 overflow-auto rounded bg-red-500/10 px-2 py-1 font-mono text-xs text-red-400">
                {step.error}
              </pre>
            ) : null}
            {pendingApproval ? (
              <div className="ml-8 mt-1">
                <WorkflowApprovalCard approval={pendingApproval} />
              </div>
            ) : null}
          </div>
        );
      })}
    </div>
  );
}
