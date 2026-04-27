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
  onDmMessage?: (event: RelayEvent, channel: Channel) => void;
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
  const dmChannelMap = React.useMemo(
    () =>
      new Map(
        channels
          .filter((channel) => channel.channelType === "dm")
          .map((channel) => [channel.id, channel]),
      ),
    [channels],
  );
  const seenDmEventIdsRef = React.useRef(new Set<string>());
  const subscriptionStartedAtRef = React.useRef(Math.floor(Date.now() / 1000));

  // Reset subscription timestamp when identity changes.
  React.useEffect(() => {
    void normalizedCurrentPubkey;
    subscriptionStartedAtRef.current = Math.floor(Date.now() / 1000);
  }, [normalizedCurrentPubkey]);

  // Effect deps use primitive keys so refetches that produce new refs with
  // identical contents don't churn subscriptions. The Set/array memos are
  // still handy for closure reads via useEffectEvent.
  const hasLiveChannels = liveChannelIds.size > 0;
  const mentionChannelIdsKey = React.useMemo(
    () => [...new Set(channels.map((channel) => channel.id))].sort().join(","),
    [channels],
  );

  const handleDmEvent = React.useEffectEvent((event: RelayEvent) => {
    // Suppress backlog events that predate our subscription — these are
    // historical replays, not live messages.
    if (event.created_at < subscriptionStartedAtRef.current) {
      return;
    }

    const channelId = getChannelIdFromTags(event.tags);
    if (!channelId) {
      return;
    }

    if (!isExternalMentionEvent(event, normalizedCurrentPubkey)) {
      return;
    }

    const dmChannel = dmChannelMap.get(channelId);
    if (!dmChannel) {
      return;
    }

    if (!rememberMentionEvent(seenDmEventIdsRef.current, event.id)) {
      return;
    }

    // Don't fire a notification for the channel the user is already viewing.
    if (channelId === activeChannelId) {
      return;
    }

    options.onDmMessage?.(event, dmChannel);
  });

  const handleIncomingMessage = React.useEffectEvent((event: RelayEvent) => {
    const channelId = getChannelIdFromTags(event.tags);
    if (!channelId) {
      return;
    }

    // Track DM events even for the active channel so the dedup set stays
    // current. The handler itself skips firing the notification callback
    // when the user is already viewing the DM.
    handleDmEvent(event);

    if (channelId === activeChannelId) {
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

      // Update the subscription timestamp so replayed backlog events
      // (which have created_at in the past) are naturally suppressed.
      subscriptionStartedAtRef.current = Math.floor(Date.now() / 1000);
    });
  }, [queryClient]);

  React.useEffect(() => {
    if (!hasLiveChannels) {
      return;
    }

    let isDisposed = false;
    let cleanup: (() => Promise<void>) | undefined;

    // Record the subscription start time so handleDmEvent can distinguish
    // backlog replays (created_at < startedAt) from live messages.
    subscriptionStartedAtRef.current = Math.floor(Date.now() / 1000);

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

  // Subscribe to mention events per channel with a diff-based manager: only
  // subscribe newly-added channels and unsubscribe removed ones on each sync.
  // The ref survives re-renders so churn-with-identical-IDs does zero work.
  const mentionSubsRef = React.useRef(new Map<string, () => Promise<void>>());
  const mentionSubsPubkeyRef = React.useRef<string | null>(null);

  React.useEffect(() => {
    if (!options.onLiveMention || normalizedCurrentPubkey.length === 0) {
      return;
    }

    let isCancelled = false;
    let retryTimeout: ReturnType<typeof setTimeout> | undefined;
    let retryAttempt = 0;

    const syncSubs = async (): Promise<boolean> => {
      const activeSubs = mentionSubsRef.current;

      if (
        mentionSubsPubkeyRef.current !== null &&
        mentionSubsPubkeyRef.current !== normalizedCurrentPubkey
      ) {
        const stale = Array.from(activeSubs.values());
        activeSubs.clear();
        await Promise.allSettled(stale.map((dispose) => dispose()));
        if (isCancelled) return true;
      }
      mentionSubsPubkeyRef.current = normalizedCurrentPubkey;

      const targetIds = new Set(
        mentionChannelIdsKey ? mentionChannelIdsKey.split(",") : [],
      );

      for (const [channelId, dispose] of activeSubs) {
        if (!targetIds.has(channelId)) {
          activeSubs.delete(channelId);
          void dispose().catch(() => {});
        }
      }

      let anyFailed = false;
      // Pass handleMentionEvent directly — it's a stable useEffectEvent
      // callback. Do NOT wrap in an isCancelled check here: subs persist
      // across effect runs (that's the point of the diff manager), so a
      // stale isCancelled flag from a prior run would silently drop events
      // on long-lived subs.
      const additions = Array.from(targetIds)
        .filter((channelId) => !activeSubs.has(channelId))
        .map(async (channelId) => {
          try {
            const dispose = await relayClient.subscribeToChannelMentionEvents(
              channelId,
              normalizedCurrentPubkey,
              handleMentionEvent,
            );
            if (isCancelled) {
              void dispose().catch(() => {});
              return;
            }
            activeSubs.set(channelId, dispose);
          } catch (err) {
            anyFailed = true;
            console.error(
              "Failed to subscribe to mention events",
              channelId,
              err,
            );
          }
        });
      await Promise.allSettled(additions);
      return !anyFailed;
    };

    const runSync = async () => {
      const ok = await syncSubs();
      if (isCancelled) return;
      if (ok) {
        retryAttempt = 0;
        return;
      }
      const delayMs = Math.min(
        LIVE_MENTION_SUBSCRIPTION_RETRY_BASE_MS * 2 ** retryAttempt,
        LIVE_MENTION_SUBSCRIPTION_RETRY_MAX_MS,
      );
      retryAttempt += 1;
      retryTimeout = window.setTimeout(() => {
        retryTimeout = undefined;
        void runSync();
      }, delayMs);
    };

    void runSync();

    return () => {
      isCancelled = true;
      if (retryTimeout !== undefined) {
        window.clearTimeout(retryTimeout);
      }
    };
  }, [mentionChannelIdsKey, normalizedCurrentPubkey, options.onLiveMention]);

  React.useEffect(() => {
    return () => {
      const subs = mentionSubsRef.current;
      for (const dispose of subs.values()) {
        void dispose().catch(() => {});
      }
      subs.clear();
      mentionSubsPubkeyRef.current = null;
    };
  }, []);
}
