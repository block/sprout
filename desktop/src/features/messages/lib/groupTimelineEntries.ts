import { KIND_SYSTEM_MESSAGE } from "@/shared/constants/kinds";
import type { MainTimelineEntry } from "@/features/messages/lib/threadPanel";

/**
 * Maximum gap (in seconds) between two consecutive messages from the same
 * author before the grouping breaks and a new "full chrome" row is shown.
 */
const COMPACT_MESSAGE_GAP_SECONDS = 120; // 2 minutes

// ---------------------------------------------------------------------------
// Annotated entry types
// ---------------------------------------------------------------------------

export type AnnotatedTimelineEntry =
  | AnnotatedMessageEntry
  | AnnotatedSystemEventGroup;

export type AnnotatedMessageEntry = MainTimelineEntry & {
  entryType: "message";
  /** True when this message should render without avatar / author / timestamp. */
  isGroupContinuation: boolean;
};

export type AnnotatedSystemEventGroup = {
  entryType: "system-event-group";
  /** The individual system events in this group (always ≥ 2). */
  entries: MainTimelineEntry[];
};

// ---------------------------------------------------------------------------
// Grouping logic
// ---------------------------------------------------------------------------

/**
 * Annotates a flat list of `MainTimelineEntry` items with grouping metadata.
 *
 * Two behaviours:
 * 1. **Consecutive message compacting** — same author, same message kind,
 *    within `COMPACT_MESSAGE_GAP_SECONDS` → subsequent messages marked as
 *    `isGroupContinuation: true` (no avatar / name chrome).
 * 2. **System event grouping** — consecutive runs of system events (≥ 2)
 *    are collapsed into a single `AnnotatedSystemEventGroup`.
 */
export function groupTimelineEntries(
  entries: MainTimelineEntry[],
): AnnotatedTimelineEntry[] {
  const result: AnnotatedTimelineEntry[] = [];
  let i = 0;

  while (i < entries.length) {
    const entry = entries[i];

    // --- System event run detection ---
    if (entry.message.kind === KIND_SYSTEM_MESSAGE) {
      const groupStart = i;
      while (
        i < entries.length &&
        entries[i].message.kind === KIND_SYSTEM_MESSAGE
      ) {
        i++;
      }

      const run = entries.slice(groupStart, i);
      if (run.length >= 2) {
        result.push({ entryType: "system-event-group", entries: run });
      } else {
        // Single system event — render normally (no accordion).
        result.push({
          ...run[0],
          entryType: "message",
          isGroupContinuation: false,
        });
      }
      continue;
    }

    // --- Chat message compacting ---
    const prev = result.length > 0 ? result[result.length - 1] : null;
    const isCompact = shouldCompact(prev, entry);

    result.push({
      ...entry,
      entryType: "message",
      isGroupContinuation: isCompact,
    });
    i++;
  }

  return result;
}

/**
 * Determines whether `current` should render in compact mode (no avatar /
 * author line) based on the previous annotated entry.
 */
function shouldCompact(
  prev: AnnotatedTimelineEntry | null,
  current: MainTimelineEntry,
): boolean {
  if (!prev) return false;

  // Can only compact after another message entry (not after a system group).
  if (prev.entryType !== "message") return false;

  const prevMsg = prev.message;
  const curMsg = current.message;

  // Must be the same author.
  if (prevMsg.pubkey !== curMsg.pubkey) return false;

  // Don't compact if either is a system message.
  if (
    prevMsg.kind === KIND_SYSTEM_MESSAGE ||
    curMsg.kind === KIND_SYSTEM_MESSAGE
  )
    return false;

  // Must be within the time window.
  if (curMsg.createdAt - prevMsg.createdAt > COMPACT_MESSAGE_GAP_SECONDS)
    return false;

  // Don't compact if the current message has a thread summary (visual break).
  if (current.summary) return false;

  // Don't compact if the previous message had a thread summary.
  if (prev.summary) return false;

  return true;
}
