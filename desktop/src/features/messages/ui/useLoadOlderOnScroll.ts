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
 * Uses the classical infinite-scroll-up algorithm: snapshot `scrollHeight`
 * the moment `fetchOlder` resolves (before React commits the new messages),
 * then in a `useLayoutEffect` after the prepend commits, advance the scroll
 * position by the resulting `scrollHeight` delta. This is robust to:
 *
 *  - The user continuing to scroll during the fetch — `scrollTop` already
 *    reflects whatever they did, we only add the prepended height on top.
 *  - Inline loading chrome that appears during the fetch (e.g. a top
 *    spinner gated on `isFetchingOlder`). The baseline `scrollHeight` is
 *    captured *after* such chrome is mounted, so when it unmounts in the
 *    same commit as the prepend, the delta still reflects the net change
 *    in content above the viewport.
 *
 * A prior implementation snapshotted an anchor element's bounding-rect top
 * *before* the fetch and tried to restore by anchor delta. That captured
 * the user's in-flight scroll into the delta and snapped them back by
 * hundreds-to-thousands of pixels per fetch.
 */
export function useLoadOlderOnScroll({
  fetchOlder,
  hasOlderMessages,
  isLoading,
  scrollContainerRef,
  sentinelRef,
}: UseLoadOlderOnScrollOptions) {
  const [, scheduleRestore] = React.useReducer((count: number) => count + 1, 0);
  const pendingPreviousScrollHeightRef = React.useRef<number | null>(null);

  React.useLayoutEffect(() => {
    const previousScrollHeight = pendingPreviousScrollHeightRef.current;
    const container = scrollContainerRef.current;
    if (previousScrollHeight === null || !container) {
      return;
    }

    pendingPreviousScrollHeightRef.current = null;
    const delta = container.scrollHeight - previousScrollHeight;
    if (delta > 0) {
      // Single synchronous pre-paint write. We deliberately do NOT route
      // through useTimelineScrollManager.restoreScrollPosition: that helper
      // schedules a 2-rAF locked-write loop (correct for
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

          void fetchOlder()
            .then(() => {
              if (disposed) {
                return;
              }
              // Capture scrollHeight in the resolved callback rather than at
              // IO-fire time so that any chrome that appears *during* the
              // fetch (e.g. an inline loading spinner gated on
              // `isFetchingOlder`) is included in the baseline. The
              // useLayoutEffect that runs after the prepended messages
              // commit measures against this baseline; if the spinner is
              // unmounted in the same React commit as the prepend, the
              // delta still reflects net prepended content correctly.
              pendingPreviousScrollHeightRef.current = container.scrollHeight;
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
