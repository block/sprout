import { useEffect, useEffectEvent, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { updateChannelLastMessageAt } from "@/features/channels/lib/channelCache";
import {
  channelMessagesKey,
  channelThreadKey,
  dedupeMessagesById,
  normalizeTimelineMessages,
  sortMessages,
} from "@/features/messages/lib/messageQueryKeys";
import {
  buildReplyTags,
  getThreadReference,
  normalizeMentionPubkeys,
  resolveReplyRootId,
} from "@/features/messages/lib/threading";
import {
  getRelayEventTraceData,
  traceRelayEvent,
} from "@/shared/lib/relayEventTrace";
import { relayClient } from "@/shared/api/relayClient";
import {
  addReaction,
  deleteMessage,
  editMessage,
  getChannelMessages,
  removeReaction,
  sendChannelMessage,
} from "@/shared/api/tauri";
import type { Channel, Identity, RelayEvent } from "@/shared/api/types";
import {
  KIND_DELETION,
  KIND_STREAM_MESSAGE,
  KIND_STREAM_MESSAGE_DIFF,
  KIND_SYSTEM_MESSAGE,
} from "@/shared/constants/kinds";

type MessageQueryContext = {
  optimisticId: string;
  previousMessages: RelayEvent[];
  queryKey: ReturnType<typeof channelMessagesKey>;
};

const CHANNEL_HISTORY_LIMIT = 200;
const TOP_LEVEL_TIMELINE_KINDS = [
  KIND_STREAM_MESSAGE,
  40001,
  KIND_STREAM_MESSAGE_DIFF,
  KIND_SYSTEM_MESSAGE,
];

function mergeMessagesWithNormalizer(
  current: RelayEvent[],
  incoming: RelayEvent,
  normalize: (messages: RelayEvent[]) => RelayEvent[],
): RelayEvent[] {
  const normalizedCurrent = dedupeMessagesById(current);
  const deduped = normalizedCurrent.filter(
    (message) =>
      message.id !== incoming.id &&
      !(message.pending && incoming.content === message.content),
  );

  return normalize([...deduped, incoming]);
}

export function mergeMessages(
  current: RelayEvent[],
  incoming: RelayEvent,
): RelayEvent[] {
  return mergeMessagesWithNormalizer(current, incoming, sortMessages);
}

export function mergeTimelineCacheMessages(
  current: RelayEvent[],
  incoming: RelayEvent,
): RelayEvent[] {
  return mergeMessagesWithNormalizer(
    current,
    incoming,
    normalizeTimelineMessages,
  );
}

async function fetchTopLevelMessagesWithThreadSummaries(channelId: string) {
  return getChannelMessages({
    channelId,
    kinds: TOP_LEVEL_TIMELINE_KINDS,
    limit: CHANNEL_HISTORY_LIMIT,
  });
}

async function fetchMergedChannelHistory(channelId: string) {
  const [history, topLevelMessages] = await Promise.all([
    relayClient.fetchChannelHistory(channelId, CHANNEL_HISTORY_LIMIT),
    fetchTopLevelMessagesWithThreadSummaries(channelId),
  ]);

  return normalizeTimelineMessages([...history, ...topLevelMessages]);
}

function mergeThreadSummariesIntoCache(
  current: RelayEvent[],
  topLevelMessages: RelayEvent[],
) {
  return normalizeTimelineMessages([...current, ...topLevelMessages]);
}

function eventAffectsThreadSummary(event: RelayEvent) {
  if (event.kind === KIND_DELETION) {
    return true;
  }

  return getThreadReference(event.tags).parentId !== null;
}

function getThreadRootId(event: RelayEvent) {
  const thread = getThreadReference(event.tags);
  return thread.rootId ?? thread.parentId;
}

function createOptimisticMessage(
  channelId: string,
  content: string,
  identity: Identity,
  currentMessages: RelayEvent[],
  mentionPubkeys: string[] = [],
  parentEventId: string | null = null,
  mediaTags: string[][] = [],
): RelayEvent {
  const tags: string[][] = [];

  if (parentEventId) {
    tags.push(
      ...buildReplyTags(
        channelId,
        identity.pubkey,
        parentEventId,
        resolveReplyRootId(parentEventId, currentMessages),
        mentionPubkeys,
      ),
    );
  } else {
    tags.push(["h", channelId]);
    tags.push(["p", identity.pubkey]);
    for (const pubkey of normalizeMentionPubkeys(
      mentionPubkeys,
      identity.pubkey,
    )) {
      tags.push(["p", pubkey]);
    }
  }

  for (const tag of mediaTags) {
    tags.push(tag);
  }

  return {
    id: `optimistic-${crypto.randomUUID()}`,
    pubkey: identity.pubkey,
    created_at: Math.floor(Date.now() / 1_000),
    kind: KIND_STREAM_MESSAGE,
    tags,
    content,
    sig: "",
    pending: true,
  };
}

export function useChannelMessagesQuery(channel: Channel | null) {
  const queryClient = useQueryClient();
  const queryKey = channelMessagesKey(channel?.id ?? "none");

  return useQuery({
    enabled: channel !== null && channel.channelType !== "forum",
    placeholderData: () => queryClient.getQueryData<RelayEvent[]>(queryKey),
    queryKey,
    queryFn: async () => {
      if (!channel) {
        throw new Error("No channel selected.");
      }

      const currentMessages =
        queryClient.getQueryData<RelayEvent[]>(queryKey) ?? [];
      const mergedHistory = await fetchMergedChannelHistory(channel.id);

      return normalizeTimelineMessages([...currentMessages, ...mergedHistory]);
    },
    staleTime: Number.POSITIVE_INFINITY,
    gcTime: 5 * 60 * 1_000,
  });
}

export function useChannelSubscription(channel: Channel | null): {
  latestLiveEvent: RelayEvent | null;
} {
  const queryClient = useQueryClient();
  const channelId = channel?.id ?? null;
  const channelType = channel?.channelType ?? null;
  const [latestLiveEvent, setLatestLiveEvent] = useState<RelayEvent | null>(
    null,
  );
  const latestLiveEventChannelRef = useRef<string | null>(null);

  useEffect(() => {
    if (latestLiveEventChannelRef.current !== channelId) {
      setLatestLiveEvent(null);
      latestLiveEventChannelRef.current = channelId;
    }
  }, [channelId]);

  const syncLatestHistory = useEffectEvent(async () => {
    if (!channelId) {
      return;
    }

    const history = await fetchMergedChannelHistory(channelId);

    queryClient.setQueryData<RelayEvent[]>(
      channelMessagesKey(channelId),
      (current = []) => normalizeTimelineMessages([...current, ...history]),
    );
  });
  const refreshThreadSummaries = useEffectEvent(async () => {
    if (!channelId) {
      return;
    }

    const topLevelMessages = await fetchTopLevelMessagesWithThreadSummaries(
      channelId,
    );
    queryClient.setQueryData<RelayEvent[]>(
      channelMessagesKey(channelId),
      (current = []) => mergeThreadSummariesIntoCache(current, topLevelMessages),
    );
  });

  const appendMessage = useEffectEvent((event: RelayEvent) => {
    if (!channelId) {
      return;
    }

    traceRelayEvent(
      "H6",
      "desktop/src/features/messages/hooks.ts:235",
      "live channel event received",
      {
        channelId,
        ...getRelayEventTraceData(event),
      },
    );

    if (
      event.kind === KIND_STREAM_MESSAGE ||
      event.kind === KIND_STREAM_MESSAGE_DIFF
    ) {
      setLatestLiveEvent(event);
    }

    updateChannelLastMessageAt(
      queryClient,
      channelId,
      new Date(event.created_at * 1_000).toISOString(),
    );
    queryClient.setQueryData<RelayEvent[]>(
      channelMessagesKey(channelId),
      (current = []) => mergeTimelineCacheMessages(current, event),
    );

    const threadRootId = getThreadRootId(event);
    if (threadRootId) {
      void queryClient.invalidateQueries({
        queryKey: channelThreadKey(channelId, threadRootId),
      });
    }

    if (eventAffectsThreadSummary(event)) {
      void refreshThreadSummaries().catch((error) => {
        console.error(
          "Failed to refresh thread summaries after channel event",
          channelId,
          error,
        );
      });
    }
  });

  useEffect(() => {
    if (!channelId || channelType === "forum") {
      return;
    }

    let isDisposed = false;
    let cleanup: (() => Promise<void>) | undefined;
    const disposeReconnectListener = relayClient.subscribeToReconnects(() => {
      void syncLatestHistory().catch((error) => {
        if (!isDisposed) {
          console.error(
            "Failed to refresh channel history after reconnecting",
            channelId,
            error,
          );
        }
      });
    });

    relayClient
      .subscribeToChannel(channelId, (event) => {
        if (!isDisposed) {
          appendMessage(event);
        }
      })
      .then((dispose) => {
        if (isDisposed) {
          void dispose();
          return;
        }

        cleanup = dispose;

        void syncLatestHistory().catch((error) => {
          if (!isDisposed) {
            console.error(
              "Failed to refresh channel history after subscribing",
              channelId,
              error,
            );
          }
        });
      })
      .catch((error) => {
        console.error("Failed to subscribe to channel", channelId, error);
      });

    return () => {
      isDisposed = true;
      disposeReconnectListener();
      if (cleanup) {
        void cleanup();
      }
    };
  }, [channelId, channelType]);

  return { latestLiveEvent };
}

export function useSendMessageMutation(
  channel: Channel | null,
  identity: Identity | undefined,
) {
  const queryClient = useQueryClient();

  return useMutation<
    RelayEvent,
    Error,
    {
      content: string;
      mentionPubkeys?: string[];
      parentEventId?: string | null;
      mediaTags?: string[][];
    },
    MessageQueryContext | undefined
  >({
    mutationFn: async ({
      content,
      mentionPubkeys,
      parentEventId,
      mediaTags,
    }) => {
      if (!channel || channel.channelType === "forum") {
        throw new Error("This channel does not support message sending yet.");
      }

      if (!identity) {
        throw new Error("No identity available for sending messages.");
      }

      // Media-bearing messages MUST go through REST so the relay's imeta
      // validation runs. The WebSocket path does not validate imeta tags.
      if (parentEventId || (mediaTags && mediaTags.length > 0)) {
        const cachedMessages =
          queryClient.getQueryData<RelayEvent[]>(
            channelMessagesKey(channel.id),
          ) ?? [];
        const result = await sendChannelMessage(
          channel.id,
          content,
          parentEventId ?? null,
          mediaTags,
          mentionPubkeys,
        );

        // Build tags matching relay-emitted shape: h, author p, mention ps, reply es, imeta.
        // For replies, buildReplyTags already includes ["p", author] and ["h", channel].
        // For non-replies (media-only), we add them ourselves.
        const replyTags = parentEventId
          ? buildReplyTags(
              channel.id,
              identity.pubkey,
              parentEventId,
              resolveReplyRootId(parentEventId, cachedMessages),
              mentionPubkeys,
            )
          : [];
        const baseTags = parentEventId
          ? replyTags // buildReplyTags includes h + author p + mention ps
          : [
              ["h", channel.id],
              ["p", identity.pubkey],
            ]; // non-reply: add ourselves

        return {
          id: result.eventId,
          pubkey: identity.pubkey,
          created_at: result.createdAt,
          kind: KIND_STREAM_MESSAGE,
          tags: [
            ...baseTags,
            // For non-replies, add mention p-tags here (replies get them via buildReplyTags)
            ...(!parentEventId
              ? normalizeMentionPubkeys(
                  mentionPubkeys ?? [],
                  identity.pubkey,
                ).map((pk) => ["p", pk])
              : []),
            ...(mediaTags ?? []),
          ],
          content: content.trim(),
          sig: "",
        };
      }

      return relayClient.sendMessage(
        channel.id,
        content,
        mentionPubkeys ?? [],
        [],
      );
    },
    onMutate: async ({ content, mentionPubkeys, parentEventId, mediaTags }) => {
      if (!channel || !identity || channel.channelType === "forum") {
        return undefined;
      }

      const queryKey = channelMessagesKey(channel.id);
      await queryClient.cancelQueries({ queryKey });

      const previousMessages =
        queryClient.getQueryData<RelayEvent[]>(queryKey) ?? [];
      const optimisticMessage = createOptimisticMessage(
        channel.id,
        content.trim(),
        identity,
        previousMessages,
        mentionPubkeys ?? [],
        parentEventId ?? null,
        mediaTags ?? [],
      );

      queryClient.setQueryData<RelayEvent[]>(
        queryKey,
        mergeTimelineCacheMessages(previousMessages, optimisticMessage),
      );

      return {
        optimisticId: optimisticMessage.id,
        previousMessages,
        queryKey,
      };
    },
    onError: (_error, _variables, context) => {
      if (!context) {
        return;
      }

      queryClient.setQueryData(context.queryKey, context.previousMessages);
    },
    onSuccess: (message, _variables, context) => {
      if (channel) {
        updateChannelLastMessageAt(
          queryClient,
          channel.id,
          new Date(message.created_at * 1_000).toISOString(),
        );
      }

      if (!context) {
        return;
      }

      queryClient.setQueryData<RelayEvent[]>(
        context.queryKey,
        (current = []) => {
          const withoutOptimistic = current.filter(
            (item) => item.id !== context.optimisticId,
          );
          return mergeTimelineCacheMessages(withoutOptimistic, message);
        },
      );

      if (_variables.parentEventId && channel) {
        void queryClient.invalidateQueries({
          queryKey: channelMessagesKey(channel.id),
        });
      }
    },
  });
}

export function useToggleReactionMutation() {
  return useMutation<
    void,
    Error,
    {
      eventId: string;
      emoji: string;
      remove: boolean;
    }
  >({
    mutationFn: async ({ eventId, emoji, remove }) => {
      if (remove) {
        await removeReaction(eventId, emoji);
        return;
      }

      await addReaction(eventId, emoji);
    },
  });
}

export function useDeleteMessageMutation(channel: Channel | null) {
  const queryClient = useQueryClient();

  return useMutation<void, Error, { eventId: string }>({
    mutationFn: async ({ eventId }) => {
      await deleteMessage(eventId);
    },
    onSuccess: (_data, { eventId }) => {
      if (!channel) return;
      queryClient.setQueryData<RelayEvent[]>(
        channelMessagesKey(channel.id),
        (current = []) => current.filter((message) => message.id !== eventId),
      );
      void queryClient.invalidateQueries({
        queryKey: channelMessagesKey(channel.id),
      });
    },
  });
}

export function useEditMessageMutation(channel: Channel | null) {
  const queryClient = useQueryClient();

  return useMutation<
    void,
    Error,
    {
      eventId: string;
      content: string;
    }
  >({
    mutationFn: async ({ eventId, content }) => {
      if (!channel) {
        throw new Error("No channel selected.");
      }

      await editMessage(channel.id, eventId, content);
    },
    onSuccess: (_data, { eventId, content }) => {
      if (!channel) {
        return;
      }

      queryClient.setQueryData<RelayEvent[]>(
        channelMessagesKey(channel.id),
        (current = []) =>
          current.map((message) =>
            message.id === eventId ? { ...message, content } : message,
          ),
      );
    },
  });
}
