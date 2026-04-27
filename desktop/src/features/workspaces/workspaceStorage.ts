import { getIdentity, getNsec, getRelayWsUrl } from "@/shared/api/tauri";

import type { Workspace } from "./types";

const WORKSPACES_KEY = "sprout-workspaces";
const ACTIVE_WORKSPACE_KEY = "sprout-active-workspace-id";

export function loadWorkspaces(): Workspace[] {
  try {
    const raw = localStorage.getItem(WORKSPACES_KEY);
    if (!raw) {
      return [];
    }
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) {
      return [];
    }
    return parsed as Workspace[];
  } catch {
    return [];
  }
}

export function saveWorkspaces(workspaces: Workspace[]): void {
  localStorage.setItem(WORKSPACES_KEY, JSON.stringify(workspaces));
}

export function loadActiveWorkspaceId(): string | null {
  return localStorage.getItem(ACTIVE_WORKSPACE_KEY);
}

export function saveActiveWorkspaceId(id: string): void {
  localStorage.setItem(ACTIVE_WORKSPACE_KEY, id);
}

/**
 * On first load, if no workspaces exist, read the current relay URL and
 * identity to create the first workspace entry automatically. Ensures
 * existing single-workspace users get a seamless migration.
 */
export async function migrateFromSingleWorkspace(): Promise<Workspace[]> {
  const existing = loadWorkspaces();
  if (existing.length > 0) {
    return existing;
  }

  try {
    const [relayUrl, identity, nsec] = await Promise.all([
      getRelayWsUrl(),
      getIdentity(),
      getNsec(),
    ]);

    const workspace: Workspace = {
      id: crypto.randomUUID(),
      name: deriveWorkspaceName(relayUrl),
      relayUrl,
      nsec,
      pubkey: identity.pubkey,
      addedAt: new Date().toISOString(),
    };

    const workspaces = [workspace];
    saveWorkspaces(workspaces);
    saveActiveWorkspaceId(workspace.id);
    return workspaces;
  } catch (error) {
    console.error("Failed to migrate single workspace:", error);
    return [];
  }
}

export function deriveWorkspaceName(relayUrl: string): string {
  try {
    const url = new URL(
      relayUrl.replace("ws://", "http://").replace("wss://", "https://"),
    );
    const host = url.hostname;
    if (host === "localhost" || host === "127.0.0.1") {
      return "Local Dev";
    }
    // Use the first subdomain segment or the domain itself
    const parts = host.split(".");
    if (parts.length >= 2) {
      return parts[0] === "relay" ? parts[1] : parts[0];
    }
    return host;
  } catch {
    return "Workspace";
  }
}
