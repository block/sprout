import * as React from "react";

import {
  formatDayHeading,
  isSameDay,
} from "@/features/messages/lib/dateFormatters";
import { buildMainTimelineEntries } from "@/features/messages/lib/threadPanel";
import type { TimelineMessage } from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { KIND_SYSTEM_MESSAGE } from "@/shared/constants/kinds";
import { DayDivider } from "./DayDivider";
import { MessageRow } from "./MessageRow";
import { MessageThreadSummaryRow } from "./MessageThreadSummaryRow";
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
  const entries = React.useMemo(
    () => buildMainTimelineEntries(messages),
    [messages],
  );
  const dayGroups: Array<{
    key: string;
    label: string;
    elements: React.ReactNode[];
  }> = [];
  let currentDayGroup: (typeof dayGroups)[number] | null = null;

  for (let i = 0; i < entries.length; i++) {
    const { message, summary } = entries[i];
    const prev = i > 0 ? entries[i - 1]?.message : null;

    if (!prev || !isSameDay(prev.createdAt, message.createdAt)) {
      currentDayGroup = {
        key: `day-${message.createdAt}`,
        label: formatDayHeading(message.createdAt),
        elements: [],
      };
      dayGroups.push(currentDayGroup);
    }

    if (message.kind === KIND_SYSTEM_MESSAGE) {
      currentDayGroup?.elements.push(
        <SystemMessageRow
          key={message.id}
          message={message}
          currentPubkey={currentPubkey}
          onToggleReaction={onToggleReaction}
          personaLookup={personaLookup}
          profiles={profiles}
        />,
      );
    } else if (summary && onReply) {
      currentDayGroup?.elements.push(
        <div key={message.id} className="flex flex-col gap-0">
          <MessageRow
            activeReplyTargetId={activeReplyTargetId}
            highlighted={message.id === highlightedMessageId}
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
          />
          <MessageThreadSummaryRow
            depth={message.depth}
            message={message}
            onOpenThread={onReply}
            summary={summary}
          />
        </div>,
      );
    } else {
      const isSearchMatch = searchMatchingMessageIds?.has(message.id) ?? false;
      const isSearchActive = message.id === searchActiveMessageId;

      currentDayGroup?.elements.push(
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
    }
  }

  return dayGroups.map((group) => (
    <section className="flex flex-col gap-2" key={group.key}>
      <DayDivider label={group.label} />
      {group.elements}
    </section>
  ));
});
