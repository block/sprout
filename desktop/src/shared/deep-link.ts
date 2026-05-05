import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { toast } from "sonner";

import type { Workspace } from "@/features/workspaces/types";
import {
  deriveWorkspaceName,
  normalizeRelayUrl,
} from "@/features/workspaces/workspaceStorage";

export interface DeepLinkDeps {
  addWorkspace: (workspace: Workspace) => string;
  switchWorkspace: (id: string) => void;
  reconnectWorkspace: () => void;
}

/**
 * Register listeners for deep-link events emitted by the Rust backend.
 *
 * When a `sprout://connect?relay=<url>` link is opened, the handler
 * adds a workspace for the relay (deduplicating by URL) and switches
 * to it. Returns an unlisten function to tear down all listeners.
 */
export function listenForDeepLinks(deps: DeepLinkDeps): Promise<UnlistenFn> {
  return listen<string>("deep-link-connect", (event) => {
    const relayUrl = normalizeRelayUrl(event.payload);
    const name = deriveWorkspaceName(relayUrl);
    const id = deps.addWorkspace({
      id: crypto.randomUUID(),
      name,
      relayUrl,
      addedAt: new Date().toISOString(),
    });
    deps.switchWorkspace(id);
    // If addWorkspace returned the already-active workspace (same relay URL),
    // switchWorkspace is a no-op — force re-init so the connection refreshes.
    deps.reconnectWorkspace();
    toast.success(`Connected to ${name}`);
  });
}
