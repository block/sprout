import { Plus, RefreshCw, Zap } from "lucide-react";
import * as React from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { allWorkflowsQueryKey } from "@/features/workflows/hooks";
import { WorkflowCard } from "@/features/workflows/ui/WorkflowCard";
import { WorkflowDeleteDialog } from "@/features/workflows/ui/WorkflowDeleteDialog";
import { WorkflowDetailPanel } from "@/features/workflows/ui/WorkflowDetailPanel";
import { WorkflowDialog } from "@/features/workflows/ui/WorkflowDialog";
import type { Channel, Workflow } from "@/shared/api/types";
import {
  deleteWorkflow,
  getChannelWorkflows,
  triggerWorkflow,
} from "@/shared/api/tauriWorkflows";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";

type WorkflowsViewProps = {
  channels: Channel[];
};

type WorkflowWithChannel = {
  workflow: Workflow;
  channelName: string;
};

type DialogState =
  | { mode: "closed" }
  | { mode: "create" }
  | { mode: "edit"; workflow: Workflow }
  | { mode: "duplicate"; workflow: Workflow };

export function WorkflowsView({ channels }: WorkflowsViewProps) {
  const [dialogState, setDialogState] = React.useState<DialogState>({
    mode: "closed",
  });
  const [deleteTarget, setDeleteTarget] = React.useState<Workflow | null>(null);
  const [selectedWorkflowId, setSelectedWorkflowId] = React.useState<
    string | null
  >(null);
  const queryClient = useQueryClient();

  const memberChannels = channels.filter((c) => c.isMember);
  const channelIds = memberChannels.map((c) => c.id).sort();
  const channelIdKey = channelIds.join(",");

  const allWorkflowsQuery = useQuery({
    queryKey: allWorkflowsQueryKey(channelIdKey),
    queryFn: async () => {
      const results: WorkflowWithChannel[] = [];
      await Promise.all(
        memberChannels.map(async (channel) => {
          const workflows = await getChannelWorkflows(channel.id);
          for (const workflow of workflows) {
            results.push({ workflow, channelName: channel.name });
          }
        }),
      );
      return results;
    },
    enabled: memberChannels.length > 0,
    staleTime: 30_000,
    refetchOnWindowFocus: true,
  });

  const allWorkflows = allWorkflowsQuery.data ?? [];

  const selectedChannelName = selectedWorkflowId
    ? allWorkflows.find((w) => w.workflow.id === selectedWorkflowId)
        ?.channelName
    : undefined;

  const triggerMutation = useMutation({
    mutationFn: (workflowId: string) => triggerWorkflow(workflowId),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        predicate: (query) => query.queryKey[0] === "workflow-runs",
      });
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (workflowId: string) => deleteWorkflow(workflowId),
    onSuccess: (_data, workflowId) => {
      setSelectedWorkflowId((current) =>
        current === workflowId ? null : current,
      );
      void queryClient.invalidateQueries({
        predicate: (query) =>
          query.queryKey[0] === "workflows" ||
          query.queryKey[0] === "workflows-all",
      });
    },
  });

  const triggerOne = triggerMutation.mutate;
  const handleTrigger = React.useCallback(
    (workflowId: string) => triggerOne(workflowId),
    [triggerOne],
  );

  const handleDelete = React.useCallback(
    (workflow: Workflow) => setDeleteTarget(workflow),
    [],
  );

  const deleteOne = deleteMutation.mutate;
  const handleConfirmDelete = React.useCallback(
    (workflow: Workflow) => {
      deleteOne(workflow.id);
      setDeleteTarget(null);
    },
    [deleteOne],
  );

  const handleEdit = React.useCallback(
    (workflow: Workflow) => setDialogState({ mode: "edit", workflow }),
    [],
  );

  const handleDuplicate = React.useCallback(
    (workflow: Workflow) => setDialogState({ mode: "duplicate", workflow }),
    [],
  );

  const handleDialogOpenChange = React.useCallback((open: boolean) => {
    if (!open) {
      setDialogState({ mode: "closed" });
    }
  }, []);

  return (
    <div
      className="flex min-h-0 flex-1 flex-col overflow-hidden lg:flex-row"
      data-testid="workflows-view"
    >
      {/* Workflow list only on overview — hidden while viewing a workflow (nav + full-width editor) */}
      <div
        className={cn(
          "flex min-h-0 flex-col overflow-y-auto p-4",
          selectedWorkflowId
            ? "hidden"
            : "min-h-0 w-full flex-1 lg:w-52 lg:max-w-xs lg:shrink-0 lg:border-border lg:border-r",
        )}
      >
        <div
          className="mb-4 flex items-center justify-between"
          data-tauri-drag-region
        >
          <div className="flex items-center gap-2">
            <h2 className="text-lg font-semibold">Workflows</h2>
            <Button
              aria-label="Refresh workflows"
              disabled={allWorkflowsQuery.isFetching}
              onClick={() => void allWorkflowsQuery.refetch()}
              size="icon"
              variant="ghost"
            >
              <RefreshCw
                className={`h-4 w-4 ${allWorkflowsQuery.isFetching ? "animate-spin" : ""}`}
              />
            </Button>
          </div>
          <Button onClick={() => setDialogState({ mode: "create" })} size="sm">
            <Plus className="mr-1 h-4 w-4" />
            Create Workflow
          </Button>
        </div>

        {allWorkflowsQuery.isLoading ? (
          <div className="flex flex-1 items-center justify-center">
            <p className="text-sm text-muted-foreground">
              Loading workflows...
            </p>
          </div>
        ) : allWorkflowsQuery.isError ? (
          <div className="flex flex-1 flex-col items-center justify-center gap-2 text-muted-foreground">
            <p className="text-sm text-red-400">Failed to load workflows</p>
            <Button
              onClick={() => void allWorkflowsQuery.refetch()}
              size="sm"
              variant="outline"
            >
              Retry
            </Button>
          </div>
        ) : allWorkflows.length === 0 ? (
          <div className="flex flex-1 flex-col items-center justify-center gap-3 text-muted-foreground">
            <Zap className="h-10 w-10 opacity-30" />
            <p className="text-sm">No workflows yet</p>
            <Button
              onClick={() => setDialogState({ mode: "create" })}
              size="sm"
              variant="outline"
            >
              <Plus className="mr-1 h-4 w-4" />
              Create your first workflow
            </Button>
          </div>
        ) : (
          <div className="space-y-2">
            {allWorkflows.map(({ workflow, channelName }) => (
              <WorkflowCard
                channelName={channelName}
                key={workflow.id}
                onDelete={handleDelete}
                onDuplicate={handleDuplicate}
                onEdit={handleEdit}
                onSelect={setSelectedWorkflowId}
                onTrigger={handleTrigger}
                selected={selectedWorkflowId === workflow.id}
                workflow={workflow}
              />
            ))}
          </div>
        )}
      </div>

      {selectedWorkflowId ? (
        <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
          <WorkflowDetailPanel
            key={selectedWorkflowId}
            channelName={selectedChannelName}
            onClose={() => setSelectedWorkflowId(null)}
            onEdit={handleEdit}
            workflowId={selectedWorkflowId}
          />
        </div>
      ) : null}

      <WorkflowDialog
        channels={memberChannels}
        mode={dialogState.mode === "closed" ? "create" : dialogState.mode}
        onOpenChange={handleDialogOpenChange}
        open={dialogState.mode !== "closed"}
        workflow={
          dialogState.mode === "edit" || dialogState.mode === "duplicate"
            ? dialogState.workflow
            : null
        }
      />

      <WorkflowDeleteDialog
        onConfirm={handleConfirmDelete}
        onOpenChange={(open) => {
          if (!open) setDeleteTarget(null);
        }}
        open={deleteTarget !== null}
        workflow={deleteTarget}
      />
    </div>
  );
}
