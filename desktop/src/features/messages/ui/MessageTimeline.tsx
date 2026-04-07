import * as React from "react";
import { ArrowDown, Loader2 } from "lucide-react";

import type {
  ThreadConversationHint,
  TimelineMessage,
} from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { Button } from "@/shared/ui/button";
import { Separator } from "@/shared/ui/separator";
import { TooltipProvider } from "@/shared/ui/tooltip";
import { TimelineSkeleton } from "./TimelineSkeleton";
import { TimelineMessageList } from "./TimelineMessageList";
import { useLoadOlderOnScroll } from "./useLoadOlderOnScroll";
import { useStickyDayHeader } from "./useStickyDayHeader";
import { useTimelineScrollManager } from "./useTimelineScrollManager";

type MessageTimelineProps = {
  channelId?: string | null;
  messages: TimelineMessage[];
  isLoading?: boolean;
  emptyTitle?: string;
  emptyDescription?: string;
  activeReplyTargetId?: string | null;
  activeThreadRootId?: string | null;
  currentPubkey?: string;
  fetchOlder?: () => Promise<void>;
  hasOlderMessages?: boolean;
  isFetchingOlder?: boolean;
  profiles?: UserProfileLookup;
  threadHintsByRootId?: Map<string, ThreadConversationHint>;
  onDelete?: (message: TimelineMessage) => void;
  onEdit?: (message: TimelineMessage) => void;
  onOpenThread?: (message: TimelineMessage) => void;
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
  activeThreadRootId = null,
  currentPubkey,
  fetchOlder,
  hasOlderMessages = true,
  isFetchingOlder = false,
  profiles,
  threadHintsByRootId,
  onDelete,
  onEdit,
  onOpenThread,
  onReply,
  onToggleReaction,
  targetMessageId = null,
  onTargetReached,
}: MessageTimelineProps) {
  const scrollContainerRef = React.useRef<HTMLDivElement>(null);
  const topSentinelRef = React.useRef<HTMLDivElement>(null);

  const {
    bottomAnchorRef,
    contentRef,
    highlightedMessageId,
    isAtBottom,
    newMessageCount,
    restoreScrollPosition,
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

  useLoadOlderOnScroll({
    fetchOlder,
    hasOlderMessages,
    isLoading,
    restoreScrollPosition,
    scrollContainerRef,
    sentinelRef: topSentinelRef,
  });

  const stickyDayLabel = useStickyDayHeader(scrollContainerRef);

  return (
    <TooltipProvider delayDuration={200}>
      <div className="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
        {stickyDayLabel && !isAtBottom ? (
          <div
            className="pointer-events-none absolute inset-x-0 top-0 z-10 flex justify-center px-4 pt-1 sm:px-6"
            data-testid="message-timeline-sticky-day"
          >
            <p className="rounded-md bg-muted/60 px-2 py-0.5 text-[11px] font-medium leading-none tracking-normal text-muted-foreground/75 shadow-sm backdrop-blur-sm">
              {stickyDayLabel}
            </p>
          </div>
        ) : null}
        <div
          className="absolute inset-0 overflow-y-auto overflow-x-hidden overscroll-contain px-4 pb-3 pt-1 [overflow-anchor:none] sm:px-6"
          data-testid="message-timeline"
          onScroll={syncScrollState}
          ref={scrollContainerRef}
        >
          <div
            className="mx-auto flex w-full max-w-4xl flex-col gap-2 pb-36 pt-10"
            ref={contentRef}
          >
            <div ref={topSentinelRef} aria-hidden className="h-px" />

            {isFetchingOlder ? (
              <div className="flex justify-center py-2">
                <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
              </div>
            ) : null}

            {!hasOlderMessages && !isLoading && messages.length > 0 ? (
              <div
                className="flex items-center gap-3 py-2"
                data-testid="message-timeline-beginning"
              >
                <Separator className="flex-1" />
                <p className="text-xs font-semibold uppercase tracking-[0.22em] text-muted-foreground">
                  Beginning of conversation
                </p>
                <Separator className="flex-1" />
              </div>
            ) : null}

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
                activeThreadRootId={activeThreadRootId}
                currentPubkey={currentPubkey}
                highlightedMessageId={highlightedMessageId}
                messages={messages}
                onDelete={onDelete}
                onEdit={onEdit}
                onOpenThread={onOpenThread}
                onReply={onReply}
                onToggleReaction={onToggleReaction}
                profiles={profiles}
                threadHintsByRootId={threadHintsByRootId}
              />
            ) : null}

            <div aria-hidden className="h-px" ref={bottomAnchorRef} />
          </div>
        </div>

        {!isAtBottom ? (
          <div className="pointer-events-none absolute inset-x-0 bottom-12 z-20 flex justify-center px-4">
            <Button
              className="pointer-events-auto h-7 min-h-7 gap-1.5 rounded-full border-border/50 bg-background/85 px-2.5 text-[11px] font-medium text-muted-foreground shadow-sm backdrop-blur-sm hover:bg-muted/70 hover:text-foreground [&_svg]:size-3.5"
              data-testid="message-scroll-to-latest"
              onClick={() => {
                scrollToBottom("smooth");
              }}
              size="sm"
              type="button"
              variant="outline"
            >
              <ArrowDown aria-hidden />
              {newMessageCount > 0
                ? `${newMessageCount} new message${newMessageCount === 1 ? "" : "s"}`
                : "Jump to latest"}
            </Button>
          </div>
        ) : null}
      </div>
    </TooltipProvider>
  );
});
