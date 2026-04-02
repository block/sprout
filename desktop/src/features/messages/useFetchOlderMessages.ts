import { useCallback, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";

import { relayClient } from "@/shared/api/relayClient";
import type { Channel, RelayEvent } from "@/shared/api/types";
import { sortMessages } from "@/features/messages/hooks";

const OLDER_MESSAGES_BATCH_SIZE = 100;

export function useFetchOlderMessages(channel: Channel | null) {
  const queryClient = useQueryClient();
  const channelId = channel?.id ?? null;
  const [isFetchingOlder, setIsFetchingOlder] = useState(false);
  const [hasOlderMessages, setHasOlderMessages] = useState(true);
  const previousChannelIdRef = useRef(channelId);
  const isFetchingOlderRef = useRef(false);
  const hasOlderMessagesRef = useRef(true);

  if (previousChannelIdRef.current !== channelId) {
    previousChannelIdRef.current = channelId;
    hasOlderMessagesRef.current = true;
    setHasOlderMessages(true);
  }

  const fetchOlder = useCallback(async () => {
    if (
      !channelId ||
      isFetchingOlderRef.current ||
      !hasOlderMessagesRef.current
    ) {
      return;
    }

    const queryKey = ["channel-messages", channelId] as const;
    const currentMessages =
      queryClient.getQueryData<RelayEvent[]>(queryKey) ?? [];
    if (currentMessages.length === 0) {
      return;
    }

    const oldestTimestamp = currentMessages[0].created_at;
    isFetchingOlderRef.current = true;
    setIsFetchingOlder(true);

    try {
      const olderMessages = await relayClient.fetchChannelHistoryBefore(
        channelId,
        oldestTimestamp,
        OLDER_MESSAGES_BATCH_SIZE,
      );

      if (olderMessages.length < OLDER_MESSAGES_BATCH_SIZE) {
        hasOlderMessagesRef.current = false;
        setHasOlderMessages(false);
      }

      if (olderMessages.length > 0) {
        queryClient.setQueryData<RelayEvent[]>(queryKey, (current = []) =>
          sortMessages([...current, ...olderMessages]),
        );
      }
    } catch (error) {
      console.error("Failed to fetch older messages", channelId, error);
    } finally {
      isFetchingOlderRef.current = false;
      setIsFetchingOlder(false);
    }
  }, [channelId, queryClient]);

  return { fetchOlder, isFetchingOlder, hasOlderMessages };
}
