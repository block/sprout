import { ArrowLeft, MessageSquare, MoreHorizontal, Trash2 } from "lucide-react";
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
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/shared/ui/alert-dialog";
import { Button } from "@/shared/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import { Markdown } from "@/shared/ui/markdown";
import { Skeleton } from "@/shared/ui/skeleton";

import { formatRelativeTime } from "../lib/time";
import { ForumComposer } from "./ForumComposer";

type ForumThreadPanelProps = {
  thread: ForumThreadResponse | undefined;
  isLoading: boolean;
  isSendingReply: boolean;
  channelId: string;
  currentPubkey?: string;
  profiles?: UserProfileLookup;
  onBack: () => void;
  onReply: (content: string, mentionPubkeys: string[]) => void;
  onDeletePost?: (eventId: string) => void;
  onDeleteReply?: (eventId: string) => void;
  onTargetReached?: (eventId: string) => void;
  canDeletePost?: boolean;
  isDeletingPost?: boolean;
  targetEventId?: string | null;
};

function canDeleteReply(
  reply: ThreadReply,
  currentPubkey: string | undefined,
): boolean {
  if (!currentPubkey) return false;
  return reply.pubkey.toLowerCase() === currentPubkey.toLowerCase();
}

function DeleteConfirmDialog({
  open,
  onOpenChange,
  onConfirm,
  label,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => void;
  label: string;
}) {
  return (
    <AlertDialog onOpenChange={onOpenChange} open={open}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Delete {label}?</AlertDialogTitle>
          <AlertDialogDescription>
            This will permanently delete this {label} and cannot be undone.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel asChild>
            <Button type="button" variant="outline">
              Cancel
            </Button>
          </AlertDialogCancel>
          <AlertDialogAction asChild>
            <Button onClick={onConfirm} type="button" variant="destructive">
              Delete {label}
            </Button>
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

function ReplyRow({
  reply,
  currentPubkey,
  profiles,
  channelNames,
  onDelete,
}: {
  reply: ThreadReply;
  currentPubkey?: string;
  profiles?: UserProfileLookup;
  channelNames?: string[];
  onDelete?: (eventId: string) => void;
}) {
  const [isDeleteOpen, setIsDeleteOpen] = React.useState(false);
  const replyAuthorLabel = resolveUserLabel({
    pubkey: reply.pubkey,
    currentPubkey,
    profiles,
    preferResolvedSelfLabel: true,
  });
  const replyAvatarUrl =
    profiles?.[reply.pubkey.toLowerCase()]?.avatarUrl ?? null;
  const showDelete = onDelete && canDeleteReply(reply, currentPubkey);
  const replyMentionNames = resolveMentionNames(reply.tags, profiles);

  return (
    <div className="group px-4 py-3" data-forum-event-id={reply.eventId}>
      <div className="flex items-center gap-2">
        <UserAvatar
          avatarUrl={replyAvatarUrl}
          displayName={replyAuthorLabel}
          size="sm"
        />
        <span className="text-sm font-medium text-foreground">
          {replyAuthorLabel}
        </span>
        <span className="text-xs text-muted-foreground">
          {formatRelativeTime(reply.createdAt)}
        </span>

        {showDelete ? (
          <div className="ml-auto opacity-0 transition-opacity group-hover:opacity-100">
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button
                  className="rounded-md p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
                  type="button"
                >
                  <MoreHorizontal className="h-3.5 w-3.5" />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem
                  className="text-destructive focus:text-destructive"
                  onClick={() => setIsDeleteOpen(true)}
                >
                  <Trash2 className="mr-2 h-4 w-4" />
                  Delete reply
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
            <DeleteConfirmDialog
              label="reply"
              onConfirm={() => onDelete(reply.eventId)}
              onOpenChange={setIsDeleteOpen}
              open={isDeleteOpen}
            />
          </div>
        ) : null}
      </div>
      <div className="mt-1.5 pl-8">
        <Markdown
          channelNames={channelNames}
          compact
          content={reply.content}
          mentionNames={replyMentionNames}
        />
      </div>
    </div>
  );
}

export function ForumThreadPanel({
  thread,
  isLoading,
  isSendingReply,
  channelId,
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

      <div
        className="flex-1 overflow-y-auto"
        data-scroll-restoration-id={`forum-thread:${channelId}`}
        ref={scrollRef}
      >
        {/* Original post */}
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

        {/* Replies header */}
        <div className="flex items-center gap-1.5 border-b border-border/60 px-4 py-2.5 text-sm font-medium text-muted-foreground">
          <MessageSquare className="h-4 w-4" />
          {replies.length} {replies.length === 1 ? "reply" : "replies"}
        </div>

        {/* Reply list */}
        <div className="divide-y divide-border/40">
          {replies.map((reply) => (
            <ReplyRow
              channelNames={channelNames}
              currentPubkey={currentPubkey}
              key={reply.eventId}
              onDelete={onDeleteReply}
              profiles={profiles}
              reply={reply}
            />
          ))}

          {replies.length === 0 ? (
            <div className="px-4 py-6 text-center text-sm text-muted-foreground">
              No replies yet. Be the first to respond.
            </div>
          ) : null}
        </div>
      </div>

      {/* Reply composer */}
      <div className="border-t border-border/60 p-4">
        <ForumComposer
          channelId={channelId}
          isSending={isSendingReply}
          onSubmit={onReply}
          placeholder="Reply to this post..."
          submitLabel="Reply"
        />
      </div>
    </div>
  );
}
