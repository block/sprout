import { createFileRoute } from "@tanstack/react-router";

import { WorkflowsRouteScreen } from "@/app/routes/workflows";

export const Route = createFileRoute("/workflows/$workflowId")({
  component: WorkflowDetailRouteComponent,
});

function WorkflowDetailRouteComponent() {
  const { workflowId } = Route.useParams();

  return <WorkflowsRouteScreen selectedWorkflowId={workflowId} />;
}
