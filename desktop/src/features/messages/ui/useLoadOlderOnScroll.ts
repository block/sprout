import * as React from "react";

type UseLoadOlderOnScrollOptions = {
  fetchOlder?: () => Promise<void>;
  hasOlderMessages: boolean;
  isLoading: boolean;
  restoreScrollPosition: (scrollTop: number) => void;
  scrollContainerRef: React.RefObject<HTMLDivElement | null>;
  sentinelRef: React.RefObject<HTMLDivElement | null>;
};

/**
 * Triggers `fetchOlder` when a sentinel element near the top of the scroll
 * container enters the viewport, then restores the scroll position so the
 * visible content doesn't jump.
 */
export function useLoadOlderOnScroll({
  fetchOlder,
  hasOlderMessages,
  isLoading,
  restoreScrollPosition,
  scrollContainerRef,
  sentinelRef,
}: UseLoadOlderOnScrollOptions) {
  const restoreScrollPositionRef = React.useRef(restoreScrollPosition);
  const [, scheduleRestore] = React.useReducer((count: number) => count + 1, 0);
  const pendingRestoreRef = React.useRef<{
    messageId: string;
    top: number;
  } | null>(null);
  React.useEffect(() => {
    restoreScrollPositionRef.current = restoreScrollPosition;
  });

  React.useLayoutEffect(() => {
    const pendingRestore = pendingRestoreRef.current;
    const container = scrollContainerRef.current;
    if (!pendingRestore || !container) {
      return;
    }

    pendingRestoreRef.current = null;
    const anchor = container.querySelector<HTMLElement>(
      `[data-message-id="${pendingRestore.messageId}"]`,
    );
    if (!anchor) {
      return;
    }

    const delta = anchor.getBoundingClientRect().top - pendingRestore.top;
    if (delta !== 0) {
      restoreScrollPositionRef.current(container.scrollTop + delta);
    }
  });

  React.useEffect(() => {
    const sentinel = sentinelRef.current;
    const container = scrollContainerRef.current;
    if (
      !sentinel ||
      !container ||
      !fetchOlder ||
      isLoading ||
      !hasOlderMessages
    ) {
      return;
    }

    let disposed = false;
    let currentObserver: IntersectionObserver | null = null;

    const observe = () => {
      if (disposed) {
        return;
      }

      currentObserver = new IntersectionObserver(
        ([entry]) => {
          if (!entry.isIntersecting || disposed) {
            return;
          }

          currentObserver?.disconnect();

          const anchor =
            container.querySelector<HTMLElement>("[data-message-id]");
          const messageId = anchor?.dataset.messageId;
          const top = anchor?.getBoundingClientRect().top;
          void fetchOlder()
            .then(() => {
              if (messageId && top !== undefined) {
                pendingRestoreRef.current = { messageId, top };
                scheduleRestore();
              }
            })
            .finally(() => {
              observe();
            });
        },
        { root: container, rootMargin: "200px 0px 0px 0px" },
      );

      currentObserver.observe(sentinel);
    };

    observe();
    return () => {
      disposed = true;
      currentObserver?.disconnect();
    };
  }, [
    fetchOlder,
    hasOlderMessages,
    isLoading,
    scrollContainerRef,
    sentinelRef,
  ]);
}
