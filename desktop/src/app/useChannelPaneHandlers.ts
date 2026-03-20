import * as React from "react";

import type { useSendMessageMutation } from "@/features/messages/hooks";
import type { useToggleReactionMutation } from "@/features/messages/hooks";

/**
 * Stable callback references for ChannelPane so that keystroke-driven
 * re-renders of AppShell don't cascade into the timeline and composer.
 *
 * Mutation objects from TanStack Query v5 are new references on every render
 * (especially when `isPending` flips), so we stash `.mutateAsync` in a ref
 * rather than listing the whole mutation as a dependency.
 */
export function useChannelPaneHandlers({
  replyTargetId,
  sendMessageMutation,
  setReplyTargetId,
  toggleReactionMutation,
}: {
  replyTargetId: string | null;
  sendMessageMutation: ReturnType<typeof useSendMessageMutation>;
  setReplyTargetId: React.Dispatch<React.SetStateAction<string | null>>;
  toggleReactionMutation: ReturnType<typeof useToggleReactionMutation>;
}) {
  // Keep mutable values in refs so callbacks never need to list them as deps.
  const replyTargetIdRef = React.useRef(replyTargetId);
  replyTargetIdRef.current = replyTargetId;

  const sendMutateRef = React.useRef(sendMessageMutation.mutateAsync);
  sendMutateRef.current = sendMessageMutation.mutateAsync;

  const toggleMutateRef = React.useRef(toggleReactionMutation.mutateAsync);
  toggleMutateRef.current = toggleReactionMutation.mutateAsync;

  const handleCancelReply = React.useCallback(() => {
    setReplyTargetId(null);
  }, [setReplyTargetId]);

  const handleReply = React.useCallback(
    (message: { id: string }) => {
      setReplyTargetId((current) =>
        current === message.id ? null : message.id,
      );
    },
    [setReplyTargetId],
  );

  const handleSend = React.useCallback(
    async (
      content: string,
      mentionPubkeys: string[],
      mediaTags?: string[][],
    ) => {
      await sendMutateRef.current({
        content,
        mentionPubkeys,
        parentEventId: replyTargetIdRef.current,
        mediaTags,
      });
      setReplyTargetId(null);
    },
    [setReplyTargetId],
  );

  const handleToggleReaction = React.useCallback(
    async (message: { id: string }, emoji: string, remove: boolean) => {
      await toggleMutateRef.current({
        emoji,
        eventId: message.id,
        remove,
      });
    },
    [],
  );

  return {
    handleCancelReply,
    handleReply,
    handleSend,
    handleToggleReaction,
  };
}
