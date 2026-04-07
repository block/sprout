/**
 * Shared date/time formatters for the message timeline.
 *
 * - `formatTime` — short clock time ("2:34 PM"), used in message rows.
 * - `formatFullDateTime` — verbose string for tooltips
 *   ("Wednesday, April 2, 2026 at 2:34 PM").
 * - `formatDayHeading` — label for day dividers / sticky headers.
 *   Returns "Today", "Yesterday", or a compact date like "Sat, Mar 21, 2026".
 * - `isSameDay` — compare two unix-second timestamps.
 */

const TIME_FORMATTER = new Intl.DateTimeFormat("en-US", {
  hour: "numeric",
  minute: "2-digit",
});

const FULL_DATE_TIME_FORMATTER = new Intl.DateTimeFormat("en-US", {
  weekday: "long",
  year: "numeric",
  month: "long",
  day: "numeric",
  hour: "numeric",
  minute: "2-digit",
});

/** Compact calendar line for dividers (not Today/Yesterday). */
const DAY_HEADING_COMPACT = new Intl.DateTimeFormat("en-US", {
  weekday: "short",
  month: "short",
  day: "numeric",
  year: "numeric",
});

/** Short clock time, e.g. "2:34 PM". */
export function formatTime(unixSeconds: number): string {
  return TIME_FORMATTER.format(new Date(unixSeconds * 1_000));
}

/** Full date + time for tooltips, e.g. "Wednesday, April 2, 2026 at 2:34 PM". */
export function formatFullDateTime(unixSeconds: number): string {
  return FULL_DATE_TIME_FORMATTER.format(new Date(unixSeconds * 1_000));
}

/**
 * Human-friendly day label for dividers and sticky headers.
 * Returns "Today", "Yesterday", or a short date like "Sat, Mar 21, 2026".
 */
export function formatDayHeading(unixSeconds: number): string {
  const date = new Date(unixSeconds * 1_000);
  const now = new Date();

  if (isSameDayDate(date, now)) {
    return "Today";
  }

  const yesterday = new Date(now);
  yesterday.setDate(yesterday.getDate() - 1);
  if (isSameDayDate(date, yesterday)) {
    return "Yesterday";
  }

  return DAY_HEADING_COMPACT.format(date);
}

/** True when two unix-second timestamps fall on the same calendar day (local time). */
export function isSameDay(a: number, b: number): boolean {
  return isSameDayDate(new Date(a * 1_000), new Date(b * 1_000));
}

function isSameDayDate(a: Date, b: Date): boolean {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}
