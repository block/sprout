import { createFileRoute } from "@tanstack/react-router";

import { useAppNavigation } from "@/app/navigation/useAppNavigation";
import { useChannelsQuery } from "@/features/channels/hooks";
import { WorkflowsScreen } from "@/features/workflows/ui/WorkflowsScreen";

export const Route = createFileRoute("/workflows")({
  component: WorkflowsRouteComponent,
});

export function WorkflowsRouteScreen({
  selectedWorkflowId,
}: {
  selectedWorkflowId: string | null;
}) {
  const { closeWorkflowDetail, goWorkflow } = useAppNavigation();
  const channelsQuery = useChannelsQuery();
  const channels = channelsQuery.data ?? [];
  const memberChannels = channels.filter((channel) => channel.isMember);

  return (
    <WorkflowsScreen
      channels={memberChannels}
      onCloseWorkflow={closeWorkflowDetail}
      onSelectWorkflow={(workflowId) => {
        void goWorkflow(workflowId);
      }}
      selectedWorkflowId={selectedWorkflowId}
    />
  );
}

function WorkflowsRouteComponent() {
  return <WorkflowsRouteScreen selectedWorkflowId={null} />;
}
