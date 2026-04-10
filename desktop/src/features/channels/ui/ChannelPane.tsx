import * as React from "react";

import { ChannelThreadPanel } from "@/features/messages/ui/ChannelThreadPanel";
import { MessageComposer } from "@/features/messages/ui/MessageComposer";
import { MessageTimeline } from "@/features/messages/ui/MessageTimeline";
import { TypingIndicatorRow } from "@/features/messages/ui/TypingIndicatorRow";
import type { TimelineMessage } from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import type { Channel } from "@/shared/api/types";

type ChannelPaneProps = {
  activeChannel: Channel | null;
  currentPubkey?: string;
  editTarget?: {
    author: string;
    body: string;
    id: string;
  } | null;
  fetchOlder?: () => Promise<void>;
  hasOlderMessages?: boolean;
  isFetchingOlder?: boolean;
  isSending: boolean;
  isTimelineLoading: boolean;
  messages: TimelineMessage[];
  onCancelEdit?: () => void;
  onCancelReply: () => void;
  onCloseThread?: () => void;
  onDelete?: (message: TimelineMessage) => void;
  onEdit?: (message: TimelineMessage) => void;
  onEditSave?: (content: string) => Promise<void>;
  onOpenThread?: (message: TimelineMessage) => void;
  onReply: (message: TimelineMessage) => void;
  onSend: (
    content: string,
    mentionPubkeys: string[],
    mediaTags?: string[][],
  ) => Promise<void>;
  onTargetReached?: (messageId: string) => void;
  onToggleReaction?: (
    message: TimelineMessage,
    emoji: string,
    remove: boolean,
  ) => Promise<void>;
  /** Map from lowercase pubkey → persona display name for bot members. */
  personaLookup?: Map<string, string>;
  profiles?: UserProfileLookup;
  replyTargetId: string | null;
  replyTargetMessage: TimelineMessage | null;
  targetMessageId: string | null;
  threadRootId: string | null;
  typingPubkeys: string[];
};

export const ChannelPane = React.memo(function ChannelPane({
  activeChannel,
  currentPubkey,
  editTarget = null,
  fetchOlder,
  hasOlderMessages,
  isFetchingOlder,
  isSending,
  isTimelineLoading,
  messages,
  onCancelEdit,
  onCancelReply,
  onCloseThread,
  onDelete,
  onEdit,
  onEditSave,
  onOpenThread,
  onReply,
  onSend,
  onTargetReached,
  onToggleReaction,
  personaLookup,
  profiles,
  replyTargetId,
  replyTargetMessage,
  targetMessageId,
  threadRootId,
  typingPubkeys,
}: ChannelPaneProps) {
  const composerDisabled =
    !activeChannel ||
    !activeChannel.isMember ||
    activeChannel.archivedAt !== null ||
    activeChannel.channelType === "forum" ||
    isSending;

  const mainPlaceholder = activeChannel?.archivedAt
    ? "Archived channels are read-only."
    : activeChannel && !activeChannel.isMember
      ? "Join this channel to message."
      : activeChannel?.channelType === "forum"
        ? "Forum posting is not wired in this pass."
        : activeChannel
          ? `Message #${activeChannel.name}`
          : "Select a channel";

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-row overflow-hidden">
      <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
        <MessageTimeline
          channelId={activeChannel?.id}
          activeReplyTargetId={replyTargetId}
          currentPubkey={currentPubkey}
          fetchOlder={fetchOlder}
          hasOlderMessages={hasOlderMessages}
          isFetchingOlder={isFetchingOlder}
          personaLookup={personaLookup}
          profiles={profiles}
          emptyDescription={
            activeChannel?.channelType === "forum"
              ? "Select a stream or DM to load real message history in this first integration pass."
              : "Messages and sub-replies will appear here once the relay has history for this channel."
          }
          emptyTitle={
            activeChannel
              ? activeChannel.channelType === "forum"
                ? "Forum channels are next"
                : "No messages yet"
              : "No channel selected"
          }
          isLoading={isTimelineLoading}
          messages={messages}
          onDelete={onDelete}
          onEdit={onEdit}
          onOpenThread={onOpenThread}
          onReply={onReply}
          onTargetReached={onTargetReached}
          onToggleReaction={onToggleReaction}
          targetMessageId={targetMessageId}
        />
        <TypingIndicatorRow
          channel={activeChannel}
          currentPubkey={currentPubkey}
          profiles={profiles}
          typingPubkeys={typingPubkeys}
        />
        {!threadRootId ? (
          <MessageComposer
            channelId={activeChannel?.id ?? null}
            channelName={activeChannel?.name ?? "channel"}
            disabled={composerDisabled}
            editTarget={editTarget}
            isSending={isSending}
            onCancelEdit={onCancelEdit}
            onCancelReply={onCancelReply}
            onEditSave={onEditSave}
            onSend={onSend}
            placeholder={mainPlaceholder}
            replyTarget={
              replyTargetMessage
                ? {
                    author: replyTargetMessage.author,
                    body: replyTargetMessage.body,
                    id: replyTargetMessage.id,
                  }
                : null
            }
          />
        ) : null}
      </div>

      {threadRootId && activeChannel && onCloseThread ? (
        <ChannelThreadPanel
          channel={activeChannel}
          currentPubkey={currentPubkey}
          disabledComposer={composerDisabled}
          editTarget={editTarget}
          isSending={isSending}
          onCancelEdit={onCancelEdit}
          onClose={onCloseThread}
          onEditSave={onEditSave}
          onSend={onSend}
          profiles={profiles}
          rootEventId={threadRootId}
        />
      ) : null}
    </div>
  );
});
