import { X } from "lucide-react";
import * as React from "react";
import { useQuery } from "@tanstack/react-query";

import { MessageComposer } from "@/features/messages/ui/MessageComposer";
import { TypingIndicatorRow } from "@/features/messages/ui/TypingIndicatorRow";
import { channelThreadKey } from "@/features/messages/lib/messageQueryKeys";
import { getForumThread } from "@/shared/api/forum";
import type { Channel } from "@/shared/api/types";
import {
  resolveUserLabel,
  type UserProfileLookup,
} from "@/features/profile/lib/identity";
import { useChannelNavigation } from "@/shared/context/ChannelNavigationContext";
import { resolveMentionNames } from "@/shared/lib/resolveMentionNames";
import { Button } from "@/shared/ui/button";
import { Markdown } from "@/shared/ui/markdown";
import { ProfileAvatar } from "@/features/profile/ui/ProfileAvatar";
import { Skeleton } from "@/shared/ui/skeleton";

import { formatRelativeTime } from "@/features/forum/lib/time";

type ChannelThreadPanelProps = {
  channel: Channel;
  currentPubkey?: string;
  profiles?: UserProfileLookup;
  rootEventId: string;
  onClose: () => void;
  onSend: (
    content: string,
    mentionPubkeys: string[],
    mediaTags?: string[][],
  ) => Promise<void>;
  isSending: boolean;
  disabledComposer: boolean;
  threadTypingPubkeys: string[];
};

function ThreadReplyRow({
  content,
  createdAt,
  pubkey,
  tags,
  currentPubkey,
  profiles,
  channelNames,
}: {
  content: string;
  createdAt: number;
  pubkey: string;
  tags: string[][];
  currentPubkey?: string;
  profiles?: UserProfileLookup;
  channelNames: string[];
}) {
  const label = resolveUserLabel({
    pubkey,
    currentPubkey,
    profiles,
    preferResolvedSelfLabel: true,
  });
  const avatarUrl = profiles?.[pubkey.toLowerCase()]?.avatarUrl ?? null;
  const mentionNames = resolveMentionNames(tags, profiles);

  return (
    <div className="border-b border-border/40 px-3 py-2.5 last:border-b-0">
      <div className="flex items-start gap-2">
        <ProfileAvatar
          avatarUrl={avatarUrl}
          className="h-7 w-7 shrink-0 rounded-md text-[10px]"
          iconClassName="h-3 w-3"
          label={label}
        />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
            <span className="text-sm font-medium">{label}</span>
            <span className="text-[11px] text-muted-foreground">
              {formatRelativeTime(createdAt)}
            </span>
          </div>
          <div className="mt-0.5 text-sm">
            <Markdown
              channelNames={channelNames}
              compact
              content={content}
              mentionNames={mentionNames}
            />
          </div>
        </div>
      </div>
    </div>
  );
}

export function ChannelThreadPanel({
  channel,
  currentPubkey,
  profiles,
  rootEventId,
  onClose,
  onSend,
  isSending,
  disabledComposer,
  threadTypingPubkeys,
}: ChannelThreadPanelProps) {
  const { channels } = useChannelNavigation();
  const channelNames = React.useMemo(
    () => channels.filter((c) => c.channelType !== "dm").map((c) => c.name),
    [channels],
  );

  const threadQuery = useQuery({
    queryKey: channelThreadKey(channel.id, rootEventId),
    queryFn: () => getForumThread(channel.id, rootEventId),
    enabled: Boolean(channel.id && rootEventId),
  });

  const thread = threadQuery.data;
  const isLoading = threadQuery.isPending;

  return (
    <aside
      className="flex h-full min-h-0 w-[min(100%,420px)] shrink-0 flex-col border-l border-border/60 bg-muted/20"
      data-testid="channel-thread-panel"
    >
      <header className="flex shrink-0 items-center justify-between border-b border-border/60 px-3 py-2">
        <p className="truncate text-sm font-semibold">Thread</p>
        <Button
          aria-label="Close thread"
          className="h-8 w-8 shrink-0 p-0"
          onClick={onClose}
          type="button"
          variant="ghost"
        >
          <X className="h-4 w-4" />
        </Button>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
        {isLoading ? (
          <div className="space-y-3 p-3">
            <Skeleton className="h-16 w-full rounded-lg" />
            <Skeleton className="h-12 w-full rounded-lg" />
            <Skeleton className="h-12 w-full rounded-lg" />
          </div>
        ) : thread ? (
          <>
            <div className="border-b border-border/50 bg-background/40 px-3 py-3">
              <div className="flex items-start gap-2">
                <ProfileAvatar
                  avatarUrl={
                    profiles?.[thread.post.pubkey.toLowerCase()]?.avatarUrl ??
                    null
                  }
                  className="h-8 w-8 shrink-0 rounded-lg text-[11px]"
                  label={resolveUserLabel({
                    pubkey: thread.post.pubkey,
                    currentPubkey,
                    profiles,
                    preferResolvedSelfLabel: true,
                  })}
                />
                <div className="min-w-0 flex-1">
                  <p className="text-sm font-semibold">
                    {resolveUserLabel({
                      pubkey: thread.post.pubkey,
                      currentPubkey,
                      profiles,
                      preferResolvedSelfLabel: true,
                    })}
                  </p>
                  <div className="mt-1 text-sm">
                    <Markdown
                      channelNames={channelNames}
                      compact
                      content={thread.post.content}
                      mentionNames={resolveMentionNames(
                        thread.post.tags,
                        profiles,
                      )}
                    />
                  </div>
                </div>
              </div>
            </div>

            <div className="pb-2">
              {thread.replies.map((reply) => (
                <ThreadReplyRow
                  channelNames={channelNames}
                  content={reply.content}
                  createdAt={reply.createdAt}
                  currentPubkey={currentPubkey}
                  key={reply.eventId}
                  profiles={profiles}
                  pubkey={reply.pubkey}
                  tags={reply.tags}
                />
              ))}
              {thread.replies.length === 0 ? (
                <p className="px-3 py-6 text-center text-sm text-muted-foreground">
                  No replies yet. Send a message below.
                </p>
              ) : null}
            </div>
          </>
        ) : (
          <p className="p-4 text-sm text-muted-foreground">
            Could not load this thread.
          </p>
        )}
      </div>

      <TypingIndicatorRow
        channel={channel}
        currentPubkey={currentPubkey}
        profiles={profiles}
        typingPubkeys={threadTypingPubkeys}
      />
      <div className="shrink-0 border-t border-border/60 bg-background/80 p-2 backdrop-blur-sm">
        <MessageComposer
          channelId={channel.id}
          channelName={channel.name}
          disabled={disabledComposer}
          draftKey={`channel:${channel.id}:thread:${rootEventId}`}
          isSending={isSending}
          onSend={onSend}
          placeholder={`Reply in thread…`}
          typingThreadRootId={rootEventId}
        />
      </div>
    </aside>
  );
}
