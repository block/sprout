import * as React from "react";

import type {
  TimelineMessage,
  TimelineReaction,
} from "@/features/messages/types";

type ReactionHandler = {
  /** Reactions sorted by count (desc) then emoji (asc). */
  reactions: TimelineReaction[];
  /** Whether the user can currently toggle reactions. */
  canToggle: boolean;
  /** Whether a reaction toggle is in flight. */
  pending: boolean;
  /** Error message from the last failed toggle, if any. */
  errorMessage: string | null;
  /** Call to toggle an emoji reaction. Safe to fire-and-forget. */
  select: (emoji: string) => Promise<void>;
};

/**
 * Shared reaction state + toggle logic used by both MessageRow and
 * SystemMessageRow. Keeps the pending/error/sorting concerns in one place.
 */
export function useReactionHandler(
  message: TimelineMessage,
  onToggleReaction?: (
    message: TimelineMessage,
    emoji: string,
    remove: boolean,
  ) => Promise<void>,
): ReactionHandler {
  const [pending, setPending] = React.useState(false);
  const [errorMessage, setErrorMessage] = React.useState<string | null>(null);

  const reactions = React.useMemo(() => {
    return [...(message.reactions ?? [])].sort((left, right) => {
      if (left.count !== right.count) {
        return right.count - left.count;
      }
      return left.emoji.localeCompare(right.emoji);
    });
  }, [message.reactions]);

  const canToggle = Boolean(onToggleReaction && !message.pending);

  const select = React.useCallback(
    async (emoji: string) => {
      if (!onToggleReaction || pending) {
        return;
      }

      const remove = reactions.some(
        (reaction) => reaction.emoji === emoji && reaction.reactedByCurrentUser,
      );

      setErrorMessage(null);
      setPending(true);
      try {
        await onToggleReaction(message, emoji, remove);
      } catch (error) {
        const nextMessage =
          error instanceof Error
            ? error.message
            : "Failed to update the reaction.";
        setErrorMessage(nextMessage);
        throw error;
      } finally {
        setPending(false);
      }
    },
    [message, onToggleReaction, pending, reactions],
  );

  return { reactions, canToggle, pending, errorMessage, select };
}
