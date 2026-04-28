import { useEffect, useState } from "react";

import { applyWorkspace, getDefaultRelayUrl } from "@/shared/api/tauri";

import {
  loadActiveWorkspaceId,
  loadWorkspaces,
  saveActiveWorkspaceId,
} from "./workspaceStorage";

type WorkspaceInitResult =
  | { isReady: true; needsSetup: false }
  | { isReady: false; needsSetup: true; defaultRelayUrl: string }
  | { isReady: false; needsSetup: false };

/**
 * Runs once on mount. Loads the active workspace from localStorage
 * and calls the Tauri backend to apply the workspace config
 * (keys, relay URL, token).
 *
 * Returns a discriminated union — only render the app after the
 * workspace is applied. When `needsSetup` is true, the caller
 * should show a first-run welcome screen.
 */
export function useWorkspaceInit(): WorkspaceInitResult {
  const [result, setResult] = useState<WorkspaceInitResult>({
    isReady: false,
    needsSetup: false,
  });

  useEffect(() => {
    let cancelled = false;

    async function init() {
      const workspaces = loadWorkspaces();

      if (workspaces.length === 0) {
        // No workspaces at all — fetch the build default relay URL
        // so the welcome screen can pre-fill it.
        try {
          const defaultRelayUrl = await getDefaultRelayUrl();
          if (!cancelled) {
            setResult({ isReady: false, needsSetup: true, defaultRelayUrl });
          }
        } catch {
          // If we can't get the default, fall back to localhost
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

      // Determine active workspace
      let activeId = loadActiveWorkspaceId();
      if (!activeId || !workspaces.find((w) => w.id === activeId)) {
        activeId = workspaces[0].id;
        saveActiveWorkspaceId(activeId);
      }

      const active = workspaces.find((w) => w.id === activeId);
      if (!active) {
        if (!cancelled) {
          setResult({ isReady: true, needsSetup: false });
        }
        return;
      }

      // Apply workspace config to the Tauri backend
      try {
        await applyWorkspace(active.relayUrl, active.nsec, active.token);
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
  }, []);

  return result;
}
