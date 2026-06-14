/**
 * Pure decision helpers for the Phase A timeline concurrency work.
 *
 * Phase A gated the heavy `MessageTimeline` render behind React's
 * `useDeferredValue` so the main thread stops freezing. The *risk* in that
 * change is not React itself — it's the decision logic that reads the deferred
 * snapshot and the three must-keep behaviors that hang off it:
 *
 *   1. sticky-bottom autoscroll
 *   2. day dividers
 *   3. jump-to-message deep links
 *
 * …plus the shared-snapshot / no-tearing guarantee: all three must read off the
 * SAME snapshot, never a mix of stale and fresh lists. If they tear apart, a
 * deep-link jump can fire against a row that hasn't committed and silently fail.
 *
 * These functions lift those decisions out of the component's render body / the
 * scroll-manager effects so they can be covered by the lib-level `*.test.mjs`
 * suite. The component keeps its React wiring (the `useDeferredValue` call, the
 * effects, the DOM refs) and delegates the actual decisions here.
 */

import type { TimelineMessage } from "@/features/messages/types";
import { isSameDay } from "./dateFormatters";

/** Distance (px) from the bottom within which the timeline counts as "at bottom". */
export const BOTTOM_THRESHOLD_PX = 72;

/** Minimal scroll geometry the sticky-bottom decision needs — a pure subset of the DOM element. */
export type ScrollMetrics = {
  scrollHeight: number;
  clientHeight: number;
  scrollTop: number;
};

/**
 * Sticky-bottom decision: is the timeline scrolled close enough to the bottom
 * to count as "at bottom"? Pure version of the old `isNearBottom(el)` so the
 * threshold math is testable without a DOM.
 */
export function isNearBottomMetrics(metrics: ScrollMetrics): boolean {
  return (
    metrics.scrollHeight - metrics.clientHeight - metrics.scrollTop <=
    BOTTOM_THRESHOLD_PX
  );
}

/**
 * Identity of the last message in a snapshot, used to detect "a new latest
 * message arrived" for autoscroll. Prefers `renderKey` (stable across optimistic
 * send-ack) and falls back to `id`. Returns `undefined` for an empty snapshot.
 */
export function selectLatestMessageKey(
  messages: readonly TimelineMessage[],
): string | undefined {
  if (messages.length === 0) {
    return undefined;
  }
  const latest = messages[messages.length - 1];
  return latest.renderKey ?? latest.id;
}

/** A single day boundary in the timeline: where it starts and how many messages it covers. */
export type DayGroupBoundary = {
  /** Stable key for the day section. */
  key: string;
  /** Index into `messages` of the first message in this day. */
  startIndex: number;
  /** Number of messages in this day group. */
  count: number;
  /** The `createdAt` (unix seconds) used to render the heading label. */
  headingTimestamp: number;
};

/**
 * Day-divider decision: walk a snapshot in order and produce the day-group
 * boundaries. A new group starts at index 0 and whenever a message falls on a
 * different calendar day than the one before it — exactly the rule the render
 * loop used inline, now pure and testable.
 */
export function buildDayGroupBoundaries(
  messages: readonly TimelineMessage[],
): DayGroupBoundary[] {
  const boundaries: DayGroupBoundary[] = [];

  for (let i = 0; i < messages.length; i++) {
    const message = messages[i];
    const prev = i > 0 ? messages[i - 1] : null;

    if (!prev || !isSameDay(prev.createdAt, message.createdAt)) {
      boundaries.push({
        key: `day-${message.createdAt}`,
        startIndex: i,
        count: 1,
        headingTimestamp: message.createdAt,
      });
    } else {
      boundaries[boundaries.length - 1].count += 1;
    }
  }

  return boundaries;
}

/** Outcome of resolving a deep-link target against the current snapshot. */
export type DeepLinkResolution = {
  /** Whether the target message exists in this snapshot (i.e. a row would be committed). */
  resolved: boolean;
  /** Index of the target in `messages`, or -1 when unresolved. */
  index: number;
};

/**
 * Deep-link decision: does a jump-to-message target resolve against THIS
 * snapshot? The scroll-manager effect only does `querySelector` +
 * `scrollIntoView` once a target row is actually committed — so the jump must
 * read the same snapshot the list rendered, or it scrolls to a row that isn't
 * there yet. This is the tearing race Phase A closed.
 */
export function resolveDeepLinkTarget(
  messages: readonly TimelineMessage[],
  targetMessageId: string | null | undefined,
): DeepLinkResolution {
  if (!targetMessageId) {
    return { resolved: false, index: -1 };
  }
  const index = messages.findIndex((message) => message.id === targetMessageId);
  return { resolved: index !== -1, index };
}
