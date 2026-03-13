import { ArrowDown, CornerUpLeft, LoaderCircle, SmilePlus } from "lucide-react";
import * as React from "react";

import type {
  TimelineMessage,
  TimelineReaction,
} from "@/features/messages/types";
import { UserProfilePopover } from "@/features/profile/ui/UserProfilePopover";
import { KIND_STREAM_MESSAGE_DIFF } from "@/shared/constants/kinds";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { Markdown } from "@/shared/ui/markdown";
import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";
import { Separator } from "@/shared/ui/separator";
import { Skeleton } from "@/shared/ui/skeleton";
import { DiffMessage } from "./DiffMessage";

const DiffMessageExpanded = React.lazy(() => import("./DiffMessageExpanded"));

type MessageTimelineProps = {
  messages: TimelineMessage[];
  isLoading?: boolean;
  emptyTitle?: string;
  emptyDescription?: string;
  activeReplyTargetId?: string | null;
  onReply?: (message: TimelineMessage) => void;
  onToggleReaction?: (
    message: TimelineMessage,
    emoji: string,
    remove: boolean,
  ) => Promise<void>;
  targetMessageId?: string | null;
  onTargetReached?: (messageId: string) => void;
};

const BOTTOM_THRESHOLD_PX = 72;
const DEFAULT_REACTION_OPTIONS = [
  "👍",
  "❤️",
  "🎉",
  "🚀",
  "👀",
  "✅",
  "🔥",
  "👎",
];

function isNearBottom(container: HTMLDivElement) {
  return (
    container.scrollHeight - container.clientHeight - container.scrollTop <=
    BOTTOM_THRESHOLD_PX
  );
}

function getReactionOptions(reactions: TimelineReaction[]) {
  const seen = new Set<string>();
  const options: string[] = [];

  for (const reaction of reactions) {
    if (seen.has(reaction.emoji)) {
      continue;
    }

    seen.add(reaction.emoji);
    options.push(reaction.emoji);
  }

  for (const emoji of DEFAULT_REACTION_OPTIONS) {
    if (seen.has(emoji)) {
      continue;
    }

    seen.add(emoji);
    options.push(emoji);
  }

  return options;
}

function MessageActionBar({
  activeReplyTargetId = null,
  message,
  onReactionSelect,
  onReply,
  reactionErrorMessage = null,
  reactions,
  reactionPending = false,
}: {
  activeReplyTargetId?: string | null;
  message: TimelineMessage;
  onReactionSelect?: (emoji: string) => Promise<void>;
  onReply?: (message: TimelineMessage) => void;
  reactionErrorMessage?: string | null;
  reactions: TimelineReaction[];
  reactionPending?: boolean;
}) {
  const [isReactionPickerOpen, setIsReactionPickerOpen] = React.useState(false);
  const hasReplyAction = Boolean(onReply);
  const hasReactionAction = Boolean(onReactionSelect);

  if (!hasReplyAction && !hasReactionAction) {
    return null;
  }

  const isReplyingToMessage = activeReplyTargetId === message.id;
  const selectedReactionCount = reactions.filter(
    (reaction) => reaction.reactedByCurrentUser,
  ).length;
  const reactionOptions = getReactionOptions(reactions);

  return (
    <div
      className={cn(
        "max-w-20 overflow-hidden rounded-full border border-border/70 bg-background/95 shadow-sm backdrop-blur supports-[backdrop-filter]:bg-background/85 transition-all duration-150 ease-out",
        "opacity-100 translate-y-0 sm:max-w-0 sm:opacity-0 sm:translate-y-1",
        "sm:group-hover/message:max-w-20 sm:group-hover/message:opacity-100 sm:group-hover/message:translate-y-0",
        "sm:group-focus-within/message:max-w-20 sm:group-focus-within/message:opacity-100 sm:group-focus-within/message:translate-y-0",
        isReplyingToMessage || isReactionPickerOpen
          ? "sm:max-w-20 sm:opacity-100 sm:translate-y-0"
          : "",
      )}
      data-testid={`message-action-bar-${message.id}`}
    >
      <div className="flex items-center gap-1 p-1">
        {hasReactionAction ? (
          <Popover
            onOpenChange={setIsReactionPickerOpen}
            open={isReactionPickerOpen}
          >
            <PopoverTrigger asChild>
              <Button
                aria-label="Open reactions"
                className="h-6 w-6 rounded-full p-0"
                data-testid={`react-message-${message.id}`}
                disabled={reactionPending}
                size="sm"
                title="React"
                type="button"
                variant={
                  isReactionPickerOpen || selectedReactionCount > 0
                    ? "secondary"
                    : "ghost"
                }
              >
                {reactionPending ? (
                  <LoaderCircle className="h-3 w-3 animate-spin" />
                ) : (
                  <SmilePlus className="h-3 w-3" />
                )}
              </Button>
            </PopoverTrigger>
            <PopoverContent
              align="end"
              className="w-56 rounded-2xl p-3"
              side="top"
              sideOffset={10}
            >
              <div className="space-y-3">
                <div className="space-y-1">
                  <p className="text-[11px] font-semibold uppercase tracking-[0.16em] text-muted-foreground">
                    React
                  </p>
                  <p
                    className={cn(
                      "text-xs",
                      reactionErrorMessage
                        ? "text-destructive"
                        : "text-muted-foreground",
                    )}
                  >
                    {reactionErrorMessage ??
                      "Click any emoji. Click it again to remove your own reaction."}
                  </p>
                </div>
                <div className="grid grid-cols-4 gap-1">
                  {reactionOptions.map((emoji) => {
                    const isActive = reactions.some(
                      (reaction) =>
                        reaction.emoji === emoji &&
                        reaction.reactedByCurrentUser,
                    );

                    return (
                      <button
                        aria-label={`React with ${emoji}`}
                        aria-pressed={isActive}
                        className={cn(
                          "flex h-10 items-center justify-center rounded-xl border text-lg transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                          isActive
                            ? "border-primary/40 bg-primary/10"
                            : "border-border/70 bg-muted/40 hover:bg-accent",
                        )}
                        data-emoji={emoji}
                        data-testid={`react-option-${message.id}`}
                        disabled={reactionPending}
                        key={`${message.id}-${emoji}`}
                        onClick={() => {
                          if (!onReactionSelect) {
                            return;
                          }

                          void onReactionSelect(emoji)
                            .then(() => {
                              setIsReactionPickerOpen(false);
                            })
                            .catch(() => {
                              return;
                            });
                        }}
                        type="button"
                      >
                        {emoji}
                      </button>
                    );
                  })}
                </div>
              </div>
            </PopoverContent>
          </Popover>
        ) : null}

        {hasReplyAction ? (
          <Button
            aria-label={isReplyingToMessage ? "Cancel reply" : "Reply"}
            className="h-6 w-6 rounded-full p-0"
            data-testid={`reply-message-${message.id}`}
            onClick={() => {
              onReply?.(message);
            }}
            size="sm"
            title={isReplyingToMessage ? "Cancel reply" : "Reply"}
            type="button"
            variant={isReplyingToMessage ? "secondary" : "ghost"}
          >
            <CornerUpLeft className="h-3 w-3" />
          </Button>
        ) : null}
      </div>
    </div>
  );
}

function MessageRow({
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
  const compressedDepth = Math.max(message.depth - visibleDepth, 0);
  const indentPx = visibleDepth * 28;
  const initials = message.author
    .split(" ")
    .map((part) => part[0])
    .join("")
    .slice(0, 2)
    .toUpperCase();

  const getTag = (name: string) =>
    message.tags?.find((t) => t[0] === name)?.[1];

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
          {message.parentId ? (
            <div className="mb-1 flex min-w-0 flex-wrap items-center gap-2 text-[11px] font-medium uppercase tracking-[0.16em] text-muted-foreground">
              <span className="rounded-full bg-muted px-2 py-0.5 text-[10px]">
                Reply
              </span>
              <span className="truncate normal-case tracking-normal">
                {message.replyToAuthor
                  ? `to ${message.replyToAuthor}`
                  : "to an earlier message"}
              </span>
              {compressedDepth > 0 ? (
                <span className="rounded-full bg-muted px-2 py-0.5 text-[10px]">
                  +{compressedDepth} more level
                  {compressedDepth === 1 ? "" : "s"}
                </span>
              ) : null}
            </div>
          ) : null}

          <div className="flex min-w-0 flex-wrap items-center gap-2">
            {message.pubkey ? (
              <UserProfilePopover pubkey={message.pubkey}>
                <button
                  className="truncate text-sm font-semibold tracking-tight hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring rounded"
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
              {reactions.map((reaction) => (
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
          <div className="mt-2 flex items-center gap-2">
            {message.replyToSnippet ? (
              <p className="truncate text-xs text-muted-foreground">
                {message.replyToSnippet}
              </p>
            ) : null}
          </div>
          {expandedDiffId === message.id && (
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
          )}
        </div>
      </article>
    </div>
  );
}

function TimelineSkeleton() {
  const skeletonRows = ["first", "second", "third", "fourth"];

  return (
    <>
      {skeletonRows.map((row) => (
        <div className="flex gap-3" key={row}>
          <Skeleton className="h-9 w-9 rounded-xl" />
          <div className="min-w-0 flex-1 space-y-1.5">
            <Skeleton className="h-3.5 w-44" />
            <Skeleton className="h-4 w-full max-w-2xl" />
            <Skeleton className="h-4 w-full max-w-xl" />
          </div>
        </div>
      ))}
    </>
  );
}

export function MessageTimeline({
  messages,
  isLoading = false,
  emptyTitle = "No messages yet",
  emptyDescription = "Send the first message to start the thread.",
  activeReplyTargetId = null,
  onReply,
  onToggleReaction,
  targetMessageId = null,
  onTargetReached,
}: MessageTimelineProps) {
  const timelineRef = React.useRef<HTMLDivElement>(null);
  const contentRef = React.useRef<HTMLDivElement>(null);
  const bottomAnchorRef = React.useRef<HTMLDivElement>(null);
  const hasInitializedRef = React.useRef(false);
  const shouldStickToBottomRef = React.useRef(true);
  const isAtBottomRef = React.useRef(true);
  const isProgrammaticBottomScrollRef = React.useRef(false);
  const previousTimelineHeightRef = React.useRef<number | null>(null);
  const previousScrollTopRef = React.useRef(0);
  const lockedScrollTopRef = React.useRef<number | null>(null);
  const previousLastMessageIdRef = React.useRef<string | undefined>(undefined);
  const previousMessageCountRef = React.useRef(0);
  const handledTargetMessageIdRef = React.useRef<string | null>(null);
  const [isAtBottom, setIsAtBottom] = React.useState(true);
  const [highlightedMessageId, setHighlightedMessageId] = React.useState<
    string | null
  >(null);
  const [newMessageCount, setNewMessageCount] = React.useState(0);
  const latestMessage =
    messages.length > 0 ? messages[messages.length - 1] : undefined;

  const syncScrollState = React.useCallback(() => {
    const timeline = timelineRef.current;
    if (!timeline) {
      return;
    }

    const scrollTop = lockedScrollTopRef.current ?? timeline.scrollTop;
    const atBottom = isNearBottom(timeline);
    const movedAwayFromBottom = scrollTop + 1 < previousScrollTopRef.current;

    if (isProgrammaticBottomScrollRef.current) {
      previousScrollTopRef.current = scrollTop;

      if (movedAwayFromBottom) {
        isProgrammaticBottomScrollRef.current = false;
      } else if (!atBottom) {
        shouldStickToBottomRef.current = true;
        isAtBottomRef.current = true;
        setIsAtBottom((current) => (current ? current : true));
        return;
      } else {
        isProgrammaticBottomScrollRef.current = false;
        shouldStickToBottomRef.current = true;
        isAtBottomRef.current = true;
        setIsAtBottom((current) => (current ? current : true));
        setNewMessageCount(0);
        return;
      }
    }

    if (shouldStickToBottomRef.current && !atBottom && !movedAwayFromBottom) {
      previousScrollTopRef.current = scrollTop;
      shouldStickToBottomRef.current = true;
      isAtBottomRef.current = true;
      setIsAtBottom((current) => (current ? current : true));
      setNewMessageCount(0);
      return;
    }

    previousScrollTopRef.current = scrollTop;
    shouldStickToBottomRef.current = atBottom;
    isAtBottomRef.current = atBottom;
    setIsAtBottom((current) => (current === atBottom ? current : atBottom));

    if (atBottom) {
      setNewMessageCount(0);
    }
  }, []);

  const restoreScrollPosition = React.useCallback(
    (scrollTop: number) => {
      const timeline = timelineRef.current;

      if (!timeline) {
        return;
      }

      isProgrammaticBottomScrollRef.current = false;
      lockedScrollTopRef.current = scrollTop;

      const restore = (remainingFrames: number) => {
        timeline.scrollTop = scrollTop;

        if (remainingFrames > 0) {
          requestAnimationFrame(() => {
            restore(remainingFrames - 1);
          });
          return;
        }

        lockedScrollTopRef.current = null;
        previousScrollTopRef.current = timeline.scrollTop;
        syncScrollState();
      };

      restore(2);
    },
    [syncScrollState],
  );

  const scrollToBottom = React.useCallback(
    (behavior: ScrollBehavior) => {
      const timeline = timelineRef.current;

      if (!timeline) {
        return;
      }

      isProgrammaticBottomScrollRef.current = true;

      const alignToBottom = (nextBehavior: ScrollBehavior) => {
        bottomAnchorRef.current?.scrollIntoView({
          block: "end",
          behavior: nextBehavior,
        });
        timeline.scrollTo({
          top: timeline.scrollHeight,
          behavior: nextBehavior,
        });
      };

      alignToBottom(behavior);
      lockedScrollTopRef.current = null;
      previousScrollTopRef.current = timeline.scrollTop;
      shouldStickToBottomRef.current = true;
      isAtBottomRef.current = true;
      setIsAtBottom(true);
      setNewMessageCount(0);

      if (behavior === "smooth") {
        requestAnimationFrame(() => {
          previousScrollTopRef.current = timeline.scrollTop;
          syncScrollState();
        });
        return;
      }

      const settleAlignment = (remainingFrames: number) => {
        requestAnimationFrame(() => {
          alignToBottom("auto");
          previousScrollTopRef.current = timeline.scrollTop;

          if (remainingFrames > 0) {
            settleAlignment(remainingFrames - 1);
            return;
          }

          syncScrollState();
        });
      };

      settleAlignment(2);
    },
    [syncScrollState],
  );

  React.useEffect(() => {
    const timeline = timelineRef.current;

    if (!timeline || typeof ResizeObserver === "undefined") {
      return;
    }

    previousTimelineHeightRef.current = timeline.clientHeight;
    previousScrollTopRef.current = timeline.scrollTop;

    const observer = new ResizeObserver(([entry]) => {
      const previousTimelineHeight = previousTimelineHeightRef.current;
      const nextTimelineHeight = entry.contentRect.height;
      previousTimelineHeightRef.current = nextTimelineHeight;

      if (
        previousTimelineHeight === null ||
        Math.abs(nextTimelineHeight - previousTimelineHeight) < 1
      ) {
        return;
      }

      if (shouldStickToBottomRef.current || isAtBottomRef.current) {
        scrollToBottom("auto");
        return;
      }

      restoreScrollPosition(previousScrollTopRef.current);
    });

    observer.observe(timeline);

    return () => {
      observer.disconnect();
    };
  }, [restoreScrollPosition, scrollToBottom]);

  React.useEffect(() => {
    const content = contentRef.current;

    if (!content || typeof ResizeObserver === "undefined") {
      return;
    }

    const observer = new ResizeObserver(() => {
      if (shouldStickToBottomRef.current) {
        scrollToBottom("auto");
        return;
      }

      syncScrollState();
    });

    observer.observe(content);

    return () => {
      observer.disconnect();
    };
  }, [scrollToBottom, syncScrollState]);

  React.useLayoutEffect(() => {
    if (!hasInitializedRef.current) {
      if (isLoading) {
        return;
      }

      scrollToBottom("auto");
      hasInitializedRef.current = true;
      previousLastMessageIdRef.current = latestMessage?.id;
      previousMessageCountRef.current = messages.length;
      return;
    }

    const previousLastMessageId = previousLastMessageIdRef.current;
    const previousMessageCount = previousMessageCountRef.current;
    const hasNewLatestMessage =
      latestMessage !== undefined && latestMessage.id !== previousLastMessageId;

    if (!hasNewLatestMessage) {
      previousLastMessageIdRef.current = latestMessage?.id;
      previousMessageCountRef.current = messages.length;
      return;
    }

    if (
      shouldStickToBottomRef.current ||
      isAtBottomRef.current ||
      latestMessage.accent
    ) {
      scrollToBottom(latestMessage.accent ? "smooth" : "auto");
    } else {
      setNewMessageCount((current) => {
        const addedMessages = Math.max(
          1,
          messages.length - previousMessageCount,
        );
        return current + addedMessages;
      });
    }

    previousLastMessageIdRef.current = latestMessage.id;
    previousMessageCountRef.current = messages.length;
  }, [isLoading, latestMessage, messages.length, scrollToBottom]);

  React.useEffect(() => {
    if (!targetMessageId) {
      handledTargetMessageIdRef.current = null;
      setHighlightedMessageId(null);
      return;
    }

    if (handledTargetMessageIdRef.current === targetMessageId || isLoading) {
      return;
    }

    const timeline = timelineRef.current;
    if (!timeline) {
      return;
    }

    const targetElement = timeline.querySelector<HTMLElement>(
      `[data-message-id="${targetMessageId}"]`,
    );
    if (!targetElement) {
      return;
    }

    handledTargetMessageIdRef.current = targetMessageId;
    shouldStickToBottomRef.current = false;
    isAtBottomRef.current = false;
    isProgrammaticBottomScrollRef.current = false;
    targetElement.scrollIntoView({
      block: "center",
      behavior: "smooth",
    });
    previousScrollTopRef.current = timeline.scrollTop;
    setIsAtBottom(false);
    setHighlightedMessageId(targetMessageId);
    setNewMessageCount(0);
    onTargetReached?.(targetMessageId);

    const timeout = window.setTimeout(() => {
      setHighlightedMessageId((current) =>
        current === targetMessageId ? null : current,
      );
    }, 2_000);

    return () => {
      window.clearTimeout(timeout);
    };
  }, [isLoading, onTargetReached, targetMessageId]);

  return (
    <div className="relative flex-1 min-h-0">
      <div
        className="h-full overflow-y-auto overflow-x-hidden overscroll-contain px-4 py-4 [overflow-anchor:none] sm:px-6"
        data-testid="message-timeline"
        onScroll={syncScrollState}
        ref={timelineRef}
      >
        <div
          className="mx-auto flex w-full max-w-4xl flex-col gap-4"
          ref={contentRef}
        >
          <div
            className="flex items-center gap-4"
            data-testid="message-timeline-day-divider"
          >
            <Separator className="flex-1" />
            <p className="text-xs font-semibold uppercase tracking-[0.22em] text-muted-foreground">
              Today
            </p>
            <Separator className="flex-1" />
          </div>

          {isLoading ? <TimelineSkeleton /> : null}

          {!isLoading && messages.length === 0 ? (
            <div
              className="rounded-3xl border border-dashed border-border/80 bg-card/70 px-6 py-10 text-center shadow-sm"
              data-testid="message-empty"
            >
              <p className="text-base font-semibold tracking-tight">
                {emptyTitle}
              </p>
              <p className="mt-2 text-sm text-muted-foreground">
                {emptyDescription}
              </p>
            </div>
          ) : null}

          {!isLoading
            ? messages.map((message) => (
                <MessageRow
                  activeReplyTargetId={activeReplyTargetId}
                  key={message.id}
                  message={{
                    ...message,
                    highlighted: message.id === highlightedMessageId,
                  }}
                  onToggleReaction={onToggleReaction}
                  onReply={onReply}
                />
              ))
            : null}
          <div aria-hidden className="h-px" ref={bottomAnchorRef} />
        </div>
      </div>

      {!isAtBottom ? (
        <div className="pointer-events-none absolute inset-x-0 bottom-4 flex justify-center px-4">
          <Button
            className="pointer-events-auto rounded-full shadow-lg"
            data-testid="message-scroll-to-latest"
            onClick={() => {
              scrollToBottom("smooth");
            }}
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
    </div>
  );
}
