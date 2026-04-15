import * as React from "react";
import { createFileRoute } from "@tanstack/react-router";

import { ViewLoadingFallback } from "@/shared/ui/ViewLoadingFallback";

export const Route = createFileRoute("/workflows")({
  component: WorkflowsRouteComponent,
});

const WorkflowsRouteScreen = React.lazy(async () => {
  const module = await import("./WorkflowsRouteScreen");
  return { default: module.WorkflowsRouteScreen };
});

function WorkflowsRouteComponent() {
  return (
    <React.Suspense
      fallback={<ViewLoadingFallback includeHeader kind="workflows" />}
    >
      <WorkflowsRouteScreen selectedWorkflowId={null} />
    </React.Suspense>
  );
}
