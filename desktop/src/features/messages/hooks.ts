import { useEffect, useEffectEvent } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { updateChannelLastMessageAt } from "@/features/channels/hooks";
import {
  buildReplyTags,
  normalizeMentionPubkeys,
  resolveReplyRootId,
} from "@/features/messages/lib/threading";
import { relayClient } from "@/shared/api/relayClient";
import {
  addReaction,
  removeReaction,
  sendChannelMessage,
} from "@/shared/api/tauri";
import type { Channel, Identity, RelayEvent } from "@/shared/api/types";
import { KIND_STREAM_MESSAGE } from "@/shared/constants/kinds";

type MessageQueryContext = {
  optimisticId: string;
  previousMessages: RelayEvent[];
  queryKey: readonly ["channel-messages", string];
};

function dedupeMessagesById(messages: RelayEvent[]) {
  const seenIds = new Set<string>();
  const deduped: RelayEvent[] = [];

  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];

    if (seenIds.has(message.id)) {
      continue;
    }

    seenIds.add(message.id);
    deduped.push(message);
  }

  return deduped.reverse();
}

export function mergeMessages(
  current: RelayEvent[],
  incoming: RelayEvent,
): RelayEvent[] {
  const normalizedCurrent = dedupeMessagesById(current);
  const deduped = normalizedCurrent.filter(
    (message) =>
      message.id !== incoming.id &&
      !(message.pending && incoming.content === message.content),
  );

  return dedupeMessagesById([...deduped, incoming]).sort(
    (left, right) => left.created_at - right.created_at,
  );
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
  const queryKey = ["channel-messages", channel?.id ?? "none"] as const;

  return useQuery({
    enabled: channel !== null && channel.channelType !== "forum",
    placeholderData: () => queryClient.getQueryData<RelayEvent[]>(queryKey),
    queryKey,
    queryFn: async () => {
      if (!channel) {
        throw new Error("No channel selected.");
      }

      const history = await relayClient.fetchChannelHistory(channel.id, 200);
      const currentMessages =
        queryClient.getQueryData<RelayEvent[]>(queryKey) ?? [];
      const mergedHistory = dedupeMessagesById([
        ...currentMessages,
        ...history,
      ]).sort((left, right) => left.created_at - right.created_at);

      return mergedHistory;
    },
    staleTime: Number.POSITIVE_INFINITY,
    gcTime: 30 * 60 * 1_000,
  });
}

export function useChannelSubscription(channel: Channel | null) {
  const queryClient = useQueryClient();
  const channelId = channel?.id ?? null;
  const channelType = channel?.channelType ?? null;
  const syncLatestHistory = useEffectEvent(async () => {
    if (!channelId) {
      return;
    }

    const history = await relayClient.fetchChannelHistory(channelId, 200);

    queryClient.setQueryData<RelayEvent[]>(
      ["channel-messages", channelId],
      (current = []) => {
        const mergedHistory = dedupeMessagesById([...current, ...history]).sort(
          (left, right) => left.created_at - right.created_at,
        );

        return mergedHistory;
      },
    );
  });

  const appendMessage = useEffectEvent((event: RelayEvent) => {
    if (!channelId) {
      return;
    }

    updateChannelLastMessageAt(
      queryClient,
      channelId,
      new Date(event.created_at * 1_000).toISOString(),
    );
    queryClient.setQueryData<RelayEvent[]>(
      ["channel-messages", channelId],
      (current = []) => mergeMessages(current, event),
    );
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
          queryClient.getQueryData<RelayEvent[]>([
            "channel-messages",
            channel.id,
          ]) ?? [];
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

      const queryKey = ["channel-messages", channel.id] as const;
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
        mergeMessages(previousMessages, optimisticMessage),
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
          return mergeMessages(withoutOptimistic, message);
        },
      );
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
