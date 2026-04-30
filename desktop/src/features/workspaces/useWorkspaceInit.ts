import { useEffect, useRef, useState } from "react";

import { relayClient } from "@/shared/api/relayClient";
import { applyWorkspace, getDefaultRelayUrl } from "@/shared/api/tauri";
import { resetMediaCaches } from "@/shared/lib/mediaUrl";
import { clearSearchHitEventCache } from "@/app/navigation/searchHitEventCache";
import { clearAllDrafts } from "@/features/messages/lib/useDrafts";
import { resetAgentObserverStore } from "@/features/agents/observerRelayStore";
import { resetStore as resetPresenceStore } from "@/features/presence/presenceStore";

import type { Workspace } from "./types";

/**
 * Tear down all workspace-scoped module singletons so the new
 * workspace starts with a clean slate. If you add a new module-level
 * cache or singleton that holds workspace data, add its reset here.
 * See AGENTS.md "Workspace Switching" for the full contract.
 */
function resetWorkspaceState(): void {
  relayClient.disconnect();
  resetAgentObserverStore();
  resetMediaCaches();
  clearSearchHitEventCache();
  clearAllDrafts();
  resetPresenceStore();
}

type WorkspaceInitResult =
  | { isReady: true; needsSetup: false }
  | { isReady: false; needsSetup: true; defaultRelayUrl: string }
  | { isReady: false; needsSetup: false };

/**
 * Applies the active workspace config to the Tauri backend and resets
 * all workspace-scoped module singletons when the workspace changes.
 *
 * Returns a discriminated union — only render the app after the
 * workspace is applied. When `needsSetup` is true, the caller
 * should show a first-run welcome screen.
 */
export function useWorkspaceInit(
  activeWorkspace: Workspace | null,
): WorkspaceInitResult {
  const [result, setResult] = useState<WorkspaceInitResult>({
    isReady: false,
    needsSetup: false,
  });

  // Track whether this is the initial mount or a workspace switch.
  // On the initial mount we skip resetting singletons (they're fresh).
  const hasInitializedRef = useRef(false);

  // biome-ignore lint/correctness/useExhaustiveDependencies: we intentionally depend on specific properties (id/relayUrl/token) — depending on the whole object would trigger resets on name-only changes
  useEffect(() => {
    let cancelled = false;

    async function init() {
      if (!activeWorkspace) {
        // No workspace — need setup
        try {
          const defaultRelayUrl = await getDefaultRelayUrl();
          if (!cancelled) {
            setResult({ isReady: false, needsSetup: true, defaultRelayUrl });
          }
        } catch {
          if (!cancelled) {
            setResult({
              isReady: false,
              needsSetup: true,
              defaultRelayUrl: "ws://localhost:3000",
            });
          }
        }
        return;
      }

      // On workspace switch (not initial mount), reset module singletons
      // so the new tree starts with a clean slate.
      if (hasInitializedRef.current) {
        resetWorkspaceState();
      }
      hasInitializedRef.current = true;

      // Show loading gate while we apply the new workspace config
      setResult({ isReady: false, needsSetup: false });

      // Apply workspace config to the Tauri backend
      try {
        await applyWorkspace(
          activeWorkspace.relayUrl,
          activeWorkspace.nsec,
          activeWorkspace.token,
        );
      } catch (error) {
        console.error("Failed to apply workspace to backend:", error);
      }

      if (!cancelled) {
        setResult({ isReady: true, needsSetup: false });
      }
    }

    void init();

    return () => {
      cancelled = true;
    };
  }, [activeWorkspace?.id, activeWorkspace?.relayUrl, activeWorkspace?.token]);

  return result;
}
