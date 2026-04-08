import { ChevronDown, ChevronRight } from "lucide-react";

import { WorkflowRunTrace } from "@/features/workflows/ui/WorkflowRunTrace";
import type {
  Workflow,
  WorkflowApproval,
  WorkflowRun,
} from "@/shared/api/types";

type RunApprovalsQuerySnapshot = {
  isFetching: boolean;
  error: unknown;
  data: WorkflowApproval[] | undefined;
};

function formatRunDuration(
  startedAt: number | null,
  completedAt: number | null,
) {
  if (startedAt === null || completedAt === null) return null;
  const seconds = completedAt - startedAt;
  if (seconds < 1) return `${Math.round(seconds * 1000)}ms`;
  return `${seconds.toFixed(1)}s`;
}

function formatStatusLabel(status: string) {
  return status.replace(/_/g, " ");
}

function RunStatusBadge({ status }: { status: string }) {
  const colors: Record<string, string> = {
    active: "bg-green-500/15 text-green-500",
    disabled: "bg-muted text-muted-foreground",
    archived: "bg-amber-500/15 text-amber-500",
    completed: "bg-green-500/15 text-green-500",
    failed: "bg-red-500/15 text-red-500",
    running: "bg-blue-500/15 text-blue-500",
    pending: "bg-muted text-muted-foreground",
    cancelled: "bg-muted text-muted-foreground",
    waiting_approval: "bg-amber-500/15 text-amber-500",
  };

  return (
    <span
      className={`inline-flex rounded-full px-2 py-0.5 text-[10px] font-medium uppercase tracking-[0.16em] ${colors[status] ?? colors.pending}`}
    >
      {formatStatusLabel(status)}
    </span>
  );
}

type WorkflowRunHistoryListProps = {
  workflow: Workflow | undefined;
  workflowQueryLoading: boolean;
  workflowQueryError: boolean;
  runs: WorkflowRun[];
  selectedRunId: string | null;
  onToggleRun: (runId: string | null) => void;
  approvalsQuery: RunApprovalsQuerySnapshot;
  /** When set, constrains vertical height (mobile strip under the map) */
  className?: string;
};

export function WorkflowRunHistoryList({
  workflow,
  workflowQueryLoading,
  workflowQueryError,
  runs,
  selectedRunId,
  onToggleRun,
  approvalsQuery,
  className,
}: WorkflowRunHistoryListProps) {
  return (
    <div className={className}>
      <h4 className="mb-3 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
        Run history
      </h4>
      {workflow ? (
        runs.length === 0 ? (
          <p className="text-sm text-muted-foreground">No runs yet.</p>
        ) : (
          <div className="space-y-2">
            {runs.map((run) => {
              const isSelected = selectedRunId === run.id;
              const duration = formatRunDuration(
                run.startedAt,
                run.completedAt,
              );

              return (
                <div
                  className={`overflow-hidden rounded-xl border bg-card/70 transition-colors ${
                    isSelected
                      ? "border-primary/40 bg-primary/5 shadow-sm"
                      : "border-border/70 hover:bg-muted/20"
                  }`}
                  key={run.id}
                >
                  <button
                    aria-expanded={isSelected}
                    className="w-full px-4 py-3 text-left"
                    data-testid={
                      isSelected ? "workflow-selected-run" : undefined
                    }
                    onClick={() => onToggleRun(isSelected ? null : run.id)}
                    type="button"
                  >
                    <div className="flex items-start justify-between gap-3">
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2">
                          {isSelected ? (
                            <ChevronDown className="h-4 w-4 text-muted-foreground" />
                          ) : (
                            <ChevronRight className="h-4 w-4 text-muted-foreground" />
                          )}
                          <span className="truncate font-mono text-xs font-medium">
                            {run.id.slice(0, 8)}
                          </span>
                          <RunStatusBadge status={run.status} />
                        </div>
                        <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 pl-6 text-[11px] text-muted-foreground">
                          <span>
                            {new Date(run.createdAt * 1000).toLocaleString()}
                          </span>
                          <span>
                            {run.executionTrace.length}{" "}
                            {run.executionTrace.length === 1 ? "step" : "steps"}
                          </span>
                          {duration ? <span>{duration}</span> : null}
                          {run.currentStep !== null ? (
                            <span>Current step {run.currentStep + 1}</span>
                          ) : null}
                        </div>
                        {run.errorMessage ? (
                          <p className="mt-2 break-words pl-6 text-xs text-destructive">
                            {run.errorMessage}
                          </p>
                        ) : null}
                      </div>
                    </div>
                  </button>

                  {isSelected ? (
                    <div className="border-t border-border/60 bg-background/60 px-4 py-4">
                      <div className="mb-3 flex items-center gap-2 text-[11px] font-medium uppercase tracking-[0.18em] text-muted-foreground">
                        <span>Execution Trace</span>
                        {approvalsQuery.isFetching ? (
                          <span className="text-[10px] tracking-[0.12em] text-muted-foreground/80">
                            Refreshing approvals...
                          </span>
                        ) : null}
                      </div>
                      {approvalsQuery.error instanceof Error ? (
                        <p className="mb-3 rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
                          {approvalsQuery.error.message}
                        </p>
                      ) : null}
                      <WorkflowRunTrace
                        approvals={approvalsQuery.data}
                        run={run}
                      />
                    </div>
                  ) : null}
                </div>
              );
            })}
          </div>
        )
      ) : workflowQueryError ? null : workflowQueryLoading ? (
        <p className="text-sm text-muted-foreground">Loading...</p>
      ) : null}
    </div>
  );
}
