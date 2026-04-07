import * as React from "react";
import { MessageSquare } from "lucide-react";

import type {
  ThreadConversationHint,
  TimelineMessage,
} from "@/features/messages/types";
import { MessageReactions } from "@/features/messages/ui/MessageReactions";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { UserProfilePopover } from "@/features/profile/ui/UserProfilePopover";
import { KIND_STREAM_MESSAGE_DIFF } from "@/shared/constants/kinds";
import { cn } from "@/shared/lib/cn";
import { rewriteRelayUrl } from "@/shared/lib/mediaUrl";
import { normalizePubkey } from "@/shared/lib/pubkey";
import { useChannelNavigation } from "@/shared/context/ChannelNavigationContext";
import { resolveMentionNames } from "@/shared/lib/resolveMentionNames";
import { Markdown } from "@/shared/ui/markdown";
import { MessageActionBar } from "./MessageActionBar";
import { MessageTimestamp } from "./MessageTimestamp";

const DiffMessage = React.lazy(() => import("./DiffMessage"));
const DiffMessageExpanded = React.lazy(() => import("./DiffMessageExpanded"));

function threadParticipantInitials(label: string) {
  const parts = label.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) {
    return "?";
  }
  if (parts.length === 1) {
    return parts[0].slice(0, 2).toUpperCase();
  }
  return `${parts[0][0] ?? ""}${parts[parts.length - 1]?.[0] ?? ""}`.toUpperCase();
}

function ThreadParticipantAvatar({
  avatarUrl,
  label,
  pubkey,
  stackIndex,
}: {
  avatarUrl?: string | null;
  label: string;
  pubkey: string;
  stackIndex: number;
}) {
  const [failed, setFailed] = React.useState(false);

  return (
    <UserProfilePopover pubkey={pubkey}>
      <button
        className={cn(
          "relative inline-flex h-5 w-5 shrink-0 overflow-hidden rounded-full border-2 border-background bg-muted text-[8px] font-semibold text-muted-foreground shadow-sm transition-transform hover:z-20 hover:scale-110 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
        )}
        onClick={(event) => {
          event.stopPropagation();
        }}
        style={{ zIndex: 10 - stackIndex }}
        title={label}
        type="button"
      >
        {avatarUrl && !failed ? (
          <img
            alt=""
            className="h-full w-full object-cover"
            onError={() => {
              setFailed(true);
            }}
            referrerPolicy="no-referrer"
            src={rewriteRelayUrl(avatarUrl)}
          />
        ) : (
          <span className="flex h-full w-full items-center justify-center">
            {threadParticipantInitials(label)}
          </span>
        )}
      </button>
    </UserProfilePopover>
  );
}

export const MessageRow = React.memo(
  function MessageRow({
    activeReplyTargetId = null,
    activeThreadRootId = null,
    highlighted = false,
    message,
    onDelete,
    onEdit,
    onOpenThread,
    onToggleReaction,
    onReply,
    profiles,
    threadHint,
  }: {
    activeReplyTargetId?: string | null;
    activeThreadRootId?: string | null;
    highlighted?: boolean;
    message: TimelineMessage;
    onDelete?: (message: TimelineMessage) => void;
    onEdit?: (message: TimelineMessage) => void;
    onOpenThread?: (message: TimelineMessage) => void;
    onToggleReaction?: (
      message: TimelineMessage,
      emoji: string,
      remove: boolean,
    ) => Promise<void>;
    onReply?: (message: TimelineMessage) => void;
    profiles?: UserProfileLookup;
    threadHint?: ThreadConversationHint;
  }) {
    const [hasAvatarError, setHasAvatarError] = React.useState(false);
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

    const { channels } = useChannelNavigation();
    const channelNames = React.useMemo(
      () => channels.filter((c) => c.channelType !== "dm").map((c) => c.name),
      [channels],
    );

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
              className="max-w-3xl"
              content={message.body}
              mentionNames={mentionNames}
              tight
            />
          );
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

    return (
      <div
        className="relative"
        style={indentPx > 0 ? { paddingLeft: `${indentPx}px` } : undefined}
      >
        {message.depth > 0 ? (
          <div
            aria-hidden
            className="absolute bottom-1.5 left-3 top-1.5 rounded-full border-l border-border/70"
            style={{ left: `${Math.max(indentPx - 14, 12)}px` }}
          />
        ) : null}

        <article
          className={cn(
            "group/message flex items-start gap-2.5 rounded-2xl px-2 py-1 transition-colors",
            highlighted ? "bg-primary/10 ring-1 ring-primary/30" : "",
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
                    className="h-8 w-8 rounded-lg bg-secondary object-cover shadow-sm"
                    data-testid="message-avatar-image"
                    onError={() => {
                      setHasAvatarError(true);
                    }}
                    referrerPolicy="no-referrer"
                    src={rewriteRelayUrl(message.avatarUrl)}
                  />
                ) : (
                  <div
                    className={cn(
                      "flex h-8 w-8 items-center justify-center rounded-lg text-[11px] font-semibold shadow-sm",
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
              className="h-8 w-8 shrink-0 rounded-lg bg-secondary object-cover shadow-sm"
              data-testid="message-avatar-image"
              onError={() => {
                setHasAvatarError(true);
              }}
              referrerPolicy="no-referrer"
              src={rewriteRelayUrl(message.avatarUrl)}
            />
          ) : (
            <div
              className={cn(
                "flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-[11px] font-semibold shadow-sm",
                message.accent
                  ? "bg-primary text-primary-foreground"
                  : "bg-secondary text-secondary-foreground",
              )}
              data-testid="message-avatar-fallback"
            >
              {initials}
            </div>
          )}

          <div className="relative min-w-0 flex-1 space-y-0">
            {/* Name + role only in flow — action bar + time are absolute so flex-wrap
                cannot insert a full-width row between the username and the body. */}
            <div className="flex min-w-0 flex-nowrap items-start gap-x-2 pr-[9.5rem] sm:pr-40">
              <div className="flex min-w-0 flex-1 items-center gap-x-2">
                {message.pubkey ? (
                  <UserProfilePopover pubkey={message.pubkey}>
                    <button
                      className="min-w-0 truncate rounded pt-px text-left text-sm font-semibold leading-tight tracking-tight hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                      type="button"
                    >
                      {message.author}
                    </button>
                  </UserProfilePopover>
                ) : (
                  <h3 className="min-w-0 truncate pt-px text-sm font-semibold leading-tight tracking-tight">
                    {message.author}
                  </h3>
                )}
                {message.role ? (
                  <p className="shrink-0 rounded-full bg-muted px-2 py-0.5 text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
                    {message.role}
                  </p>
                ) : null}
              </div>
            </div>
            <div className="absolute right-0 top-0 z-10 flex items-start justify-end gap-2 pt-px text-xs text-muted-foreground">
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
              {message.pending ? (
                <p className="shrink-0 font-medium uppercase tracking-[0.14em] text-primary/80">
                  Sending
                </p>
              ) : null}
              {message.edited ? (
                <p
                  className="shrink-0 text-muted-foreground/70"
                  title="This message has been edited"
                >
                  (edited)
                </p>
              ) : null}
              <MessageTimestamp
                createdAt={message.createdAt}
                time={message.time}
              />
            </div>
            <div className="pt-1">{renderBody()}</div>
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
            {message.depth === 0 &&
            threadHint &&
            threadHint.replyCount > 0 &&
            onOpenThread ? (
              <div
                className={cn(
                  "mt-1.5 inline-flex max-w-full flex-wrap items-center gap-2 rounded-md py-0.5 text-left text-[11px] transition-colors",
                  "text-muted-foreground/90 hover:bg-muted/50 hover:text-foreground",
                  activeThreadRootId === message.id
                    ? "bg-muted/40 text-foreground"
                    : "",
                )}
                data-testid="message-thread-conversation-hint"
              >
                <button
                  className="inline-flex items-center gap-x-1.5 rounded-md px-0 text-left font-medium focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                  onClick={(event) => {
                    event.stopPropagation();
                    onOpenThread(message);
                  }}
                  type="button"
                >
                  <MessageSquare
                    aria-hidden
                    className="h-3 w-3 shrink-0 opacity-70"
                  />
                  <span>
                    {threadHint.replyCount}{" "}
                    {threadHint.replyCount === 1 ? "reply" : "replies"}
                  </span>
                </button>
                {threadHint.participantPubkeys.length > 0 ? (
                  <div
                    aria-hidden
                    className="flex items-center -space-x-1.5 pl-0.5"
                  >
                    {threadHint.participantPubkeys.map((pubkey, i) => {
                      const label =
                        threadHint.participantLabels[i] ?? pubkey.slice(0, 8);
                      const resolved =
                        profiles?.[normalizePubkey(pubkey)] ?? null;
                      return (
                        <ThreadParticipantAvatar
                          avatarUrl={resolved?.avatarUrl ?? null}
                          key={pubkey}
                          label={label}
                          pubkey={pubkey}
                          stackIndex={i}
                        />
                      );
                    })}
                  </div>
                ) : null}
              </div>
            ) : null}
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
      </div>
    );
    // Callbacks (onReply, onToggleReaction) intentionally excluded: inline arrows
    // from parent create new refs every render — including them defeats memo.
  },
  (prev, next) =>
    prev.message.id === next.message.id &&
    prev.message.pubkey === next.message.pubkey &&
    prev.message.body === next.message.body &&
    prev.message.author === next.message.author &&
    prev.message.avatarUrl === next.message.avatarUrl &&
    prev.message.accent === next.message.accent &&
    prev.message.time === next.message.time &&
    prev.message.depth === next.message.depth &&
    prev.message.kind === next.message.kind &&
    prev.message.pending === next.message.pending &&
    prev.message.edited === next.message.edited &&
    prev.message.reactions === next.message.reactions &&
    prev.message.tags === next.message.tags &&
    prev.message.role === next.message.role &&
    prev.highlighted === next.highlighted &&
    prev.activeReplyTargetId === next.activeReplyTargetId &&
    prev.activeThreadRootId === next.activeThreadRootId &&
    prev.profiles === next.profiles &&
    (prev.threadHint === next.threadHint ||
      (prev.threadHint?.replyCount === next.threadHint?.replyCount &&
        prev.threadHint?.participantPubkeys.join("\0") ===
          next.threadHint?.participantPubkeys.join("\0") &&
        prev.threadHint?.participantLabels.join("\0") ===
          next.threadHint?.participantLabels.join("\0"))),
);

MessageRow.displayName = "MessageRow";
