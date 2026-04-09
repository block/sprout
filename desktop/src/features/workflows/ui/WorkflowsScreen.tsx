import * as React from "react";

import { ChatHeader } from "@/features/chat/ui/ChatHeader";
import type { Channel } from "@/shared/api/types";
import { ViewLoadingFallback } from "@/shared/ui/ViewLoadingFallback";

const WorkflowsView = React.lazy(async () => {
  const module = await import("@/features/workflows/ui/WorkflowsView");
  return { default: module.WorkflowsView };
});

type WorkflowsScreenProps = {
  channels: Channel[];
  onCloseWorkflow: () => void;
  onSelectWorkflow: (workflowId: string) => void;
  selectedWorkflowId: string | null;
};

export function WorkflowsScreen({
  channels,
  onCloseWorkflow,
  onSelectWorkflow,
  selectedWorkflowId,
}: WorkflowsScreenProps) {
  return (
    <>
      <ChatHeader
        description="Create, manage, and monitor automated workflows across your channels."
        mode="workflows"
        title="Workflows"
      />

      <React.Suspense fallback={<ViewLoadingFallback kind="workflows" />}>
        <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
          <WorkflowsView
            channels={channels}
            onCloseWorkflow={onCloseWorkflow}
            onSelectWorkflow={onSelectWorkflow}
            selectedWorkflowId={selectedWorkflowId}
          />
        </div>
      </React.Suspense>
    </>
  );
}
