import {
  ArrowLeft,
  MessageSquare,
  MoreHorizontal,
  Trash2,
  X,
} from "lucide-react";
import * as React from "react";

import {
  resolveUserLabel,
  type UserProfileLookup,
} from "@/features/profile/lib/identity";
import { UserAvatar } from "@/shared/ui/UserAvatar";
import type { ForumThreadResponse, ThreadReply } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import { useChannelNavigation } from "@/shared/context/ChannelNavigationContext";
import { resolveMentionNames } from "@/shared/lib/resolveMentionNames";
import { Button } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { Markdown } from "@/shared/ui/markdown";
import { Skeleton } from "@/shared/ui/skeleton";

import {
  buildReplyTree,
  findNodeInTree,
  type ThreadNode,
} from "../lib/threadTree";
import { formatRelativeTime } from "../lib/time";
import { useForumThreadTyping } from "../useForumThreadTyping";
import { ForumComposer } from "./ForumComposer";
import {
  BranchReplyList,
  DeleteConfirmDialog,
  ForumTypingStrip,
  MainColumnReplyRow,
} from "./ForumThreadPanelSections";

type ForumThreadPanelProps = {
  thread: ForumThreadResponse | undefined;
  isLoading: boolean;
  isSendingReply: boolean;
  channelId: string;
  /** Thread root (forum post) event id — used for reply tree and cache keys. */
  threadRootEventId: string;
  currentPubkey?: string;
  profiles?: UserProfileLookup;
  onBack: () => void;
  onReply: (
    content: string,
    mentionPubkeys: string[],
    parentEventId: string,
  ) => void;
  onDeletePost?: (eventId: string) => void;
  onDeleteReply?: (eventId: string) => void;
  onTargetReached?: (eventId: string) => void;
  canDeletePost?: boolean;
  isDeletingPost?: boolean;
  targetEventId?: string | null;
};

export function ForumThreadPanel({
  thread,
  isLoading,
  isSendingReply,
  channelId,
  threadRootEventId,
  currentPubkey,
  profiles,
  onBack,
  onReply,
  onDeletePost,
  onDeleteReply,
  onTargetReached,
  canDeletePost,
  isDeletingPost,
  targetEventId,
}: ForumThreadPanelProps) {
  const scrollRef = React.useRef<HTMLDivElement>(null);
  const [isDeletePostOpen, setIsDeletePostOpen] = React.useState(false);
  const [focusedBranchId, setFocusedBranchId] = React.useState<string | null>(
    null,
  );
  const [branchReplyParentId, setBranchReplyParentId] = React.useState<
    string | null
  >(null);
  /** When set, main composer replies to this event id (else thread root). */
  const [mainReplyParentId, setMainReplyParentId] = React.useState<
    string | null
  >(null);

  const { channels } = useChannelNavigation();
  const channelNames = React.useMemo(
    () => channels.filter((c) => c.channelType !== "dm").map((c) => c.name),
    [channels],
  );

  React.useEffect(() => {
    if (!thread || !targetEventId) {
      return;
    }

    const targetElement =
      scrollRef.current?.querySelector<HTMLElement>(
        `[data-forum-event-id="${targetEventId}"]`,
      ) ?? null;
    if (!targetElement) {
      return;
    }

    targetElement.scrollIntoView({ block: "center" });
    onTargetReached?.(targetEventId);
  }, [onTargetReached, targetEventId, thread]);

  const tree = React.useMemo(() => {
    if (!thread) {
      return [];
    }
    return buildReplyTree(thread.replies, threadRootEventId);
  }, [thread, threadRootEventId]);

  const branchNodes = React.useMemo(() => {
    if (!focusedBranchId || tree.length === 0) {
      return [];
    }
    const stack = [...tree];
    while (stack.length > 0) {
      const n = stack.pop();
      if (!n) break;
      if (n.eventId === focusedBranchId) {
        return n.children;
      }
      for (const c of n.children) {
        stack.push(c);
      }
    }
    return [];
  }, [focusedBranchId, tree]);

  const focusedHead = React.useMemo(() => {
    if (!focusedBranchId || !thread) {
      return null;
    }
    const findIn = (nodes: ThreadNode[]): ThreadNode | null => {
      for (const n of nodes) {
        if (n.eventId === focusedBranchId) {
          return n;
        }
        const inner = findIn(n.children);
        if (inner) {
          return inner;
        }
      }
      return null;
    };
    return findIn(tree);
  }, [focusedBranchId, thread, tree]);

  React.useEffect(() => {
    if (focusedBranchId) {
      setBranchReplyParentId((prev) => prev ?? focusedBranchId);
    } else {
      setBranchReplyParentId(null);
    }
  }, [focusedBranchId]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: reset reply target when opening a different thread
  React.useEffect(() => {
    setMainReplyParentId(null);
  }, [threadRootEventId]);

  const effectiveMainParentId = mainReplyParentId ?? threadRootEventId;
  const branchTypingParentForHook =
    focusedBranchId !== null ? (branchReplyParentId ?? focusedBranchId) : null;

  const { branchComposerTypingPubkeys, mainComposerTypingPubkeys } =
    useForumThreadTyping(
      channelId,
      currentPubkey,
      Boolean(!isLoading && thread),
      threadRootEventId,
      effectiveMainParentId,
      focusedBranchId !== null,
      branchTypingParentForHook,
    );

  if (isLoading || !thread) {
    return (
      <div className="flex h-full flex-col">
        <div className="border-b border-border/60 px-4 py-3">
          <Button
            className="gap-1.5 text-muted-foreground"
            onClick={onBack}
            size="sm"
            variant="ghost"
          >
            <ArrowLeft className="h-4 w-4" />
            Back to posts
          </Button>
        </div>
        <div className="flex-1 space-y-4 p-4">
          <Skeleton className="h-8 w-3/4" />
          <Skeleton className="h-24 w-full" />
          <Skeleton className="h-16 w-full" />
        </div>
      </div>
    );
  }

  const { post, replies } = thread;
  const postMentionNames = resolveMentionNames(post.tags, profiles);
  const postAuthorLabel = resolveUserLabel({
    pubkey: post.pubkey,
    currentPubkey,
    profiles,
    preferResolvedSelfLabel: true,
  });
  const postAvatarUrl =
    profiles?.[post.pubkey.toLowerCase()]?.avatarUrl ?? null;

  /** Open the right thread column and reply under this comment (Slack-style: Reply opens the panel). */
  const handleOpenThreadForReply = (reply: ThreadReply) => {
    setFocusedBranchId(reply.eventId);
    setBranchReplyParentId(reply.eventId);
    setMainReplyParentId(null);
  };

  const handlePressReplyBranch = (reply: ThreadReply) => {
    setBranchReplyParentId(reply.eventId);
  };

  const branchHeadLabel = focusedHead
    ? resolveUserLabel({
        pubkey: focusedHead.pubkey,
        currentPubkey,
        profiles,
        preferResolvedSelfLabel: true,
      })
    : "";

  const mainReplyParent = mainReplyParentId
    ? findNodeInTree(tree, mainReplyParentId)
    : null;
  const mainReplyAuthorLabel = mainReplyParent
    ? resolveUserLabel({
        pubkey: mainReplyParent.pubkey,
        currentPubkey,
        profiles,
        preferResolvedSelfLabel: true,
      })
    : null;

  const branchReplyParentNode =
    branchReplyParentId && branchReplyParentId !== focusedBranchId
      ? findNodeInTree(tree, branchReplyParentId)
      : null;
  const branchReplyAuthorLabel =
    branchReplyParentNode && focusedBranchId
      ? resolveUserLabel({
          pubkey: branchReplyParentNode.pubkey,
          currentPubkey,
          profiles,
          preferResolvedSelfLabel: true,
        })
      : null;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="border-b border-border/60 px-4 py-3">
        <Button
          className="gap-1.5 text-muted-foreground"
          onClick={onBack}
          size="sm"
          variant="ghost"
        >
          <ArrowLeft className="h-4 w-4" />
          Back to posts
        </Button>
      </div>

      <div className="flex min-h-0 flex-1 flex-row">
        <div className="flex min-w-0 min-h-0 flex-1 flex-col" ref={scrollRef}>
          <div
            className="flex-1 overflow-y-auto"
            data-scroll-restoration-id={`forum-thread:${channelId}`}
          >
            <div
              className={cn(
                "group border-b border-border/60 p-4",
                isDeletingPost && "pointer-events-none opacity-50",
              )}
              data-forum-event-id={post.eventId}
            >
              <div className="flex items-center gap-2">
                <UserAvatar
                  avatarUrl={postAvatarUrl}
                  displayName={postAuthorLabel}
                />
                <div>
                  <span className="text-sm font-semibold text-foreground">
                    {postAuthorLabel}
                  </span>
                  <span className="ml-2 text-xs text-muted-foreground">
                    {formatRelativeTime(post.createdAt)}
                  </span>
                </div>

                {canDeletePost && onDeletePost ? (
                  <div className="ml-auto opacity-0 transition-opacity group-hover:opacity-100">
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <button
                          className="rounded-md p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
                          type="button"
                        >
                          <MoreHorizontal className="h-4 w-4" />
                        </button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end">
                        <DropdownMenuItem
                          className="text-destructive focus:text-destructive"
                          onClick={() => setIsDeletePostOpen(true)}
                        >
                          <Trash2 className="mr-2 h-4 w-4" />
                          Delete post
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                    <DeleteConfirmDialog
                      label="post"
                      onConfirm={() => onDeletePost(post.eventId)}
                      onOpenChange={setIsDeletePostOpen}
                      open={isDeletePostOpen}
                    />
                  </div>
                ) : null}
              </div>
              <div className="mt-3">
                <Markdown
                  channelNames={channelNames}
                  content={post.content}
                  mentionNames={postMentionNames}
                />
              </div>
            </div>

            <div className="flex items-center gap-1.5 border-b border-border/60 px-4 py-2.5 text-sm font-medium text-muted-foreground">
              <MessageSquare className="h-4 w-4" />
              {replies.length} {replies.length === 1 ? "reply" : "replies"}
            </div>

            <div className="divide-y divide-border/40">
              {tree.map((node) => (
                <MainColumnReplyRow
                  channelNames={channelNames}
                  currentPubkey={currentPubkey}
                  key={node.eventId}
                  node={node}
                  onDelete={onDeleteReply}
                  onOpenBranch={handleOpenThreadForReply}
                  onPressReply={handleOpenThreadForReply}
                  profiles={profiles}
                />
              ))}

              {replies.length === 0 ? (
                <div className="px-4 py-6 text-center text-sm text-muted-foreground">
                  No replies yet. Be the first to respond.
                </div>
              ) : null}
            </div>
          </div>

          <div className="shrink-0 border-t border-border/60 p-4">
            {mainComposerTypingPubkeys.length > 0 ? (
              <ForumTypingStrip
                currentPubkey={currentPubkey}
                profiles={profiles}
                typingPubkeys={mainComposerTypingPubkeys}
              />
            ) : null}
            <ForumComposer
              channelId={channelId}
              isSending={isSendingReply}
              onCancelReplyTo={
                mainReplyAuthorLabel
                  ? () => {
                      setMainReplyParentId(null);
                    }
                  : undefined
              }
              onSubmit={(content, mentionPubkeys) => {
                onReply(content, mentionPubkeys, effectiveMainParentId);
                setMainReplyParentId(null);
              }}
              placeholder="Reply to this post..."
              replyToAuthorLabel={mainReplyAuthorLabel}
              submitLabel="Reply"
              typingReplyParentId={effectiveMainParentId}
            />
          </div>
        </div>

        {focusedBranchId && focusedHead ? (
          <aside className="flex w-[min(100%,24rem)] shrink-0 flex-col border-l border-border/60 bg-muted/20">
            <div className="flex items-center gap-2 border-b border-border/60 px-3 py-2">
              <span className="min-w-0 flex-1 truncate text-xs font-medium text-muted-foreground">
                Thread with {branchHeadLabel}
              </span>
              <Button
                aria-label="Close thread panel"
                className="h-8 w-8 shrink-0"
                onClick={() => {
                  setFocusedBranchId(null);
                  setBranchReplyParentId(null);
                }}
                size="icon"
                type="button"
                variant="ghost"
              >
                <X className="h-4 w-4" />
              </Button>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto">
              <div
                className="border-b border-border/60 bg-background/50 px-3 py-2"
                data-forum-event-id={focusedHead.eventId}
              >
                <p className="text-xs text-muted-foreground">Root of thread</p>
                <div className="mt-1 text-sm text-foreground">
                  <Markdown
                    channelNames={channelNames}
                    compact
                    content={focusedHead.content}
                    mentionNames={resolveMentionNames(
                      focusedHead.tags,
                      profiles,
                    )}
                  />
                </div>
              </div>
              {branchNodes.length === 0 ? (
                <div className="px-3 py-4 text-center text-xs text-muted-foreground">
                  No replies in this thread yet.
                </div>
              ) : (
                <BranchReplyList
                  channelNames={channelNames}
                  currentPubkey={currentPubkey}
                  nodes={branchNodes}
                  onDelete={onDeleteReply}
                  onPressReply={handlePressReplyBranch}
                  profiles={profiles}
                />
              )}
            </div>
            <div className="shrink-0 border-t border-border/60 p-3">
              {branchComposerTypingPubkeys.length > 0 ? (
                <ForumTypingStrip
                  currentPubkey={currentPubkey}
                  profiles={profiles}
                  typingPubkeys={branchComposerTypingPubkeys}
                />
              ) : null}
              <ForumComposer
                channelId={channelId}
                isSending={isSendingReply}
                onCancelReplyTo={
                  branchReplyAuthorLabel
                    ? () => {
                        setBranchReplyParentId(focusedBranchId);
                      }
                    : undefined
                }
                onSubmit={(content, mentionPubkeys) => {
                  const parent = branchReplyParentId ?? focusedBranchId;
                  onReply(content, mentionPubkeys, parent);
                }}
                placeholder="Reply in thread..."
                replyToAuthorLabel={branchReplyAuthorLabel}
                submitLabel="Reply"
                typingReplyParentId={
                  branchReplyParentId ?? focusedBranchId ?? null
                }
              />
            </div>
          </aside>
        ) : null}
      </div>
    </div>
  );
}
