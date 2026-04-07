import type { RelayEvent } from "@/shared/api/types";

const MAX_TIMELINE_MESSAGES = 2_000;

export function channelMessagesKey(channelId: string) {
  return ["channel-messages", channelId] as const;
}

/** React-query key for GET /api/channels/:id/threads/:root_event_id (stream or forum). */
export function channelThreadKey(channelId: string, rootEventId: string) {
  return ["channel-thread", channelId, rootEventId] as const;
}

export function dedupeMessagesById(messages: RelayEvent[]) {
  const seenIds = new Set<string>();
  const deduped: RelayEvent[] = [];

  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];

    if (seenIds.has(message.id)) {
      continue;
    }

    seenIds.add(message.id);
    deduped.push(message);
  }

  return deduped.reverse();
}

export function sortMessages(messages: RelayEvent[]) {
  return dedupeMessagesById(messages).sort(
    (left, right) => left.created_at - right.created_at,
  );
}

/**
 * Sort, dedupe, and cap the timeline at {@link MAX_TIMELINE_MESSAGES} so
 * de-virtualized rendering does not grow into an unbounded DOM during
 * long-lived channel sessions.
 */
export function normalizeTimelineMessages(messages: RelayEvent[]) {
  const normalized = sortMessages(messages);

  if (normalized.length <= MAX_TIMELINE_MESSAGES) {
    return normalized;
  }

  return normalized.slice(-MAX_TIMELINE_MESSAGES);
}
