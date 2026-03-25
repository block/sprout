import * as React from "react";

import type { TimelineMessage } from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { KIND_SYSTEM_MESSAGE } from "@/shared/constants/kinds";
import { MessageRow } from "./MessageRow";
import { SystemMessageRow } from "./SystemMessageRow";

type TimelineMessageListProps = {
  activeReplyTargetId?: string | null;
  currentPubkey?: string;
  highlightedMessageId?: string | null;
  messages: TimelineMessage[];
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
  onReply,
  onToggleReaction,
  profiles,
}: TimelineMessageListProps) {
  return messages.map((message) =>
    message.kind === KIND_SYSTEM_MESSAGE ? (
      <SystemMessageRow
        key={message.id}
        body={message.body}
        currentPubkey={currentPubkey}
        profiles={profiles}
        time={message.time}
      />
    ) : (
      <MessageRow
        key={message.id}
        activeReplyTargetId={activeReplyTargetId}
        highlighted={message.id === highlightedMessageId}
        message={message}
        onToggleReaction={onToggleReaction}
        onReply={onReply}
        profiles={profiles}
      />
    ),
  );
});
