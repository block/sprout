import * as React from "react";

const STICKY_DAY_TRIGGER_OFFSET_PX = 64;

/**
 * Tracks the day label of the topmost visible day-divider in the scroll
 * container so a floating header can mirror it.
 *
 * Returns `null` when the first divider is still fully visible (no header
 * needed) or when there are no dividers in the DOM.
 */
export function useStickyDayHeader(
  scrollContainerRef: React.RefObject<HTMLDivElement | null>,
) {
  const [label, setLabel] = React.useState<string | null>(null);

  const update = React.useCallback(() => {
    const container = scrollContainerRef.current;
    if (!container) {
      return;
    }

    const dividers = container.querySelectorAll<HTMLElement>(
      "[data-testid='message-timeline-day-divider']",
    );
    if (dividers.length === 0) {
      setLabel(null);
      return;
    }

    const containerTop = container.getBoundingClientRect().top;
    const stickyTriggerTop = containerTop + STICKY_DAY_TRIGGER_OFFSET_PX;

    // Walk dividers from the end — the last one whose top has reached the
    // floating label's visual zone is the "current" day.
    let current: string | null = null;
    for (let i = dividers.length - 1; i >= 0; i--) {
      const rect = dividers[i].getBoundingClientRect();
      if (rect.top <= stickyTriggerTop) {
        current = dividers[i].getAttribute("data-day-label");
        break;
      }
    }

    setLabel(current);
  }, [scrollContainerRef]);

  React.useEffect(() => {
    const container = scrollContainerRef.current;
    if (!container) {
      return;
    }

    container.addEventListener("scroll", update, { passive: true });
    // Run once on mount so the header appears if already scrolled.
    update();
    return () => {
      container.removeEventListener("scroll", update);
    };
  }, [scrollContainerRef, update]);

  return label;
}
