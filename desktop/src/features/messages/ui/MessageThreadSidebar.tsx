import { ArrowLeft, X } from "lucide-react";

import type { CollapsedThreadPreview } from "@/features/messages/lib/collapsedThreads";
import type { TimelineMessage } from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import type { Channel } from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import { MessageComposer } from "./MessageComposer";
import { MessageRow } from "./MessageRow";
import { TimelineMessageList } from "./TimelineMessageList";
import { TypingIndicatorRow } from "./TypingIndicatorRow";

type MessageThreadSidebarProps = {
  channel: Channel | null;
  canGoBack?: boolean;
  collapsedThreadSummaryByMessageId?: Map<string, CollapsedThreadPreview>;
  currentPubkey?: string;
  headMessage: TimelineMessage;
  isSending: boolean;
  messages: TimelineMessage[];
  prefillMentionTarget?: {
    displayName: string;
    id: string;
    pubkey: string;
  } | null;
  replyCount?: number;
  onBack?: () => void;
  onCancelReply: () => void;
  onClose: () => void;
  onDelete?: (message: TimelineMessage) => void;
  onOpenThread?: (message: TimelineMessage) => void;
  onReply: (message: TimelineMessage) => void;
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
  personaLookup?: Map<string, string>;
  profiles?: UserProfileLookup;
  replyTargetMessage: TimelineMessage | null;
  typingPubkeys: string[];
};

export function MessageThreadSidebar({
  channel,
  canGoBack = false,
  collapsedThreadSummaryByMessageId,
  currentPubkey,
  headMessage,
  isSending,
  messages,
  prefillMentionTarget = null,
  replyCount = messages.length,
  onBack,
  onCancelReply,
  onClose,
  onDelete,
  onOpenThread,
  onReply,
  onSend,
  onToggleReaction,
  personaLookup,
  profiles,
  replyTargetMessage,
  typingPubkeys,
}: MessageThreadSidebarProps) {
  const threadReplyTarget =
    replyTargetMessage && replyTargetMessage.id !== headMessage.id
      ? {
          author: replyTargetMessage.author,
          body: replyTargetMessage.body,
          id: replyTargetMessage.id,
        }
      : null;
  const canReplyInChannel =
    !!channel &&
    channel.isMember &&
    channel.archivedAt === null &&
    channel.channelType !== "forum";

  return (
    <aside className="flex w-[min(100%,24rem)] shrink-0 flex-col border-l border-border/60 bg-muted/20">
      <div className="flex items-center gap-2 border-b border-border/60 px-3 py-2">
        {canGoBack && onBack ? (
          <Button
            className="h-8 gap-1 px-2"
            onClick={onBack}
            size="sm"
            type="button"
            variant="ghost"
          >
            <ArrowLeft className="h-4 w-4" />
            Back
          </Button>
        ) : null}
        <div className="min-w-0 flex-1">
          <p className="text-sm font-medium text-foreground">Thread</p>
          <p className="truncate text-xs text-muted-foreground">
            {replyCount} {replyCount === 1 ? "reply" : "replies"}
          </p>
        </div>
        <Button
          aria-label="Close thread panel"
          className="h-8 w-8 shrink-0"
          onClick={onClose}
          size="icon"
          type="button"
          variant="ghost"
        >
          <X className="h-4 w-4" />
        </Button>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="border-b border-border/60 bg-background/70 px-3 py-3">
          <p className="mb-2 text-xs text-muted-foreground">Original message</p>
          <MessageRow
            message={{ ...headMessage, depth: 0 }}
            onDelete={
              currentPubkey && headMessage.pubkey === currentPubkey
                ? onDelete
                : undefined
            }
            onReply={onReply}
            onToggleReaction={onToggleReaction}
            profiles={profiles}
          />
        </div>

        <div className="px-3 py-2">
          {messages.length === 0 ? (
            <div className="rounded-2xl border border-dashed border-border/70 bg-background/50 px-4 py-6 text-center text-sm text-muted-foreground">
              No replies in this thread yet.
            </div>
          ) : (
            <TimelineMessageList
              activeReplyTargetId={threadReplyTarget?.id ?? null}
              collapsedThreadSummaryByMessageId={
                collapsedThreadSummaryByMessageId
              }
              currentPubkey={currentPubkey}
              messages={messages}
              onDelete={onDelete}
              onOpenThread={onOpenThread}
              onReply={onReply}
              onToggleReaction={onToggleReaction}
              personaLookup={personaLookup}
              profiles={profiles}
            />
          )}
        </div>
      </div>

      {typingPubkeys.length > 0 ? (
        <TypingIndicatorRow
          channel={channel}
          currentPubkey={currentPubkey}
          profiles={profiles}
          typingPubkeys={typingPubkeys}
        />
      ) : null}

      <MessageComposer
        channelId={channel?.id ?? null}
        channelName={channel?.name ?? "channel"}
        disabled={!canReplyInChannel || isSending}
        draftKey={channel ? `${channel.id}:thread:${headMessage.id}` : null}
        isSending={isSending}
        onCancelReply={threadReplyTarget ? onCancelReply : undefined}
        onSend={onSend}
        placeholder="Reply in thread..."
        prefillMentionTarget={prefillMentionTarget}
        replyTarget={threadReplyTarget}
        typingReplyParentId={replyTargetMessage?.id ?? headMessage.id}
      />
    </aside>
  );
}
