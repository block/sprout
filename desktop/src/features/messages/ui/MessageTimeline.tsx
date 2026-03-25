import * as React from "react";
import { ArrowDown } from "lucide-react";

import type { TimelineMessage } from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { Button } from "@/shared/ui/button";
import { Separator } from "@/shared/ui/separator";
import { TimelineSkeleton } from "./TimelineSkeleton";
import { TimelineMessageList } from "./TimelineMessageList";
import { useTimelineScrollManager } from "./useTimelineScrollManager";

type MessageTimelineProps = {
  channelId?: string | null;
  messages: TimelineMessage[];
  isLoading?: boolean;
  emptyTitle?: string;
  emptyDescription?: string;
  activeReplyTargetId?: string | null;
  currentPubkey?: string;
  profiles?: UserProfileLookup;
  onReply?: (message: TimelineMessage) => void;
  onToggleReaction?: (
    message: TimelineMessage,
    emoji: string,
    remove: boolean,
  ) => Promise<void>;
  targetMessageId?: string | null;
  onTargetReached?: (messageId: string) => void;
};

export const MessageTimeline = React.memo(function MessageTimeline({
  channelId,
  messages,
  isLoading = false,
  emptyTitle = "No messages yet",
  emptyDescription = "Send the first message to start the thread.",
  activeReplyTargetId = null,
  currentPubkey,
  profiles,
  onReply,
  onToggleReaction,
  targetMessageId = null,
  onTargetReached,
}: MessageTimelineProps) {
  const scrollContainerRef = React.useRef<HTMLDivElement>(null);

  const {
    bottomAnchorRef,
    contentRef,
    highlightedMessageId,
    isAtBottom,
    newMessageCount,
    scrollToBottom,
    syncScrollState,
  } = useTimelineScrollManager({
    channelId,
    isLoading,
    messages,
    onTargetReached,
    scrollContainerRef,
    targetMessageId,
  });

  return (
    <div className="relative min-h-0 flex-1">
      <div
        className="h-full overflow-y-auto overflow-x-hidden overscroll-contain px-4 py-3 [overflow-anchor:none] sm:px-6"
        data-testid="message-timeline"
        onScroll={syncScrollState}
        ref={scrollContainerRef}
      >
        <div
          className="mx-auto flex w-full max-w-4xl flex-col gap-2"
          ref={contentRef}
        >
          <div
            className="flex items-center gap-3"
            data-testid="message-timeline-day-divider"
          >
            <Separator className="flex-1" />
            <p className="text-xs font-semibold uppercase tracking-[0.22em] text-muted-foreground">
              Today
            </p>
            <Separator className="flex-1" />
          </div>

          {isLoading ? <TimelineSkeleton /> : null}

          {!isLoading && messages.length === 0 ? (
            <div
              className="rounded-3xl border border-dashed border-border/80 bg-card/70 px-6 py-10 text-center shadow-sm"
              data-testid="message-empty"
            >
              <p className="text-base font-semibold tracking-tight">
                {emptyTitle}
              </p>
              <p className="mt-2 text-sm text-muted-foreground">
                {emptyDescription}
              </p>
            </div>
          ) : null}

          {!isLoading && messages.length > 0 ? (
            <TimelineMessageList
              activeReplyTargetId={activeReplyTargetId}
              currentPubkey={currentPubkey}
              highlightedMessageId={highlightedMessageId}
              messages={messages}
              onReply={onReply}
              onToggleReaction={onToggleReaction}
              profiles={profiles}
            />
          ) : null}

          <div aria-hidden className="h-px" ref={bottomAnchorRef} />
        </div>
      </div>

      {!isAtBottom ? (
        <div className="pointer-events-none absolute inset-x-0 bottom-4 flex justify-center px-4">
          <Button
            className="pointer-events-auto rounded-full shadow-lg"
            data-testid="message-scroll-to-latest"
            onClick={() => {
              scrollToBottom("smooth");
            }}
            size="sm"
            type="button"
          >
            <ArrowDown className="h-4 w-4" />
            {newMessageCount > 0
              ? `${newMessageCount} new message${newMessageCount === 1 ? "" : "s"}`
              : "Jump to latest"}
          </Button>
        </div>
      ) : null}
    </div>
  );
});
