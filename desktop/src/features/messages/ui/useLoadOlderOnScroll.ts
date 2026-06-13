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
  const pendingRestoreRef = React.useRef<{
    scrollHeight: number;
    scrollTop: number;
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
    const delta = container.scrollHeight - pendingRestore.scrollHeight;
    if (delta > 0) {
      restoreScrollPositionRef.current(pendingRestore.scrollTop + delta);
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
          void fetchOlder()
            .then(() => {
              pendingRestoreRef.current = {
                scrollHeight: previousHeight,
                scrollTop: previousScrollTop,
              };
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
