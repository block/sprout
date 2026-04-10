import * as React from "react";

import {
  formatDayHeading,
  isSameDay,
} from "@/features/messages/lib/dateFormatters";
import type { TimelineMessage } from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { KIND_SYSTEM_MESSAGE } from "@/shared/constants/kinds";
import { DayDivider } from "./DayDivider";
import { MessageRow } from "./MessageRow";
import { MessageThreadSummary } from "./MessageThreadSummary";
import { SystemMessageRow } from "./SystemMessageRow";

type TimelineMessageListProps = {
  activeReplyTargetId?: string | null;
  currentPubkey?: string;
  highlightedMessageId?: string | null;
  messages: TimelineMessage[];
  onDelete?: (message: TimelineMessage) => void;
  onEdit?: (message: TimelineMessage) => void;
  onOpenThread?: (message: TimelineMessage) => void;
  onReply?: (message: TimelineMessage) => void;
  onToggleReaction?: (
    message: TimelineMessage,
    emoji: string,
    remove: boolean,
  ) => Promise<void>;
  /** Map from lowercase pubkey → persona display name for bot members. */
  personaLookup?: Map<string, string>;
  profiles?: UserProfileLookup;
};

export const TimelineMessageList = React.memo(function TimelineMessageList({
  activeReplyTargetId = null,
  currentPubkey,
  highlightedMessageId = null,
  messages,
  onDelete,
  onEdit,
  onOpenThread,
  onReply,
  onToggleReaction,
  personaLookup,
  profiles,
}: TimelineMessageListProps) {
  const elements: React.ReactNode[] = [];

  for (let i = 0; i < messages.length; i++) {
    const message = messages[i];
    const prev = i > 0 ? messages[i - 1] : null;

    if (!prev || !isSameDay(prev.createdAt, message.createdAt)) {
      elements.push(
        <DayDivider
          key={`day-${message.createdAt}`}
          label={formatDayHeading(message.createdAt)}
        />,
      );
    }

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
    } else {
      elements.push(
        <React.Fragment key={message.id}>
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
          {message.depth === 0 &&
          message.threadSummary &&
          message.threadSummary.descendantCount > 0 ? (
            <MessageThreadSummary
              message={message}
              onOpenThread={onOpenThread}
              profiles={profiles}
            />
          ) : null}
        </React.Fragment>,
      );
    }
  }

  return elements;
});
