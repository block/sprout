import * as React from "react";
import { ArrowDown, X } from "lucide-react";

import type { MainTimelineEntry } from "@/features/messages/lib/threadPanel";
import type { TimelineMessage } from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import type { Channel } from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import { MessageComposer } from "./MessageComposer";
import { MessageRow } from "./MessageRow";
import { MessageThreadSummaryRow } from "./MessageThreadSummaryRow";
import { TypingIndicatorRow } from "./TypingIndicatorRow";
import { useTimelineScrollManager } from "./useTimelineScrollManager";

type MessageThreadPanelProps = {
  canResetWidth: boolean;
  channel: Channel | null;
  channelId: string | null;
  channelName: string;
  currentPubkey?: string;
  disabled?: boolean;
  isSending: boolean;
  onCancelReply: () => void;
  onClose: () => void;
  onDelete?: (message: TimelineMessage) => void;
  onExpandReplies: (message: TimelineMessage) => void;
  onResetWidth: () => void;
  onResizeStart: (event: React.PointerEvent<HTMLButtonElement>) => void;
  onScrollTargetResolved: () => void;
  onSelectReplyTarget: (message: TimelineMessage) => void;
  onSend: (
    content: string,
    mentionPubkeys: string[],
    mediaTags?: string[][],
  ) => Promise<void>;
  onToggleReaction?: (
    message: TimelineMessage,
    emoji: string,
    remove: boolean,
  ) => Promise<void>;
  profiles?: UserProfileLookup;
  replyTargetId: string | null;
  replyTargetMessage: TimelineMessage | null;
  scrollTargetId: string | null;
  threadHead: TimelineMessage | null;
  threadReplies: MainTimelineEntry[];
  threadTypingPubkeys: string[];
  widthPx: number;
};

function canManageMessage(
  message: TimelineMessage,
  currentPubkey: string | undefined,
): boolean {
  return Boolean(
    currentPubkey &&
      message.pubkey &&
      currentPubkey.toLowerCase() === message.pubkey.toLowerCase(),
  );
}

export function MessageThreadPanel({
  canResetWidth,
  channel,
  channelId,
  channelName,
  currentPubkey,
  disabled = false,
  isSending,
  onCancelReply,
  onClose,
  onDelete,
  onExpandReplies,
  onResetWidth,
  onResizeStart,
  onScrollTargetResolved,
  onSelectReplyTarget,
  onSend,
  onToggleReaction,
  profiles,
  replyTargetId,
  replyTargetMessage,
  scrollTargetId,
  threadHead,
  threadReplies,
  threadTypingPubkeys,
  widthPx,
}: MessageThreadPanelProps) {
  const threadBodyRef = React.useRef<HTMLDivElement>(null);
  const threadHeadId = threadHead?.id ?? null;

  const composerReplyTarget =
    replyTargetMessage && threadHead && replyTargetMessage.id !== threadHead.id
      ? {
          author: replyTargetMessage.author,
          body: replyTargetMessage.body,
          id: replyTargetMessage.id,
        }
      : null;

  const threadMessages = React.useMemo(
    () => threadReplies.map((entry) => entry.message),
    [threadReplies],
  );

  const {
    bottomAnchorRef,
    contentRef,
    isAtBottom,
    newMessageCount,
    scrollToBottom,
    syncScrollState,
  } = useTimelineScrollManager({
    channelId: threadHeadId,
    isLoading: false,
    messages: threadMessages,
    onTargetReached: onScrollTargetResolved,
    scrollContainerRef: threadBodyRef,
    targetMessageId: scrollTargetId,
  });

  if (!threadHead) {
    return null;
  }

  return (
    <aside
      className="relative hidden h-full shrink-0 flex-col border-l border-border/80 bg-background lg:flex"
      data-testid="message-thread-panel"
      style={{ width: `${widthPx}px` }}
    >
      <button
        aria-label="Resize thread panel"
        className="group absolute inset-y-0 left-0 z-20 w-3 -translate-x-1/2 cursor-col-resize"
        data-testid="message-thread-resize-handle"
        onDoubleClick={canResetWidth ? onResetWidth : undefined}
        onPointerDown={onResizeStart}
        title={
          canResetWidth
            ? "Drag to resize. Double-click to reset width."
            : "Drag to resize."
        }
        type="button"
      >
        <span className="absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-transparent transition-colors group-hover:bg-border/80" />
      </button>

      <div className="flex items-center gap-3 px-4 py-3">
        <div className="min-w-0 flex-1">
          <h2 className="text-sm font-semibold tracking-tight">Thread</h2>
        </div>
        <Button
          aria-label="Close thread"
          data-testid="message-thread-close"
          onClick={onClose}
          size="icon"
          type="button"
          variant="ghost"
        >
          <X className="h-4 w-4" />
        </Button>
      </div>

      <div
        className="min-h-0 flex-1 overflow-y-auto pb-6"
        data-testid="message-thread-body"
        onScroll={syncScrollState}
        ref={threadBodyRef}
      >
        <div ref={contentRef}>
          <div className="px-3 pb-1 pt-0" data-testid="message-thread-head">
            <div className="rounded-2xl">
              <MessageRow
                activeReplyTargetId={replyTargetId}
                layoutVariant="thread-reply"
                message={threadHead}
                onDelete={
                  onDelete && canManageMessage(threadHead, currentPubkey)
                    ? onDelete
                    : undefined
                }
                onToggleReaction={onToggleReaction}
                profiles={profiles}
              />
            </div>
          </div>

          <div className="px-3 pb-3 pt-1" data-testid="message-thread-replies">
            {threadReplies.length > 0 ? (
              <div className="space-y-2">
                {threadReplies.map((entry, index) => {
                  const nextDepth =
                    threadReplies[index + 1]?.message.depth ?? -1;
                  const isExpanded = nextDepth > entry.message.depth;

                  return (
                    <div key={entry.message.id}>
                      <MessageRow
                        activeReplyTargetId={replyTargetId}
                        layoutVariant="thread-reply"
                        message={entry.message}
                        onDelete={
                          onDelete &&
                          canManageMessage(entry.message, currentPubkey)
                            ? onDelete
                            : undefined
                        }
                        onReply={onSelectReplyTarget}
                        onToggleReaction={onToggleReaction}
                        profiles={profiles}
                      />
                      {entry.summary && !isExpanded ? (
                        <MessageThreadSummaryRow
                          depth={entry.message.depth}
                          message={entry.message}
                          onOpenThread={onExpandReplies}
                          summary={entry.summary}
                        />
                      ) : null}
                    </div>
                  );
                })}
              </div>
            ) : (
              <div className="rounded-2xl border border-dashed border-border/70 bg-card/40 px-4 py-6 text-center">
                <p className="text-sm font-medium text-foreground/80">
                  No replies in this branch yet
                </p>
                <p className="mt-1 text-xs text-muted-foreground">
                  Reply in the thread to continue this branch.
                </p>
              </div>
            )}
            <div aria-hidden className="h-px" ref={bottomAnchorRef} />
          </div>
        </div>
      </div>

      {!isAtBottom ? (
        <div className="pointer-events-none absolute inset-x-0 bottom-16 flex justify-center px-4">
          <Button
            className="pointer-events-auto rounded-full shadow-lg"
            data-testid="thread-scroll-to-latest"
            onClick={() => scrollToBottom("smooth")}
            size="sm"
            type="button"
          >
            <ArrowDown className="h-4 w-4" />
            {newMessageCount > 0
              ? `${newMessageCount} new message${newMessageCount === 1 ? "" : "s"}`
              : "Jump to latest"}
          </Button>
        </div>
      ) : null}

      <div>
        <MessageComposer
          channelId={channelId}
          channelName={channelName}
          disabled={disabled || isSending || !channelId}
          isSending={isSending}
          onCancelReply={composerReplyTarget ? onCancelReply : undefined}
          onSend={onSend}
          placeholder={`Reply in thread to ${threadHead.author}`}
          replyTarget={composerReplyTarget}
          typingParentEventId={threadHead.id}
          typingRootEventId={threadHead.rootId}
        />
        <TypingIndicatorRow
          channel={channel}
          currentPubkey={currentPubkey}
          profiles={profiles}
          typingPubkeys={threadTypingPubkeys}
        />
      </div>
    </aside>
  );
}
