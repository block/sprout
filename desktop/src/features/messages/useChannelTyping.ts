import {
  useEffect,
  useEffectEvent,
  useMemo,
  useRef,
  useState,
  type Dispatch,
  type SetStateAction,
} from "react";

import {
  getChannelIdFromTags,
  getThreadReference,
} from "@/features/messages/lib/threading";
import { relayClient } from "@/shared/api/relayClient";
import type { Channel, RelayEvent } from "@/shared/api/types";
import {
  getRelayEventTraceData,
  traceRelayEvent,
} from "@/shared/lib/relayEventTrace";
import {
  KIND_STREAM_MESSAGE,
  KIND_STREAM_MESSAGE_DIFF,
  KIND_TYPING_INDICATOR,
} from "@/shared/constants/kinds";

type TypingEntry = { expiresAt: number; firstSeenAt: number };
type TypingState = Record<string, TypingEntry>;
type TypingLists = {
  channelTypingPubkeys: string[];
  threadTypingPubkeys: string[];
};

const TYPING_INDICATOR_TTL_MS = 8_000;
const TYPING_PRUNE_INTERVAL_MS = 1_000;
const TYPING_POST_MESSAGE_SUPPRESS_MS = 2_000;
const CHANNEL_SCOPE_KEY = "__channel__";
const TYPING_EVENT_MAX_AGE_MS = TYPING_INDICATOR_TTL_MS + 2_000;

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

export function useChannelTyping(
  channel: Channel | null,
  currentPubkey?: string,
  latestMessageEvent?: RelayEvent | null,
  activeThreadRootId?: string | null,
): TypingLists {
  const channelId = channel?.id ?? null;
  const channelType = channel?.channelType ?? null;
  const [channelTypingByPubkey, setChannelTypingByPubkey] =
    useState<TypingState>({});
  const [threadTypingByPubkey, setThreadTypingByPubkey] =
    useState<TypingState>({});
  const normalizedCurrentPubkey = currentPubkey?.toLowerCase();
  const typingSuppressUntilByScopeRef = useRef<Record<string, number>>({});
  const latestMessageCreatedAtByScopeRef = useRef<Record<string, number>>({});

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

    const eventAgeMs = Date.now() - event.created_at * 1_000;
    if (eventAgeMs > TYPING_EVENT_MAX_AGE_MS) {
      return;
    }

    const threadRootId = getThreadReference(event.tags).rootId;
    const scopeKey = `${threadRootId ?? CHANNEL_SCOPE_KEY}:${typingPubkey}`;
    const suppressUntil = typingSuppressUntilByScopeRef.current[scopeKey] ?? 0;
    if (suppressUntil > Date.now()) {
      return;
    }
    if (suppressUntil > 0) {
      delete typingSuppressUntilByScopeRef.current[scopeKey];
    }

    const latestMessageCreatedAt =
      latestMessageCreatedAtByScopeRef.current[scopeKey] ?? 0;
    if (event.created_at <= latestMessageCreatedAt) {
      return;
    }

    const branch =
      threadRootId && activeThreadRootId === threadRootId
        ? "thread"
        : !threadRootId
          ? "channel"
          : "ignored_non_active_thread";

    traceRelayEvent(
      threadRootId ? "H2" : "H3",
      "desktop/src/features/messages/useChannelTyping.ts:114",
      "typing event classified",
      {
        channelId,
        typingPubkey,
        activeThreadRootId,
        parsedThreadRootId: threadRootId,
        branch,
        ...getRelayEventTraceData(event),
      },
    );

    const now = Date.now();
    const registerInState = (setState: Dispatch<SetStateAction<TypingState>>) => {
      setState((current) => {
        const pruned = pruneTypingState(current, now);
        const existing = pruned[typingPubkey];
        return {
          ...pruned,
          [typingPubkey]: {
            expiresAt: now + TYPING_INDICATOR_TTL_MS,
            firstSeenAt: existing?.firstSeenAt ?? now,
          },
        };
      });
    };
    const clearTypingInState = (
      setState: Dispatch<SetStateAction<TypingState>>,
    ) => {
      setState((current) => {
        const next = pruneTypingState(current, now);
        if (!(typingPubkey in next)) {
          return next;
        }

        const updated = { ...next };
        delete updated[typingPubkey];
        return updated;
      });
    };

    if (threadRootId && activeThreadRootId === threadRootId) {
      clearTypingInState(setChannelTypingByPubkey);
      registerInState(setThreadTypingByPubkey);
      return;
    }

    if (!threadRootId) {
      clearTypingInState(setThreadTypingByPubkey);
      registerInState(setChannelTypingByPubkey);
    }
  });

  // biome-ignore lint/correctness/useExhaustiveDependencies: channel changes should clear local typing state
  useEffect(() => {
    setChannelTypingByPubkey({});
    setThreadTypingByPubkey({});
    typingSuppressUntilByScopeRef.current = {};
    latestMessageCreatedAtByScopeRef.current = {};
  }, [channelId]);

  useEffect(() => {
    setThreadTypingByPubkey({});
  }, [activeThreadRootId]);

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
    const threadRootId = getThreadReference(latestMessageEvent.tags).rootId;
    const scopeKey = `${threadRootId ?? CHANNEL_SCOPE_KEY}:${authorPubkey}`;
    latestMessageCreatedAtByScopeRef.current[scopeKey] = Math.max(
      latestMessageCreatedAtByScopeRef.current[scopeKey] ?? 0,
      latestMessageEvent.created_at,
    );
    typingSuppressUntilByScopeRef.current[scopeKey] =
      Date.now() + TYPING_POST_MESSAGE_SUPPRESS_MS;
    const clearTypingInState = (
      setState: Dispatch<SetStateAction<TypingState>>,
    ) => {
      setState((current) => {
        const next = pruneTypingState(current);
        if (!(authorPubkey in next)) {
          return next;
        }

        const updated = { ...next };
        delete updated[authorPubkey];
        return updated;
      });
    };

    if (threadRootId && activeThreadRootId === threadRootId) {
      clearTypingInState(setThreadTypingByPubkey);
    }

    clearTypingInState(setChannelTypingByPubkey);
  }, [activeThreadRootId, channelId, latestMessageEvent]);

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

  const hasActiveTypers =
    Object.keys(channelTypingByPubkey).length > 0 ||
    Object.keys(threadTypingByPubkey).length > 0;

  useEffect(() => {
    if (!hasActiveTypers) {
      return;
    }

    const interval = window.setInterval(() => {
      setChannelTypingByPubkey((current) => pruneTypingState(current));
      setThreadTypingByPubkey((current) => pruneTypingState(current));
    }, TYPING_PRUNE_INTERVAL_MS);

    return () => {
      window.clearInterval(interval);
    };
  }, [hasActiveTypers]);

  const channelTypingPubkeys = useMemo(
    () =>
      Object.entries(channelTypingByPubkey)
        .sort((left, right) => left[1].firstSeenAt - right[1].firstSeenAt)
        .map(([pubkey]) => pubkey),
    [channelTypingByPubkey],
  );

  const threadTypingPubkeys = useMemo(
    () =>
      Object.entries(threadTypingByPubkey)
        .sort((left, right) => left[1].firstSeenAt - right[1].firstSeenAt)
        .map(([pubkey]) => pubkey),
    [threadTypingByPubkey],
  );

  useEffect(() => {
    if (
      !channelId ||
      (!activeThreadRootId &&
        channelTypingPubkeys.length === 0 &&
        threadTypingPubkeys.length === 0)
    ) {
      return;
    }

    traceRelayEvent(
      "H2",
      "desktop/src/features/messages/useChannelTyping.ts:286",
      "typing lists updated",
      {
        channelId,
        activeThreadRootId,
        channelTypingPubkeys,
        threadTypingPubkeys,
      },
    );
  }, [activeThreadRootId, channelId, channelTypingPubkeys, threadTypingPubkeys]);

  return useMemo(
    () => ({
      channelTypingPubkeys,
      threadTypingPubkeys,
    }),
    [channelTypingPubkeys, threadTypingPubkeys],
  );
}
