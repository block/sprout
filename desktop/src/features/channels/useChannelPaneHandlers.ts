import * as React from "react";

import type {
  useDeleteMessageMutation,
  useEditMessageMutation,
  useSendMessageMutation,
  useToggleReactionMutation,
} from "@/features/messages/hooks";

/**
 * Stable callback references for ChannelPane so that keystroke-driven
 * re-renders of ChannelScreen don't cascade into the timeline and composer.
 *
 * Mutation objects from TanStack Query v5 are new references on every render
 * (especially when `isPending` flips), so we stash `.mutateAsync` in a ref
 * rather than listing the whole mutation as a dependency.
 */
export function useChannelPaneHandlers({
  deleteMessageMutation,
  editMessageMutation,
  editTargetId,
  openThreadHeadId,
  sendMessageMutation,
  setEditTargetId,
  setThreadHeadPath,
  setThreadReplyTargetId,
  threadReplyTargetId,
  toggleReactionMutation,
}: {
  deleteMessageMutation: ReturnType<typeof useDeleteMessageMutation>;
  editMessageMutation: ReturnType<typeof useEditMessageMutation>;
  editTargetId: string | null;
  openThreadHeadId: string | null;
  sendMessageMutation: ReturnType<typeof useSendMessageMutation>;
  setEditTargetId: React.Dispatch<React.SetStateAction<string | null>>;
  setThreadHeadPath: React.Dispatch<React.SetStateAction<string[]>>;
  setThreadReplyTargetId: React.Dispatch<React.SetStateAction<string | null>>;
  threadReplyTargetId: string | null;
  toggleReactionMutation: ReturnType<typeof useToggleReactionMutation>;
}) {
  // Keep mutable values in refs so callbacks never need to list them as deps.
  const openThreadHeadIdRef = React.useRef(openThreadHeadId);
  openThreadHeadIdRef.current = openThreadHeadId;

  const threadReplyTargetIdRef = React.useRef(threadReplyTargetId);
  threadReplyTargetIdRef.current = threadReplyTargetId;

  const editTargetIdRef = React.useRef(editTargetId);
  editTargetIdRef.current = editTargetId;

  const sendMutateRef = React.useRef(sendMessageMutation.mutateAsync);
  sendMutateRef.current = sendMessageMutation.mutateAsync;

  const deleteMutateRef = React.useRef(deleteMessageMutation.mutateAsync);
  deleteMutateRef.current = deleteMessageMutation.mutateAsync;

  const editMutateRef = React.useRef(editMessageMutation.mutateAsync);
  editMutateRef.current = editMessageMutation.mutateAsync;

  const toggleMutateRef = React.useRef(toggleReactionMutation.mutateAsync);
  toggleMutateRef.current = toggleReactionMutation.mutateAsync;

  const handleCancelThreadReply = React.useCallback(() => {
    setThreadReplyTargetId(openThreadHeadIdRef.current);
  }, [setThreadReplyTargetId]);

  const handleCloseThread = React.useCallback(() => {
    setThreadHeadPath([]);
    setThreadReplyTargetId(null);
  }, [setThreadHeadPath, setThreadReplyTargetId]);

  const handleBackThread = React.useCallback(() => {
    setThreadHeadPath((current) => {
      if (current.length <= 1) {
        return current;
      }
      const nextPath = current.slice(0, -1);
      setThreadReplyTargetId(nextPath[nextPath.length - 1] ?? null);
      return nextPath;
    });
  }, [setThreadHeadPath, setThreadReplyTargetId]);

  const handleCancelEdit = React.useCallback(() => {
    setEditTargetId(null);
  }, [setEditTargetId]);

  const handleDelete = React.useCallback(async (message: { id: string }) => {
    await deleteMutateRef.current({ eventId: message.id });
  }, []);

  const handleEdit = React.useCallback(
    (message: { id: string }) => {
      setEditTargetId((current) =>
        current === message.id ? null : message.id,
      );
      setThreadReplyTargetId(openThreadHeadIdRef.current);
    },
    [setEditTargetId, setThreadReplyTargetId],
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

  const handleOpenThread = React.useCallback(
    (message: { id: string }) => {
      if (openThreadHeadIdRef.current === message.id) {
        setThreadHeadPath([]);
        setThreadReplyTargetId(null);
        setEditTargetId(null);
        return;
      }

      setThreadHeadPath([message.id]);
      setThreadReplyTargetId(message.id);
      setEditTargetId(null);
    },
    [setEditTargetId, setThreadHeadPath, setThreadReplyTargetId],
  );

  const handleOpenNestedThread = React.useCallback(
    (message: { id: string }) => {
      setThreadHeadPath((current) => {
        if (current[current.length - 1] === message.id) {
          return current;
        }
        return [...current, message.id];
      });
      setThreadReplyTargetId(message.id);
      setEditTargetId(null);
    },
    [setEditTargetId, setThreadHeadPath, setThreadReplyTargetId],
  );

  const handleSendMessage = React.useCallback(
    async (
      content: string,
      mentionPubkeys: string[],
      mediaTags?: string[][],
    ) => {
      await sendMutateRef.current({
        content,
        mentionPubkeys,
        mediaTags,
      });
    },
    [],
  );

  const handleSendThreadReply = React.useCallback(
    async (
      content: string,
      mentionPubkeys: string[],
      mediaTags?: string[][],
    ) => {
      const parentEventId =
        threadReplyTargetIdRef.current ?? openThreadHeadIdRef.current;
      if (!parentEventId) {
        return;
      }

      await sendMutateRef.current({
        content,
        mentionPubkeys,
        parentEventId,
        mediaTags,
      });
      setThreadReplyTargetId(openThreadHeadIdRef.current);
    },
    [setThreadReplyTargetId],
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
    handleCancelThreadReply,
    handleBackThread,
    handleCloseThread,
    handleDelete,
    handleEdit,
    handleEditSave,
    handleOpenNestedThread,
    handleOpenThread,
    handleSendMessage,
    handleSendThreadReply,
    handleToggleReaction,
  };
}
