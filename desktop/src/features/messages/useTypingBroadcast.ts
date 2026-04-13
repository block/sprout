import { useCallback, useRef } from "react";

import { relayClient } from "@/shared/api/relayClient";

const TYPING_SEND_INTERVAL_MS = 3_000;

/**
 * Publishes kind:20002 typing indicators for the current user,
 * throttled to at most once every 3 seconds per channel.
 *
 * @param replyParentEventId When set, adds `["e", id, "", "reply"]` so receivers can
 *   scope indicators to a composer (main vs sub-thread).
 */
export function useTypingBroadcast(
  channelId: string | null | undefined,
  replyParentEventId?: string | null,
) {
  const lastSentRef = useRef(0);
  const lastChannelRef = useRef(channelId);
  const lastScopeRef = useRef<string | null | undefined>(undefined);
  const channelIdRef = useRef(channelId);
  const replyParentRef = useRef(replyParentEventId);
  channelIdRef.current = channelId;
  replyParentRef.current = replyParentEventId;

  const notifyTyping = useCallback(() => {
    const id = channelIdRef.current;
    if (!id) {
      return;
    }

    // Reset throttle when channel or reply scope changes.
    if (
      lastChannelRef.current !== id ||
      lastScopeRef.current !== replyParentRef.current
    ) {
      lastChannelRef.current = id;
      lastScopeRef.current = replyParentRef.current;
      lastSentRef.current = 0;
    }

    const now = Date.now();
    if (now - lastSentRef.current < TYPING_SEND_INTERVAL_MS) {
      return;
    }

    lastSentRef.current = now;
    relayClient
      .sendTypingIndicator(id, replyParentRef.current ?? undefined)
      .catch(() => {});
  }, []);

  return notifyTyping;
}
