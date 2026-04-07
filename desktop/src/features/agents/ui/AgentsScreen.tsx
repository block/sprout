import * as React from "react";

import { ChatHeader } from "@/features/chat/ui/ChatHeader";
import { ViewLoadingFallback } from "@/shared/ui/ViewLoadingFallback";

const AgentsView = React.lazy(async () => {
  const module = await import("@/features/agents/ui/AgentsView");
  return { default: module.AgentsView };
});

export function AgentsScreen() {
  return (
    <>
      <ChatHeader
        description="Create local ACP workers, mint agent tokens, and monitor the relay-visible agent directory."
        mode="agents"
        title="Agents"
      />

      <React.Suspense
        fallback={<ViewLoadingFallback label="Loading agents..." />}
      >
        <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
          <AgentsView />
        </div>
      </React.Suspense>
    </>
  );
}
