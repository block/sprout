import { isNearBottomMetrics } from "@/features/messages/lib/timelineDecisions";

export function isNearBottom(container: HTMLDivElement) {
  // Decision delegated to a pure, lib-tested helper; this wrapper just reads the
  // live geometry off the DOM element.
  return isNearBottomMetrics({
    scrollHeight: container.scrollHeight,
    clientHeight: container.clientHeight,
    scrollTop: container.scrollTop,
  });
}
