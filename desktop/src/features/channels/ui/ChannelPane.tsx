import * as React from "react";

import { MessageComposer } from "@/features/messages/ui/MessageComposer";
import { MessageThreadPanel } from "@/features/messages/ui/MessageThreadPanel";
import { MessageTimeline } from "@/features/messages/ui/MessageTimeline";
import { TypingIndicatorRow } from "@/features/messages/ui/TypingIndicatorRow";
import type { MainTimelineEntry } from "@/features/messages/lib/threadPanel";
import type { TimelineMessage } from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import type { Channel } from "@/shared/api/types";

const THREAD_PANEL_DEFAULT_WIDTH_PX = 380;
const THREAD_PANEL_MIN_WIDTH_PX = 320;
const THREAD_PANEL_MAX_WIDTH_PX = 720;
const THREAD_PANEL_WIDTH_SESSION_KEY = "sprout.desktop.thread-panel-width";

function clampThreadPanelWidth(width: number): number {
  return Math.max(
    THREAD_PANEL_MIN_WIDTH_PX,
    Math.min(THREAD_PANEL_MAX_WIDTH_PX, width),
  );
}

function getInitialThreadPanelWidth(): number {
  if (typeof window === "undefined") {
    return THREAD_PANEL_DEFAULT_WIDTH_PX;
  }

  try {
    const raw = window.sessionStorage.getItem(THREAD_PANEL_WIDTH_SESSION_KEY);
    if (!raw) {
      return THREAD_PANEL_DEFAULT_WIDTH_PX;
    }

    const parsed = Number.parseInt(raw, 10);
    if (!Number.isFinite(parsed)) {
      return THREAD_PANEL_DEFAULT_WIDTH_PX;
    }

    return clampThreadPanelWidth(parsed);
  } catch {
    return THREAD_PANEL_DEFAULT_WIDTH_PX;
  }
}

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
  threadReplyTargetId: string | null;
  threadReplyTargetMessage: TimelineMessage | null;
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
  threadScrollTargetId,
  threadTypingPubkeys,
  threadReplyTargetId,
  threadReplyTargetMessage,
  typingPubkeys,
}: ChannelPaneProps) {
  const [threadPanelWidthPx, setThreadPanelWidthPx] = React.useState<number>(
    () => getInitialThreadPanelWidth(),
  );

  React.useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }

    try {
      window.sessionStorage.setItem(
        THREAD_PANEL_WIDTH_SESSION_KEY,
        String(threadPanelWidthPx),
      );
    } catch {
      // Ignore storage failures and keep in-memory width for this session.
    }
  }, [threadPanelWidthPx]);

  const handleThreadPanelResizeStart = React.useCallback(
    (event: React.PointerEvent<HTMLButtonElement>) => {
      event.preventDefault();

      const startX = event.clientX;
      const startWidth = threadPanelWidthPx;
      const previousCursor = document.body.style.cursor;
      const previousUserSelect = document.body.style.userSelect;

      document.body.style.cursor = "col-resize";
      document.body.style.userSelect = "none";

      const handlePointerMove = (moveEvent: PointerEvent) => {
        const deltaX = startX - moveEvent.clientX;
        const nextWidth = clampThreadPanelWidth(startWidth + deltaX);
        setThreadPanelWidthPx(nextWidth);
      };

      const handlePointerUp = () => {
        document.body.style.cursor = previousCursor;
        document.body.style.userSelect = previousUserSelect;
        window.removeEventListener("pointermove", handlePointerMove);
      };

      window.addEventListener("pointermove", handlePointerMove);
      window.addEventListener("pointerup", handlePointerUp, { once: true });
    },
    [threadPanelWidthPx],
  );

  const handleThreadPanelWidthReset = React.useCallback(() => {
    setThreadPanelWidthPx(THREAD_PANEL_DEFAULT_WIDTH_PX);
  }, []);

  const canResetThreadPanelWidth =
    threadPanelWidthPx !== THREAD_PANEL_DEFAULT_WIDTH_PX;

  const isComposerDisabled =
    !activeChannel?.isMember ||
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
        <TypingIndicatorRow
          channel={activeChannel}
          currentPubkey={currentPubkey}
          profiles={profiles}
          typingPubkeys={typingPubkeys}
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
          profiles={profiles}
          replyTargetId={threadReplyTargetId}
          replyTargetMessage={threadReplyTargetMessage}
          scrollTargetId={threadScrollTargetId}
          canResetWidth={canResetThreadPanelWidth}
          onResetWidth={handleThreadPanelWidthReset}
          onResizeStart={handleThreadPanelResizeStart}
          threadHead={threadHeadMessage}
          widthPx={threadPanelWidthPx}
          threadReplies={threadMessages}
          threadTypingPubkeys={threadTypingPubkeys}
        />
      ) : null}
    </div>
  );
});
