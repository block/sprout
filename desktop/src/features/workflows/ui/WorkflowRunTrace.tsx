import { Check, Clock, SkipForward, X } from "lucide-react";

import type { WorkflowApproval, WorkflowRun } from "@/shared/api/types";
import { WorkflowApprovalCard } from "@/features/workflows/ui/WorkflowApprovalCard";

type WorkflowRunTraceProps = {
  run: WorkflowRun;
  approvals?: WorkflowApproval[];
};

function formatStatusLabel(status: string) {
  return status.replace(/_/g, " ");
}

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

function formatDuration(startedAt: number | null, completedAt: number | null) {
  if (startedAt === null || completedAt === null) return null;
  const seconds = completedAt - startedAt;
  if (seconds < 1) return `${Math.round(seconds * 1000)}ms`;
  return `${seconds.toFixed(1)}s`;
}

export function WorkflowRunTrace({
  run,
  approvals = [],
}: WorkflowRunTraceProps) {
  if (run.executionTrace.length === 0) {
    return (
      <p className="rounded-xl border border-dashed border-border/70 bg-background/60 px-4 py-6 text-center text-sm text-muted-foreground">
        No steps recorded yet.
      </p>
    );
  }

  return (
    <div className="space-y-3" data-testid="workflow-run-trace">
      {run.executionTrace.map((step) => {
        const duration = formatDuration(step.startedAt, step.completedAt);
        const pendingApproval = approvals.find(
          (a) => a.stepId === step.stepId && a.status === "pending",
        );

        return (
          <div
            className="rounded-xl border border-border/60 bg-background/80 p-3 shadow-sm"
            key={step.stepId}
          >
            <div className="flex flex-wrap items-center gap-2 text-sm">
              <StepStatusIcon status={step.status} />
              <span className="min-w-0 flex-1 truncate font-mono text-xs font-medium">
                {step.stepId}
              </span>
              <span className="rounded-full bg-muted px-2 py-0.5 text-[10px] font-medium uppercase tracking-[0.16em] text-muted-foreground">
                {formatStatusLabel(step.status)}
              </span>
              {duration ? (
                <span className="text-xs text-muted-foreground">
                  {duration}
                </span>
              ) : null}
            </div>
            {Object.keys(step.output).length > 0 ? (
              <div className="mt-3">
                <p className="mb-1 text-[11px] font-medium uppercase tracking-[0.16em] text-muted-foreground">
                  Output
                </p>
                <pre className="max-h-32 overflow-auto rounded-lg bg-muted/40 px-3 py-2 font-mono text-xs text-muted-foreground">
                  {JSON.stringify(step.output, null, 2)}
                </pre>
              </div>
            ) : null}
            {step.error ? (
              <div className="mt-3">
                <p className="mb-1 text-[11px] font-medium uppercase tracking-[0.16em] text-red-400">
                  Error
                </p>
                <pre className="max-h-32 overflow-auto rounded-lg bg-red-500/10 px-3 py-2 font-mono text-xs text-red-400">
                  {step.error}
                </pre>
              </div>
            ) : null}
            {pendingApproval ? (
              <div className="mt-3">
                <p className="mb-2 text-[11px] font-medium uppercase tracking-[0.16em] text-amber-600">
                  Pending approval
                </p>
                <WorkflowApprovalCard approval={pendingApproval} />
              </div>
            ) : null}
          </div>
        );
      })}
    </div>
  );
}
