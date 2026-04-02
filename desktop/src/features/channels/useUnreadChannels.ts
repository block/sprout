import * as React from "react";
import { useQueryClient } from "@tanstack/react-query";

import {
  channelsQueryKey,
  updateChannelLastMessageAt,
} from "@/features/channels/hooks";
import { channelMessagesKey } from "@/features/messages/lib/messageQueryKeys";
import { getChannelIdFromTags } from "@/features/messages/lib/threading";
import { mergeTimelineCacheMessages } from "@/features/messages/hooks";
import { relayClient } from "@/shared/api/relayClient";
import type { Channel, RelayEvent } from "@/shared/api/types";

const CHANNEL_READ_STATE_STORAGE_KEY = "sprout.channel-read-state.v1";

type ChannelReadState = Record<string, string | null>;

function parseTimestamp(value: string | null | undefined) {
  if (!value) {
    return null;
  }

  const timestamp = Date.parse(value);
  return Number.isNaN(timestamp) ? null : timestamp;
}

function normalizeTimestamp(value: string | null | undefined) {
  const timestamp = parseTimestamp(value);
  return timestamp === null ? null : new Date(timestamp).toISOString();
}

function isNewerTimestamp(
  candidate: string | null | undefined,
  current: string | null | undefined,
) {
  const candidateTimestamp = parseTimestamp(candidate);
  if (candidateTimestamp === null) {
    return false;
  }

  const currentTimestamp = parseTimestamp(current);
  return currentTimestamp === null || candidateTimestamp > currentTimestamp;
}

function readStoredChannelReadState(): ChannelReadState {
  if (typeof window === "undefined") {
    return {};
  }

  const rawState = window.localStorage.getItem(CHANNEL_READ_STATE_STORAGE_KEY);
  if (!rawState) {
    return {};
  }

  try {
    const parsed = JSON.parse(rawState);
    if (!parsed || typeof parsed !== "object") {
      return {};
    }

    return Object.fromEntries(
      Object.entries(parsed).map(([channelId, value]) => [
        channelId,
        typeof value === "string" || value === null
          ? normalizeTimestamp(value)
          : null,
      ]),
    );
  } catch {
    return {};
  }
}

function getMessageTimestamp(event: RelayEvent) {
  return new Date(event.created_at * 1_000).toISOString();
}

export function useUnreadChannels(
  channels: Channel[],
  activeChannel: Channel | null,
  activeReadAt?: string | null,
) {
  const queryClient = useQueryClient();
  const [lastReadByChannel, setLastReadByChannel] =
    React.useState<ChannelReadState>(readStoredChannelReadState);
  const hasInitializedChannelsRef = React.useRef(false);
  const activeChannelId = activeChannel?.id ?? null;
  const activeChannelLastMessageAt = activeChannel?.lastMessageAt ?? null;
  const effectiveActiveReadAt = activeReadAt ?? activeChannelLastMessageAt;

  const markChannelRead = React.useCallback(
    (channelId: string, readAt: string | null | undefined) => {
      const normalizedReadAt = normalizeTimestamp(readAt);

      setLastReadByChannel((current) => {
        const previousReadAt = current[channelId] ?? null;

        if (normalizedReadAt === null) {
          if (channelId in current) {
            return current;
          }

          return {
            ...current,
            [channelId]: null,
          };
        }

        if (!isNewerTimestamp(normalizedReadAt, previousReadAt)) {
          return current;
        }

        return {
          ...current,
          [channelId]: normalizedReadAt,
        };
      });
    },
    [],
  );

  React.useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }

    window.localStorage.setItem(
      CHANNEL_READ_STATE_STORAGE_KEY,
      JSON.stringify(lastReadByChannel),
    );
  }, [lastReadByChannel]);

  React.useEffect(() => {
    if (channels.length === 0) {
      return;
    }

    setLastReadByChannel((current) => {
      const knownChannelIds = new Set(channels.map((channel) => channel.id));
      const nextReadState: ChannelReadState = {};
      let didChange = false;

      for (const channel of channels) {
        if (channel.id in current) {
          nextReadState[channel.id] = current[channel.id] ?? null;
          continue;
        }

        nextReadState[channel.id] = hasInitializedChannelsRef.current
          ? null
          : normalizeTimestamp(channel.lastMessageAt);
        didChange = true;
      }

      for (const channelId of Object.keys(current)) {
        if (!knownChannelIds.has(channelId)) {
          didChange = true;
        }
      }

      return didChange ? nextReadState : current;
    });

    hasInitializedChannelsRef.current = true;
  }, [channels]);

  React.useEffect(() => {
    if (!activeChannelId) {
      return;
    }

    markChannelRead(activeChannelId, effectiveActiveReadAt);
  }, [activeChannelId, effectiveActiveReadAt, markChannelRead]);

  const liveChannelIds = React.useMemo(
    () =>
      new Set(
        channels
          .filter((channel) => channel.channelType !== "forum")
          .map((channel) => channel.id),
      ),
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

  const unreadChannelIds = React.useMemo(
    () =>
      new Set(
        channels
          .filter((channel) => channel.id !== activeChannelId)
          .filter((channel) =>
            isNewerTimestamp(
              channel.lastMessageAt,
              lastReadByChannel[channel.id],
            ),
          )
          .map((channel) => channel.id),
      ),
    [activeChannelId, channels, lastReadByChannel],
  );

  return {
    unreadChannelIds,
    markChannelRead,
  };
}
