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
        description="Choose personas from Persona Catalog, create local ACP workers, and monitor the relay-visible agent directory."
        mode="agents"
        overlaysContent
        title="Agents"
      />

      <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
        <React.Suspense fallback={<ViewLoadingFallback kind="agents" />}>
          <AgentsView />
        </React.Suspense>
      </div>
    </>
  );
}
