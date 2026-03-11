import * as React from "react";
import { useQueryClient } from "@tanstack/react-query";

import { updateChannelLastMessageAt } from "@/features/channels/hooks";
import { mergeMessages } from "@/features/messages/hooks";
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
) {
  const queryClient = useQueryClient();
  const [lastReadByChannel, setLastReadByChannel] =
    React.useState<ChannelReadState>(readStoredChannelReadState);
  const hasInitializedChannelsRef = React.useRef(false);
  const activeChannelId = activeChannel?.id ?? null;
  const activeChannelLastMessageAt = activeChannel?.lastMessageAt ?? null;

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

    markChannelRead(activeChannelId, activeChannelLastMessageAt);
  }, [activeChannelId, activeChannelLastMessageAt, markChannelRead]);

  const inactiveLiveChannelKey = React.useMemo(
    () =>
      channels
        .filter(
          (channel) =>
            channel.channelType !== "forum" && channel.id !== activeChannelId,
        )
        .map((channel) => channel.id)
        .join("|"),
    [activeChannelId, channels],
  );

  React.useEffect(() => {
    const inactiveLiveChannelIds = inactiveLiveChannelKey
      ? inactiveLiveChannelKey.split("|")
      : [];

    if (inactiveLiveChannelIds.length === 0) {
      return;
    }

    let isDisposed = false;
    const cleanupCallbacks: Array<() => Promise<void>> = [];

    function handleIncomingMessage(channelId: string, event: RelayEvent) {
      const messageTimestamp = getMessageTimestamp(event);

      updateChannelLastMessageAt(queryClient, channelId, messageTimestamp);
      queryClient.setQueryData<RelayEvent[]>(
        ["channel-messages", channelId],
        (current) => {
          if (!current) {
            return current;
          }

          return mergeMessages(current, event);
        },
      );
    }

    void Promise.all(
      inactiveLiveChannelIds.map((channelId) =>
        relayClient
          .subscribeToChannel(channelId, (event) => {
            handleIncomingMessage(channelId, event);
          })
          .then((dispose) => {
            if (isDisposed) {
              void dispose();
              return;
            }

            cleanupCallbacks.push(dispose);
          }),
      ),
    ).catch((error) => {
      console.error("Failed to subscribe to unread channel updates", error);
    });

    return () => {
      isDisposed = true;
      for (const cleanup of cleanupCallbacks) {
        void cleanup();
      }
    };
  }, [inactiveLiveChannelKey, queryClient]);

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
