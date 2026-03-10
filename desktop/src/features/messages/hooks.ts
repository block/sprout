import { useEffect, useEffectEvent } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { relayClient } from "@/shared/api/relayClient";
import type { Channel, Identity, RelayEvent } from "@/shared/api/types";

type MessageQueryContext = {
  optimisticId: string;
  previousMessages: RelayEvent[];
  queryKey: readonly ["channel-messages", string];
};

export function mergeMessages(
  current: RelayEvent[],
  incoming: RelayEvent,
): RelayEvent[] {
  const deduped = current.filter(
    (message) =>
      message.id !== incoming.id &&
      !(message.pending && incoming.content === message.content),
  );

  return [...deduped, incoming].sort(
    (left, right) => left.created_at - right.created_at,
  );
}

function createOptimisticMessage(
  channelId: string,
  content: string,
  identity: Identity,
): RelayEvent {
  return {
    id: `optimistic-${crypto.randomUUID()}`,
    pubkey: identity.pubkey,
    created_at: Math.floor(Date.now() / 1_000),
    kind: 4_0001,
    tags: [["h", channelId]],
    content,
    sig: "",
    pending: true,
  };
}

export function useChannelMessagesQuery(channel: Channel | null) {
  return useQuery({
    enabled: channel !== null && channel.channelType !== "forum",
    queryKey: ["channel-messages", channel?.id ?? "none"],
    queryFn: async () => {
      if (!channel) {
        throw new Error("No channel selected.");
      }

      return relayClient.fetchChannelHistory(channel.id);
    },
    staleTime: Number.POSITIVE_INFINITY,
    gcTime: 30 * 60 * 1_000,
  });
}

export function useChannelSubscription(channel: Channel | null) {
  const queryClient = useQueryClient();

  const appendMessage = useEffectEvent((event: RelayEvent) => {
    if (!channel) {
      return;
    }

    queryClient.setQueryData<RelayEvent[]>(
      ["channel-messages", channel.id],
      (current = []) => mergeMessages(current, event),
    );
  });

  useEffect(() => {
    if (!channel || channel.channelType === "forum") {
      return;
    }

    let isDisposed = false;
    let cleanup: (() => Promise<void>) | undefined;

    relayClient
      .subscribeToChannel(channel.id, (event) => {
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
      })
      .catch((error) => {
        console.error("Failed to subscribe to channel", channel.id, error);
      });

    return () => {
      isDisposed = true;
      if (cleanup) {
        void cleanup();
      }
    };
  }, [channel]);
}

export function useSendMessageMutation(
  channel: Channel | null,
  identity: Identity | undefined,
) {
  const queryClient = useQueryClient();

  return useMutation<
    RelayEvent,
    Error,
    string,
    MessageQueryContext | undefined
  >({
    mutationFn: async (content) => {
      if (!channel || channel.channelType === "forum") {
        throw new Error("This channel does not support message sending yet.");
      }

      return relayClient.sendMessage(channel.id, content);
    },
    onMutate: async (content) => {
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
    onError: (_error, _content, context) => {
      if (!context) {
        return;
      }

      queryClient.setQueryData(context.queryKey, context.previousMessages);
    },
    onSuccess: (message, _content, context) => {
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
