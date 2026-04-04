import * as React from "react";
import {
  useLiveChannelUpdates,
  type UseLiveChannelUpdatesOptions,
} from "@/features/channels/useLiveChannelUpdates";
import type { Channel } from "@/shared/api/types";
import { parseTimestamp } from "@/shared/lib/time";

const CHANNEL_READ_STATE_STORAGE_KEY = "sprout.channel-read-state.v1";

type ChannelReadState = Record<string, string | null>;
type UseUnreadChannelsOptions = UseLiveChannelUpdatesOptions;

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

export function useUnreadChannels(
  channels: Channel[],
  activeChannel: Channel | null,
  activeReadAt?: string | null,
  options: UseUnreadChannelsOptions = {},
) {
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
  useLiveChannelUpdates(channels, activeChannelId, options);

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
