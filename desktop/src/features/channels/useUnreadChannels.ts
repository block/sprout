import * as React from "react";
import {
  useLiveChannelUpdates,
  type UseLiveChannelUpdatesOptions,
} from "@/features/channels/useLiveChannelUpdates";
import { useReadState } from "@/features/channels/readState/useReadState";
import type { RelayClient } from "@/shared/api/relayClientSession";
import type { Channel } from "@/shared/api/types";

type UseUnreadChannelsOptions = UseLiveChannelUpdatesOptions & {
  pubkey?: string;
  relayClient?: RelayClient;
};

function parseTimestamp(value: string | null | undefined) {
  if (!value) {
    return null;
  }

  const timestamp = Date.parse(value);
  return Number.isNaN(timestamp) ? null : timestamp;
}

function toUnixSeconds(isoOrMs: string | null | undefined): number | null {
  const ms = parseTimestamp(isoOrMs);
  return ms === null ? null : Math.floor(ms / 1_000);
}

export function useUnreadChannels(
  channels: Channel[],
  activeChannel: Channel | null,
  activeReadAt?: string | null,
  options: UseUnreadChannelsOptions = {},
) {
  const { pubkey, relayClient, ...liveUpdateOptions } = options;
  const activeChannelId = activeChannel?.id ?? null;
  const activeChannelLastMessageAt = activeChannel?.lastMessageAt ?? null;

  // Let callers pass `null` to intentionally suppress the optimistic
  // channel-metadata fallback until a real timeline position is known.
  const effectiveActiveReadAt =
    activeReadAt === undefined ? activeChannelLastMessageAt : activeReadAt;

  const {
    getEffectiveTimestamp,
    isReady: isReadStateReady,
    markContextRead,
    readStateVersion,
    seedContextRead,
  } = useReadState(pubkey, relayClient);

  // Track whether channels have been initialized (for first-load seeding)
  const hasInitializedChannelsRef = React.useRef(false);

  const markChannelRead = React.useCallback(
    (channelId: string, readAt: string | null | undefined) => {
      const unixSeconds = toUnixSeconds(readAt);
      if (unixSeconds === null) return;
      markContextRead(channelId, unixSeconds);
    },
    [markContextRead],
  );

  // Seed new channels so they don't flash as unread on first load.
  // For channels the user hasn't read yet, initialize read-at to the
  // channel's current lastMessageAt so they appear as "read."
  React.useEffect(() => {
    if (!isReadStateReady) return;
    if (channels.length === 0) return;

    for (const channel of channels) {
      const existing = getEffectiveTimestamp(channel.id);
      if (existing !== null) continue;

      // Only seed on first initialization, not when new channels appear later
      if (hasInitializedChannelsRef.current) continue;

      const lastMsgUnix = toUnixSeconds(channel.lastMessageAt);
      if (lastMsgUnix !== null) {
        seedContextRead(channel.id, lastMsgUnix);
      }
    }

    hasInitializedChannelsRef.current = true;
  }, [channels, getEffectiveTimestamp, isReadStateReady, seedContextRead]);

  // Mark the active channel as read when it changes or new messages arrive
  React.useEffect(() => {
    if (!isReadStateReady) return;
    if (!activeChannelId) return;
    markChannelRead(activeChannelId, effectiveActiveReadAt);
  }, [
    activeChannelId,
    effectiveActiveReadAt,
    isReadStateReady,
    markChannelRead,
  ]);

  // Keep live channel updates (drives channel.lastMessageAt cache updates)
  useLiveChannelUpdates(channels, activeChannelId, liveUpdateOptions);

  // Compute unread channel IDs by comparing channel.lastMessageAt against
  // the NIP-RS effective timestamp.
  // readStateVersion is intentionally included to force recomputation when
  // cross-device state arrives (getEffectiveTimestamp is referentially stable).
  // biome-ignore lint/correctness/useExhaustiveDependencies: readStateVersion is an intentional invalidation signal
  const unreadChannelIds = React.useMemo(() => {
    if (!isReadStateReady) {
      return new Set<string>();
    }

    return new Set(
      channels
        .filter((channel) => channel.id !== activeChannelId)
        .filter((channel) => {
          const lastMsgUnix = toUnixSeconds(channel.lastMessageAt);
          if (lastMsgUnix === null) return false;

          const readAt = getEffectiveTimestamp(channel.id);
          return readAt === null || lastMsgUnix > readAt;
        })
        .map((channel) => channel.id),
    );
  }, [
    activeChannelId,
    channels,
    getEffectiveTimestamp,
    isReadStateReady,
    readStateVersion,
  ]);

  return {
    unreadChannelIds,
    markChannelRead,
  };
}
