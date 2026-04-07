import * as React from "react";

import {
  formatDayHeading,
  isSameDay,
} from "@/features/messages/lib/dateFormatters";
import type {
  ThreadConversationHint,
  TimelineMessage,
} from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { KIND_SYSTEM_MESSAGE } from "@/shared/constants/kinds";
import { DayDivider } from "./DayDivider";
import { MessageRow } from "./MessageRow";
import { SystemMessageRow } from "./SystemMessageRow";

type TimelineMessageListProps = {
  activeReplyTargetId?: string | null;
  activeThreadRootId?: string | null;
  currentPubkey?: string;
  highlightedMessageId?: string | null;
  messages: TimelineMessage[];
  threadHintsByAnchorId?: Map<string, ThreadConversationHint>;
  onDelete?: (message: TimelineMessage) => void;
  onEdit?: (message: TimelineMessage) => void;
  onOpenThread?: (message: TimelineMessage) => void;
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
  activeThreadRootId = null,
  currentPubkey,
  highlightedMessageId = null,
  messages,
  threadHintsByAnchorId,
  onDelete,
  onEdit,
  onOpenThread,
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
      const threadHint = threadHintsByAnchorId?.get(message.id);
      elements.push(
        <MessageRow
          key={message.id}
          activeReplyTargetId={activeReplyTargetId}
          activeThreadRootId={activeThreadRootId}
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
          onOpenThread={onOpenThread}
          onToggleReaction={onToggleReaction}
          onReply={onReply}
          profiles={profiles}
          threadHint={threadHint}
        />,
      );
    }
  }

  return elements;
});
