import * as React from "react";
import {
  useLiveChannelUpdates,
  type UseLiveChannelUpdatesOptions,
} from "@/features/channels/useLiveChannelUpdates";
import type { Channel } from "@/shared/api/types";

const CHANNEL_READ_STATE_STORAGE_KEY = "sprout.channel-read-state.v1";
const LEGACY_CHANNEL_READ_STATE_STORAGE_KEY = CHANNEL_READ_STATE_STORAGE_KEY;
const CHANNEL_READ_STATE_STORAGE_KEY_V2 = "sprout.channel-read-state.v2";

type ChannelReadState = Record<string, string>;
type UseUnreadChannelsOptions = UseLiveChannelUpdatesOptions;

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

function channelReadStateStorageKey(pubkey: string) {
  return `${CHANNEL_READ_STATE_STORAGE_KEY_V2}:${pubkey}`;
}

function parseStoredChannelReadState(
  rawState: string | null,
): ChannelReadState {
  if (!rawState) {
    return {};
  }

  try {
    const parsed = JSON.parse(rawState);
    if (!parsed || typeof parsed !== "object") {
      return {};
    }

    return Object.fromEntries(
      Object.entries(parsed)
        .map(([channelId, value]) => [
          channelId,
          typeof value === "string" ? normalizeTimestamp(value) : null,
        ])
        .filter(
          (entry): entry is [string, string] => typeof entry[1] === "string",
        ),
    );
  } catch {
    return {};
  }
}

function readStoredChannelReadState(pubkey: string): ChannelReadState {
  if (typeof window === "undefined" || pubkey.length === 0) {
    return {};
  }

  const scopedValue = window.localStorage.getItem(
    channelReadStateStorageKey(pubkey),
  );
  if (scopedValue) {
    return parseStoredChannelReadState(scopedValue);
  }

  return parseStoredChannelReadState(
    window.localStorage.getItem(LEGACY_CHANNEL_READ_STATE_STORAGE_KEY),
  );
}

function writeStoredChannelReadState(
  pubkey: string,
  lastReadByChannel: ChannelReadState,
) {
  if (typeof window === "undefined" || pubkey.length === 0) {
    return;
  }

  window.localStorage.setItem(
    channelReadStateStorageKey(pubkey),
    JSON.stringify(lastReadByChannel),
  );
}

function reconcileChannelReadState(
  current: ChannelReadState,
  channels: Channel[],
  initializeUnknownChannels: boolean,
) {
  const knownChannelIds = new Set(channels.map((channel) => channel.id));
  const nextReadState: ChannelReadState = {};
  let didChange = false;

  for (const channel of channels) {
    const existingReadAt = current[channel.id];
    if (typeof existingReadAt === "string") {
      nextReadState[channel.id] = existingReadAt;
      continue;
    }

    if (initializeUnknownChannels) {
      const initialReadAt = normalizeTimestamp(channel.lastMessageAt);
      if (typeof initialReadAt === "string") {
        nextReadState[channel.id] = initialReadAt;
        didChange = true;
      }
    }
  }

  for (const channelId of Object.keys(current)) {
    if (!knownChannelIds.has(channelId)) {
      didChange = true;
    }
  }

  return {
    didChange,
    nextReadState,
  };
}

export function useUnreadChannels(
  channels: Channel[],
  activeChannel: Channel | null,
  activeReadAt?: string | null,
  options: UseUnreadChannelsOptions = {},
) {
  const normalizedPubkey = options.currentPubkey?.trim().toLowerCase() ?? "";
  const [lastReadByChannel, setLastReadByChannel] =
    React.useState<ChannelReadState>(() =>
      readStoredChannelReadState(normalizedPubkey),
    );
  const hasInitializedChannelsRef = React.useRef(false);
  const hydratedPubkeyRef = React.useRef(normalizedPubkey);
  const activeChannelId = activeChannel?.id ?? null;
  const activeChannelLastMessageAt = activeChannel?.lastMessageAt ?? null;
  // Let callers pass `null` to intentionally suppress the optimistic
  // channel-metadata fallback until a real timeline position is known.
  const effectiveActiveReadAt =
    activeReadAt === undefined ? activeChannelLastMessageAt : activeReadAt;
  const hasHydratedCurrentPubkey =
    hydratedPubkeyRef.current === normalizedPubkey;

  const markChannelRead = React.useCallback(
    (channelId: string, readAt: string | null | undefined) => {
      const normalizedReadAt = normalizeTimestamp(readAt);

      setLastReadByChannel((current) => {
        if (normalizedReadAt === null) {
          return current;
        }

        const previousReadAt = current[channelId];
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
    // Identity loads asynchronously on app startup. Skip persistence until
    // we've rehydrated the current user's stored state, otherwise we can
    // overwrite it with pre-hydration defaults from a previous render.
    if (!hasHydratedCurrentPubkey) {
      return;
    }

    writeStoredChannelReadState(normalizedPubkey, lastReadByChannel);
  }, [hasHydratedCurrentPubkey, lastReadByChannel, normalizedPubkey]);

  React.useEffect(() => {
    if (hydratedPubkeyRef.current === normalizedPubkey) {
      return;
    }

    hydratedPubkeyRef.current = normalizedPubkey;

    const storedState = readStoredChannelReadState(normalizedPubkey);
    const nextReadState =
      channels.length > 0
        ? reconcileChannelReadState(storedState, channels, true).nextReadState
        : storedState;
    hasInitializedChannelsRef.current = channels.length > 0;
    setLastReadByChannel(nextReadState);
  }, [channels, normalizedPubkey]);

  React.useEffect(() => {
    if (channels.length === 0) {
      return;
    }

    setLastReadByChannel((current) => {
      const { didChange, nextReadState } = reconcileChannelReadState(
        current,
        channels,
        !hasInitializedChannelsRef.current,
      );
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
