import {
  CornerUpLeft,
  MoreHorizontal,
  PanelRightClose,
  Trash2,
} from "lucide-react";
import * as React from "react";

import {
  resolveUserLabel,
  type UserProfileLookup,
} from "@/features/profile/lib/identity";
import { UserAvatar } from "@/shared/ui/UserAvatar";
import type { ThreadReply } from "@/shared/api/types";
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

import type { ThreadNode } from "../lib/threadTree";
import { formatRelativeTime } from "../lib/time";

export function canDeleteReply(
  reply: ThreadReply,
  currentPubkey: string | undefined,
): boolean {
  if (!currentPubkey) return false;
  return reply.pubkey.toLowerCase() === currentPubkey.toLowerCase();
}

export function DeleteConfirmDialog({
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

type ReplyRowProps = {
  node: ThreadNode;
  depth: number;
  currentPubkey?: string;
  profiles?: UserProfileLookup;
  channelNames?: string[];
  onDelete?: (eventId: string) => void;
  onPressReply: (reply: ThreadReply) => void;
  onOpenBranch: (reply: ThreadReply) => void;
  showThreadAction: boolean;
};

/** Nested replies under a parent — used in the branch column only. */
export function ReplyRow({
  node,
  depth,
  currentPubkey,
  profiles,
  channelNames,
  onDelete,
  onPressReply,
  onOpenBranch,
  showThreadAction,
}: ReplyRowProps) {
  const [isDeleteOpen, setIsDeleteOpen] = React.useState(false);
  const reply = node;
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
  const visibleDepth = Math.min(depth, 6);
  const indentPx = visibleDepth * 12;

  const hasChildren = node.children.length > 0;

  return (
    <div className="border-b border-border/40 last:border-b-0">
      <div
        className="group px-4 py-3"
        data-forum-event-id={reply.eventId}
        style={{ paddingLeft: `${16 + indentPx}px` }}
      >
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

          <div className="ml-auto flex items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100">
            <Button
              aria-label="Reply"
              className="h-7 px-2 text-muted-foreground"
              onClick={() => onPressReply(reply)}
              size="sm"
              title="Reply"
              type="button"
              variant="ghost"
            >
              <CornerUpLeft className="h-3.5 w-3.5" />
            </Button>
            {showThreadAction ? (
              <Button
                aria-label="Open thread"
                className="h-7 px-2 text-muted-foreground"
                onClick={() => onOpenBranch(reply)}
                size="sm"
                title={hasChildren ? "Open thread" : "Focus thread"}
                type="button"
                variant="ghost"
              >
                <PanelRightClose className="h-3.5 w-3.5" />
              </Button>
            ) : null}

            {showDelete ? (
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
            ) : null}
            {showDelete ? (
              <DeleteConfirmDialog
                label="reply"
                onConfirm={() => onDelete?.(reply.eventId)}
                onOpenChange={setIsDeleteOpen}
                open={isDeleteOpen}
              />
            ) : null}
          </div>
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
      {node.children.map((child) => (
        <ReplyRow
          channelNames={channelNames}
          currentPubkey={currentPubkey}
          depth={depth + 1}
          key={child.eventId}
          onDelete={onDelete}
          onOpenBranch={onOpenBranch}
          onPressReply={onPressReply}
          node={child}
          profiles={profiles}
          showThreadAction={showThreadAction}
        />
      ))}
    </div>
  );
}

/** Single top-level reply in the main column — nested replies appear in the branch panel. */
export function MainColumnReplyRow({
  node,
  currentPubkey,
  profiles,
  channelNames,
  onDelete,
  onPressReply,
  onOpenBranch,
}: {
  node: ThreadNode;
  currentPubkey?: string;
  profiles?: UserProfileLookup;
  channelNames?: string[];
  onDelete?: (eventId: string) => void;
  onPressReply: (reply: ThreadReply) => void;
  onOpenBranch: (reply: ThreadReply) => void;
}) {
  const [isDeleteOpen, setIsDeleteOpen] = React.useState(false);
  const reply = node;
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
  const hasChildren = node.children.length > 0;

  return (
    <div className="border-b border-border/40 last:border-b-0">
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

          <div className="ml-auto flex items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100">
            <Button
              aria-label="Reply"
              className="h-7 px-2 text-muted-foreground"
              onClick={() => onPressReply(reply)}
              size="sm"
              title="Reply"
              type="button"
              variant="ghost"
            >
              <CornerUpLeft className="h-3.5 w-3.5" />
            </Button>
            <Button
              aria-label="Open thread"
              className="h-7 px-2 text-muted-foreground"
              onClick={() => onOpenBranch(reply)}
              size="sm"
              title={hasChildren ? "Open thread" : "Focus thread"}
              type="button"
              variant="ghost"
            >
              <PanelRightClose className="h-3.5 w-3.5" />
            </Button>

            {showDelete ? (
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
            ) : null}
            {showDelete ? (
              <DeleteConfirmDialog
                label="reply"
                onConfirm={() => onDelete?.(reply.eventId)}
                onOpenChange={setIsDeleteOpen}
                open={isDeleteOpen}
              />
            ) : null}
          </div>
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
    </div>
  );
}

/** Renders a flat list of nodes (no recursion into children) — for the branch column. */
export function BranchReplyList({
  nodes,
  currentPubkey,
  profiles,
  channelNames,
  onDelete,
  onPressReply,
}: {
  nodes: ThreadNode[];
  currentPubkey?: string;
  profiles?: UserProfileLookup;
  channelNames?: string[];
  onDelete?: (eventId: string) => void;
  onPressReply: (reply: ThreadReply) => void;
}) {
  return (
    <div className="divide-y divide-border/40">
      {nodes.map((node) => (
        <ReplyRow
          channelNames={channelNames}
          currentPubkey={currentPubkey}
          depth={0}
          key={node.eventId}
          onDelete={onDelete}
          onOpenBranch={() => {}}
          onPressReply={onPressReply}
          node={node}
          profiles={profiles}
          showThreadAction={false}
        />
      ))}
    </div>
  );
}

export function ForumTypingStrip({
  currentPubkey,
  profiles,
  typingPubkeys,
}: {
  currentPubkey?: string;
  profiles?: UserProfileLookup;
  typingPubkeys: string[];
}) {
  if (typingPubkeys.length === 0) {
    return null;
  }

  const names = typingPubkeys.map((pubkey) =>
    resolveUserLabel({
      pubkey,
      currentPubkey,
      profiles,
      preferResolvedSelfLabel: true,
    }),
  );
  const label =
    names.length === 1
      ? `${names[0]} is typing...`
      : names.length === 2
        ? `${names[0]} and ${names[1]} are typing...`
        : `${names[0]}, ${names[1]}, and ${names.length - 2} others are typing...`;

  return (
    <div
      className="mb-2 flex items-center gap-2 text-xs text-muted-foreground"
      data-testid="forum-typing-indicator"
    >
      <span className="truncate" data-testid="forum-typing-indicator-label">
        {label}
      </span>
    </div>
  );
}
