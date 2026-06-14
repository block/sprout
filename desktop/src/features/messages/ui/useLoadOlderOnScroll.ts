import * as React from "react";

type UseLoadOlderOnScrollOptions = {
  fetchOlder?: () => Promise<void>;
  hasOlderMessages: boolean;
  isLoading: boolean;
  scrollContainerRef: React.RefObject<HTMLDivElement | null>;
  sentinelRef: React.RefObject<HTMLDivElement | null>;
};

type ScrollAnchor = {
  id: string;
  top: number;
};

function getMessageTop(container: HTMLDivElement, message: HTMLElement) {
  return (
    message.getBoundingClientRect().top - container.getBoundingClientRect().top
  );
}

function captureFirstVisibleMessage(
  container: HTMLDivElement,
): ScrollAnchor | null {
  const containerRect = container.getBoundingClientRect();
  const messages = Array.from(
    container.querySelectorAll<HTMLElement>("[data-message-id]"),
  );

  for (const message of messages) {
    const rect = message.getBoundingClientRect();
    if (rect.bottom <= containerRect.top || rect.top >= containerRect.bottom) {
      continue;
    }

    const id = message.dataset.messageId;
    if (!id) {
      continue;
    }

    return {
      id,
      top: rect.top - containerRect.top,
    };
  }

  return null;
}

function restoreAnchor(container: HTMLDivElement, anchor: ScrollAnchor) {
  const message = container.querySelector<HTMLElement>(
    `[data-message-id="${CSS.escape(anchor.id)}"]`,
  );
  if (!message) {
    return false;
  }

  const delta = getMessageTop(container, message) - anchor.top;
  if (Math.abs(delta) > 0.5) {
    // Use a relative write. On WebKit/macOS, reading scrollTop during active
    // wheel input can be stale; scrollBy applies the measured DOM delta without
    // deriving a new absolute scrollTop from that potentially stale value.
    container.scrollBy(0, delta);
  }

  return true;
}

/**
 * Triggers `fetchOlder` when a sentinel near the top of the scroll container
 * enters the viewport, then preserves the user's visual position across the
 * prepend by anchoring a stable message DOM node.
 *
 * The important invariant is: do not infer the user's viewport from global
 * `scrollHeight` changes. Capture the first visible `[data-message-id]`, keep
 * that anchor fresh while the user continues scrolling during the fetch, and
 * after React commits the prepended rows, move the scroll container by the
 * anchor node's visual delta exactly once per layout change.
 */
export function useLoadOlderOnScroll({
  fetchOlder,
  hasOlderMessages,
  isLoading,
  scrollContainerRef,
  sentinelRef,
}: UseLoadOlderOnScrollOptions) {
  const [, scheduleLayoutCheck] = React.useReducer(
    (count: number) => count + 1,
    0,
  );
  const activeAnchorRef = React.useRef<ScrollAnchor | null>(null);
  const fetchSettledRef = React.useRef(false);
  const isRestoringRef = React.useRef(false);

  React.useLayoutEffect(() => {
    const container = scrollContainerRef.current;
    const anchor = activeAnchorRef.current;
    if (!container || !anchor) {
      return;
    }

    isRestoringRef.current = true;
    const restored = restoreAnchor(container, anchor);
    requestAnimationFrame(() => {
      isRestoringRef.current = false;
    });

    if (!restored) {
      activeAnchorRef.current = null;
      fetchSettledRef.current = false;
      return;
    }

    if (fetchSettledRef.current) {
      activeAnchorRef.current = null;
      fetchSettledRef.current = false;
    }
  });

  React.useEffect(() => {
    const container = scrollContainerRef.current;
    if (!container) {
      return;
    }

    const updateAnchorFromUserScroll = () => {
      if (!activeAnchorRef.current || isRestoringRef.current) {
        return;
      }
      activeAnchorRef.current = captureFirstVisibleMessage(container);
    };

    container.addEventListener("scroll", updateAnchorFromUserScroll, {
      passive: true,
    });

    return () => {
      container.removeEventListener("scroll", updateAnchorFromUserScroll);
    };
  }, [scrollContainerRef]);

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
          fetchSettledRef.current = false;
          activeAnchorRef.current = captureFirstVisibleMessage(container);

          void fetchOlder().finally(() => {
            if (disposed) {
              return;
            }
            fetchSettledRef.current = true;
            scheduleLayoutCheck();
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
