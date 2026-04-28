import { useCallback, useMemo, useRef, useState } from "react";

import type { Workspace } from "./types";
import {
  loadActiveWorkspaceId,
  loadWorkspaces,
  saveActiveWorkspaceId,
  saveWorkspaces,
} from "./workspaceStorage";

export type UseWorkspacesReturn = {
  workspaces: Workspace[];
  activeWorkspace: Workspace | null;
  /** Add a workspace, deduplicating by relayUrl. Returns the final ID in the list. */
  addWorkspace: (workspace: Workspace) => string;
  removeWorkspace: (id: string) => void;
  switchWorkspace: (id: string) => void;
  updateWorkspace: (
    id: string,
    updates: Partial<Pick<Workspace, "name" | "relayUrl" | "token">>,
  ) => void;
};

const WORKSPACE_SWITCHING_KEY = "sprout.desktop.workspace-switching";

export function useWorkspaces(): UseWorkspacesReturn {
  const [workspaces, setWorkspacesState] =
    useState<Workspace[]>(loadWorkspaces);
  const [activeId, setActiveId] = useState<string | null>(
    loadActiveWorkspaceId,
  );
  const workspacesRef = useRef(workspaces);
  workspacesRef.current = workspaces;

  const activeWorkspace = useMemo(
    () => workspaces.find((w) => w.id === activeId) ?? workspaces[0] ?? null,
    [workspaces, activeId],
  );

  const addWorkspace = useCallback((workspace: Workspace): string => {
    const existing = workspacesRef.current.find(
      (w) => w.relayUrl === workspace.relayUrl,
    );
    const resolvedId = existing?.id ?? workspace.id;
    setWorkspacesState((prev) => {
      const dup = prev.find((w) => w.relayUrl === workspace.relayUrl);
      let next: Workspace[];
      if (dup) {
        next = prev.map((w) =>
          w.id === dup.id
            ? {
                ...w,
                name: workspace.name || w.name,
                token: workspace.token ?? w.token,
                nsec: workspace.nsec ?? w.nsec,
                pubkey: workspace.pubkey ?? w.pubkey,
              }
            : w,
        );
      } else {
        next = [...prev, workspace];
      }
      saveWorkspaces(next);
      return next;
    });
    return resolvedId;
  }, []);

  const removeWorkspace = useCallback(
    (id: string) => {
      setWorkspacesState((prev) => {
        // Never allow removing the last workspace
        if (prev.length <= 1) {
          return prev;
        }
        const next = prev.filter((w) => w.id !== id);
        saveWorkspaces(next);

        // If removing the active workspace, switch to first remaining
        if (activeId === id && next.length > 0) {
          setActiveId(next[0].id);
          saveActiveWorkspaceId(next[0].id);
          sessionStorage.setItem(WORKSPACE_SWITCHING_KEY, "1");
          window.location.reload();
        }

        return next;
      });
    },
    [activeId],
  );

  const switchWorkspace = useCallback(
    (id: string) => {
      if (id === activeId) {
        return;
      }
      saveActiveWorkspaceId(id);
      sessionStorage.setItem(WORKSPACE_SWITCHING_KEY, "1");
      window.location.reload();
    },
    [activeId],
  );

  const updateWorkspace = useCallback(
    (
      id: string,
      updates: Partial<Pick<Workspace, "name" | "relayUrl" | "token">>,
    ) => {
      setWorkspacesState((prev) => {
        // Prevent duplicate relay URLs across workspaces
        if (
          updates.relayUrl &&
          prev.some((w) => w.id !== id && w.relayUrl === updates.relayUrl)
        ) {
          return prev;
        }
        const next = prev.map((w) => (w.id === id ? { ...w, ...updates } : w));
        saveWorkspaces(next);
        return next;
      });
      // If the active workspace's relay URL or token changed, reload to reconnect
      if (
        id === activeId &&
        (updates.relayUrl || updates.token !== undefined)
      ) {
        sessionStorage.setItem(WORKSPACE_SWITCHING_KEY, "1");
        window.location.reload();
      }
    },
    [activeId],
  );

  return {
    workspaces,
    activeWorkspace,
    addWorkspace,
    removeWorkspace,
    switchWorkspace,
    updateWorkspace,
  };
}
