import * as React from "react";
import { X } from "lucide-react";

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
  openKey: number;
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
  openKey,
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

  React.useEffect(() => {
    if (!threadHeadId) {
      return;
    }

    const threadBody = threadBodyRef.current;
    if (!threadBody) {
      return;
    }

    const scrollToBottom = () => {
      threadBody.scrollTop = threadBody.scrollHeight;
    };
    const frame = requestAnimationFrame(() => {
      scrollToBottom();
    });
    const timeoutId = window.setTimeout(scrollToBottom, 300);

    return () => {
      cancelAnimationFrame(frame);
      window.clearTimeout(timeoutId);
    };
  }, [openKey, threadHeadId]);

  React.useEffect(() => {
    if (!scrollTargetId) {
      return;
    }

    const threadBody = threadBodyRef.current;
    if (!threadBody) {
      return;
    }

    const target = threadBody.querySelector<HTMLElement>(
      `[data-message-id="${scrollTargetId}"]`,
    );
    if (!target) {
      return;
    }

    const frame = requestAnimationFrame(() => {
      target.scrollIntoView({
        behavior: "smooth",
        block: "start",
      });
    });
    onScrollTargetResolved();

    return () => {
      cancelAnimationFrame(frame);
    };
  }, [onScrollTargetResolved, scrollTargetId, threadReplies]);

  if (!threadHead) {
    return null;
  }

  return (
    <aside
      className="hidden h-full w-[380px] shrink-0 flex-col border-l border-border/80 bg-background lg:flex"
      data-testid="message-thread-panel"
    >
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
        className="min-h-0 flex-1 overflow-y-auto"
        data-testid="message-thread-body"
        ref={threadBodyRef}
      >
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
                const nextDepth = threadReplies[index + 1]?.message.depth ?? -1;
                const isExpanded = nextDepth > entry.message.depth;

                return (
                  <div key={entry.message.id}>
                    <MessageRow
                      activeReplyTargetId={replyTargetId}
                      layoutVariant="thread-reply"
                      message={entry.message}
                      onDelete={
                        onDelete && canManageMessage(entry.message, currentPubkey)
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
        </div>
      </div>

      <div className="p-4">
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
