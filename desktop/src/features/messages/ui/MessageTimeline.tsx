import { ArrowDown } from "lucide-react";
import * as React from "react";

import type { TimelineMessage } from "@/features/messages/types";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { Markdown } from "@/shared/ui/markdown";
import { Separator } from "@/shared/ui/separator";
import { Skeleton } from "@/shared/ui/skeleton";

type MessageTimelineProps = {
  messages: TimelineMessage[];
  isLoading?: boolean;
  emptyTitle?: string;
  emptyDescription?: string;
};

const BOTTOM_THRESHOLD_PX = 72;

function isNearBottom(container: HTMLDivElement) {
  return (
    container.scrollHeight - container.clientHeight - container.scrollTop <=
    BOTTOM_THRESHOLD_PX
  );
}

function MessageRow({ message }: { message: TimelineMessage }) {
  const initials = message.author
    .split(" ")
    .map((part) => part[0])
    .join("")
    .slice(0, 2)
    .toUpperCase();

  return (
    <article className="flex gap-3" data-testid="message-row">
      <div
        className={cn(
          "flex h-9 w-9 shrink-0 items-center justify-center rounded-xl text-xs font-semibold shadow-sm",
          message.accent
            ? "bg-primary text-primary-foreground"
            : "bg-secondary text-secondary-foreground",
        )}
      >
        {initials}
      </div>

      <div className="min-w-0 flex-1 space-y-1">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <h3 className="truncate text-sm font-semibold tracking-tight">
            {message.author}
          </h3>
          {message.role ? (
            <p className="rounded-full bg-muted px-2 py-0.5 text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
              {message.role}
            </p>
          ) : null}
          <div className="ml-auto flex items-center gap-2 text-xs text-muted-foreground">
            {message.pending ? (
              <p className="font-medium uppercase tracking-[0.14em] text-primary/80">
                Sending
              </p>
            ) : null}
            <p className="whitespace-nowrap">{message.time}</p>
          </div>
        </div>
        <Markdown className="max-w-3xl" compact content={message.body} />
      </div>
    </article>
  );
}

function TimelineSkeleton() {
  const skeletonRows = ["first", "second", "third", "fourth"];

  return (
    <>
      {skeletonRows.map((row) => (
        <div className="flex gap-3" key={row}>
          <Skeleton className="h-9 w-9 rounded-xl" />
          <div className="min-w-0 flex-1 space-y-1.5">
            <Skeleton className="h-3.5 w-44" />
            <Skeleton className="h-4 w-full max-w-2xl" />
            <Skeleton className="h-4 w-full max-w-xl" />
          </div>
        </div>
      ))}
    </>
  );
}

export function MessageTimeline({
  messages,
  isLoading = false,
  emptyTitle = "No messages yet",
  emptyDescription = "Send the first message to start the thread.",
}: MessageTimelineProps) {
  const timelineRef = React.useRef<HTMLDivElement>(null);
  const contentRef = React.useRef<HTMLDivElement>(null);
  const bottomAnchorRef = React.useRef<HTMLDivElement>(null);
  const hasInitializedRef = React.useRef(false);
  const shouldStickToBottomRef = React.useRef(true);
  const isAtBottomRef = React.useRef(true);
  const isProgrammaticBottomScrollRef = React.useRef(false);
  const previousTimelineHeightRef = React.useRef<number | null>(null);
  const previousScrollTopRef = React.useRef(0);
  const lockedScrollTopRef = React.useRef<number | null>(null);
  const previousLastMessageIdRef = React.useRef<string | undefined>(undefined);
  const previousMessageCountRef = React.useRef(0);
  const [isAtBottom, setIsAtBottom] = React.useState(true);
  const [newMessageCount, setNewMessageCount] = React.useState(0);
  const latestMessage =
    messages.length > 0 ? messages[messages.length - 1] : undefined;

  const syncScrollState = React.useCallback(() => {
    const timeline = timelineRef.current;
    if (!timeline) {
      return;
    }

    const scrollTop = lockedScrollTopRef.current ?? timeline.scrollTop;
    const atBottom = isNearBottom(timeline);
    const movedAwayFromBottom = scrollTop + 1 < previousScrollTopRef.current;

    if (isProgrammaticBottomScrollRef.current) {
      previousScrollTopRef.current = scrollTop;

      if (movedAwayFromBottom) {
        isProgrammaticBottomScrollRef.current = false;
      } else if (!atBottom) {
        shouldStickToBottomRef.current = true;
        isAtBottomRef.current = true;
        setIsAtBottom((current) => (current ? current : true));
        return;
      } else {
        isProgrammaticBottomScrollRef.current = false;
        shouldStickToBottomRef.current = true;
        isAtBottomRef.current = true;
        setIsAtBottom((current) => (current ? current : true));
        setNewMessageCount(0);
        return;
      }
    }

    if (shouldStickToBottomRef.current && !atBottom && !movedAwayFromBottom) {
      previousScrollTopRef.current = scrollTop;
      shouldStickToBottomRef.current = true;
      isAtBottomRef.current = true;
      setIsAtBottom((current) => (current ? current : true));
      setNewMessageCount(0);
      return;
    }

    previousScrollTopRef.current = scrollTop;
    shouldStickToBottomRef.current = atBottom;
    isAtBottomRef.current = atBottom;
    setIsAtBottom((current) => (current === atBottom ? current : atBottom));

    if (atBottom) {
      setNewMessageCount(0);
    }
  }, []);

  const restoreScrollPosition = React.useCallback(
    (scrollTop: number) => {
      const timeline = timelineRef.current;

      if (!timeline) {
        return;
      }

      isProgrammaticBottomScrollRef.current = false;
      lockedScrollTopRef.current = scrollTop;

      const restore = (remainingFrames: number) => {
        timeline.scrollTop = scrollTop;

        if (remainingFrames > 0) {
          requestAnimationFrame(() => {
            restore(remainingFrames - 1);
          });
          return;
        }

        lockedScrollTopRef.current = null;
        previousScrollTopRef.current = timeline.scrollTop;
        syncScrollState();
      };

      restore(2);
    },
    [syncScrollState],
  );

  const scrollToBottom = React.useCallback(
    (behavior: ScrollBehavior) => {
      const timeline = timelineRef.current;

      if (!timeline) {
        return;
      }

      isProgrammaticBottomScrollRef.current = true;

      const alignToBottom = (nextBehavior: ScrollBehavior) => {
        bottomAnchorRef.current?.scrollIntoView({
          block: "end",
          behavior: nextBehavior,
        });
        timeline.scrollTo({
          top: timeline.scrollHeight,
          behavior: nextBehavior,
        });
      };

      alignToBottom(behavior);
      lockedScrollTopRef.current = null;
      previousScrollTopRef.current = timeline.scrollTop;
      shouldStickToBottomRef.current = true;
      isAtBottomRef.current = true;
      setIsAtBottom(true);
      setNewMessageCount(0);

      if (behavior === "smooth") {
        requestAnimationFrame(() => {
          previousScrollTopRef.current = timeline.scrollTop;
          syncScrollState();
        });
        return;
      }

      const settleAlignment = (remainingFrames: number) => {
        requestAnimationFrame(() => {
          alignToBottom("auto");
          previousScrollTopRef.current = timeline.scrollTop;

          if (remainingFrames > 0) {
            settleAlignment(remainingFrames - 1);
            return;
          }

          syncScrollState();
        });
      };

      settleAlignment(2);
    },
    [syncScrollState],
  );

  React.useEffect(() => {
    const timeline = timelineRef.current;

    if (!timeline || typeof ResizeObserver === "undefined") {
      return;
    }

    previousTimelineHeightRef.current = timeline.clientHeight;
    previousScrollTopRef.current = timeline.scrollTop;

    const observer = new ResizeObserver(([entry]) => {
      const previousTimelineHeight = previousTimelineHeightRef.current;
      const nextTimelineHeight = entry.contentRect.height;
      previousTimelineHeightRef.current = nextTimelineHeight;

      if (
        previousTimelineHeight === null ||
        Math.abs(nextTimelineHeight - previousTimelineHeight) < 1
      ) {
        return;
      }

      if (shouldStickToBottomRef.current || isAtBottomRef.current) {
        scrollToBottom("auto");
        return;
      }

      restoreScrollPosition(previousScrollTopRef.current);
    });

    observer.observe(timeline);

    return () => {
      observer.disconnect();
    };
  }, [restoreScrollPosition, scrollToBottom]);

  React.useEffect(() => {
    const content = contentRef.current;

    if (!content || typeof ResizeObserver === "undefined") {
      return;
    }

    const observer = new ResizeObserver(() => {
      if (shouldStickToBottomRef.current) {
        scrollToBottom("auto");
        return;
      }

      syncScrollState();
    });

    observer.observe(content);

    return () => {
      observer.disconnect();
    };
  }, [scrollToBottom, syncScrollState]);

  React.useLayoutEffect(() => {
    if (!hasInitializedRef.current) {
      if (isLoading) {
        return;
      }

      scrollToBottom("auto");
      hasInitializedRef.current = true;
      previousLastMessageIdRef.current = latestMessage?.id;
      previousMessageCountRef.current = messages.length;
      return;
    }

    const previousLastMessageId = previousLastMessageIdRef.current;
    const previousMessageCount = previousMessageCountRef.current;
    const hasNewLatestMessage =
      latestMessage !== undefined && latestMessage.id !== previousLastMessageId;

    if (!hasNewLatestMessage) {
      previousLastMessageIdRef.current = latestMessage?.id;
      previousMessageCountRef.current = messages.length;
      return;
    }

    if (
      shouldStickToBottomRef.current ||
      isAtBottomRef.current ||
      latestMessage.accent
    ) {
      scrollToBottom(latestMessage.accent ? "smooth" : "auto");
    } else {
      setNewMessageCount((current) => {
        const addedMessages = Math.max(
          1,
          messages.length - previousMessageCount,
        );
        return current + addedMessages;
      });
    }

    previousLastMessageIdRef.current = latestMessage.id;
    previousMessageCountRef.current = messages.length;
  }, [isLoading, latestMessage, messages.length, scrollToBottom]);

  return (
    <div className="relative flex-1 min-h-0">
      <div
        className="h-full overflow-y-auto overflow-x-hidden overscroll-contain px-4 py-4 [overflow-anchor:none] sm:px-6"
        data-testid="message-timeline"
        onScroll={syncScrollState}
        ref={timelineRef}
      >
        <div
          className="mx-auto flex w-full max-w-4xl flex-col gap-4"
          ref={contentRef}
        >
          <div
            className="flex items-center gap-4"
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

          {!isLoading
            ? messages.map((message) => (
                <MessageRow key={message.id} message={message} />
              ))
            : null}
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
}
