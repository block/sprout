import * as React from "react";
import { ArrowLeft, X } from "lucide-react";

import type { MainTimelineEntry } from "@/features/messages/lib/threadPanel";
import type { TimelineMessage } from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import type { Channel } from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import { MessageComposer } from "./MessageComposer";
import { MessageRow } from "./MessageRow";
import { MessageThreadSummaryRow } from "./MessageThreadSummaryRow";
import { TypingIndicatorRow } from "./TypingIndicatorRow";

type MessageThreadPanelProps = {
  canGoBack: boolean;
  channel: Channel | null;
  channelId: string | null;
  channelName: string;
  currentPubkey?: string;
  disabled?: boolean;
  isSending: boolean;
  onBack: () => void;
  onCancelReply: () => void;
  onClose: () => void;
  onDelete?: (message: TimelineMessage) => void;
  onOpenNestedThread: (message: TimelineMessage) => void;
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
  threadHead: TimelineMessage | null;
  threadReplies: MainTimelineEntry[];
  threadTypingPubkeys: string[];
  totalReplyCount: number;
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
  canGoBack,
  channel,
  channelId,
  channelName,
  currentPubkey,
  disabled = false,
  isSending,
  onBack,
  onCancelReply,
  onClose,
  onDelete,
  onOpenNestedThread,
  onSend,
  onToggleReaction,
  profiles,
  replyTargetId,
  replyTargetMessage,
  threadHead,
  threadReplies,
  threadTypingPubkeys,
}: MessageThreadPanelProps) {
  const threadBodyRef = React.useRef<HTMLDivElement>(null);
  const threadContentRef = React.useRef<HTMLDivElement>(null);

  const composerReplyTarget =
    threadHead && replyTargetMessage && replyTargetMessage.id !== threadHead.id
      ? {
          author: replyTargetMessage.author,
          body: replyTargetMessage.body,
          id: replyTargetMessage.id,
        }
      : null;

  const scrollThreadToBottom = React.useCallback(() => {
    const threadBody = threadBodyRef.current;
    if (!threadBody) {
      return;
    }

    threadBody.scrollTo({
      top: threadBody.scrollHeight,
      behavior: "auto",
    });
  }, []);

  React.useLayoutEffect(() => {
    if (!threadHead) {
      return;
    }
    scrollThreadToBottom();
  }, [scrollThreadToBottom, threadHead?.id]);

  React.useEffect(() => {
    if (!threadHead) {
      return;
    }

    const threadContent = threadContentRef.current;
    if (!threadContent || typeof ResizeObserver === "undefined") {
      return;
    }

    const observer = new ResizeObserver(() => {
      scrollThreadToBottom();
    });

    observer.observe(threadContent);
    return () => {
      observer.disconnect();
    };
  }, [scrollThreadToBottom, threadHead?.id]);

  if (!threadHead) {
    return null;
  }

  return (
    <aside
      className="relative z-10 hidden h-full min-h-0 w-[min(100%,420px)] shrink-0 flex-col border-l border-border/60 bg-muted/20 pt-14 lg:flex"
      data-testid="message-thread-panel"
    >
      <div className="relative z-20 flex shrink-0 items-center justify-between bg-background/25 px-2 py-1 shadow-[0_4px_24px_rgba(0,0,0,0.06)] backdrop-blur-xl supports-[backdrop-filter]:bg-background/20 dark:shadow-[0_4px_24px_rgba(0,0,0,0.25)]">
        <div className="flex min-w-0 items-center gap-2">
          {canGoBack ? (
            <Button
              aria-label="Back"
              data-testid="message-thread-back"
              onClick={onBack}
              size="icon"
              type="button"
              variant="ghost"
            >
              <ArrowLeft className="h-4 w-4" />
            </Button>
          ) : null}
          <div className="min-w-0">
            <h2 className="text-sm font-semibold tracking-tight">Thread</h2>
          </div>
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
        className="min-h-0 flex-1 overflow-y-auto overscroll-contain pb-12"
        data-testid="message-thread-body"
        ref={threadBodyRef}
      >
        <div ref={threadContentRef}>
          <div className="px-4 py-3" data-testid="message-thread-head">
            <div className="mx-auto w-full max-w-[23rem]">
              <MessageRow
                activeReplyTargetId={replyTargetId}
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

          <div className="px-4 pb-2 pt-3" data-testid="message-thread-replies">
            {threadReplies.length > 0 ? (
              <div className="mx-auto w-full max-w-[23rem] space-y-2">
                {threadReplies.map((entry) => (
                  <div
                    key={entry.message.id}
                    className="flex flex-col gap-0"
                  >
                    <MessageRow
                      activeReplyTargetId={replyTargetId}
                      message={entry.message}
                      onDelete={
                        onDelete && canManageMessage(entry.message, currentPubkey)
                          ? onDelete
                          : undefined
                      }
                      onReply={onOpenNestedThread}
                      onToggleReaction={onToggleReaction}
                      profiles={profiles}
                    />
                    {entry.summary ? (
                      <MessageThreadSummaryRow
                        message={entry.message}
                        onOpenThread={onOpenNestedThread}
                        summary={entry.summary}
                      />
                    ) : null}
                  </div>
                ))}
              </div>
            ) : null}
          </div>
        </div>
      </div>

      <div className="relative z-10 -mt-10 shrink-0">
        <TypingIndicatorRow
          channel={channel}
          currentPubkey={currentPubkey}
          profiles={profiles}
          typingPubkeys={threadTypingPubkeys}
        />
        <MessageComposer
          channelId={channelId}
          channelName={channelName}
          disabled={disabled || isSending || !channelId}
          isSending={isSending}
          onCancelReply={composerReplyTarget ? onCancelReply : undefined}
          onSend={onSend}
          placeholder={`Reply in thread to ${threadHead.author}`}
          replyTarget={composerReplyTarget}
          showTopBorder={false}
          typingParentEventId={threadHead.id}
          typingRootEventId={threadHead.rootId}
        />
      </div>
    </aside>
  );
}
