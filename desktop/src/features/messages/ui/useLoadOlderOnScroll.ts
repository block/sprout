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
  const [, scheduleRestore] = React.useReducer((count: number) => count + 1, 0);
  const pendingRestoreRef = React.useRef<{
    previousHeight: number;
    previousScrollTop: number;
  } | null>(null);
  const restoreScrollPositionRef = React.useRef(restoreScrollPosition);
  React.useEffect(() => {
    restoreScrollPositionRef.current = restoreScrollPosition;
  });

  React.useLayoutEffect(() => {
    const container = scrollContainerRef.current;
    const pendingRestore = pendingRestoreRef.current;
    if (!container || !pendingRestore) {
      return;
    }

    pendingRestoreRef.current = null;
    const delta = container.scrollHeight - pendingRestore.previousHeight;
    if (delta > 0) {
      restoreScrollPositionRef.current(
        pendingRestore.previousScrollTop + delta,
      );
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

          const previousHeight = container.scrollHeight;
          const previousScrollTop = container.scrollTop;
          void fetchOlder().finally(() => {
            pendingRestoreRef.current = {
              previousHeight,
              previousScrollTop,
            };
            scheduleRestore();
            requestAnimationFrame(() => {
              observe();
            });
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
