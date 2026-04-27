import { useCallback, useMemo, useState } from "react";

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
  addWorkspace: (workspace: Workspace) => void;
  removeWorkspace: (id: string) => void;
  switchWorkspace: (id: string) => void;
  renameWorkspace: (id: string, name: string) => void;
  setWorkspaces: (workspaces: Workspace[]) => void;
  setActiveWorkspaceId: (id: string) => void;
};

export function useWorkspaces(): UseWorkspacesReturn {
  const [workspaces, setWorkspacesState] =
    useState<Workspace[]>(loadWorkspaces);
  const [activeId, setActiveId] = useState<string | null>(
    loadActiveWorkspaceId,
  );

  const activeWorkspace = useMemo(
    () => workspaces.find((w) => w.id === activeId) ?? workspaces[0] ?? null,
    [workspaces, activeId],
  );

  const setWorkspaces = useCallback((next: Workspace[]) => {
    setWorkspacesState(next);
    saveWorkspaces(next);
  }, []);

  const setActiveWorkspaceId = useCallback((id: string) => {
    setActiveId(id);
    saveActiveWorkspaceId(id);
  }, []);

  const addWorkspace = useCallback((workspace: Workspace) => {
    setWorkspacesState((prev) => {
      // Dedup by relayUrl: update creds if same URL exists
      const existing = prev.find((w) => w.relayUrl === workspace.relayUrl);
      let next: Workspace[];
      if (existing) {
        next = prev.map((w) =>
          w.id === existing.id
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
  }, []);

  const removeWorkspace = useCallback(
    (id: string) => {
      setWorkspacesState((prev) => {
        const next = prev.filter((w) => w.id !== id);
        saveWorkspaces(next);

        // If removing the active workspace, switch to first remaining
        if (activeId === id && next.length > 0) {
          setActiveId(next[0].id);
          saveActiveWorkspaceId(next[0].id);
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
      window.location.reload();
    },
    [activeId],
  );

  const renameWorkspace = useCallback((id: string, name: string) => {
    setWorkspacesState((prev) => {
      const next = prev.map((w) => (w.id === id ? { ...w, name } : w));
      saveWorkspaces(next);
      return next;
    });
  }, []);

  return {
    workspaces,
    activeWorkspace,
    addWorkspace,
    removeWorkspace,
    switchWorkspace,
    renameWorkspace,
    setWorkspaces,
    setActiveWorkspaceId,
  };
}
