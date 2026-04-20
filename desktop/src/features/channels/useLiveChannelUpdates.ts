import * as React from "react";
import { useQueryClient } from "@tanstack/react-query";

import { updateChannelLastMessageAt } from "@/features/channels/lib/channelCache";
import { channelsQueryKey } from "@/features/channels/hooks";
import { mergeTimelineCacheMessages } from "@/features/messages/hooks";
import { channelMessagesKey } from "@/features/messages/lib/messageQueryKeys";
import { getChannelIdFromTags } from "@/features/messages/lib/threading";
import { relayClient } from "@/shared/api/relayClient";
import type { Channel, RelayEvent } from "@/shared/api/types";

export type UseLiveChannelUpdatesOptions = {
  currentPubkey?: string;
  onLiveMention?: () => void;
};

const LIVE_MENTION_SUBSCRIPTION_RETRY_BASE_MS = 1_000;
const LIVE_MENTION_SUBSCRIPTION_RETRY_MAX_MS = 30_000;

function getMessageTimestamp(event: RelayEvent) {
  return new Date(event.created_at * 1_000).toISOString();
}

function isExternalMentionEvent(event: RelayEvent, currentPubkey: string) {
  return (
    currentPubkey.length > 0 && event.pubkey.toLowerCase() !== currentPubkey
  );
}

function rememberMentionEvent(
  seenMentionEventIds: Set<string>,
  eventId: string,
): boolean {
  if (seenMentionEventIds.has(eventId)) {
    return false;
  }

  seenMentionEventIds.add(eventId);
  if (seenMentionEventIds.size > 200) {
    const oldestEventId = seenMentionEventIds.values().next().value;
    if (oldestEventId) {
      seenMentionEventIds.delete(oldestEventId);
    }
  }

  return true;
}

export function useLiveChannelUpdates(
  channels: Channel[],
  activeChannelId: string | null,
  options: UseLiveChannelUpdatesOptions = {},
) {
  const queryClient = useQueryClient();
  const normalizedCurrentPubkey =
    options.currentPubkey?.trim().toLowerCase() ?? "";
  const seenMentionEventIdsRef = React.useRef(new Set<string>());
  const liveChannelIds = React.useMemo(
    () =>
      new Set(
        channels
          .filter((channel) => channel.channelType !== "forum")
          .map((channel) => channel.id),
      ),
    [channels],
  );
  // Effect dep uses a primitive so refetches that produce new Set refs with
  // identical contents don't churn subscriptions. The Set is still handy for
  // closure reads via useEffectEvent.
  const hasLiveChannels = liveChannelIds.size > 0;

  const handleIncomingMessage = React.useEffectEvent((event: RelayEvent) => {
    const channelId = getChannelIdFromTags(event.tags);
    if (!channelId || channelId === activeChannelId) {
      return;
    }

    if (!liveChannelIds.has(channelId)) {
      void queryClient.invalidateQueries({ queryKey: channelsQueryKey });
      return;
    }

    const messageTimestamp = getMessageTimestamp(event);

    updateChannelLastMessageAt(queryClient, channelId, messageTimestamp);
    queryClient.setQueryData<RelayEvent[]>(
      channelMessagesKey(channelId),
      (current) => {
        if (!current) {
          return current;
        }

        return mergeTimelineCacheMessages(current, event);
      },
    );
  });

  const handleMentionEvent = React.useEffectEvent((event: RelayEvent) => {
    if (!isExternalMentionEvent(event, normalizedCurrentPubkey)) {
      return;
    }

    if (!rememberMentionEvent(seenMentionEventIdsRef.current, event.id)) {
      return;
    }

    options.onLiveMention?.();
  });

  React.useEffect(() => {
    return relayClient.subscribeToReconnects(() => {
      void queryClient.invalidateQueries({ queryKey: channelsQueryKey });
    });
  }, [queryClient]);

  React.useEffect(() => {
    if (!hasLiveChannels) {
      return;
    }

    let isDisposed = false;
    let cleanup: (() => Promise<void>) | undefined;

    relayClient
      .subscribeToAllStreamMessages((event) => {
        if (!isDisposed) {
          handleIncomingMessage(event);
        }
      })
      .then((dispose) => {
        if (isDisposed) {
          void dispose();
          return;
        }

        cleanup = dispose;
      })
      .catch((error) => {
        console.error("Failed to subscribe to unread channel updates", error);
      });

    return () => {
      isDisposed = true;
      if (cleanup) {
        void cleanup();
      }
    };
  }, [hasLiveChannels]);

  React.useEffect(() => {
    if (!options.onLiveMention || normalizedCurrentPubkey.length === 0) {
      return;
    }

    let isDisposed = false;
    let cleanup: (() => Promise<void>) | undefined;
    let retryTimeout: ReturnType<typeof setTimeout> | undefined;
    let retryAttempt = 0;

    const subscribe = async () => {
      try {
        const dispose = await relayClient.subscribeToMentionsForPubkey(
          normalizedCurrentPubkey,
          (event) => {
            if (!isDisposed) {
              handleMentionEvent(event);
            }
          },
        );
        if (isDisposed) {
          void dispose();
          return;
        }
        cleanup = dispose;
        retryAttempt = 0;
      } catch (error) {
        if (isDisposed) {
          return;
        }
        const delayMs = Math.min(
          LIVE_MENTION_SUBSCRIPTION_RETRY_BASE_MS * 2 ** retryAttempt,
          LIVE_MENTION_SUBSCRIPTION_RETRY_MAX_MS,
        );
        retryAttempt += 1;
        console.error(
          `Failed to subscribe to Home mention updates; retrying in ${delayMs}ms`,
          error,
        );
        retryTimeout = window.setTimeout(() => {
          retryTimeout = undefined;
          void subscribe();
        }, delayMs);
      }
    };

    void subscribe();

    return () => {
      isDisposed = true;
      if (retryTimeout !== undefined) {
        window.clearTimeout(retryTimeout);
      }
      if (cleanup) {
        void cleanup();
      }
    };
  }, [normalizedCurrentPubkey, options.onLiveMention]);
}
