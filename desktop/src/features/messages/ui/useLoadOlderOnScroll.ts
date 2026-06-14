import * as React from "react";

type UseLoadOlderOnScrollOptions = {
  fetchOlder?: () => Promise<void>;
  hasOlderMessages: boolean;
  isLoading: boolean;
  scrollContainerRef: React.RefObject<HTMLDivElement | null>;
  sentinelRef: React.RefObject<HTMLDivElement | null>;
};

/**
 * Triggers `fetchOlder` when a sentinel element near the top of the scroll
 * container enters the viewport, then restores the scroll position so the
 * visible content doesn't jump.
 *
 * Uses the classical infinite-scroll-up algorithm: while an older-history
 * request is in flight, each render compares the current `scrollHeight` to
 * the previous render's `scrollHeight` and advances the scroll position by
 * that delta. This keeps compensation aligned with the real React commit that
 * changed the DOM instead of assuming `fetchOlder().then(...)` runs before the
 * query cache update is rendered. This is robust to:
 *
 *  - The user continuing to scroll during the fetch — `scrollTop` already
 *    reflects whatever they did, we only add height changes on top.
 *  - Inline loading chrome that appears during the fetch (e.g. a top
 *    spinner gated on `isFetchingOlder`). Spinner mount is compensated one
 *    way; spinner removal in the prepend commit is compensated the other way,
 *    so the net viewport stays anchored to the same content.
 *
 * A prior implementation snapshotted after `fetchOlder` resolved, but the real
 * fetch path mutates the React Query cache before resolving the promise. That
 * can miss the actual prepend commit and leave the browser to apply its own
 * scroll-range adjustment.
 */
export function useLoadOlderOnScroll({
  fetchOlder,
  hasOlderMessages,
  isLoading,
  scrollContainerRef,
  sentinelRef,
}: UseLoadOlderOnScrollOptions) {
  const [, scheduleRestore] = React.useReducer((count: number) => count + 1, 0);
  const pendingRestoreRef = React.useRef<"loading" | "settling" | null>(null);
  const previousScrollHeightRef = React.useRef<number | null>(null);

  React.useLayoutEffect(() => {
    const container = scrollContainerRef.current;
    if (!container) {
      previousScrollHeightRef.current = null;
      return;
    }

    const previousScrollHeight = previousScrollHeightRef.current;
    const currentScrollHeight = container.scrollHeight;

    if (pendingRestoreRef.current !== null && previousScrollHeight !== null) {
      const delta = currentScrollHeight - previousScrollHeight;
      if (delta !== 0) {
        // Single synchronous pre-paint delta write. We deliberately do NOT
        // route through useTimelineScrollManager.restoreScrollPosition: that
        // helper schedules a 2-rAF locked-write loop (correct for
        // ResizeObserver-driven resizes that may settle across frames, wrong
        // for prepend), which fights live wheel input for 2–3 frames after
        // every fetchOlder.
        //
        // Use `scrollBy` rather than `scrollTop = scrollTop + delta`: on
        // WebKit/macOS (which Tauri uses for the desktop app) scrolling
        // happens off the main thread, so a `scrollTop` *read* during active
        // wheel input can be stale relative to what the user actually sees.
        // `scrollBy` is delta-based — it doesn't read first — and avoids that
        // class of stale-read bug. (Element/Matrix documents the same
        // failure mode in element-web's docs/scrolling.md.)
        container.scrollBy(0, delta);
      }

      if (pendingRestoreRef.current === "settling") {
        pendingRestoreRef.current = null;
      }
    }

    previousScrollHeightRef.current = currentScrollHeight;
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

          pendingRestoreRef.current = "loading";
          void fetchOlder()
            .then(() => {
              if (disposed) {
                return;
              }
              pendingRestoreRef.current = "settling";
              scheduleRestore();
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
