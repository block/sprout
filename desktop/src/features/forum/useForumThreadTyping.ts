import { useEffect, useEffectEvent, useMemo, useState } from "react";

import {
  getChannelIdFromTags,
  getTypingReplyParentFromTags,
} from "@/features/messages/lib/threading";
import { relayClient } from "@/shared/api/relayClient";
import type { RelayEvent } from "@/shared/api/types";
import { KIND_TYPING_INDICATOR } from "@/shared/constants/kinds";

type TypingEntry = {
  expiresAt: number;
  firstSeenAt: number;
  /** `null` = legacy event without `e` tag — treat as “typing toward thread root only”. */
  replyParentId: string | null;
};

type TypingState = Record<string, TypingEntry>;

const TYPING_INDICATOR_TTL_MS = 8_000;
const TYPING_PRUNE_INTERVAL_MS = 1_000;

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

/**
 * Subscribes to kind 20002 for a forum channel and splits typers by optional
 * `["e", parent, "", "reply"]` tag so indicators can sit beside the matching composer.
 */
export function useForumThreadTyping(
  channelId: string | null | undefined,
  currentPubkey: string | undefined,
  enabled: boolean,
  threadRootEventId: string,
  mainComposerParentId: string,
  branchPanelOpen: boolean,
  branchComposerParentId: string | null,
) {
  const normalizedSelf = currentPubkey?.toLowerCase();
  const [typingByPubkey, setTypingByPubkey] = useState<TypingState>({});

  const registerTyping = useEffectEvent((event: RelayEvent) => {
    if (!channelId || event.kind !== KIND_TYPING_INDICATOR) {
      return;
    }

    if (getChannelIdFromTags(event.tags) !== channelId) {
      return;
    }

    const typingPubkey = event.pubkey.toLowerCase();
    if (normalizedSelf && typingPubkey === normalizedSelf) {
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

  // biome-ignore lint/correctness/useExhaustiveDependencies: reset when channel or thread changes
  useEffect(() => {
    setTypingByPubkey({});
  }, [channelId, threadRootEventId]);

  useEffect(() => {
    if (!channelId || !enabled) {
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
  }, [channelId, enabled]);

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

  const mainComposerTypingPubkeys = useMemo(() => {
    const list: { pubkey: string; firstSeenAt: number }[] = [];
    for (const [pubkey, entry] of Object.entries(typingByPubkey)) {
      const { replyParentId } = entry;
      const matchesMain =
        replyParentId === mainComposerParentId ||
        (replyParentId === null && mainComposerParentId === threadRootEventId);
      if (matchesMain) {
        list.push({ pubkey, firstSeenAt: entry.firstSeenAt });
      }
    }
    return list
      .sort((a, b) => a.firstSeenAt - b.firstSeenAt)
      .map((x) => x.pubkey);
  }, [mainComposerParentId, threadRootEventId, typingByPubkey]);

  const branchComposerTypingPubkeys = useMemo(() => {
    if (!branchPanelOpen || !branchComposerParentId) {
      return [];
    }
    const list: { pubkey: string; firstSeenAt: number }[] = [];
    for (const [pubkey, entry] of Object.entries(typingByPubkey)) {
      if (entry.replyParentId === branchComposerParentId) {
        list.push({ pubkey, firstSeenAt: entry.firstSeenAt });
      }
    }
    return list
      .sort((a, b) => a.firstSeenAt - b.firstSeenAt)
      .map((x) => x.pubkey);
  }, [branchComposerParentId, branchPanelOpen, typingByPubkey]);

  return { branchComposerTypingPubkeys, mainComposerTypingPubkeys };
}
