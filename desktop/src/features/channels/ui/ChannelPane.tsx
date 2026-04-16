import * as React from "react";

import { MessageComposer } from "@/features/messages/ui/MessageComposer";
import { MessageThreadPanel } from "@/features/messages/ui/MessageThreadPanel";
import { MessageTimeline } from "@/features/messages/ui/MessageTimeline";
import { TypingIndicatorRow } from "@/features/messages/ui/TypingIndicatorRow";
import type { MainTimelineEntry } from "@/features/messages/lib/threadPanel";
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
  onCancelThreadReply: () => void;
  onCloseThread: () => void;
  onDelete?: (message: TimelineMessage) => void;
  onEdit?: (message: TimelineMessage) => void;
  onEditSave?: (content: string) => Promise<void>;
  onExpandThreadReplies: (message: TimelineMessage) => void;
  onOpenThread: (message: TimelineMessage) => void;
  onSelectThreadReplyTarget: (message: TimelineMessage) => void;
  onSendMessage: (
    content: string,
    mentionPubkeys: string[],
    mediaTags?: string[][],
  ) => Promise<void>;
  onSendThreadReply: (
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
  onThreadScrollTargetResolved: () => void;
  /** Map from lowercase pubkey → persona display name for bot members. */
  personaLookup?: Map<string, string>;
  profiles?: UserProfileLookup;
  openThreadHeadId: string | null;
  threadHeadMessage: TimelineMessage | null;
  threadMessages: MainTimelineEntry[];
  threadTypingPubkeys: string[];
  threadTotalReplyCount: number;
  threadReplyTargetId: string | null;
  threadReplyTargetMessage: TimelineMessage | null;
  threadPanelOpenKey: number;
  threadScrollTargetId: string | null;
  targetMessageId: string | null;
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
  onCancelThreadReply,
  onCloseThread,
  onDelete,
  onEdit,
  onEditSave,
  onExpandThreadReplies,
  onOpenThread,
  onSelectThreadReplyTarget,
  onSendMessage,
  onSendThreadReply,
  onThreadScrollTargetResolved,
  onTargetReached,
  onToggleReaction,
  personaLookup,
  profiles,
  openThreadHeadId,
  targetMessageId,
  threadHeadMessage,
  threadMessages,
  threadPanelOpenKey,
  threadScrollTargetId,
  threadTypingPubkeys,
  threadTotalReplyCount,
  threadReplyTargetId,
  threadReplyTargetMessage,
  typingPubkeys,
}: ChannelPaneProps) {
  const isComposerDisabled =
    !activeChannel ||
    !activeChannel.isMember ||
    activeChannel.archivedAt !== null ||
    activeChannel.channelType === "forum" ||
    isSending;

  return (
    <div className="flex min-h-0 flex-1 overflow-hidden">
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        <MessageTimeline
          channelId={activeChannel?.id}
          activeReplyTargetId={openThreadHeadId}
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
          onReply={onOpenThread}
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
        <MessageComposer
          channelId={activeChannel?.id ?? null}
          channelName={activeChannel?.name ?? "channel"}
          disabled={isComposerDisabled}
          editTarget={editTarget}
          isSending={isSending}
          onCancelEdit={onCancelEdit}
          onEditSave={onEditSave}
          onSend={onSendMessage}
          placeholder={
            activeChannel?.archivedAt
              ? "Archived channels are read-only."
              : activeChannel && !activeChannel.isMember
                ? "Join this channel to message."
                : activeChannel?.channelType === "forum"
                  ? "Forum posting is not wired in this pass."
                  : activeChannel
                    ? `Message #${activeChannel.name}`
                    : "Select a channel"
          }
        />
      </div>

      {threadHeadMessage ? (
        <MessageThreadPanel
          channel={activeChannel}
          channelId={activeChannel?.id ?? null}
          channelName={activeChannel?.name ?? "channel"}
          currentPubkey={currentPubkey}
          disabled={isComposerDisabled}
          isSending={isSending}
          onCancelReply={onCancelThreadReply}
          onClose={onCloseThread}
          onDelete={onDelete}
          onExpandReplies={onExpandThreadReplies}
          onSelectReplyTarget={onSelectThreadReplyTarget}
          onSend={onSendThreadReply}
          onScrollTargetResolved={onThreadScrollTargetResolved}
          onToggleReaction={onToggleReaction}
          openKey={threadPanelOpenKey}
          profiles={profiles}
          replyTargetId={threadReplyTargetId}
          replyTargetMessage={threadReplyTargetMessage}
          scrollTargetId={threadScrollTargetId}
          threadHead={threadHeadMessage}
          threadReplies={threadMessages}
          threadTypingPubkeys={threadTypingPubkeys}
          totalReplyCount={threadTotalReplyCount}
        />
      ) : null}
    </div>
  );
});
