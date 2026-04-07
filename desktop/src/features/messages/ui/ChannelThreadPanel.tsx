import { X } from "lucide-react";
import * as React from "react";
import { useQuery } from "@tanstack/react-query";

import {
  useManagedAgentsQuery,
  useRelayAgentsQuery,
} from "@/features/agents/hooks";
import { MessageComposer } from "@/features/messages/ui/MessageComposer";
import { channelThreadKey } from "@/features/messages/lib/messageQueryKeys";
import { getForumThread } from "@/shared/api/forum";
import type {
  Channel,
  ForumPost,
  ForumThreadResponse,
  ThreadReply,
} from "@/shared/api/types";
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

type ThreadHeaderSlice =
  | { type: "post"; data: ForumPost }
  | { type: "reply"; data: ThreadReply };

type ThreadTailItem =
  | { type: "post"; data: ForumPost }
  | { type: "reply"; data: ThreadReply };

/** Anchor header on the opened message; list messages strictly after it in time. */
function sliceThreadForFocus(
  thread: ForumThreadResponse,
  focusEventId: string | null,
): { header: ThreadHeaderSlice; tail: ThreadTailItem[] } {
  const post = thread.post;
  const entries = [
    {
      kind: "post" as const,
      eventId: post.eventId,
      createdAt: post.createdAt,
      post,
    },
    ...thread.replies.map((reply) => ({
      kind: "reply" as const,
      eventId: reply.eventId,
      createdAt: reply.createdAt,
      reply,
    })),
  ].sort((a, b) => a.createdAt - b.createdAt);

  const focusId =
    focusEventId && focusEventId.length > 0 ? focusEventId : post.eventId;
  const focusIdx = entries.findIndex((e) => e.eventId === focusId);

  if (focusIdx < 0) {
    return {
      header: { type: "post", data: post },
      tail: entries
        .filter((e) => e.eventId !== post.eventId)
        .map((e) =>
          e.kind === "post"
            ? { type: "post" as const, data: e.post }
            : { type: "reply" as const, data: e.reply },
        ),
    };
  }

  const focusEntry = entries[focusIdx];
  if (!focusEntry) {
    return {
      header: { type: "post", data: post },
      tail: entries
        .filter((e) => e.eventId !== post.eventId)
        .map((e) =>
          e.kind === "post"
            ? { type: "post" as const, data: e.post }
            : { type: "reply" as const, data: e.reply },
        ),
    };
  }
  const header: ThreadHeaderSlice =
    focusEntry.kind === "post"
      ? { type: "post", data: focusEntry.post }
      : { type: "reply", data: focusEntry.reply };

  const tail = entries
    .slice(focusIdx + 1)
    .map((e) =>
      e.kind === "post"
        ? { type: "post" as const, data: e.post }
        : { type: "reply" as const, data: e.reply },
    );

  return { header, tail };
}

type ChannelThreadPanelProps = {
  channel: Channel;
  currentPubkey?: string;
  profiles?: UserProfileLookup;
  rootEventId: string;
  /** Message id of the timeline row that opened this thread (header anchor). */
  focusEventId: string | null;
  onClose: () => void;
  onCancelReply: () => void;
  onSend: (
    content: string,
    mentionPubkeys: string[],
    mediaTags?: string[][],
  ) => Promise<void>;
  isSending: boolean;
  editTarget?: {
    author: string;
    body: string;
    id: string;
  } | null;
  onCancelEdit?: () => void;
  onEditSave?: (content: string) => Promise<void>;
  disabledComposer: boolean;
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
    <div className="px-3 py-2.5">
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
  focusEventId,
  onClose,
  onCancelReply,
  onSend,
  isSending,
  editTarget = null,
  onCancelEdit,
  onEditSave,
  disabledComposer,
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

  const managedAgentsQuery = useManagedAgentsQuery();
  const relayAgentsQuery = useRelayAgentsQuery();

  const thread = threadQuery.data;
  const isLoading = threadQuery.isPending;

  const { headerSlice, tailItems } = React.useMemo(() => {
    if (!thread) {
      return { headerSlice: null as ThreadHeaderSlice | null, tailItems: [] };
    }
    const { header, tail } = sliceThreadForFocus(thread, focusEventId);
    return { headerSlice: header, tailItems: tail };
  }, [focusEventId, thread]);

  const implicitThreadAgentMention = React.useMemo(() => {
    const headerPk = headerSlice?.data.pubkey ?? thread?.post.pubkey;
    if (!headerPk) {
      return null;
    }

    const lower = headerPk.toLowerCase();
    const managed = (managedAgentsQuery.data ?? []).find(
      (a) => a.pubkey.toLowerCase() === lower,
    );
    if (managed) {
      return { displayName: managed.name, pubkey: managed.pubkey };
    }

    const relay = (relayAgentsQuery.data ?? []).find(
      (a) => a.pubkey.toLowerCase() === lower,
    );
    if (relay) {
      return { displayName: relay.name, pubkey: relay.pubkey };
    }

    return null;
  }, [
    headerSlice,
    managedAgentsQuery.data,
    relayAgentsQuery.data,
    thread?.post.pubkey,
  ]);

  return (
    <aside
      className="relative z-10 flex h-full min-h-0 w-[min(100%,420px)] shrink-0 flex-col border-l border-border/60 bg-muted/20 pt-12"
      data-testid="channel-thread-panel"
    >
      <header className="flex shrink-0 justify-end px-2 py-2">
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
        ) : thread && headerSlice ? (
          <>
            <div className="bg-background/40 px-3 py-3">
              <div className="flex items-start gap-2">
                <ProfileAvatar
                  avatarUrl={
                    profiles?.[headerSlice.data.pubkey.toLowerCase()]
                      ?.avatarUrl ?? null
                  }
                  className="h-8 w-8 shrink-0 rounded-lg text-[11px]"
                  label={resolveUserLabel({
                    pubkey: headerSlice.data.pubkey,
                    currentPubkey,
                    profiles,
                    preferResolvedSelfLabel: true,
                  })}
                />
                <div className="min-w-0 flex-1">
                  <p className="text-sm font-semibold">
                    {resolveUserLabel({
                      pubkey: headerSlice.data.pubkey,
                      currentPubkey,
                      profiles,
                      preferResolvedSelfLabel: true,
                    })}
                  </p>
                  <div className="mt-1 text-sm">
                    <Markdown
                      channelNames={channelNames}
                      compact
                      content={headerSlice.data.content}
                      mentionNames={resolveMentionNames(
                        headerSlice.data.tags,
                        profiles,
                      )}
                    />
                  </div>
                </div>
              </div>
            </div>

            <div className="pb-2">
              {tailItems.map((item) => (
                <ThreadReplyRow
                  channelNames={channelNames}
                  content={item.data.content}
                  createdAt={item.data.createdAt}
                  currentPubkey={currentPubkey}
                  key={item.data.eventId}
                  profiles={profiles}
                  pubkey={item.data.pubkey}
                  tags={item.data.tags}
                />
              ))}
              {tailItems.length === 0 ? (
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

      <div className="shrink-0 bg-background/80 p-2 backdrop-blur-sm">
        <MessageComposer
          channelId={channel.id}
          channelName={channel.name}
          disabled={disabledComposer}
          draftStorageKey={`${channel.id}:thread:${rootEventId}`}
          editTarget={editTarget}
          implicitThreadAgentMention={implicitThreadAgentMention}
          isSending={isSending}
          onCancelEdit={onCancelEdit}
          onCancelReply={onCancelReply}
          onEditSave={onEditSave}
          onSend={onSend}
          placeholder="Reply in thread…"
          replyTarget={null}
        />
      </div>
    </aside>
  );
}
