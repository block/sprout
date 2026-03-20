import * as React from "react";

import type { TimelineReaction } from "@/features/messages/types";
import { cn } from "@/shared/lib/cn";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/shared/ui/tooltip";

export function MessageReactions({
  messageId,
  reactions,
  canToggle,
  pending,
  onSelect,
}: {
  messageId: string;
  reactions: TimelineReaction[];
  canToggle: boolean;
  pending: boolean;
  onSelect: (emoji: string) => void;
}) {
  if (reactions.length === 0) {
    return null;
  }

  return (
    <TooltipProvider delayDuration={200}>
      <div className="mt-1.5 flex flex-wrap items-center gap-1.5 pt-1">
        {reactions.map((reaction) => {
          const tooltipText =
            reaction.users.length > 0
              ? reaction.users.map((u) => u.displayName).join(", ")
              : undefined;

          const pill = (
            <button
              aria-label={`Toggle ${reaction.emoji} reaction`}
              aria-pressed={reaction.reactedByCurrentUser}
              className={cn(
                "inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-xs font-medium transition-colors",
                reaction.reactedByCurrentUser
                  ? "border-primary/40 bg-primary/10 text-primary"
                  : "border-border/70 bg-muted/70 text-foreground/90",
                canToggle
                  ? "hover:bg-accent hover:text-accent-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                  : "cursor-default",
              )}
              disabled={!canToggle || pending}
              onClick={() => {
                if (!canToggle) return;
                onSelect(reaction.emoji);
              }}
              type="button"
            >
              <span>{reaction.emoji}</span>
              <span className="text-muted-foreground">{reaction.count}</span>
            </button>
          );

          if (!tooltipText) {
            return (
              <React.Fragment key={`${messageId}-${reaction.emoji}`}>
                {pill}
              </React.Fragment>
            );
          }

          // Wrap in a span so the tooltip trigger receives hover/focus events
          // even when the inner button is disabled (Radix tooltips require it).
          return (
            <Tooltip key={`${messageId}-${reaction.emoji}`}>
              <TooltipTrigger asChild>
                <span className="inline-flex">{pill}</span>
              </TooltipTrigger>
              <TooltipContent>{tooltipText}</TooltipContent>
            </Tooltip>
          );
        })}
      </div>
    </TooltipProvider>
  );
}
