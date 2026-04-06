import type { QueryClient } from "@tanstack/react-query";

import type { Channel } from "@/shared/api/types";
import { channelsQueryKey } from "@/features/channels/hooks";

function parseTimestamp(value: string | null | undefined) {
  if (!value) {
    return null;
  }

  const timestamp = Date.parse(value);
  return Number.isNaN(timestamp) ? null : timestamp;
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

export function updateChannelLastMessageAt(
  queryClient: QueryClient,
  channelId: string,
  lastMessageAt: string | null | undefined,
) {
  const lastMessageTimestamp = parseTimestamp(lastMessageAt);
  const normalizedLastMessageAt =
    lastMessageTimestamp === null
      ? null
      : new Date(lastMessageTimestamp).toISOString();

  if (!normalizedLastMessageAt) {
    return;
  }

  queryClient.setQueryData<Channel[]>(channelsQueryKey, (current) => {
    if (!current) {
      return current;
    }

    let didUpdate = false;
    const nextChannels = current.map((channel) => {
      if (
        channel.id !== channelId ||
        !isNewerTimestamp(normalizedLastMessageAt, channel.lastMessageAt)
      ) {
        return channel;
      }

      didUpdate = true;
      return {
        ...channel,
        lastMessageAt: normalizedLastMessageAt,
      };
    });

    return didUpdate ? nextChannels : current;
  });
}
