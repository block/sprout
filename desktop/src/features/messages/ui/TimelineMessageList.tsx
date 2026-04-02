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
import { SystemMessageRow } from "./SystemMessageRow";

type TimelineMessageListProps = {
  activeReplyTargetId?: string | null;
  currentPubkey?: string;
  highlightedMessageId?: string | null;
  messages: TimelineMessage[];
  onEdit?: (message: TimelineMessage) => void;
  onReply?: (message: TimelineMessage) => void;
  onToggleReaction?: (
    message: TimelineMessage,
    emoji: string,
    remove: boolean,
  ) => Promise<void>;
  profiles?: UserProfileLookup;
};

export const TimelineMessageList = React.memo(function TimelineMessageList({
  activeReplyTargetId = null,
  currentPubkey,
  highlightedMessageId = null,
  messages,
  onEdit,
  onReply,
  onToggleReaction,
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
          profiles={profiles}
          time={message.time}
        />,
      );
    } else {
      elements.push(
        <MessageRow
          key={message.id}
          activeReplyTargetId={activeReplyTargetId}
          highlighted={message.id === highlightedMessageId}
          message={message}
          onEdit={
            onEdit && currentPubkey && message.pubkey === currentPubkey
              ? onEdit
              : undefined
          }
          onToggleReaction={onToggleReaction}
          onReply={onReply}
          profiles={profiles}
        />,
      );
    }
  }

  return elements;
});
