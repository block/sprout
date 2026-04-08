import { ArrowLeft, LayoutList, Pencil, Play } from "lucide-react";
import * as React from "react";

import {
  useRunApprovalsQuery,
  useTriggerWorkflowMutation,
  useWorkflowQuery,
  useWorkflowRunsQuery,
} from "@/features/workflows/hooks";
import { WorkflowDefinitionGraph } from "@/features/workflows/ui/WorkflowDefinitionGraph";
import { WorkflowPropertiesPanel } from "@/features/workflows/ui/WorkflowPropertiesPanel";
import { WorkflowRunHistoryList } from "@/features/workflows/ui/WorkflowRunHistoryList";
import type { Workflow } from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
} from "@/shared/ui/sheet";
import {
  getWorkflowDisplayStatus,
  getWorkflowDisplayTitle,
} from "./workflowDefinition";

type WorkflowDetailPanelProps = {
  workflowId: string;
  channelName?: string | null;
  onClose: () => void;
  onEdit: (workflow: Workflow) => void;
};

/**
 * Diagram uses the full content area; on large screens the properties panel is a
 * floating rounded column over the map (no split layout / no vertical chrome).
 */
export function WorkflowDetailPanel({
  workflowId,
  channelName,
  onClose,
  onEdit,
}: WorkflowDetailPanelProps) {
  const workflowQuery = useWorkflowQuery(workflowId);
  const runsQuery = useWorkflowRunsQuery(workflowId);
  const triggerMutation = useTriggerWorkflowMutation(workflowId);
  const [selectedRunId, setSelectedRunId] = React.useState<string | null>(null);
  const [graphSelection, setGraphSelection] = React.useState<string | null>(
    null,
  );
  const [propertiesOpen, setPropertiesOpen] = React.useState(false);

  const handleGraphSelection = React.useCallback((id: string | null) => {
    setGraphSelection(id);
    if (
      typeof window !== "undefined" &&
      window.matchMedia("(max-width: 1023px)").matches
    ) {
      setPropertiesOpen(true);
    }
  }, []);

  const workflow = workflowQuery.data;
  const runs = runsQuery.data ?? [];
  const approvalsQuery = useRunApprovalsQuery(workflowId, selectedRunId);
  const workflowStatus = workflow ? getWorkflowDisplayStatus(workflow) : null;

  const approvalsSnapshot = React.useMemo(
    () => ({
      isFetching: approvalsQuery.isFetching,
      error: approvalsQuery.error,
      data: approvalsQuery.data,
    }),
    [approvalsQuery.isFetching, approvalsQuery.error, approvalsQuery.data],
  );

  async function handleTrigger() {
    try {
      const response = await triggerMutation.mutateAsync();
      setSelectedRunId(response.runId);
    } catch {
      // React Query stores the error; keep the current selection unchanged.
    }
  }

  const propertiesTitle =
    graphSelection === null
      ? "Workflow"
      : graphSelection === "trigger"
        ? "Trigger"
        : "Step";

  const runHistoryProps = {
    approvalsQuery: approvalsSnapshot,
    onToggleRun: (id: string | null) => setSelectedRunId(id),
    runs,
    selectedRunId,
    workflow,
    workflowQueryError: workflowQuery.isError,
    workflowQueryLoading: workflowQuery.isLoading,
  };

  return (
    <div
      className="flex min-h-0 flex-1 flex-col overflow-hidden bg-background"
      data-testid="workflow-detail-panel"
    >
      <header
        className="flex shrink-0 items-center justify-between gap-2 border-b border-border px-3 py-2 sm:px-4"
        data-tauri-drag-region
      >
        <div className="flex min-w-0 flex-1 items-center gap-2 sm:gap-3">
          <Button
            aria-label="Back to workflow list"
            className="shrink-0 gap-1.5 px-2 sm:px-3"
            onClick={onClose}
            size="sm"
            variant="ghost"
          >
            <ArrowLeft className="h-4 w-4 shrink-0" />
            <span className="hidden sm:inline">Back</span>
          </Button>
          <div className="min-w-0 pl-0 sm:pl-1">
            <div className="flex items-center gap-2">
              <h3 className="truncate text-sm font-semibold">
                {workflow ? getWorkflowDisplayTitle(workflow) : "Loading..."}
              </h3>
              {workflowStatus ? (
                <RunStatusBadge status={workflowStatus} />
              ) : null}
            </div>
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {workflow ? (
            <Button
              className="lg:hidden"
              onClick={() => setPropertiesOpen(true)}
              size="sm"
              variant="outline"
            >
              <LayoutList className="mr-1 h-3 w-3" />
              Properties
            </Button>
          ) : null}
          {workflow ? (
            <Button
              onClick={() => onEdit(workflow)}
              size="sm"
              variant="outline"
            >
              <Pencil className="mr-1 h-3 w-3" />
              Edit
            </Button>
          ) : null}
          <Button
            disabled={triggerMutation.isPending}
            onClick={() => void handleTrigger()}
            size="sm"
            variant="outline"
          >
            <Play className="mr-1 h-3 w-3" />
            {triggerMutation.isPending ? "Triggering..." : "Trigger"}
          </Button>
        </div>
      </header>

      {triggerMutation.isError ? (
        <div className="border-b border-border px-4 py-2 text-xs text-red-400">
          Failed to trigger workflow
        </div>
      ) : null}

      <div className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
        {/* Map: edge-to-edge on lg so the canvas reads as full-bleed behind the overlay */}
        <div className="relative z-0 flex min-h-0 flex-1 flex-col overflow-hidden">
          <div className="flex min-h-0 flex-1 flex-col p-2 sm:p-3 lg:absolute lg:inset-0 lg:p-0">
            {workflow ? (
              <WorkflowDefinitionGraph
                className="min-h-0 flex-1 lg:rounded-none lg:border-0 lg:bg-muted/25"
                controlsPosition="bottom-left"
                definition={workflow.definition}
                onSelectedNodeIdChange={handleGraphSelection}
                selectedNodeId={graphSelection}
              />
            ) : workflowQuery.isError ? (
              <div className="flex min-h-[12rem] flex-1 flex-col items-center justify-center gap-2 rounded-xl border border-border/70">
                <p className="text-sm text-red-400">Failed to load workflow</p>
              </div>
            ) : (
              <div className="flex min-h-[12rem] flex-1 items-center justify-center rounded-xl border border-border/70">
                <p className="text-sm text-muted-foreground">Loading...</p>
              </div>
            )}
          </div>

          {workflow ? (
            <aside
              aria-label="Flow diagram properties"
              className="pointer-events-none absolute inset-y-2 right-2 z-10 hidden w-[min(20rem,calc(100vw-8rem))] max-w-sm flex-col lg:flex"
            >
              <div className="pointer-events-auto flex h-full min-h-0 flex-1 flex-col overflow-hidden rounded-2xl border border-white/10 bg-background/45 shadow-2xl backdrop-blur-xl backdrop-saturate-150 dark:border-white/5 dark:bg-background/35 supports-[backdrop-filter]:bg-background/40">
                <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-4 py-4">
                  <WorkflowPropertiesPanel
                    channelName={channelName}
                    className="gap-4 p-0"
                    selectedNodeId={graphSelection}
                    workflow={workflow}
                  />
                  <div className="mt-5 border-t border-border/40 pt-5">
                    <WorkflowRunHistoryList {...runHistoryProps} />
                  </div>
                </div>
              </div>
            </aside>
          ) : null}
        </div>

        <div className="max-h-[min(40vh,20rem)] shrink-0 overflow-hidden border-t border-border/80 bg-background/95 lg:hidden">
          <div className="max-h-[min(40vh,20rem)] overflow-y-auto p-3 sm:p-4">
            <WorkflowRunHistoryList {...runHistoryProps} />
          </div>
        </div>
      </div>

      {workflow ? (
        <div className="lg:hidden">
          <Sheet onOpenChange={setPropertiesOpen} open={propertiesOpen}>
            <SheetContent
              className="flex max-h-[90vh] flex-col overflow-hidden p-0"
              side="bottom"
            >
              <SheetHeader className="border-b px-4 py-3 text-left">
                <SheetTitle>{propertiesTitle}</SheetTitle>
              </SheetHeader>
              <div className="min-h-0 flex-1 overflow-y-auto">
                <WorkflowPropertiesPanel
                  channelName={channelName}
                  selectedNodeId={graphSelection}
                  workflow={workflow}
                />
              </div>
            </SheetContent>
          </Sheet>
        </div>
      ) : null}
    </div>
  );
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
