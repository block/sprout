import * as React from "react";

import type {
  useEditMessageMutation,
  useSendMessageMutation,
  useToggleReactionMutation,
} from "@/features/messages/hooks";

/**
 * Stable callback references for ChannelPane so that keystroke-driven
 * re-renders of AppShell don't cascade into the timeline and composer.
 *
 * Mutation objects from TanStack Query v5 are new references on every render
 * (especially when `isPending` flips), so we stash `.mutateAsync` in a ref
 * rather than listing the whole mutation as a dependency.
 */
export function useChannelPaneHandlers({
  editMessageMutation,
  editTargetId,
  replyTargetId,
  sendMessageMutation,
  setEditTargetId,
  setReplyTargetId,
  toggleReactionMutation,
}: {
  editMessageMutation: ReturnType<typeof useEditMessageMutation>;
  editTargetId: string | null;
  replyTargetId: string | null;
  sendMessageMutation: ReturnType<typeof useSendMessageMutation>;
  setEditTargetId: React.Dispatch<React.SetStateAction<string | null>>;
  setReplyTargetId: React.Dispatch<React.SetStateAction<string | null>>;
  toggleReactionMutation: ReturnType<typeof useToggleReactionMutation>;
}) {
  // Keep mutable values in refs so callbacks never need to list them as deps.
  const replyTargetIdRef = React.useRef(replyTargetId);
  replyTargetIdRef.current = replyTargetId;

  const editTargetIdRef = React.useRef(editTargetId);
  editTargetIdRef.current = editTargetId;

  const sendMutateRef = React.useRef(sendMessageMutation.mutateAsync);
  sendMutateRef.current = sendMessageMutation.mutateAsync;

  const editMutateRef = React.useRef(editMessageMutation.mutateAsync);
  editMutateRef.current = editMessageMutation.mutateAsync;

  const toggleMutateRef = React.useRef(toggleReactionMutation.mutateAsync);
  toggleMutateRef.current = toggleReactionMutation.mutateAsync;

  const handleCancelReply = React.useCallback(() => {
    setReplyTargetId(null);
  }, [setReplyTargetId]);

  const handleCancelEdit = React.useCallback(() => {
    setEditTargetId(null);
  }, [setEditTargetId]);

  const handleEdit = React.useCallback(
    (message: { id: string }) => {
      setEditTargetId((current) =>
        current === message.id ? null : message.id,
      );
      // Clear reply when entering edit mode.
      setReplyTargetId(null);
    },
    [setEditTargetId, setReplyTargetId],
  );

  const handleEditSave = React.useCallback(
    async (content: string) => {
      const eventId = editTargetIdRef.current;
      if (!eventId) {
        return;
      }

      await editMutateRef.current({ eventId, content });
      setEditTargetId(null);
    },
    [setEditTargetId],
  );

  const handleReply = React.useCallback(
    (message: { id: string }) => {
      setReplyTargetId((current) =>
        current === message.id ? null : message.id,
      );
      // Clear edit when entering reply mode.
      setEditTargetId(null);
    },
    [setReplyTargetId, setEditTargetId],
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
    handleCancelEdit,
    handleCancelReply,
    handleEdit,
    handleEditSave,
    handleReply,
    handleSend,
    handleToggleReaction,
  };
}
