import { Plus, Zap } from "lucide-react";
import * as React from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { CreateWorkflowDialog } from "@/features/workflows/ui/CreateWorkflowDialog";
import { WorkflowCard } from "@/features/workflows/ui/WorkflowCard";
import { WorkflowDetailPanel } from "@/features/workflows/ui/WorkflowDetailPanel";
import type { Channel, Workflow } from "@/shared/api/types";
import {
  deleteWorkflow,
  getChannelWorkflows,
  triggerWorkflow,
} from "@/shared/api/tauriWorkflows";
import { Button } from "@/shared/ui/button";

type WorkflowsViewProps = {
  channels: Channel[];
};

type WorkflowWithChannel = {
  workflow: Workflow;
  channelName: string;
};

export function WorkflowsView({ channels }: WorkflowsViewProps) {
  const [isCreateOpen, setIsCreateOpen] = React.useState(false);
  const [selectedWorkflowId, setSelectedWorkflowId] = React.useState<
    string | null
  >(null);
  const queryClient = useQueryClient();

  const memberChannels = channels.filter((c) => c.isMember);
  const channelIds = memberChannels.map((c) => c.id).sort();
  const channelIdKey = channelIds.join(",");

  const allWorkflowsQuery = useQuery({
    queryKey: ["workflows-all", channelIdKey],
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
    refetchInterval: 30_000,
  });

  const allWorkflows = allWorkflowsQuery.data ?? [];

  async function handleTrigger(workflowId: string) {
    await triggerWorkflow(workflowId);
    void queryClient.invalidateQueries({
      predicate: (query) => query.queryKey[0] === "workflow-runs",
    });
  }

  async function handleDelete(workflowId: string) {
    await deleteWorkflow(workflowId);
    if (selectedWorkflowId === workflowId) {
      setSelectedWorkflowId(null);
    }
    void queryClient.invalidateQueries({
      predicate: (query) =>
        query.queryKey[0] === "workflows" ||
        query.queryKey[0] === "workflows-all",
    });
  }

  return (
    <div
      className="flex min-h-0 flex-1 overflow-hidden"
      data-testid="workflows-view"
    >
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto p-4">
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-lg font-semibold">Workflows</h2>
          <Button onClick={() => setIsCreateOpen(true)} size="sm">
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
        ) : allWorkflows.length === 0 ? (
          <div className="flex flex-1 flex-col items-center justify-center gap-3 text-muted-foreground">
            <Zap className="h-10 w-10 opacity-30" />
            <p className="text-sm">No workflows yet</p>
            <Button
              onClick={() => setIsCreateOpen(true)}
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
                onSelect={setSelectedWorkflowId}
                onTrigger={handleTrigger}
                workflow={workflow}
              />
            ))}
          </div>
        )}
      </div>

      {selectedWorkflowId ? (
        <div className="w-[400px] shrink-0">
          <WorkflowDetailPanel
            key={selectedWorkflowId}
            onClose={() => setSelectedWorkflowId(null)}
            workflowId={selectedWorkflowId}
          />
        </div>
      ) : null}

      <CreateWorkflowDialog
        channels={memberChannels}
        onOpenChange={setIsCreateOpen}
        open={isCreateOpen}
      />
    </div>
  );
}
