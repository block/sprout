import * as React from "react";
import { ArrowDown, Loader2 } from "lucide-react";

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
  fetchOlder?: () => Promise<void>;
  hasOlderMessages?: boolean;
  isFetchingOlder?: boolean;
  profiles?: UserProfileLookup;
  onEdit?: (message: TimelineMessage) => void;
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
  fetchOlder,
  hasOlderMessages = true,
  isFetchingOlder = false,
  profiles,
  onEdit,
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

  React.useEffect(() => {
    const sentinel = topSentinelRef.current;
    const container = scrollContainerRef.current;
    if (!sentinel || !container || !fetchOlder || isLoading) {
      return;
    }

    let currentObserver: IntersectionObserver | null = null;

    const observe = () => {
      currentObserver = new IntersectionObserver(
        ([entry]) => {
          if (!entry.isIntersecting || isFetchingOlder) {
            return;
          }

          // Disconnect immediately to prevent rapid-fire callbacks during
          // the React render gap between fetchOlder resolving and the DOM
          // updating with new messages.
          currentObserver?.disconnect();

          const previousHeight = container.scrollHeight;
          const previousScrollTop = container.scrollTop;
          void fetchOlder().then(() => {
            // Two nested rAF calls: the first lets React flush its commit
            // phase, the second lets the browser recalculate layout so
            // scrollHeight reflects the newly prepended messages.
            requestAnimationFrame(() => {
              requestAnimationFrame(() => {
                const newHeight = container.scrollHeight;
                const delta = newHeight - previousHeight;
                if (delta > 0) {
                  restoreScrollPosition(previousScrollTop + delta);
                }
                // Re-observe after the scroll restoration settles so the
                // next scroll-to-top triggers a fresh fetch.
                observe();
              });
            });
          });
        },
        { root: container, rootMargin: "200px 0px 0px 0px" },
      );

      currentObserver.observe(sentinel);
    };

    observe();
    return () => currentObserver?.disconnect();
  }, [fetchOlder, isFetchingOlder, isLoading, restoreScrollPosition]);

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
              onEdit={onEdit}
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
