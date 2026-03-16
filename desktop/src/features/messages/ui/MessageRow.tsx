import * as React from "react";

import type {
  TimelineMessage,
  TimelineReaction,
} from "@/features/messages/types";
import { UserProfilePopover } from "@/features/profile/ui/UserProfilePopover";
import { KIND_STREAM_MESSAGE_DIFF } from "@/shared/constants/kinds";
import { cn } from "@/shared/lib/cn";
import { Markdown } from "@/shared/ui/markdown";
import { DiffMessage } from "./DiffMessage";
import { MessageActionBar } from "./MessageActionBar";

const DiffMessageExpanded = React.lazy(() => import("./DiffMessageExpanded"));

export function MessageRow({
  activeReplyTargetId = null,
  message,
  onToggleReaction,
  onReply,
}: {
  activeReplyTargetId?: string | null;
  message: TimelineMessage;
  onToggleReaction?: (
    message: TimelineMessage,
    emoji: string,
    remove: boolean,
  ) => Promise<void>;
  onReply?: (message: TimelineMessage) => void;
}) {
  const [hasAvatarError, setHasAvatarError] = React.useState(false);
  const [expandedDiffId, setExpandedDiffId] = React.useState<string | null>(
    null,
  );
  const [reactionErrorMessage, setReactionErrorMessage] = React.useState<
    string | null
  >(null);
  const [reactionPending, setReactionPending] = React.useState(false);
  const visibleDepth = Math.min(message.depth, 6);
  const indentPx = visibleDepth * 28;
  const initials = message.author
    .split(" ")
    .map((part) => part[0])
    .join("")
    .slice(0, 2)
    .toUpperCase();

  const getTag = (name: string) =>
    message.tags?.find((tag) => tag[0] === name)?.[1];

  const renderBody = () => {
    switch (message.kind) {
      case KIND_STREAM_MESSAGE_DIFF:
        return (
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
        );
      default:
        return <Markdown className="max-w-3xl" content={message.body} tight />;
    }
  };

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
        (reaction) => reaction.emoji === emoji && reaction.reactedByCurrentUser,
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

  return (
    <div
      className="relative"
      style={indentPx > 0 ? { paddingLeft: `${indentPx}px` } : undefined}
    >
      {message.depth > 0 ? (
        <div
          aria-hidden
          className="absolute bottom-2 left-3 top-2 rounded-full border-l border-border/70"
          style={{ left: `${Math.max(indentPx - 14, 12)}px` }}
        />
      ) : null}

      <article
        className={cn(
          "group/message flex gap-3 rounded-2xl px-2 py-2 transition-colors",
          message.highlighted ? "bg-primary/10 ring-1 ring-primary/30" : "",
          activeReplyTargetId === message.id
            ? "bg-muted/60 ring-1 ring-border"
            : "",
        )}
        data-message-id={message.id}
        data-testid="message-row"
      >
        {message.pubkey ? (
          <UserProfilePopover pubkey={message.pubkey}>
            <button
              className="shrink-0 rounded-xl focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              type="button"
            >
              {message.avatarUrl && !hasAvatarError ? (
                <img
                  alt={`${message.author} avatar`}
                  className="h-9 w-9 rounded-xl object-cover shadow-sm"
                  data-testid="message-avatar-image"
                  onError={() => {
                    setHasAvatarError(true);
                  }}
                  referrerPolicy="no-referrer"
                  src={message.avatarUrl}
                />
              ) : (
                <div
                  className={cn(
                    "flex h-9 w-9 items-center justify-center rounded-xl text-xs font-semibold shadow-sm",
                    message.accent
                      ? "bg-primary text-primary-foreground"
                      : "bg-secondary text-secondary-foreground",
                  )}
                  data-testid="message-avatar-fallback"
                >
                  {initials}
                </div>
              )}
            </button>
          </UserProfilePopover>
        ) : message.avatarUrl && !hasAvatarError ? (
          <img
            alt={`${message.author} avatar`}
            className="h-9 w-9 shrink-0 rounded-xl object-cover shadow-sm"
            data-testid="message-avatar-image"
            onError={() => {
              setHasAvatarError(true);
            }}
            referrerPolicy="no-referrer"
            src={message.avatarUrl}
          />
        ) : (
          <div
            className={cn(
              "flex h-9 w-9 shrink-0 items-center justify-center rounded-xl text-xs font-semibold shadow-sm",
              message.accent
                ? "bg-primary text-primary-foreground"
                : "bg-secondary text-secondary-foreground",
            )}
            data-testid="message-avatar-fallback"
          >
            {initials}
          </div>
        )}

        <div className="min-w-0 flex-1 space-y-0">
          <div className="flex min-w-0 flex-wrap items-center gap-2">
            {message.pubkey ? (
              <UserProfilePopover pubkey={message.pubkey}>
                <button
                  className="truncate rounded text-sm font-semibold tracking-tight hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                  type="button"
                >
                  {message.author}
                </button>
              </UserProfilePopover>
            ) : (
              <h3 className="truncate text-sm font-semibold tracking-tight">
                {message.author}
              </h3>
            )}
            {message.role ? (
              <p className="rounded-full bg-muted px-2 py-0.5 text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
                {message.role}
              </p>
            ) : null}
            <div className="ml-auto flex items-center gap-2 text-xs text-muted-foreground">
              <MessageActionBar
                activeReplyTargetId={activeReplyTargetId}
                message={message}
                onReactionSelect={
                  canToggleReactions ? handleReactionSelect : undefined
                }
                onReply={onReply}
                reactionErrorMessage={reactionErrorMessage}
                reactionPending={reactionPending}
                reactions={reactions}
              />
              {message.pending ? (
                <p className="font-medium uppercase tracking-[0.14em] text-primary/80">
                  Sending
                </p>
              ) : null}
              <p className="whitespace-nowrap">{message.time}</p>
            </div>
          </div>
          {renderBody()}
          {reactions.length > 0 ? (
            <div className="mt-2 flex flex-wrap items-center gap-2">
              {reactions.map((reaction: TimelineReaction) => (
                <button
                  aria-label={`Toggle ${reaction.emoji} reaction`}
                  aria-pressed={reaction.reactedByCurrentUser}
                  className={cn(
                    "inline-flex items-center gap-1 rounded-full border px-2.5 py-1 text-xs font-medium transition-colors",
                    reaction.reactedByCurrentUser
                      ? "border-primary/40 bg-primary/10 text-primary"
                      : "border-border/70 bg-muted/70 text-foreground/90",
                    canToggleReactions
                      ? "hover:bg-accent hover:text-accent-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                      : "cursor-default",
                  )}
                  disabled={!canToggleReactions || reactionPending}
                  key={`${message.id}-${reaction.emoji}`}
                  onClick={() => {
                    if (!canToggleReactions) {
                      return;
                    }

                    void handleReactionSelect(reaction.emoji).catch(() => {
                      return;
                    });
                  }}
                  type="button"
                >
                  <span>{reaction.emoji}</span>
                  <span className="text-muted-foreground">
                    {reaction.count}
                  </span>
                </button>
              ))}
            </div>
          ) : null}
          {reactionErrorMessage ? (
            <p className="mt-2 text-xs text-destructive">
              {reactionErrorMessage}
            </p>
          ) : null}
          {expandedDiffId === message.id ? (
            <React.Suspense
              fallback={
                <div className="p-4 text-sm text-muted-foreground">
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
    </div>
  );
}
