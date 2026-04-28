import * as React from "react";

import {
  formatDayHeading,
  isSameDay,
} from "@/features/messages/lib/dateFormatters";
import {
  groupTimelineEntries,
  type AnnotatedTimelineEntry,
} from "@/features/messages/lib/groupTimelineEntries";
import { buildMainTimelineEntries } from "@/features/messages/lib/threadPanel";
import type { TimelineMessage } from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { KIND_SYSTEM_MESSAGE } from "@/shared/constants/kinds";
import { CompactMessageRow } from "./CompactMessageRow";
import { DayDivider } from "./DayDivider";
import { MessageRow } from "./MessageRow";
import { MessageThreadSummaryRow } from "./MessageThreadSummaryRow";
import { SystemEventGroupRow } from "./SystemEventGroupRow";
import { SystemMessageRow } from "./SystemMessageRow";

type TimelineMessageListProps = {
  activeReplyTargetId?: string | null;
  currentPubkey?: string;
  highlightedMessageId?: string | null;
  messages: TimelineMessage[];
  onDelete?: (message: TimelineMessage) => void;
  onEdit?: (message: TimelineMessage) => void;
  onReply?: (message: TimelineMessage) => void;
  onToggleReaction?: (
    message: TimelineMessage,
    emoji: string,
    remove: boolean,
  ) => Promise<void>;
  /** Map from lowercase pubkey → persona display name for bot members. */
  personaLookup?: Map<string, string>;
  profiles?: UserProfileLookup;
  /** The message ID of the currently active find-in-channel match. */
  searchActiveMessageId?: string | null;
  /** Set of message IDs that match the current find-in-channel query. */
  searchMatchingMessageIds?: Set<string>;
  /** The current find-in-channel query string. */
  searchQuery?: string;
};

/** Return the first message's createdAt for a given annotated entry. */
function getEntryLeadTimestamp(entry: AnnotatedTimelineEntry): number {
  if (entry.entryType === "system-event-group") {
    return entry.entries[0].message.createdAt;
  }
  return entry.message.createdAt;
}

export const TimelineMessageList = React.memo(function TimelineMessageList({
  activeReplyTargetId = null,
  currentPubkey,
  highlightedMessageId = null,
  messages,
  onDelete,
  onEdit,
  onReply,
  onToggleReaction,
  personaLookup,
  profiles,
  searchActiveMessageId = null,
  searchMatchingMessageIds,
  searchQuery,
}: TimelineMessageListProps) {
  const elements: React.ReactNode[] = [];

  const annotated = React.useMemo(() => {
    const raw = buildMainTimelineEntries(messages);
    return groupTimelineEntries(raw);
  }, [messages]);

  let prevTimestamp: number | null = null;

  for (let i = 0; i < annotated.length; i++) {
    const entry = annotated[i];
    const leadTimestamp = getEntryLeadTimestamp(entry);

    // Day divider
    if (prevTimestamp === null || !isSameDay(prevTimestamp, leadTimestamp)) {
      elements.push(
        <DayDivider
          key={`day-${leadTimestamp}`}
          label={formatDayHeading(leadTimestamp)}
        />,
      );
    }
    prevTimestamp = leadTimestamp;

    // --- System event group (accordion) ---
    if (entry.entryType === "system-event-group") {
      const groupKey = entry.entries.map((e) => e.message.id).join(",");
      elements.push(
        <SystemEventGroupRow
          key={`sys-group-${groupKey}`}
          entries={entry.entries}
          currentPubkey={currentPubkey}
          personaLookup={personaLookup}
          profiles={profiles}
        />,
      );
      prevTimestamp = entry.entries[entry.entries.length - 1].message.createdAt;
      continue;
    }

    // --- Single system message (not grouped) ---
    const { message, summary } = entry;

    if (message.kind === KIND_SYSTEM_MESSAGE) {
      elements.push(
        <SystemMessageRow
          key={message.id}
          body={message.body}
          createdAt={message.createdAt}
          currentPubkey={currentPubkey}
          personaLookup={personaLookup}
          profiles={profiles}
          time={message.time}
        />,
      );
      continue;
    }

    // --- Search highlight state ---
    const isSearchMatch = searchMatchingMessageIds?.has(message.id) ?? false;
    const isSearchActive = message.id === searchActiveMessageId;

    // --- Compact message (continuation) ---
    if (entry.isGroupContinuation) {
      elements.push(
        <CompactMessageRow
          key={message.id}
          activeReplyTargetId={activeReplyTargetId}
          highlighted={message.id === highlightedMessageId || isSearchActive}
          message={message}
          onDelete={
            onDelete && currentPubkey && message.pubkey === currentPubkey
              ? onDelete
              : undefined
          }
          onEdit={
            onEdit && currentPubkey && message.pubkey === currentPubkey
              ? onEdit
              : undefined
          }
          onToggleReaction={onToggleReaction}
          onReply={onReply}
          profiles={profiles}
          searchQuery={isSearchMatch ? searchQuery : undefined}
        />,
      );
      continue;
    }

    // --- Full message row ---
    elements.push(
      <MessageRow
        key={message.id}
        activeReplyTargetId={activeReplyTargetId}
        highlighted={message.id === highlightedMessageId || isSearchActive}
        message={message}
        onDelete={
          onDelete && currentPubkey && message.pubkey === currentPubkey
            ? onDelete
            : undefined
        }
        onEdit={
          onEdit && currentPubkey && message.pubkey === currentPubkey
            ? onEdit
            : undefined
        }
        onToggleReaction={onToggleReaction}
        onReply={onReply}
        profiles={profiles}
        searchQuery={isSearchMatch ? searchQuery : undefined}
      />,
    );

    if (summary && onReply) {
      elements.push(
        <MessageThreadSummaryRow
          key={`thread-summary-${message.id}`}
          message={message}
          onOpenThread={onReply}
          summary={summary}
        />,
      );
    }
  }

  return elements;
});
