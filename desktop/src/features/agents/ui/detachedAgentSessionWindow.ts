import { isTauri } from "@tauri-apps/api/core";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

type DetachedAgentSessionWindowInput = {
  agentName?: string;
  agentPubkey: string;
  channelId?: string | null;
};

const DETACHED_AGENT_SESSION_WINDOW_PREFIX = "agent-session";

function detachedAgentSessionLabel(
  agentPubkey: string,
  channelId?: string | null,
) {
  const channelPart = channelId ? `-${channelId}` : "";
  return `${DETACHED_AGENT_SESSION_WINDOW_PREFIX}-${agentPubkey}${channelPart}`;
}

function detachedAgentSessionUrl({
  agentPubkey,
  channelId,
}: DetachedAgentSessionWindowInput) {
  const params = new URLSearchParams({
    detachedAgentSession: "1",
    agentPubkey,
  });

  if (channelId) {
    params.set("channelId", channelId);
  }

  return `/?${params.toString()}`;
}

export async function openDetachedAgentSessionWindow(
  input: DetachedAgentSessionWindowInput,
) {
  const url = detachedAgentSessionUrl(input);

  if (!isTauri()) {
    window.open(url, "_blank", "noopener,noreferrer");
    return;
  }

  const label = detachedAgentSessionLabel(input.agentPubkey, input.channelId);
  const existingWindow = await WebviewWindow.getByLabel(label);
  if (existingWindow) {
    await existingWindow.close();
  }

  const detachedWindow = new WebviewWindow(label, {
    center: true,
    height: 760,
    minHeight: 480,
    minWidth: 420,
    title: input.agentName ? `${input.agentName} activity` : "Agent activity",
    url,
    width: 560,
  });

  await new Promise<void>((resolve, reject) => {
    const unlistenCreated = detachedWindow.once("tauri://created", () => {
      cleanup();
      resolve();
    });
    const unlistenError = detachedWindow.once("tauri://error", (event) => {
      cleanup();
      reject(
        new Error(String(event.payload ?? "Failed to open agent window.")),
      );
    });

    function cleanup() {
      void unlistenCreated.then((unlisten) => unlisten());
      void unlistenError.then((unlisten) => unlisten());
    }
  });
}
