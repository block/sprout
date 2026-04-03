/**
 * Shared time utilities.
 */

const MINUTE = 60;
const HOUR = 3_600;
const DAY = 86_400;
const WEEK = 604_800;

const relativeFormatter = new Intl.RelativeTimeFormat("en-US", {
  numeric: "auto",
});

/**
 * Format a unix-seconds timestamp or ISO date string as a human-readable
 * relative time string (e.g. "just now", "5m ago", "yesterday").
 * Returns null when the input is null/undefined/unparseable.
 */
export function formatRelativeTime(
  input: number | string | null | undefined,
): string | null {
  if (input == null) return null;

  let unixSeconds: number;
  if (typeof input === "number") {
    unixSeconds = input;
  } else {
    const parsed = Date.parse(input);
    if (Number.isNaN(parsed)) return null;
    unixSeconds = parsed / 1_000;
  }

  const diff = unixSeconds - Math.floor(Date.now() / 1_000);
  const absDiff = Math.abs(diff);

  if (absDiff < MINUTE) {
    return relativeFormatter.format(Math.round(diff), "second");
  }

  if (absDiff < HOUR) {
    return relativeFormatter.format(Math.round(diff / MINUTE), "minute");
  }

  if (absDiff < DAY) {
    return relativeFormatter.format(Math.round(diff / HOUR), "hour");
  }

  if (absDiff < WEEK) {
    return relativeFormatter.format(Math.round(diff / DAY), "day");
  }

  return new Intl.DateTimeFormat("en-US", {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(new Date(unixSeconds * 1_000));
}

/**
 * Parse an ISO date string or null/undefined to a millisecond timestamp.
 * Returns null when the input is absent or unparseable.
 */
export function parseTimestamp(
  value: string | null | undefined,
): number | null {
  if (!value) return null;
  const ts = Date.parse(value);
  return Number.isNaN(ts) ? null : ts;
}
