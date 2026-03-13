import { CornerUpLeft, LoaderCircle, SmilePlus } from "lucide-react";
import * as React from "react";

import type {
  TimelineMessage,
  TimelineReaction,
} from "@/features/messages/types";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";
import { getReactionOptions } from "./messageTimelineUtils";

export function MessageActionBar({
  activeReplyTargetId = null,
  message,
  onReactionSelect,
  onReply,
  reactionErrorMessage = null,
  reactions,
  reactionPending = false,
}: {
  activeReplyTargetId?: string | null;
  message: TimelineMessage;
  onReactionSelect?: (emoji: string) => Promise<void>;
  onReply?: (message: TimelineMessage) => void;
  reactionErrorMessage?: string | null;
  reactions: TimelineReaction[];
  reactionPending?: boolean;
}) {
  const [isReactionPickerOpen, setIsReactionPickerOpen] = React.useState(false);
  const hasReplyAction = Boolean(onReply);
  const hasReactionAction = Boolean(onReactionSelect);

  if (!hasReplyAction && !hasReactionAction) {
    return null;
  }

  const isReplyingToMessage = activeReplyTargetId === message.id;
  const selectedReactionCount = reactions.filter(
    (reaction) => reaction.reactedByCurrentUser,
  ).length;
  const reactionOptions = getReactionOptions(reactions);

  return (
    <div
      className={cn(
        "max-w-20 overflow-hidden rounded-full border border-border/70 bg-background/95 shadow-sm backdrop-blur supports-[backdrop-filter]:bg-background/85 transition-all duration-150 ease-out",
        "translate-y-0 opacity-100 sm:max-w-0 sm:translate-y-1 sm:opacity-0",
        "sm:group-hover/message:max-w-20 sm:group-hover/message:translate-y-0 sm:group-hover/message:opacity-100",
        "sm:group-focus-within/message:max-w-20 sm:group-focus-within/message:translate-y-0 sm:group-focus-within/message:opacity-100",
        isReplyingToMessage || isReactionPickerOpen
          ? "sm:max-w-20 sm:translate-y-0 sm:opacity-100"
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
              className="w-56 rounded-2xl p-3"
              side="top"
              sideOffset={10}
            >
              <div className="space-y-3">
                <div className="space-y-1">
                  <p className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
                    React
                  </p>
                  <p
                    className={cn(
                      "text-xs",
                      reactionErrorMessage
                        ? "text-destructive"
                        : "text-muted-foreground",
                    )}
                  >
                    {reactionErrorMessage ??
                      "Click any emoji. Click it again to remove your own reaction."}
                  </p>
                </div>
                <div className="grid grid-cols-4 gap-1">
                  {reactionOptions.map((emoji) => {
                    const isActive = reactions.some(
                      (reaction) =>
                        reaction.emoji === emoji &&
                        reaction.reactedByCurrentUser,
                    );

                    return (
                      <button
                        aria-label={`React with ${emoji}`}
                        aria-pressed={isActive}
                        className={cn(
                          "flex h-10 items-center justify-center rounded-xl border text-lg transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                          isActive
                            ? "border-primary/40 bg-primary/10"
                            : "border-border/70 bg-muted/40 hover:bg-accent",
                        )}
                        data-emoji={emoji}
                        data-testid={`react-option-${message.id}`}
                        disabled={reactionPending}
                        key={`${message.id}-${emoji}`}
                        onClick={() => {
                          if (!onReactionSelect) {
                            return;
                          }

                          void onReactionSelect(emoji)
                            .then(() => {
                              setIsReactionPickerOpen(false);
                            })
                            .catch(() => {
                              return;
                            });
                        }}
                        type="button"
                      >
                        {emoji}
                      </button>
                    );
                  })}
                </div>
              </div>
            </PopoverContent>
          </Popover>
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
