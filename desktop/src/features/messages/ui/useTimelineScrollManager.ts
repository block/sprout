import * as React from "react";

import type { TimelineMessage } from "@/features/messages/types";
import { isNearBottom } from "./messageTimelineUtils";

export function useTimelineScrollManager({
  isLoading,
  messages,
  onTargetReached,
  targetMessageId,
}: {
  isLoading: boolean;
  messages: TimelineMessage[];
  onTargetReached?: (messageId: string) => void;
  targetMessageId?: string | null;
}) {
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
  const handledTargetMessageIdRef = React.useRef<string | null>(null);
  const [isAtBottom, setIsAtBottom] = React.useState(true);
  const [highlightedMessageId, setHighlightedMessageId] = React.useState<
    string | null
  >(null);
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

  React.useEffect(() => {
    if (!targetMessageId) {
      handledTargetMessageIdRef.current = null;
      setHighlightedMessageId(null);
      return;
    }

    if (handledTargetMessageIdRef.current === targetMessageId || isLoading) {
      return;
    }

    const timeline = timelineRef.current;
    if (!timeline) {
      return;
    }

    const targetElement = timeline.querySelector<HTMLElement>(
      `[data-message-id="${targetMessageId}"]`,
    );
    if (!targetElement) {
      return;
    }

    handledTargetMessageIdRef.current = targetMessageId;
    shouldStickToBottomRef.current = false;
    isAtBottomRef.current = false;
    isProgrammaticBottomScrollRef.current = false;
    targetElement.scrollIntoView({
      block: "center",
      behavior: "smooth",
    });
    previousScrollTopRef.current = timeline.scrollTop;
    setIsAtBottom(false);
    setHighlightedMessageId(targetMessageId);
    setNewMessageCount(0);
    onTargetReached?.(targetMessageId);

    const timeout = window.setTimeout(() => {
      setHighlightedMessageId((current) =>
        current === targetMessageId ? null : current,
      );
    }, 2_000);

    return () => {
      window.clearTimeout(timeout);
    };
  }, [isLoading, onTargetReached, targetMessageId]);

  return {
    bottomAnchorRef,
    contentRef,
    highlightedMessageId,
    isAtBottom,
    newMessageCount,
    scrollToBottom,
    syncScrollState,
    timelineRef,
  };
}
