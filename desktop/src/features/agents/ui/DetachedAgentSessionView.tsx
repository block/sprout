import { getCurrentWindow } from "@tauri-apps/api/window";
import { useQuery } from "@tanstack/react-query";
import { X } from "lucide-react";
import * as React from "react";

import { useManagedAgentObserverBridge } from "@/features/agents/observerRelayStore";
import { ManagedAgentSessionPanel } from "@/features/agents/ui/ManagedAgentSessionPanel";
import { listManagedAgents } from "@/shared/api/tauri";
import { Button } from "@/shared/ui/button";
import { Skeleton } from "@/shared/ui/skeleton";

type DetachedAgentSessionParams = {
  agentPubkey: string | null;
  channelId: string | null;
};

type DetachedAgentSessionErrorBoundaryState = {
  errorMessage: string | null;
};

export function getDetachedAgentSessionParams(): DetachedAgentSessionParams | null {
  if (typeof window === "undefined") {
    return null;
  }

  const params = new URL(window.location.href).searchParams;
  if (params.get("detachedAgentSession") !== "1") {
    return null;
  }

  return {
    agentPubkey: params.get("agentPubkey"),
    channelId: params.get("channelId"),
  };
}

export function DetachedAgentSessionView(props: DetachedAgentSessionParams) {
  return (
    <DetachedAgentSessionErrorBoundary>
      <DetachedAgentSessionContent {...props} />
    </DetachedAgentSessionErrorBoundary>
  );
}

class DetachedAgentSessionErrorBoundary extends React.Component<
  React.PropsWithChildren,
  DetachedAgentSessionErrorBoundaryState
> {
  state: DetachedAgentSessionErrorBoundaryState = { errorMessage: null };

  static getDerivedStateFromError(error: unknown) {
    return {
      errorMessage:
        error instanceof Error
          ? error.message
          : "Detached agent activity failed to render.",
    };
  }

  render() {
    if (this.state.errorMessage) {
      return (
        <main className="flex h-dvh min-h-0 flex-col bg-background text-foreground">
          <div className="flex min-h-0 flex-1 flex-col items-center justify-center px-6 text-center">
            <p className="text-sm font-medium">Detached activity crashed</p>
            <p className="mt-1 max-w-sm text-sm text-muted-foreground">
              {this.state.errorMessage}
            </p>
          </div>
        </main>
      );
    }

    return this.props.children;
  }
}

function DetachedAgentSessionContent({
  agentPubkey,
  channelId,
}: DetachedAgentSessionParams) {
  const agentsQuery = useQuery({
    queryKey: ["detached-agent-session", "managed-agents"],
    queryFn: listManagedAgents,
    refetchInterval: 5_000,
    staleTime: 5_000,
  });
  useManagedAgentObserverBridge(agentsQuery.data ?? []);
  const agent = React.useMemo(
    () =>
      agentPubkey
        ? (agentsQuery.data?.find(
            (candidate) =>
              candidate.pubkey.toLowerCase() === agentPubkey.toLowerCase(),
          ) ?? null)
        : null,
    [agentPubkey, agentsQuery.data],
  );

  React.useLayoutEffect(() => {
    void getCurrentWindow().show();
  }, []);

  async function closeWindow() {
    await getCurrentWindow().close();
  }

  return (
    <main className="flex h-dvh min-h-0 flex-col bg-background text-foreground">
      <header
        className="flex shrink-0 items-center justify-between border-b border-border/70 px-4 py-3"
        data-tauri-drag-region
      >
        <div className="min-w-0">
          <h1 className="truncate text-sm font-semibold tracking-tight">
            {agent?.name ?? "Agent activity"}
          </h1>
          <p className="truncate text-xs text-muted-foreground">
            Detached live session
          </p>
        </div>
        <Button
          aria-label="Close detached agent window"
          onClick={() => {
            void closeWindow();
          }}
          size="icon"
          type="button"
          variant="ghost"
        >
          <X className="h-4 w-4" />
        </Button>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto p-4">
        {!agentPubkey ? (
          <DetachedWindowMessage
            description="This detached window is missing an agent identifier."
            title="No agent selected"
          />
        ) : agentsQuery.isLoading ? (
          <DetachedWindowSkeleton />
        ) : agent ? (
          <ManagedAgentSessionPanel
            agent={agent}
            channelId={channelId}
            className="border-0 bg-transparent p-0 shadow-none"
            enableInlineNavigation={false}
            emptyDescription={`Mention ${agent.name} to see its work here.`}
            showHeader
            showRaw={false}
          />
        ) : (
          <DetachedWindowMessage
            description="The selected managed agent is no longer available."
            title="Agent not found"
          />
        )}
      </div>
    </main>
  );
}

function DetachedWindowSkeleton() {
  return (
    <div className="space-y-4">
      <Skeleton className="h-16 rounded-xl" />
      <Skeleton className="h-28 rounded-xl" />
      <Skeleton className="h-28 rounded-xl" />
    </div>
  );
}

function DetachedWindowMessage({
  description,
  title,
}: {
  description: string;
  title: string;
}) {
  return (
    <div className="flex min-h-64 flex-col items-center justify-center text-center">
      <p className="text-sm font-medium">{title}</p>
      <p className="mt-1 max-w-sm text-sm text-muted-foreground">
        {description}
      </p>
    </div>
  );
}
