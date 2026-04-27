import { useEffect, useState } from "react";

import { applyWorkspace } from "@/shared/api/tauri";

import {
  loadActiveWorkspaceId,
  loadWorkspaces,
  migrateFromSingleWorkspace,
  saveActiveWorkspaceId,
} from "./workspaceStorage";

/**
 * Runs once on mount. Loads the active workspace from localStorage,
 * performs legacy migration if needed, and calls the Tauri backend
 * to apply the workspace config (keys, relay URL, token).
 *
 * Returns `{ isReady }` — only render the app after workspace is applied.
 */
export function useWorkspaceInit(): { isReady: boolean } {
  const [isReady, setIsReady] = useState(false);

  useEffect(() => {
    let cancelled = false;

    async function init() {
      // Run legacy migration if this is a first-time user
      let workspaces = loadWorkspaces();
      if (workspaces.length === 0) {
        workspaces = await migrateFromSingleWorkspace();
      }

      if (workspaces.length === 0) {
        // No workspaces at all — let the app proceed so onboarding can handle it
        if (!cancelled) {
          setIsReady(true);
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
          setIsReady(true);
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
        setIsReady(true);
      }
    }

    void init();

    return () => {
      cancelled = true;
    };
  }, []);

  return { isReady };
}
