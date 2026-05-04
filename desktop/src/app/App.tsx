import { getCurrentWindow } from "@tauri-apps/api/window";
import { useQueryClient } from "@tanstack/react-query";
import { lazy, Suspense, useCallback, useLayoutEffect } from "react";

import { AppLoadingGate } from "@/app/AppLoadingGate";
import {
  DetachedAgentSessionView,
  getDetachedAgentSessionParams,
} from "@/features/agents/ui/DetachedAgentSessionView";
import { useWorkspaceInit } from "@/features/workspaces/useWorkspaceInit";
import { useWorkspaces } from "@/features/workspaces/useWorkspaces";
import { WelcomeSetup } from "@/features/workspaces/ui/WelcomeSetup";

const LazyMainAppReady = lazy(async () => {
  const module = await import("@/app/MainAppReady");
  return { default: module.MainAppReady };
});

function AppReady() {
  const detachedAgentSessionParams = getDetachedAgentSessionParams();

  if (detachedAgentSessionParams) {
    return <DetachedAgentSessionView {...detachedAgentSessionParams} />;
  }

  return (
    <Suspense fallback={<AppLoadingGate />}>
      <LazyMainAppReady />
    </Suspense>
  );
}

export function App() {
  useLayoutEffect(() => {
    void getCurrentWindow().show();
  }, []);

  const queryClient = useQueryClient();
  const { activeWorkspace, reinitKey } = useWorkspaces();
  const workspace = useWorkspaceInit(activeWorkspace);

  // Composite key: changes when workspace ID changes OR when
  // the active workspace's config is updated (relayUrl/token).
  const workspaceKey = `${activeWorkspace?.id ?? "none"}-${reinitKey}`;

  // Clear stale React Query cache synchronously when workspace changes.
  // useLayoutEffect fires before child useEffect hooks, preventing stale
  // data from being served to the new workspace's components.
  // biome-ignore lint/correctness/useExhaustiveDependencies: workspaceKey drives the re-run intentionally
  useLayoutEffect(() => {
    queryClient.clear();
  }, [workspaceKey, queryClient]);

  const handleSetupComplete = useCallback(() => {
    // Force a full reload so useWorkspaces re-initializes from localStorage.
    // This only runs once — during first-run setup when no workspace existed.
    window.location.reload();
  }, []);

  // Show welcome setup for first-run users with no workspaces
  if (workspace.needsSetup) {
    return (
      <WelcomeSetup
        defaultRelayUrl={workspace.defaultRelayUrl}
        onComplete={handleSetupComplete}
      />
    );
  }

  // Wait for workspace config to be applied to the backend before
  // rendering anything that connects to the relay.
  if (!workspace.isReady) {
    return <AppLoadingGate />;
  }

  return <AppReady key={workspaceKey} />;
}
