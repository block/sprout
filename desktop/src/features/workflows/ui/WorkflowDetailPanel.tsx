import { Play, X } from "lucide-react";
import * as React from "react";

import {
  useRunApprovalsQuery,
  useTriggerWorkflowMutation,
  useWorkflowQuery,
  useWorkflowRunsQuery,
} from "@/features/workflows/hooks";
import { WorkflowRunTrace } from "@/features/workflows/ui/WorkflowRunTrace";
import { Button } from "@/shared/ui/button";

type WorkflowDetailPanelProps = {
  workflowId: string;
  onClose: () => void;
};

export function WorkflowDetailPanel({
  workflowId,
  onClose,
}: WorkflowDetailPanelProps) {
  const workflowQuery = useWorkflowQuery(workflowId);
  const runsQuery = useWorkflowRunsQuery(workflowId);
  const triggerMutation = useTriggerWorkflowMutation(workflowId);
  const [selectedRunId, setSelectedRunId] = React.useState<string | null>(null);

  const workflow = workflowQuery.data;
  const runs = runsQuery.data ?? [];
  const selectedRun = selectedRunId
    ? (runs.find((r) => r.id === selectedRunId) ?? null)
    : null;
  const approvalsQuery = useRunApprovalsQuery(workflowId, selectedRunId);

  return (
    <div
      className="flex h-full flex-col border-l bg-background"
      data-testid="workflow-detail-panel"
    >
      <div className="flex items-center justify-between border-b px-4 py-3">
        <h3 className="truncate text-sm font-semibold">
          {workflow?.name ?? "Loading..."}
        </h3>
        <div className="flex items-center gap-1">
          <Button
            disabled={triggerMutation.isPending}
            onClick={() => triggerMutation.mutate()}
            size="sm"
            variant="outline"
          >
            <Play className="mr-1 h-3 w-3" />
            {triggerMutation.isPending ? "Triggering..." : "Trigger"}
          </Button>
          <Button
            aria-label="Close detail panel"
            onClick={onClose}
            size="icon"
            variant="ghost"
          >
            <X className="h-4 w-4" />
          </Button>
        </div>
      </div>

      {triggerMutation.isError ? (
        <div className="border-b px-4 py-2 text-xs text-red-400">
          Failed to trigger workflow
        </div>
      ) : null}

      <div className="flex-1 overflow-y-auto">
        {workflow ? (
          <div className="space-y-4 p-4">
            <div>
              <h4 className="mb-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                Definition
              </h4>
              <pre className="max-h-64 overflow-auto rounded-md bg-muted/50 p-3 font-mono text-xs leading-relaxed">
                {JSON.stringify(workflow.definition, null, 2)}
              </pre>
            </div>

            <div>
              <h4 className="mb-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                Run History
              </h4>
              {runs.length === 0 ? (
                <p className="text-sm text-muted-foreground">No runs yet.</p>
              ) : (
                <div className="space-y-2">
                  {runs.map((run) => (
                    <button
                      className={`w-full rounded-md border px-3 py-2 text-left text-xs transition-colors hover:bg-muted/50 ${
                        selectedRunId === run.id
                          ? "border-primary bg-primary/5"
                          : ""
                      }`}
                      key={run.id}
                      onClick={() =>
                        setSelectedRunId(
                          selectedRunId === run.id ? null : run.id,
                        )
                      }
                      type="button"
                    >
                      <div className="flex items-center justify-between">
                        <span className="font-mono">{run.id.slice(0, 8)}</span>
                        <RunStatusBadge status={run.status} />
                      </div>
                      <div className="mt-1 text-muted-foreground">
                        {new Date(run.createdAt * 1000).toLocaleString()}
                      </div>
                    </button>
                  ))}
                </div>
              )}
            </div>

            {selectedRun ? (
              <div>
                <h4 className="mb-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                  Execution Trace
                </h4>
                <WorkflowRunTrace
                  approvals={approvalsQuery.data}
                  run={selectedRun}
                />
              </div>
            ) : null}
          </div>
        ) : workflowQuery.isError ? (
          <div className="flex h-32 flex-col items-center justify-center gap-2">
            <p className="text-sm text-red-400">Failed to load workflow</p>
          </div>
        ) : (
          <div className="flex h-32 items-center justify-center">
            <p className="text-sm text-muted-foreground">Loading...</p>
          </div>
        )}
      </div>
    </div>
  );
}

function RunStatusBadge({ status }: { status: string }) {
  const colors: Record<string, string> = {
    completed: "bg-green-500/15 text-green-500",
    failed: "bg-red-500/15 text-red-500",
    running: "bg-blue-500/15 text-blue-500",
    pending: "bg-muted text-muted-foreground",
    cancelled: "bg-muted text-muted-foreground",
    waiting_approval: "bg-amber-500/15 text-amber-500",
  };

  return (
    <span
      className={`inline-flex rounded-full px-2 py-0.5 text-[10px] font-medium ${colors[status] ?? colors.pending}`}
    >
      {status}
    </span>
  );
}
