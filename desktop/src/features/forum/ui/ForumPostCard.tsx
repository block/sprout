import { MessageSquare, MoreHorizontal, Trash2 } from "lucide-react";
import * as React from "react";

import { ProfileAvatar } from "@/features/profile/ui/ProfileAvatar";
import {
  resolveUserLabel,
  type UserProfileLookup,
} from "@/features/profile/lib/identity";
import type { ForumPost } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
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

import { formatRelativeTime } from "../lib/time";

type ForumPostCardProps = {
  post: ForumPost;
  currentPubkey?: string;
  profiles?: UserProfileLookup;
  isActive?: boolean;
  canDelete?: boolean;
  isDeleting?: boolean;
  onClick: (post: ForumPost) => void;
  onDelete?: (eventId: string) => void;
};

export function ForumPostCard({
  post,
  currentPubkey,
  profiles,
  isActive,
  canDelete,
  isDeleting,
  onClick,
  onDelete,
}: ForumPostCardProps) {
  const [isDeleteDialogOpen, setIsDeleteDialogOpen] = React.useState(false);
  const authorLabel = resolveUserLabel({
    pubkey: post.pubkey,
    currentPubkey,
    profiles,
    preferResolvedSelfLabel: true,
  });
  const avatarUrl = profiles?.[post.pubkey.toLowerCase()]?.avatarUrl ?? null;
  const summary = post.threadSummary;
  const previewContent =
    post.content.length > 200
      ? `${post.content.slice(0, 200)}...`
      : post.content;

  return (
    <button
      className={cn(
        "group w-full cursor-pointer rounded-xl border border-border/60 bg-card p-4 text-left transition-colors hover:border-border hover:bg-accent/40",
        isActive && "border-primary/40 bg-accent/60",
        isDeleting && "pointer-events-none opacity-50",
      )}
      onClick={() => onClick(post)}
      type="button"
    >
      <div className="flex items-center gap-2">
        <ProfileAvatar
          avatarUrl={avatarUrl}
          className="h-6 w-6 rounded-full text-[10px]"
          iconClassName="h-3 w-3"
          label={authorLabel}
        />
        <span className="text-sm font-medium text-foreground">
          {authorLabel}
        </span>
        <span className="text-xs text-muted-foreground">
          {formatRelativeTime(post.createdAt)}
        </span>

        {canDelete && onDelete ? (
          <div
            className="ml-auto opacity-0 transition-opacity group-hover:opacity-100"
            onClickCapture={(e) => e.stopPropagation()}
            role="presentation"
          >
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <button
                  className="rounded-md p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
                  tabIndex={-1}
                  type="button"
                >
                  <MoreHorizontal className="h-4 w-4" />
                </button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem
                  className="text-destructive focus:text-destructive"
                  onClick={() => setIsDeleteDialogOpen(true)}
                >
                  <Trash2 className="mr-2 h-4 w-4" />
                  Delete post
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>

            <AlertDialog
              onOpenChange={setIsDeleteDialogOpen}
              open={isDeleteDialogOpen}
            >
              <AlertDialogContent>
                <AlertDialogHeader>
                  <AlertDialogTitle>Delete post?</AlertDialogTitle>
                  <AlertDialogDescription>
                    This will permanently delete this post and cannot be undone.
                  </AlertDialogDescription>
                </AlertDialogHeader>
                <AlertDialogFooter>
                  <AlertDialogCancel asChild>
                    <Button type="button" variant="outline">
                      Cancel
                    </Button>
                  </AlertDialogCancel>
                  <AlertDialogAction asChild>
                    <Button
                      onClick={() => onDelete(post.eventId)}
                      type="button"
                      variant="destructive"
                    >
                      Delete post
                    </Button>
                  </AlertDialogAction>
                </AlertDialogFooter>
              </AlertDialogContent>
            </AlertDialog>
          </div>
        ) : null}
      </div>

      <div className="mt-2">
        <Markdown compact content={previewContent} />
      </div>

      {summary && summary.replyCount > 0 ? (
        <div className="mt-3 flex items-center gap-1.5 text-xs text-muted-foreground">
          <MessageSquare className="h-3.5 w-3.5" />
          <span>
            {summary.replyCount}{" "}
            {summary.replyCount === 1 ? "reply" : "replies"}
          </span>
          {summary.lastReplyAt ? (
            <>
              <span className="text-muted-foreground/50">·</span>
              <span>last {formatRelativeTime(summary.lastReplyAt)}</span>
            </>
          ) : null}
        </div>
      ) : null}
    </button>
  );
}
