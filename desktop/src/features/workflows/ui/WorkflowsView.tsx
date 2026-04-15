import { Plus, RefreshCw, Zap } from "lucide-react";
import * as React from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { allWorkflowsQueryKey } from "@/features/workflows/hooks";
import { WorkflowCard } from "@/features/workflows/ui/WorkflowCard";
import { WorkflowCreatePromptPanel } from "@/features/workflows/ui/WorkflowCreatePromptPanel";
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
import { Skeleton } from "@/shared/ui/skeleton";
import { draftWorkflowYamlFromPrompt } from "./workflowPromptScaffold";

type WorkflowsViewProps = {
  channels: Channel[];
  onCloseWorkflow: () => void;
  onSelectWorkflow: (workflowId: string) => void;
  selectedWorkflowId: string | null;
};

type WorkflowWithChannel = {
  workflow: Workflow;
  channelName: string;
};

type DialogState =
  | { mode: "closed" }
  | { mode: "edit"; workflow: Workflow }
  | { mode: "duplicate"; workflow: Workflow };

function WorkflowsListSkeleton() {
  return (
    <div className="space-y-2">
      {["first", "second", "third", "fourth"].map((card) => (
        <div
          className="rounded-xl border border-border/70 bg-card/80 p-4 shadow-sm"
          key={card}
        >
          <div className="flex items-start justify-between gap-4">
            <div className="min-w-0 flex-1 space-y-3">
              <div className="flex items-center gap-2">
                <Skeleton className="h-5 w-44" />
                <Skeleton className="h-5 w-16 rounded-full" />
              </div>
              <Skeleton className="h-4 w-full max-w-2xl" />
              <div className="flex flex-wrap gap-2">
                <Skeleton className="h-5 w-20 rounded-full" />
                <Skeleton className="h-5 w-24 rounded-full" />
                <Skeleton className="h-5 w-16 rounded-full" />
              </div>
            </div>
            <div className="hidden shrink-0 gap-2 sm:flex">
              <Skeleton className="h-8 w-8 rounded-lg" />
              <Skeleton className="h-8 w-8 rounded-lg" />
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}

export function WorkflowsView({
  channels,
  onCloseWorkflow,
  onSelectWorkflow,
  selectedWorkflowId,
}: WorkflowsViewProps) {
  const [dialogState, setDialogState] = React.useState<DialogState>({
    mode: "closed",
  });
  const [createPrompt, setCreatePrompt] = React.useState("");
  const [createPromptOpen, setCreatePromptOpen] = React.useState(false);
  const [createDraftSeed, setCreateDraftSeed] = React.useState<{
    prompt: string;
    yaml: string;
  } | null>(null);
  const [deleteTarget, setDeleteTarget] = React.useState<Workflow | null>(null);
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
      if (selectedWorkflowId === workflowId) {
        onCloseWorkflow();
      }
      void queryClient.invalidateQueries({
        predicate: (query) =>
          query.queryKey[0] === "workflows" ||
          query.queryKey[0] === "workflows-all",
      });
    },
  });

  const handleTrigger = React.useCallback(
    (workflowId: string) => {
      triggerMutation.mutate(workflowId);
    },
    [triggerMutation],
  );

  const handleDelete = React.useCallback(
    (workflow: Workflow) => setDeleteTarget(workflow),
    [],
  );

  const handleConfirmDelete = React.useCallback(
    (workflow: Workflow) => {
      deleteMutation.mutate(workflow.id);
      setDeleteTarget(null);
    },
    [deleteMutation],
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
      setCreateDraftSeed(null);
    }
  }, []);

  const handleStartCreate = React.useCallback(() => {
    setCreatePrompt("");
    setCreatePromptOpen(true);
  }, []);

  const handleCancelCreatePrompt = React.useCallback(() => {
    setCreatePromptOpen(false);
    setCreatePrompt("");
  }, []);

  const handleDraftCreatePrompt = React.useCallback(() => {
    const trimmedPrompt = createPrompt.trim();
    if (!trimmedPrompt) {
      return;
    }

    const defaultChannelId = memberChannels[0]?.id ?? "";
    setCreateDraftSeed({
      prompt: trimmedPrompt,
      yaml: draftWorkflowYamlFromPrompt(trimmedPrompt, {
        defaultChannelId: defaultChannelId || undefined,
      }),
    });
    setCreatePromptOpen(false);
  }, [createPrompt, memberChannels]);

  const showDetailPane = selectedWorkflowId !== null || createPromptOpen;
  const showCreateSetupDialog = createDraftSeed !== null;

  return (
    <div
      className="flex min-h-0 flex-1 flex-col overflow-hidden lg:flex-row"
      data-testid="workflows-view"
    >
      <div
        className={cn(
          "flex min-h-0 flex-col overflow-y-auto p-4",
          showDetailPane
            ? "hidden"
            : "min-h-0 w-full flex-1 lg:w-52 lg:max-w-xs lg:shrink-0 lg:border-r lg:border-border",
        )}
        data-scroll-restoration-id="workflows-list"
      >
        <div className="mb-4 flex items-center justify-between" data-tauri-drag-region>
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
          <Button onClick={handleStartCreate} size="sm">
            <Plus className="mr-1 h-4 w-4" />
            Create Workflow
          </Button>
        </div>

        {allWorkflowsQuery.isLoading ? (
          <WorkflowsListSkeleton />
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
              onClick={handleStartCreate}
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
                isActive={selectedWorkflowId === workflow.id}
                key={workflow.id}
                onDelete={handleDelete}
                onDuplicate={handleDuplicate}
                onEdit={handleEdit}
                onSelect={onSelectWorkflow}
                onTrigger={handleTrigger}
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
            onClose={onCloseWorkflow}
            workflowId={selectedWorkflowId}
          />
        </div>
      ) : createPromptOpen ? (
        <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
          <WorkflowCreatePromptPanel
            onCancel={handleCancelCreatePrompt}
            onChange={setCreatePrompt}
            onSubmit={handleDraftCreatePrompt}
            prompt={createPrompt}
          />
        </div>
      ) : null}

      <WorkflowDialog
        channels={memberChannels}
        initialCreateDraft={createDraftSeed}
        mode={
          dialogState.mode === "closed"
            ? "create"
            : dialogState.mode
        }
        onOpenChange={handleDialogOpenChange}
        open={showCreateSetupDialog || dialogState.mode !== "closed"}
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
