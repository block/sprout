import * as React from "react";
import { useQueryClient } from "@tanstack/react-query";

import {
  channelsQueryKey,
  updateChannelLastMessageAt,
} from "@/features/channels/hooks";
import { mergeTimelineCacheMessages } from "@/features/messages/hooks";
import { channelMessagesKey } from "@/features/messages/lib/messageQueryKeys";
import { getChannelIdFromTags } from "@/features/messages/lib/threading";
import { relayClient } from "@/shared/api/relayClient";
import type { Channel, RelayEvent } from "@/shared/api/types";

export type UseLiveChannelUpdatesOptions = {
  currentPubkey?: string;
  onLiveMention?: () => void;
};

const LIVE_MENTION_SUBSCRIPTION_RETRY_MS = 1_000;

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

async function disposeLiveSubscriptions(
  subscriptions: Array<() => Promise<void>>,
) {
  await Promise.allSettled(subscriptions.map((dispose) => dispose()));
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
  const mentionChannelIds = React.useMemo(
    () => [...new Set(channels.map((channel) => channel.id))].sort(),
    [channels],
  );

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
    if (liveChannelIds.size === 0) {
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
  }, [liveChannelIds]);

  React.useEffect(() => {
    if (
      !options.onLiveMention ||
      normalizedCurrentPubkey.length === 0 ||
      mentionChannelIds.length === 0
    ) {
      return;
    }

    let isDisposed = false;
    let cleanup: Array<() => Promise<void>> = [];
    let retryTimeout: ReturnType<typeof setTimeout> | undefined;

    const subscribeToMentionChannels = async () => {
      const settled = await Promise.allSettled(
        mentionChannelIds.map((channelId) =>
          relayClient.subscribeToChannelMentionEvents(
            channelId,
            normalizedCurrentPubkey,
            (event) => {
              if (!isDisposed) {
                handleMentionEvent(event);
              }
            },
          ),
        ),
      );

      const nextCleanup = settled.flatMap((result) =>
        result.status === "fulfilled" ? [result.value] : [],
      );

      if (isDisposed) {
        await disposeLiveSubscriptions(nextCleanup);
        return;
      }

      const firstFailure = settled.find(
        (result) => result.status === "rejected",
      );
      if (!firstFailure) {
        cleanup = nextCleanup;
        return;
      }

      await disposeLiveSubscriptions(nextCleanup);
      if (isDisposed) {
        return;
      }

      console.error(
        "Failed to subscribe to all Home mention updates; retrying",
        firstFailure.reason,
      );
      retryTimeout = window.setTimeout(() => {
        retryTimeout = undefined;
        void subscribeToMentionChannels();
      }, LIVE_MENTION_SUBSCRIPTION_RETRY_MS);
    };

    void subscribeToMentionChannels();

    return () => {
      isDisposed = true;
      if (retryTimeout !== undefined) {
        window.clearTimeout(retryTimeout);
      }
      void disposeLiveSubscriptions(cleanup);
    };
  }, [mentionChannelIds, normalizedCurrentPubkey, options.onLiveMention]);
}
