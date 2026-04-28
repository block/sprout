import * as React from "react";

import type { TimelineMessage } from "@/features/messages/types";
import { MessageReactions } from "@/features/messages/ui/MessageReactions";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { KIND_STREAM_MESSAGE_DIFF } from "@/shared/constants/kinds";
import { cn } from "@/shared/lib/cn";
import { useChannelNavigation } from "@/shared/context/ChannelNavigationContext";
import { parseImetaTags } from "@/features/messages/lib/parseImeta";
import { resolveMentionNames } from "@/shared/lib/resolveMentionNames";
import { Markdown } from "@/shared/ui/markdown";
import { MessageActionBar } from "./MessageActionBar";
import { MessageTimestamp } from "./MessageTimestamp";

const DiffMessage = React.lazy(() => import("./DiffMessage"));
const DiffMessageExpanded = React.lazy(() => import("./DiffMessageExpanded"));

/**
 * Compact message row — renders body only (no avatar / author line).
 * Used for consecutive messages from the same author within 2 minutes.
 * A subtle timestamp appears on hover in the left gutter where the avatar
 * would normally sit.
 */
export const CompactMessageRow = React.memo(
  function CompactMessageRow({
    activeReplyTargetId = null,
    highlighted = false,
    message,
    onDelete,
    onEdit,
    onToggleReaction,
    onReply,
    profiles,
    searchQuery,
  }: {
    activeReplyTargetId?: string | null;
    highlighted?: boolean;
    message: TimelineMessage;
    onDelete?: (message: TimelineMessage) => void;
    onEdit?: (message: TimelineMessage) => void;
    onToggleReaction?: (
      message: TimelineMessage,
      emoji: string,
      remove: boolean,
    ) => Promise<void>;
    onReply?: (message: TimelineMessage) => void;
    profiles?: UserProfileLookup;
    searchQuery?: string;
  }) {
    const [expandedDiffId, setExpandedDiffId] = React.useState<string | null>(
      null,
    );
    const [reactionErrorMessage, setReactionErrorMessage] = React.useState<
      string | null
    >(null);
    const [reactionPending, setReactionPending] = React.useState(false);

    const mentionNames = React.useMemo(
      () => resolveMentionNames(message.tags, profiles),
      [profiles, message.tags],
    );

    const imetaByUrl = React.useMemo(
      () => (message.tags ? parseImetaTags(message.tags) : undefined),
      [message.tags],
    );

    const { channels } = useChannelNavigation();
    const channelNames = React.useMemo(
      () => channels.filter((c) => c.channelType !== "dm").map((c) => c.name),
      [channels],
    );

    const getTag = (name: string) =>
      message.tags?.find((tag) => tag[0] === name)?.[1];

    const reactions = [...(message.reactions ?? [])].sort((left, right) => {
      if (left.count !== right.count) {
        return right.count - left.count;
      }
      return left.emoji.localeCompare(right.emoji);
    });
    const canToggleReactions = Boolean(onToggleReaction && !message.pending);

    const handleReactionSelect = React.useCallback(
      async (emoji: string) => {
        if (!onToggleReaction || reactionPending) {
          return;
        }

        const remove = reactions.some(
          (reaction) =>
            reaction.emoji === emoji && reaction.reactedByCurrentUser,
        );

        setReactionErrorMessage(null);
        setReactionPending(true);

        try {
          await onToggleReaction(message, emoji, remove);
        } catch (error) {
          const nextMessage =
            error instanceof Error
              ? error.message
              : "Failed to update the reaction.";
          setReactionErrorMessage(nextMessage);
          throw error;
        } finally {
          setReactionPending(false);
        }
      },
      [message, onToggleReaction, reactionPending, reactions],
    );

    const renderBody = () => {
      switch (message.kind) {
        case KIND_STREAM_MESSAGE_DIFF:
          return (
            <React.Suspense
              fallback={
                <div className="p-3 text-sm text-muted-foreground">
                  Loading diff…
                </div>
              }
            >
              <DiffMessage
                commitSha={getTag("commit")}
                content={message.body}
                description={getTag("description")}
                filePath={getTag("file")}
                onExpand={() => {
                  setExpandedDiffId(message.id);
                }}
                repoUrl={getTag("repo")}
                truncated={getTag("truncated") === "true"}
              />
            </React.Suspense>
          );
        default:
          return (
            <Markdown
              channelNames={channelNames}
              className="max-w-full"
              content={message.body}
              imetaByUrl={imetaByUrl}
              mentionNames={mentionNames}
              searchQuery={searchQuery}
              tight
            />
          );
      }
    };

    return (
      <article
        className={cn(
          "group/message flex items-start gap-2.5 rounded-2xl px-2 py-0.5 transition-colors",
          highlighted ? "bg-primary/10 ring-1 ring-primary/30" : "",
        )}
        data-message-id={message.id}
        data-testid="compact-message-row"
      >
        {/* Hover timestamp in the avatar gutter */}
        <div className="flex w-[42px] shrink-0 items-start justify-center pt-1">
          <span className="text-[10px] leading-none text-muted-foreground/0 transition-colors group-hover/message:text-muted-foreground/60">
            <MessageTimestamp
              createdAt={message.createdAt}
              time={message.time}
            />
          </span>
        </div>

        <div className="min-w-0 flex-1 space-y-1">
          <div className="flex items-start">
            <div className="min-w-0 flex-1">{renderBody()}</div>
            <div className="relative ml-2 shrink-0">
              <div className="absolute right-0 top-1/2 -translate-y-1/2">
                <MessageActionBar
                  activeReplyTargetId={activeReplyTargetId}
                  message={message}
                  onDelete={onDelete}
                  onEdit={onEdit}
                  onReactionSelect={
                    canToggleReactions ? handleReactionSelect : undefined
                  }
                  onReply={onReply}
                  reactionErrorMessage={reactionErrorMessage}
                  reactionPending={reactionPending}
                  reactions={reactions}
                />
              </div>
            </div>
          </div>
          {message.pending ? (
            <p className="text-xs font-medium uppercase tracking-[0.14em] text-primary/80">
              Sending
            </p>
          ) : null}
          {message.edited ? (
            <p
              className="text-xs text-muted-foreground/70"
              title="This message has been edited"
            >
              (edited)
            </p>
          ) : null}
          <MessageReactions
            messageId={message.id}
            reactions={reactions}
            canToggle={canToggleReactions}
            pending={reactionPending}
            onSelect={(emoji) => {
              void handleReactionSelect(emoji).catch(() => {
                return;
              });
            }}
          />
          {reactionErrorMessage ? (
            <p className="mt-1.5 text-xs text-destructive">
              {reactionErrorMessage}
            </p>
          ) : null}
          {expandedDiffId === message.id ? (
            <React.Suspense
              fallback={
                <div className="p-3 text-sm text-muted-foreground">
                  Loading diff viewer…
                </div>
              }
            >
              <DiffMessageExpanded
                content={message.body}
                filePath={getTag("file")}
                onClose={() => {
                  setExpandedDiffId(null);
                }}
              />
            </React.Suspense>
          ) : null}
        </div>
      </article>
    );
  },
  (prev, next) =>
    prev.message.id === next.message.id &&
    prev.message.pubkey === next.message.pubkey &&
    prev.message.body === next.message.body &&
    prev.message.author === next.message.author &&
    prev.message.avatarUrl === next.message.avatarUrl &&
    prev.message.accent === next.message.accent &&
    prev.message.time === next.message.time &&
    prev.message.kind === next.message.kind &&
    prev.message.pending === next.message.pending &&
    prev.message.edited === next.message.edited &&
    prev.message.reactions === next.message.reactions &&
    prev.message.tags === next.message.tags &&
    prev.message.role === next.message.role &&
    prev.highlighted === next.highlighted &&
    prev.activeReplyTargetId === next.activeReplyTargetId &&
    prev.profiles === next.profiles &&
    prev.searchQuery === next.searchQuery,
);

CompactMessageRow.displayName = "CompactMessageRow";
