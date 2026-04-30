import * as React from "react";
import { ArrowDown } from "lucide-react";

import type { TimelineMessage } from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { Button } from "@/shared/ui/button";
import { Separator } from "@/shared/ui/separator";
import { Spinner } from "@/shared/ui/spinner";
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
  conversationFooter?: React.ReactNode;
  activeReplyTargetId?: string | null;
  currentPubkey?: string;
  fetchOlder?: () => Promise<void>;
  hasOlderMessages?: boolean;
  isFetchingOlder?: boolean;
  /** Map from lowercase pubkey → persona display name for bot members. */
  personaLookup?: Map<string, string>;
  profiles?: UserProfileLookup;
  onDelete?: (message: TimelineMessage) => void;
  onEdit?: (message: TimelineMessage) => void;
  onReply?: (message: TimelineMessage) => void;
  onToggleReaction?: (
    message: TimelineMessage,
    emoji: string,
    remove: boolean,
  ) => Promise<void>;
  /** The message ID of the currently active find-in-channel match. */
  searchActiveMessageId?: string | null;
  /** Set of message IDs that match the current find-in-channel query. */
  searchMatchingMessageIds?: Set<string>;
  /** The current find-in-channel query string. */
  searchQuery?: string;
  targetMessageId?: string | null;
  onTargetReached?: (messageId: string) => void;
};

export const MessageTimeline = React.memo(function MessageTimeline({
  channelId,
  messages,
  isLoading = false,
  emptyTitle = "No messages yet",
  emptyDescription = "Send the first message to start the thread.",
  conversationFooter,
  activeReplyTargetId = null,
  currentPubkey,
  fetchOlder,
  hasOlderMessages = true,
  isFetchingOlder = false,
  personaLookup,
  profiles,
  onDelete,
  onEdit,
  onReply,
  onToggleReaction,
  searchActiveMessageId = null,
  searchMatchingMessageIds,
  searchQuery,
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

  // Scroll to the active search match when it changes.
  const prevSearchActiveRef = React.useRef<string | null>(null);
  React.useEffect(() => {
    if (
      !searchActiveMessageId ||
      searchActiveMessageId === prevSearchActiveRef.current
    ) {
      prevSearchActiveRef.current = searchActiveMessageId;
      return;
    }
    prevSearchActiveRef.current = searchActiveMessageId;

    const container = scrollContainerRef.current;
    if (!container) return;

    const el = container.querySelector<HTMLElement>(
      `[data-message-id="${searchActiveMessageId}"]`,
    );
    if (el) {
      el.scrollIntoView({ block: "center", behavior: "smooth" });
    }
  }, [searchActiveMessageId]);

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
      <div className="relative min-h-0 flex-1">
        {stickyDayLabel && !isAtBottom ? (
          <div
            className="pointer-events-none absolute inset-x-0 top-0 z-10 flex justify-center px-4 pt-2 sm:px-6"
            data-testid="message-timeline-sticky-day"
          >
            <p className="rounded-full bg-muted/90 px-3 py-1 text-xs font-semibold uppercase tracking-[0.22em] text-muted-foreground shadow-sm backdrop-blur-sm">
              {stickyDayLabel}
            </p>
          </div>
        ) : null}
        <div
          className="h-full overflow-y-auto overflow-x-hidden overscroll-contain px-4 pb-6 pt-3 [overflow-anchor:none] sm:px-6"
          data-scroll-restoration-id="message-timeline"
          data-testid="message-timeline"
          onScroll={syncScrollState}
          ref={scrollContainerRef}
        >
          <div className="flex w-full flex-col gap-3 px-4" ref={contentRef}>
            <div ref={topSentinelRef} aria-hidden className="h-px" />

            {isFetchingOlder ? (
              <div className="flex justify-center py-2">
                <Spinner className="h-4 w-4 text-muted-foreground" />
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
                currentPubkey={currentPubkey}
                highlightedMessageId={highlightedMessageId}
                messages={messages}
                onDelete={onDelete}
                onEdit={onEdit}
                onReply={onReply}
                onToggleReaction={onToggleReaction}
                personaLookup={personaLookup}
                profiles={profiles}
                searchActiveMessageId={searchActiveMessageId}
                searchMatchingMessageIds={searchMatchingMessageIds}
                searchQuery={searchQuery}
                trailingContent={conversationFooter}
              />
            ) : null}

            <div aria-hidden className="h-px" ref={bottomAnchorRef} />
          </div>
        </div>

        {!isAtBottom ? (
          <div className="pointer-events-none absolute inset-x-0 bottom-8 flex justify-center px-4">
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
    </TooltipProvider>
  );
});
