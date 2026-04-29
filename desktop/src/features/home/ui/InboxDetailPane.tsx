import {
  ArrowUpRight,
  CheckCheck,
  CircleDot,
  Mail,
  MailOpen,
  MoreHorizontal,
  Reply,
  Trash2,
} from "lucide-react";
import * as React from "react";

import type { InboxItem, InboxReply } from "@/features/home/lib/inbox";
import { MessageComposer } from "@/features/messages/ui/MessageComposer";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { Markdown } from "@/shared/ui/markdown";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/shared/ui/dropdown-menu";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/shared/ui/tooltip";
import { UserAvatar } from "@/shared/ui/UserAvatar";

type InboxDetailPaneProps = {
  canDelete: boolean;
  canOpenChannel: boolean;
  canReply: boolean;
  disabledReplyReason?: string | null;
  isDone: boolean;
  isDeletingMessage?: boolean;
  isSendingReply?: boolean;
  item: InboxItem | null;
  replies?: InboxReply[];
  onDelete: () => void;
  onOpenChannel: (channelId: string) => void;
  onSendReply: (
    content: string,
    mentionPubkeys: string[],
    mediaTags?: string[][],
  ) => Promise<void>;
  onToggleDone: () => void;
};

export function InboxDetailPane({
  canDelete,
  canOpenChannel,
  canReply,
  disabledReplyReason,
  isDone,
  isDeletingMessage = false,
  isSendingReply = false,
  item,
  replies = [],
  onDelete,
  onOpenChannel,
  onSendReply,
  onToggleDone,
}: InboxDetailPaneProps) {
  const detailPaneRef = React.useRef<HTMLElement | null>(null);

  const focusComposer = React.useCallback(() => {
    window.requestAnimationFrame(() => {
      const textarea = detailPaneRef.current?.querySelector<HTMLTextAreaElement>(
        '[data-testid="message-input"]',
      );
      textarea?.focus();
    });
  }, []);

  if (!item) {
    return (
      <section
        className="flex min-h-0 min-w-0 items-center justify-center bg-background px-6 py-10 text-center"
        data-testid="home-inbox-detail-empty"
      >
        <div className="max-w-sm">
          <div className="mx-auto flex h-14 w-14 items-center justify-center rounded-full bg-muted text-muted-foreground">
            <Mail className="h-6 w-6" />
          </div>
          <p className="mt-4 text-base font-semibold">Select a message</p>
          <p className="mt-1 text-sm text-muted-foreground">
            Pick an inbox item to see the full message and react to it.
          </p>
        </div>
      </section>
    );
  }

  const channelId = item.item.channelId;

  return (
    <section
      className="flex min-h-0 min-w-0 flex-col overflow-hidden bg-background"
      data-testid="home-inbox-detail"
      ref={detailPaneRef}
    >
      <div className="border-b border-border/70 px-6 py-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex min-w-0 items-center gap-3">
            <UserAvatar
              avatarUrl={item.avatarUrl}
              className="h-10 w-10 rounded-md"
              displayName={item.senderLabel}
              size="md"
            />
            <div className="min-w-0">
              <div className="flex min-w-0 flex-wrap items-center gap-2">
                <p className="truncate text-base font-semibold">{item.senderLabel}</p>
                <span
                  className={cn(
                    "inline-flex items-center text-[10px] font-semibold uppercase tracking-[0.14em]",
                    item.isActionRequired
                      ? "text-amber-600 dark:text-amber-300"
                      : "text-primary",
                  )}
                >
                  {item.categoryLabel}
                </span>
                {item.channelLabel ? (
                  <span className="inline-flex items-center text-[11px] font-medium text-muted-foreground">
                    #{item.channelLabel}
                  </span>
                ) : null}
              </div>

              <div className="mt-1 flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
                <span>{item.fullTimestampLabel}</span>
                {canOpenChannel ? <CircleDot className="h-3.5 w-3.5" /> : null}
                {canOpenChannel ? (
                  <span>Linked to an active channel</span>
                ) : (
                  <span>Inbox only</span>
                )}
              </div>
            </div>
          </div>

          <div className="flex shrink-0 items-center gap-4">
            <TooltipProvider delayDuration={200}>
              <div className="flex items-center gap-4">
                {canReply ? (
                  <div className="flex items-center gap-0.5">
                    <HeaderIconAction
                      label="Reply"
                      onClick={focusComposer}
                      icon={<Reply className="h-4 w-4" />}
                    />
                  </div>
                ) : null}
                <div className="flex items-center gap-0.5">
                  {canOpenChannel && channelId ? (
                    <HeaderIconAction
                      label="Open channel"
                      onClick={() => onOpenChannel(channelId)}
                      icon={<ArrowUpRight className="h-4 w-4" />}
                    />
                  ) : null}
                  <HeaderIconAction
                    label={isDone ? "Mark unread" : "Mark done"}
                    onClick={onToggleDone}
                    icon={
                      isDone ? (
                        <MailOpen className="h-4 w-4" />
                      ) : (
                        <CheckCheck className="h-4 w-4" />
                      )
                    }
                  />
                </div>
                {canDelete ? (
                  <HeaderMoreMenu
                    isDeletingMessage={isDeletingMessage}
                    onDelete={onDelete}
                  />
                ) : null}
              </div>
            </TooltipProvider>
          </div>
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto py-6">
        <div>
          <div className="px-6 pb-5">
            <Markdown
              className="max-w-none text-left text-[15px] text-foreground"
              content={item.preview}
              mentionNames={item.mentionNames}
              tight
            />
          </div>
          {replies.length > 0 ? (
            <div
              className="border-t border-border/60 px-6 pt-5 text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground"
              data-testid="home-inbox-replies-header"
            >
              {replies.length === 1 ? "1 reply" : `${replies.length} replies`}
            </div>
          ) : null}
          {replies.map((reply) => (
            <div
              className="px-6 py-5"
              data-testid="home-inbox-reply"
              key={reply.id}
            >
              <div className="mb-3 flex items-center gap-3">
                <UserAvatar
                  avatarUrl={reply.avatarUrl}
                  className="h-8 w-8 rounded-md"
                  displayName={reply.authorLabel}
                  size="md"
                />
                <div className="min-w-0">
                  <p className="truncate text-sm font-semibold text-foreground">
                    {reply.authorLabel}
                  </p>
                  <p className="text-xs text-muted-foreground">
                    {reply.fullTimestampLabel}
                  </p>
                </div>
              </div>
              <Markdown
                className="max-w-none text-left text-[15px] text-foreground"
                content={reply.content}
                tight
              />
            </div>
          ))}
        </div>
      </div>

      <MessageComposer
        channelId={item.item.channelId}
        channelName={item.channelLabel ?? "channel"}
        containerClassName="px-6 pb-4 sm:px-6"
        disabled={!canReply}
        draftKey={`inbox-reply:${item.id}`}
        isSending={isSendingReply}
        onSend={onSendReply}
        placeholder={
          canReply
            ? `Send reply to ${item.channelLabel ? `#${item.channelLabel} thread` : "channel thread"}`
            : (disabledReplyReason ?? "Replies are not available for this item.")
        }
      />
    </section>
  );
}

function HeaderIconAction({
  icon,
  label,
  onClick,
}: {
  icon: React.ReactNode;
  label: string;
  onClick?: () => void;
}) {
  const button = (
    <Button
      aria-label={label}
      className="h-8 w-8 rounded-full p-0 text-muted-foreground"
      onClick={onClick}
      size="icon"
      type="button"
      variant="ghost"
    >
      {icon}
    </Button>
  );

  return (
    <Tooltip>
      <TooltipTrigger asChild>{button}</TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

function HeaderMoreMenu({
  isDeletingMessage,
  onDelete,
}: {
  isDeletingMessage: boolean;
  onDelete: () => void;
}) {
  const trigger = (
    <Button
      aria-label="More actions"
      className="h-8 w-8 rounded-full p-0 text-muted-foreground"
      size="icon"
      type="button"
      variant="ghost"
    >
      <MoreHorizontal className="h-4 w-4" />
    </Button>
  );

  return (
    <DropdownMenu modal={false}>
      <Tooltip>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>{trigger}</DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent>More actions</TooltipContent>
      </Tooltip>
      <DropdownMenuContent align="end">
        <DropdownMenuItem
          className="text-destructive focus:text-destructive"
          disabled={isDeletingMessage}
          onClick={onDelete}
        >
          <Trash2 className="h-4 w-4" />
          Delete message
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
