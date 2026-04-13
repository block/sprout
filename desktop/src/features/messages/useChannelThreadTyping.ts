import { useEffect, useEffectEvent, useMemo, useRef, useState } from "react";

import {
  getChannelIdFromTags,
  getTypingReplyParentFromTags,
} from "@/features/messages/lib/threading";
import { relayClient } from "@/shared/api/relayClient";
import type { Channel, RelayEvent } from "@/shared/api/types";
import {
  KIND_STREAM_MESSAGE,
  KIND_STREAM_MESSAGE_DIFF,
  KIND_TYPING_INDICATOR,
} from "@/shared/constants/kinds";

type TypingEntry = {
  expiresAt: number;
  firstSeenAt: number;
  replyParentId: string | null;
};

type TypingState = Record<string, TypingEntry>;

const TYPING_INDICATOR_TTL_MS = 8_000;
const TYPING_PRUNE_INTERVAL_MS = 1_000;
const TYPING_POST_MESSAGE_SUPPRESS_MS = 2_000;

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
    event.kind === KIND_STREAM_MESSAGE ||
    event.kind === KIND_STREAM_MESSAGE_DIFF
  );
}

export function useChannelThreadTyping(
  channel: Channel | null,
  currentPubkey: string | undefined,
  latestMessageEvent: RelayEvent | null | undefined,
  openThreadMessageIds?: ReadonlySet<string> | null,
) {
  const channelId = channel?.id ?? null;
  const channelType = channel?.channelType ?? null;
  const [typingByPubkey, setTypingByPubkey] = useState<TypingState>({});
  const normalizedCurrentPubkey = currentPubkey?.toLowerCase();
  const typingSuppressUntilByPubkeyRef = useRef<Record<string, number>>({});
  const latestMessageCreatedAtByPubkeyRef = useRef<Record<string, number>>({});

  const registerTyping = useEffectEvent((event: RelayEvent) => {
    if (!channelId || event.kind !== KIND_TYPING_INDICATOR) {
      return;
    }

    if (getChannelIdFromTags(event.tags) !== channelId) {
      return;
    }

    const typingPubkey = event.pubkey.toLowerCase();
    if (normalizedCurrentPubkey && typingPubkey === normalizedCurrentPubkey) {
      return;
    }

    const suppressUntil =
      typingSuppressUntilByPubkeyRef.current[typingPubkey] ?? 0;
    if (suppressUntil > Date.now()) {
      return;
    }
    if (suppressUntil > 0) {
      delete typingSuppressUntilByPubkeyRef.current[typingPubkey];
    }

    const latestMessageCreatedAt =
      latestMessageCreatedAtByPubkeyRef.current[typingPubkey] ?? 0;
    if (event.created_at <= latestMessageCreatedAt) {
      return;
    }

    const replyParentId = getTypingReplyParentFromTags(event.tags);
    const now = Date.now();
    setTypingByPubkey((current) => {
      const pruned = pruneTypingState(current, now);
      const existing = pruned[typingPubkey];
      return {
        ...pruned,
        [typingPubkey]: {
          expiresAt: now + TYPING_INDICATOR_TTL_MS,
          firstSeenAt: existing?.firstSeenAt ?? now,
          replyParentId,
        },
      };
    });
  });

  // biome-ignore lint/correctness/useExhaustiveDependencies: channel changes should clear local typing state
  useEffect(() => {
    setTypingByPubkey({});
    typingSuppressUntilByPubkeyRef.current = {};
    latestMessageCreatedAtByPubkeyRef.current = {};
  }, [channelId]);

  useEffect(() => {
    if (
      !channelId ||
      !latestMessageEvent ||
      !isTypingCompletionEvent(latestMessageEvent)
    ) {
      return;
    }

    if (getChannelIdFromTags(latestMessageEvent.tags) !== channelId) {
      return;
    }

    const authorPubkey = latestMessageEvent.pubkey.toLowerCase();
    latestMessageCreatedAtByPubkeyRef.current[authorPubkey] = Math.max(
      latestMessageCreatedAtByPubkeyRef.current[authorPubkey] ?? 0,
      latestMessageEvent.created_at,
    );
    typingSuppressUntilByPubkeyRef.current[authorPubkey] =
      Date.now() + TYPING_POST_MESSAGE_SUPPRESS_MS;
    setTypingByPubkey((current) => {
      const next = pruneTypingState(current);
      if (!(authorPubkey in next)) {
        return next;
      }

      const updated = { ...next };
      delete updated[authorPubkey];
      return updated;
    });
  }, [channelId, latestMessageEvent]);

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

  const orderedEntries = useMemo(
    () =>
      Object.entries(typingByPubkey)
        .map(([pubkey, entry]) => ({ pubkey, ...entry }))
        .sort((left, right) => left.firstSeenAt - right.firstSeenAt),
    [typingByPubkey],
  );

  const mainComposerTypingPubkeys = useMemo(
    () =>
      orderedEntries
        .filter((entry) => entry.replyParentId === null)
        .map((entry) => entry.pubkey),
    [orderedEntries],
  );

  const threadComposerTypingPubkeys = useMemo(() => {
    if (!openThreadMessageIds || openThreadMessageIds.size === 0) {
      return [];
    }

    return orderedEntries
      .filter(
        (entry) =>
          entry.replyParentId !== null &&
          openThreadMessageIds.has(entry.replyParentId),
      )
      .map((entry) => entry.pubkey);
  }, [openThreadMessageIds, orderedEntries]);

  return { mainComposerTypingPubkeys, threadComposerTypingPubkeys };
}
