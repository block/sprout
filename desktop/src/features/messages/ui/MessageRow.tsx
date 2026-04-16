import * as React from "react";

import type { TimelineMessage } from "@/features/messages/types";
import { MessageReactions } from "@/features/messages/ui/MessageReactions";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { UserProfilePopover } from "@/features/profile/ui/UserProfilePopover";
import { KIND_STREAM_MESSAGE_DIFF } from "@/shared/constants/kinds";
import { cn } from "@/shared/lib/cn";
import { UserAvatar } from "@/shared/ui/UserAvatar";
import { useChannelNavigation } from "@/shared/context/ChannelNavigationContext";
import { parseImetaTags } from "@/features/messages/lib/parseImeta";
import { resolveMentionNames } from "@/shared/lib/resolveMentionNames";
import { Markdown } from "@/shared/ui/markdown";
import { MessageActionBar } from "./MessageActionBar";
import { MessageTimestamp } from "./MessageTimestamp";

const DiffMessage = React.lazy(() => import("./DiffMessage"));
const DiffMessageExpanded = React.lazy(() => import("./DiffMessageExpanded"));

export const MessageRow = React.memo(
  function MessageRow({
    activeReplyTargetId = null,
    highlighted = false,
    layoutVariant = "default",
    message,
    onDelete,
    onEdit,
    onToggleReaction,
    onReply,
    profiles,
  }: {
    activeReplyTargetId?: string | null;
    highlighted?: boolean;
    layoutVariant?: "default" | "thread-reply";
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

    const visibleDepth = Math.min(message.depth, 6);
    const indentPx = visibleDepth * 28;
    const depthGuideOffsets = React.useMemo(() => {
      return Array.from({ length: visibleDepth }, (_, index) => 14 + index * 28);
    }, [visibleDepth]);
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
              className="max-w-full"
              content={message.body}
              imetaByUrl={imetaByUrl}
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

    const isThreadReplyLayout = layoutVariant === "thread-reply";
    const guideBleedPx = isThreadReplyLayout ? 4 : 0;
    const avatarSizeClass = isThreadReplyLayout
      ? "!h-5 !w-5 !rounded-md"
      : "!h-[42px] !w-[42px]";
    const avatarButtonRadiusClass = isThreadReplyLayout ? "rounded-md" : "rounded-xl";

    const avatarNode = message.pubkey ? (
      <UserProfilePopover
        pubkey={message.pubkey}
        role={message.role}
        botIdenticonValue={message.author}
      >
        <button
          className={cn(
            "shrink-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
            avatarButtonRadiusClass,
          )}
          type="button"
        >
          <UserAvatar
            accent={message.accent}
            avatarUrl={message.avatarUrl ?? null}
            className={avatarSizeClass}
            displayName={message.author}
            testId="message-avatar"
          />
        </button>
      </UserProfilePopover>
    ) : (
      <UserAvatar
        accent={message.accent}
        avatarUrl={message.avatarUrl ?? null}
        className={cn("shrink-0", avatarSizeClass)}
        displayName={message.author}
        testId="message-avatar"
      />
    );

    const authorNode = message.pubkey ? (
      <UserProfilePopover
        pubkey={message.pubkey}
        role={message.role}
        botIdenticonValue={message.author}
      >
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
    );

    const metadataNode = (
      <div className="ml-auto flex items-center gap-2 text-xs text-muted-foreground">
        <div className="relative">
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
        {message.pending ? (
          <p className="font-medium uppercase tracking-[0.14em] text-primary/80">
            Sending
          </p>
        ) : null}
        {message.edited ? (
          <p className="text-muted-foreground/70" title="This message has been edited">
            (edited)
          </p>
        ) : null}
        <MessageTimestamp createdAt={message.createdAt} time={message.time} />
      </div>
    );

    const messageBodyNode = (
      <>
        {renderBody()}
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
          <p className="mt-1.5 text-xs text-destructive">{reactionErrorMessage}</p>
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
      </>
    );

    return (
      <div
        className="relative"
        style={indentPx > 0 ? { paddingLeft: `${indentPx}px` } : undefined}
      >
        {depthGuideOffsets.length > 0 ? (
          <div
            aria-hidden
            className="pointer-events-none absolute left-0"
            style={{
              bottom: `${-guideBleedPx}px`,
              top: `${-guideBleedPx}px`,
            }}
          >
            {depthGuideOffsets.map((offset, index) => (
              <div
                className="absolute bottom-0 top-0 border-l border-border/70"
                key={`${message.id}-depth-guide-${offset}`}
                style={{
                  left: `${offset}px`,
                  opacity: index === depthGuideOffsets.length - 1 ? 0.9 : 0.55,
                }}
              />
            ))}
          </div>
        ) : null}

        <article
          className={cn(
            "group/message rounded-2xl px-2 py-1.5 transition-colors",
            isThreadReplyLayout ? "space-y-1.5" : "flex items-start gap-2.5",
            highlighted ? "bg-primary/10 ring-1 ring-primary/30" : "",
          )}
          data-message-id={message.id}
          data-testid="message-row"
        >
          {isThreadReplyLayout ? (
            <>
              <div className="flex min-w-0 items-start gap-1.5">
                <div className="flex shrink-0 items-start">{avatarNode}</div>
                <div className="min-w-0 flex-1">
                  <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
                    {authorNode}
                    {message.personaDisplayName &&
                    message.personaDisplayName !== message.author ? (
                      <span className="text-xs text-muted-foreground">
                        {message.personaDisplayName}
                      </span>
                    ) : message.role ? (
                      <p className="rounded-full bg-muted px-2 py-0.5 text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
                        {message.role}
                      </p>
                    ) : null}
                    {metadataNode}
                  </div>
                </div>
              </div>
              <div className="min-w-0 space-y-1">{messageBodyNode}</div>
            </>
          ) : (
            <>
              <div className="flex shrink-0 items-start">{avatarNode}</div>
              <div className="min-w-0 flex-1 space-y-1">
                <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
                  {authorNode}
                  {message.personaDisplayName &&
                  message.personaDisplayName !== message.author ? (
                    <span className="text-xs text-muted-foreground">
                      {message.personaDisplayName}
                    </span>
                  ) : message.role ? (
                    <p className="rounded-full bg-muted px-2 py-0.5 text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
                      {message.role}
                    </p>
                  ) : null}
                  {metadataNode}
                </div>
                {messageBodyNode}
              </div>
            </>
          )}
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
    prev.message.personaDisplayName === next.message.personaDisplayName &&
    prev.highlighted === next.highlighted &&
    prev.activeReplyTargetId === next.activeReplyTargetId &&
    prev.layoutVariant === next.layoutVariant &&
    prev.profiles === next.profiles,
);

MessageRow.displayName = "MessageRow";
