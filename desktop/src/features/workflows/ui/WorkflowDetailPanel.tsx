import { Play, X } from "lucide-react";
import * as React from "react";

import {
  useTriggerWorkflowMutation,
  useWorkflowQuery,
  useWorkflowRunsQuery,
} from "@/features/workflows/hooks";
import { WorkflowRunTrace } from "@/features/workflows/ui/WorkflowRunTrace";
import type { WorkflowRun } from "@/shared/api/types";
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
  const [selectedRun, setSelectedRun] = React.useState<WorkflowRun | null>(
    null,
  );

  const workflow = workflowQuery.data;
  const runs = runsQuery.data ?? [];

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
            Trigger
          </Button>
          <Button onClick={onClose} size="icon" variant="ghost">
            <X className="h-4 w-4" />
          </Button>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto">
        {workflow ? (
          <div className="space-y-4 p-4">
            {workflow.description ? (
              <p className="text-sm text-muted-foreground">
                {workflow.description}
              </p>
            ) : null}

            <div>
              <h4 className="mb-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                Definition
              </h4>
              <pre className="max-h-64 overflow-auto rounded-md bg-muted/50 p-3 font-mono text-xs leading-relaxed">
                {workflow.definition}
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
                        selectedRun?.id === run.id
                          ? "border-primary bg-primary/5"
                          : ""
                      }`}
                      key={run.id}
                      onClick={() =>
                        setSelectedRun(selectedRun?.id === run.id ? null : run)
                      }
                      type="button"
                    >
                      <div className="flex items-center justify-between">
                        <span className="font-mono">{run.id.slice(0, 8)}</span>
                        <RunStatusBadge status={run.status} />
                      </div>
                      <div className="mt-1 text-muted-foreground">
                        {new Date(run.createdAt).toLocaleString()}
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
                <WorkflowRunTrace run={selectedRun} />
              </div>
            ) : null}
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
  };

  return (
    <span
      className={`inline-flex rounded-full px-2 py-0.5 text-[10px] font-medium ${colors[status] ?? colors.pending}`}
    >
      {status}
    </span>
  );
}
