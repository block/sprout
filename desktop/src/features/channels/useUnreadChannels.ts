import * as React from "react";
import {
  useLiveChannelUpdates,
  type UseLiveChannelUpdatesOptions,
} from "@/features/channels/useLiveChannelUpdates";
import { useReadStateSync } from "@/features/channels/useReadStateSync";
import type { Channel } from "@/shared/api/types";

type UseUnreadChannelsOptions = UseLiveChannelUpdatesOptions;

/** Parse an ISO timestamp string to unix seconds, or null. */
function isoToUnixSeconds(value: string | null | undefined): number | null {
  if (!value) return null;
  const ms = Date.parse(value);
  return Number.isNaN(ms) ? null : Math.floor(ms / 1000);
}

export function useUnreadChannels(
  channels: Channel[],
  activeChannel: Channel | null,
  activeReadAt?: string | null,
  options: UseUnreadChannelsOptions = {},
) {
  const activeChannelId = activeChannel?.id ?? null;
  const activeChannelLastMessageAt = activeChannel?.lastMessageAt ?? null;
  // Let callers pass `null` to intentionally suppress the optimistic
  // channel-metadata fallback until a real timeline position is known.
  const effectiveActiveReadAt =
    activeReadAt === undefined ? activeChannelLastMessageAt : activeReadAt;

  const { mergedState, markContextRead, syncEnabled, setSyncEnabled } =
    useReadStateSync(options.currentPubkey);

  const hasInitializedChannelsRef = React.useRef(false);
  const mergedStateRef = React.useRef(mergedState);
  mergedStateRef.current = mergedState;
  const markContextReadRef = React.useRef(markContextRead);
  markContextReadRef.current = markContextRead;

  // ── markChannelRead: preserve the same external API (ISO string) ─────

  const markChannelRead = React.useCallback(
    (channelId: string, readAt: string | null | undefined) => {
      const unixSeconds = isoToUnixSeconds(readAt);
      if (unixSeconds === null) return;
      markContextRead(channelId, unixSeconds);
    },
    [markContextRead],
  );

  // ── Initialize new channels: mark as read on first load ──────────────

  React.useEffect(() => {
    if (channels.length === 0) return;

    if (!hasInitializedChannelsRef.current) {
      // On first channel load, mark channels that are unknown to the merged
      // read state as read at their lastMessageAt, so everything starts as
      // "read" (same as the old localStorage-only behaviour).
      const currentMerged = mergedStateRef.current;
      const currentMarkRead = markContextReadRef.current;
      for (const channel of channels) {
        if (currentMerged[channel.id] === undefined) {
          const ts = isoToUnixSeconds(channel.lastMessageAt);
          if (ts !== null) {
            currentMarkRead(channel.id, ts);
          }
        }
      }
      hasInitializedChannelsRef.current = true;
    }
  }, [channels]);

  // ── Auto-mark active channel as read ─────────────────────────────────

  React.useEffect(() => {
    if (!activeChannelId) return;
    markChannelRead(activeChannelId, effectiveActiveReadAt);
  }, [activeChannelId, effectiveActiveReadAt, markChannelRead]);

  // ── Live channel updates (message cache, callbacks) ──────────────────

  useLiveChannelUpdates(channels, activeChannelId, options);

  // ── Compute unread set ───────────────────────────────────────────────

  const unreadChannelIds = React.useMemo(() => {
    return new Set(
      channels
        .filter((channel) => channel.id !== activeChannelId)
        .filter((channel) => {
          if (!channel.lastMessageAt) return false;
          const lastMessageMs = Date.parse(channel.lastMessageAt);
          if (Number.isNaN(lastMessageMs)) return false;
          // Read state stores unix seconds (NIP-RS). Convert to ms for
          // sub-second–accurate comparison against the ISO lastMessageAt.
          const lastReadMs = (mergedState[channel.id] ?? 0) * 1000;
          return lastMessageMs > lastReadMs;
        })
        .map((channel) => channel.id),
    );
  }, [activeChannelId, channels, mergedState]);

  return {
    unreadChannelIds,
    markChannelRead,
    syncEnabled,
    setSyncEnabled,
  };
}
