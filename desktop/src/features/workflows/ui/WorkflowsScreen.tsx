import * as React from "react";

import type { Channel } from "@/shared/api/types";
import { ViewLoadingFallback } from "@/shared/ui/ViewLoadingFallback";

const WorkflowsView = React.lazy(async () => {
  const module = await import("@/features/workflows/ui/WorkflowsView");
  return { default: module.WorkflowsView };
});

type WorkflowsScreenProps = {
  channels: Channel[];
};

/**
 * Full height below the app sidebar — no duplicate page header (the list column
 * shows “Workflows”). Matches a 3-column wireframe: list | diagram | properties.
 */
export function WorkflowsScreen({ channels }: WorkflowsScreenProps) {
  return (
    <React.Suspense
      fallback={<ViewLoadingFallback label="Loading workflows..." />}
    >
      <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden pt-2">
        <WorkflowsView channels={channels} />
      </div>
    </React.Suspense>
  );
}
