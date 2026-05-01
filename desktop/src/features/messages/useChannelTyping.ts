import { useEffect, useEffectEvent, useMemo, useRef, useState } from "react";

import {
  getChannelIdFromTags,
  getThreadReference,
} from "@/features/messages/lib/threading";
import { relayClient } from "@/shared/api/relayClient";
import type { Channel, RelayEvent } from "@/shared/api/types";
import {
  KIND_STREAM_MESSAGE,
  KIND_STREAM_MESSAGE_V2,
  KIND_TYPING_INDICATOR,
} from "@/shared/constants/kinds";
import { resolveEventAuthorPubkey } from "@/shared/lib/authors";

export type TypingIndicatorEntry = {
  pubkey: string;
  threadHeadId: string | null;
};

type TypingEntry = {
  expiresAt: number;
  firstSeenAt: number;
  pubkey: string;
  threadHeadId: string | null;
};
type TypingState = Record<string, TypingEntry>;

const TYPING_INDICATOR_TTL_MS = 8_000;
const TYPING_PRUNE_INTERVAL_MS = 1_000;
const TYPING_POST_MESSAGE_SUPPRESS_MS = 5_000;

function pruneTypingState(state: TypingState, now = Date.now()) {
  let changed = false;
  const next: TypingState = {};

  for (const [pubkey, entry] of Object.entries(state)) {
    if (entry.expiresAt > now) {
      next[pubkey] = entry;
      continue;
    }

    changed = true;
  }

  return changed ? next : state;
}

function isTypingCompletionEvent(event: RelayEvent | null | undefined) {
  if (!event) {
    return false;
  }

  return (
    event.kind === KIND_STREAM_MESSAGE || event.kind === KIND_STREAM_MESSAGE_V2
  );
}

function getTypingScopeId(event: RelayEvent) {
  return getThreadReference(event.tags).parentId ?? null;
}

function getTypingStateKey(pubkey: string, threadHeadId: string | null) {
  return `${pubkey}:${threadHeadId ?? "channel"}`;
}

function getTypingPubkey(event: RelayEvent) {
  return resolveEventAuthorPubkey({
    pubkey: event.pubkey,
    tags: event.tags,
    preferActorTag: true,
    requireChannelTagForPTags: true,
  }).toLowerCase();
}

export function useChannelTyping(
  channel: Channel | null,
  currentPubkey?: string,
  completionEvents: RelayEvent[] = [],
) {
  const channelId = channel?.id ?? null;
  const channelType = channel?.channelType ?? null;
  const [typingByPubkey, setTypingByPubkey] = useState<TypingState>({});
  const normalizedCurrentPubkey = currentPubkey?.toLowerCase();
  const hasSeededCompletionEventsRef = useRef(false);
  const processedCompletionEventIdsRef = useRef<Set<string>>(new Set());
  const typingSuppressUntilByAgentRef = useRef<Record<string, number>>({});
  const typingSuppressUntilByPubkeyRef = useRef<Record<string, number>>({});
  const latestMessageCreatedAtByPubkeyRef = useRef<Record<string, number>>({});

  const registerTyping = useEffectEvent((event: RelayEvent) => {
    if (!channelId || event.kind !== KIND_TYPING_INDICATOR) {
      return;
    }

    if (getChannelIdFromTags(event.tags) !== channelId) {
      return;
    }

    const typingPubkey = getTypingPubkey(event);
    const threadHeadId = getTypingScopeId(event);
    const typingKey = getTypingStateKey(typingPubkey, threadHeadId);
    if (normalizedCurrentPubkey && typingPubkey === normalizedCurrentPubkey) {
      return;
    }

    const agentSuppressUntil =
      typingSuppressUntilByAgentRef.current[typingPubkey] ?? 0;
    if (agentSuppressUntil > Date.now()) {
      return;
    }
    if (agentSuppressUntil > 0) {
      delete typingSuppressUntilByAgentRef.current[typingPubkey];
    }

    const suppressUntil =
      typingSuppressUntilByPubkeyRef.current[typingKey] ?? 0;
    if (suppressUntil > Date.now()) {
      return;
    }
    if (suppressUntil > 0) {
      delete typingSuppressUntilByPubkeyRef.current[typingKey];
    }

    const latestMessageCreatedAt =
      latestMessageCreatedAtByPubkeyRef.current[typingKey] ?? 0;
    if (event.created_at <= latestMessageCreatedAt) {
      return;
    }

    const now = Date.now();
    setTypingByPubkey((current) => {
      const pruned = pruneTypingState(current, now);
      const existing = pruned[typingKey];
      return {
        ...pruned,
        [typingKey]: {
          expiresAt: now + TYPING_INDICATOR_TTL_MS,
          firstSeenAt: existing?.firstSeenAt ?? now,
          pubkey: typingPubkey,
          threadHeadId,
        },
      };
    });
  });

  // biome-ignore lint/correctness/useExhaustiveDependencies: channel changes should clear local typing state
  useEffect(() => {
    setTypingByPubkey({});
    hasSeededCompletionEventsRef.current = false;
    processedCompletionEventIdsRef.current = new Set();
    typingSuppressUntilByAgentRef.current = {};
    typingSuppressUntilByPubkeyRef.current = {};
    latestMessageCreatedAtByPubkeyRef.current = {};
  }, [channelId]);

  useEffect(() => {
    if (!channelId || completionEvents.length === 0) {
      return;
    }

    const completionKeys = new Set<string>();
    for (const event of completionEvents) {
      if (
        !isTypingCompletionEvent(event) ||
        getChannelIdFromTags(event.tags) !== channelId
      ) {
        continue;
      }

      const authorPubkey = getTypingPubkey(event);
      const threadHeadId = getTypingScopeId(event);
      const typingKey = getTypingStateKey(authorPubkey, threadHeadId);
      latestMessageCreatedAtByPubkeyRef.current[typingKey] = Math.max(
        latestMessageCreatedAtByPubkeyRef.current[typingKey] ?? 0,
        event.created_at,
      );

      if (!hasSeededCompletionEventsRef.current) {
        processedCompletionEventIdsRef.current.add(event.id);
        continue;
      }

      if (processedCompletionEventIdsRef.current.has(event.id)) {
        continue;
      }

      processedCompletionEventIdsRef.current.add(event.id);
      typingSuppressUntilByAgentRef.current[authorPubkey] =
        Date.now() + TYPING_POST_MESSAGE_SUPPRESS_MS;
      completionKeys.add(typingKey);
    }

    hasSeededCompletionEventsRef.current = true;

    if (completionKeys.size === 0) {
      return;
    }

    setTypingByPubkey((current) => {
      const next = pruneTypingState(current);
      let updated: TypingState | null = null;

      for (const typingKey of completionKeys) {
        const pubkey = typingKey.slice(0, typingKey.indexOf(":"));
        const keysToClear = Object.keys(next).filter((currentKey) =>
          currentKey.startsWith(`${pubkey}:`),
        );

        if (keysToClear.length === 0) {
          keysToClear.push(typingKey);
        }

        for (const keyToClear of keysToClear) {
          if (!(keyToClear in next)) {
            continue;
          }

          updated ??= { ...next };
          delete updated[keyToClear];
          typingSuppressUntilByPubkeyRef.current[keyToClear] =
            Date.now() + TYPING_POST_MESSAGE_SUPPRESS_MS;
        }
      }

      if (!updated) {
        return next;
      }

      return updated;
    });
  }, [channelId, completionEvents]);

  useEffect(() => {
    if (!channelId || channelType === "forum") {
      return;
    }

    let isDisposed = false;
    let cleanup: (() => Promise<void>) | undefined;

    relayClient
      .subscribeToTypingIndicators(channelId, (event) => {
        if (!isDisposed) {
          registerTyping(event);
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
        console.error(
          "Failed to subscribe to typing indicators",
          channelId,
          error,
        );
      });

    return () => {
      isDisposed = true;
      if (cleanup) {
        void cleanup();
      }
    };
  }, [channelId, channelType]);

  const hasActiveTypers = Object.keys(typingByPubkey).length > 0;

  useEffect(() => {
    if (!hasActiveTypers) {
      return;
    }

    const interval = window.setInterval(() => {
      setTypingByPubkey((current) => pruneTypingState(current));
    }, TYPING_PRUNE_INTERVAL_MS);

    return () => {
      window.clearInterval(interval);
    };
  }, [hasActiveTypers]);

  return useMemo(
    () =>
      Object.values(typingByPubkey)
        .sort((left, right) => left.firstSeenAt - right.firstSeenAt)
        .map(({ pubkey, threadHeadId }) => ({ pubkey, threadHeadId })),
    [typingByPubkey],
  );
}
