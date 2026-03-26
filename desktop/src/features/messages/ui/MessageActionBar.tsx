import Picker from "@emoji-mart/react";
import data from "@emoji-mart/data";
import { CornerUpLeft, LoaderCircle, Pencil, SmilePlus } from "lucide-react";
import * as React from "react";

import type {
  TimelineMessage,
  TimelineReaction,
} from "@/features/messages/types";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";

export function MessageActionBar({
  activeReplyTargetId = null,
  message,
  onEdit,
  onReactionSelect,
  onReply,
  reactionErrorMessage = null,
  reactions,
  reactionPending = false,
}: {
  activeReplyTargetId?: string | null;
  message: TimelineMessage;
  onEdit?: (message: TimelineMessage) => void;
  onReactionSelect?: (emoji: string) => Promise<void>;
  onReply?: (message: TimelineMessage) => void;
  reactionErrorMessage?: string | null;
  reactions: TimelineReaction[];
  reactionPending?: boolean;
}) {
  const [isReactionPickerOpen, setIsReactionPickerOpen] = React.useState(false);
  const hasEditAction = Boolean(onEdit);
  const hasReplyAction = Boolean(onReply);
  const hasReactionAction = Boolean(onReactionSelect);

  if (!hasReplyAction && !hasReactionAction && !hasEditAction) {
    return null;
  }

  const isReplyingToMessage = activeReplyTargetId === message.id;
  const selectedReactionCount = reactions.filter(
    (reaction) => reaction.reactedByCurrentUser,
  ).length;

  return (
    <div
      className={cn(
        "max-w-28 overflow-hidden rounded-full border border-border/70 bg-background/95 shadow-sm backdrop-blur supports-[backdrop-filter]:bg-background/85 transition-all duration-150 ease-out",
        "translate-y-0 opacity-100 sm:max-w-0 sm:translate-y-1 sm:opacity-0",
        "sm:group-hover/message:max-w-28 sm:group-hover/message:translate-y-0 sm:group-hover/message:opacity-100",
        "sm:group-focus-within/message:max-w-28 sm:group-focus-within/message:translate-y-0 sm:group-focus-within/message:opacity-100",
        isReplyingToMessage || isReactionPickerOpen
          ? "sm:max-w-28 sm:translate-y-0 sm:opacity-100"
          : "",
      )}
      data-testid={`message-action-bar-${message.id}`}
    >
      <div className="flex items-center gap-1 p-1">
        {hasReactionAction ? (
          <Popover
            onOpenChange={setIsReactionPickerOpen}
            open={isReactionPickerOpen}
          >
            <PopoverTrigger asChild>
              <Button
                aria-label="Open reactions"
                className="h-6 w-6 rounded-full p-0"
                data-testid={`react-message-${message.id}`}
                disabled={reactionPending}
                size="sm"
                title="React"
                type="button"
                variant={
                  isReactionPickerOpen || selectedReactionCount > 0
                    ? "secondary"
                    : "ghost"
                }
              >
                {reactionPending ? (
                  <LoaderCircle className="h-3 w-3 animate-spin" />
                ) : (
                  <SmilePlus className="h-3 w-3" />
                )}
              </Button>
            </PopoverTrigger>
            <PopoverContent
              align="end"
              className="w-auto p-0 rounded-2xl overflow-hidden border-0 bg-transparent shadow-none"
              side="top"
              sideOffset={10}
            >
              {reactionErrorMessage ? (
                <div className="px-3 pt-3 pb-0">
                  <p className="text-xs text-destructive">
                    {reactionErrorMessage}
                  </p>
                </div>
              ) : null}
              <Picker
                data={data}
                onEmojiSelect={(emoji: any) => {
                  if (!onReactionSelect) {
                    return;
                  }

                  void onReactionSelect(emoji.native)
                    .then(() => {
                      setIsReactionPickerOpen(false);
                    })
                    .catch(() => {
                      return;
                    });
                }}
                theme="auto"
                previewPosition="none"
                skinTonePosition="search"
                set="native"
                maxFrequentRows={2}
                perLine={8}
              />
            </PopoverContent>
          </Popover>
        ) : null}

        {hasEditAction ? (
          <Button
            aria-label="Edit"
            className="h-6 w-6 rounded-full p-0"
            data-testid={`edit-message-${message.id}`}
            onClick={() => {
              onEdit?.(message);
            }}
            size="sm"
            title="Edit"
            type="button"
            variant="ghost"
          >
            <Pencil className="h-3 w-3" />
          </Button>
        ) : null}

        {hasReplyAction ? (
          <Button
            aria-label={isReplyingToMessage ? "Cancel reply" : "Reply"}
            className="h-6 w-6 rounded-full p-0"
            data-testid={`reply-message-${message.id}`}
            onClick={() => {
              onReply?.(message);
            }}
            size="sm"
            title={isReplyingToMessage ? "Cancel reply" : "Reply"}
            type="button"
            variant={isReplyingToMessage ? "secondary" : "ghost"}
          >
            <CornerUpLeft className="h-3 w-3" />
          </Button>
        ) : null}
      </div>
    </div>
  );
}
