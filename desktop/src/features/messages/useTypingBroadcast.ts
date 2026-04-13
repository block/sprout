import { useCallback, useRef } from "react";

import { relayClient } from "@/shared/api/relayClient";

const TYPING_SEND_INTERVAL_MS = 3_000;

/**
 * Publishes kind:20002 typing indicators for the current user,
 * throttled to at most once every 3 seconds per channel.
 */
export function useTypingBroadcast(
  channelId: string | null | undefined,
  threadRootId?: string | null,
) {
  const lastSentRef = useRef(0);
  const lastScopeRef = useRef(`${channelId ?? ""}:${threadRootId ?? ""}`);
  const channelIdRef = useRef(channelId);
  const threadRootIdRef = useRef(threadRootId ?? null);
  channelIdRef.current = channelId;
  threadRootIdRef.current = threadRootId ?? null;

  const notifyTyping = useCallback(() => {
    const id = channelIdRef.current;
    if (!id) {
      return;
    }

    // Reset throttle when channel or thread scope changes.
    const scope = `${id}:${threadRootIdRef.current ?? ""}`;
    if (lastScopeRef.current !== scope) {
      lastScopeRef.current = scope;
      lastSentRef.current = 0;
    }

    const now = Date.now();
    if (now - lastSentRef.current < TYPING_SEND_INTERVAL_MS) {
      return;
    }

    lastSentRef.current = now;
    relayClient.sendTypingIndicator(id, threadRootIdRef.current).catch(() => {});
  }, []);

  return notifyTyping;
}
