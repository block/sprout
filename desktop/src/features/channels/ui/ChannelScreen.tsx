import * as React from "react";
import { useQueryClient } from "@tanstack/react-query";

import { ChatHeader } from "@/features/chat/ui/ChatHeader";
import { useActiveChannelHeader } from "@/features/channels/useActiveChannelHeader";
import { useChannelPaneHandlers } from "@/features/channels/useChannelPaneHandlers";
import { ChannelMembersBar } from "@/features/channels/ui/ChannelMembersBar";
import { MembersSidebar } from "@/features/channels/ui/MembersSidebar";
import {
  mergeMessages,
  useChannelMessagesQuery,
  useChannelSubscription,
  useDeleteMessageMutation,
  useEditMessageMutation,
  useSendMessageMutation,
  useToggleReactionMutation,
} from "@/features/messages/hooks";
import type {
  ThreadConversationHint,
  TimelineMessage,
} from "@/features/messages/types";
import {
  channelMessagesKey,
  channelThreadKey,
} from "@/features/messages/lib/messageQueryKeys";
import {
  collectMessageAuthorPubkeys,
  formatTimelineMessages,
} from "@/features/messages/lib/formatTimelineMessages";
import { threadHintAnchorIdForNestedMessage } from "@/features/messages/lib/threadHintAnchor";
import {
  getChannelIdFromTags,
  getThreadReference,
} from "@/features/messages/lib/threading";
import { useFetchOlderMessages } from "@/features/messages/useFetchOlderMessages";
import { useChannelTyping } from "@/features/messages/useChannelTyping";
import { PresenceBadge } from "@/features/presence/ui/PresenceBadge";
import { useUsersBatchQuery } from "@/features/profile/hooks";
import {
  mergeCurrentProfileIntoLookup,
  resolveUserLabel,
} from "@/features/profile/lib/identity";
import { getEventById } from "@/shared/api/tauri";
import type {
  Channel,
  Identity,
  Profile,
  RelayEvent,
  SearchHit,
} from "@/shared/api/types";
import { KIND_SYSTEM_MESSAGE } from "@/shared/constants/kinds";
import { ViewLoadingFallback } from "@/shared/ui/ViewLoadingFallback";

const ChannelPane = React.lazy(async () => {
  const module = await import("@/features/channels/ui/ChannelPane");
  return { default: module.ChannelPane };
});

const ForumView = React.lazy(async () => {
  const module = await import("@/features/forum/ui/ForumView");
  return { default: module.ForumView };
});

type ChannelScreenProps = {
  activeChannel: Channel | null;
  currentIdentity?: Identity;
  currentProfile?: Profile;
  onManageChannel: () => void;
  onMarkChannelRead: (
    channelId: string,
    readAt: string | null | undefined,
  ) => void;
  onTargetReached: (messageId: string) => void;
  searchAnchor: SearchHit | null;
  searchAnchorChannelId: string | null;
  searchAnchorEvent: RelayEvent | null;
};

export function ChannelScreen({
  activeChannel,
  currentIdentity,
  currentProfile,
  onManageChannel,
  onMarkChannelRead,
  onTargetReached,
  searchAnchor,
  searchAnchorChannelId,
  searchAnchorEvent,
}: ChannelScreenProps) {
  const queryClient = useQueryClient();
  const [isMembersSidebarOpen, setIsMembersSidebarOpen] = React.useState(false);
  const [replyTargetId, setReplyTargetId] = React.useState<string | null>(null);
  const [editTargetId, setEditTargetId] = React.useState<string | null>(null);
  const [threadRootId, setThreadRootId] = React.useState<string | null>(null);
  /** Event id of the message row used to open the thread (Reply / thread hint). */
  const [threadFocusEventId, setThreadFocusEventId] = React.useState<
    string | null
  >(null);
  const currentPubkey = currentIdentity?.pubkey;
  const activeChannelId = activeChannel?.id ?? null;

  const messagesQuery = useChannelMessagesQuery(activeChannel);
  useChannelSubscription(activeChannel);
  const { fetchOlder, hasOlderMessages, isFetchingOlder } =
    useFetchOlderMessages(activeChannel);
  const latestActiveMessage =
    messagesQuery.data?.[messagesQuery.data.length - 1] ?? null;
  const activeReadAt = latestActiveMessage
    ? new Date(latestActiveMessage.created_at * 1_000).toISOString()
    : (activeChannel?.lastMessageAt ?? null);

  React.useEffect(() => {
    if (!activeChannelId) {
      return;
    }

    onMarkChannelRead(activeChannelId, activeReadAt);
  }, [activeChannelId, activeReadAt, onMarkChannelRead]);

  const { activeChannelTitle, activeDmPresenceStatus } = useActiveChannelHeader(
    activeChannel,
    currentPubkey,
  );
  const sendMessageMutation = useSendMessageMutation(
    activeChannel,
    currentIdentity,
  );
  const toggleReactionMutation = useToggleReactionMutation();
  const deleteMessageMutation = useDeleteMessageMutation(activeChannel);
  const editMessageMutation = useEditMessageMutation(activeChannel);

  const resolvedMessages = React.useMemo(() => {
    const currentMessages = messagesQuery.data ?? [];

    if (
      !activeChannel ||
      !searchAnchorEvent ||
      searchAnchorChannelId !== activeChannel.id
    ) {
      return currentMessages;
    }

    return mergeMessages(currentMessages, searchAnchorEvent);
  }, [
    activeChannel,
    messagesQuery.data,
    searchAnchorChannelId,
    searchAnchorEvent,
  ]);
  const messageAuthorPubkeys = React.useMemo(
    () => collectMessageAuthorPubkeys(resolvedMessages),
    [resolvedMessages],
  );
  const latestMessageEvent = React.useMemo(
    () => resolvedMessages[resolvedMessages.length - 1] ?? null,
    [resolvedMessages],
  );
  const typingPubkeys = useChannelTyping(
    activeChannel,
    currentPubkey,
    latestMessageEvent,
  );
  const messageProfilePubkeys = React.useMemo(
    () => [...new Set([...messageAuthorPubkeys, ...typingPubkeys])],
    [messageAuthorPubkeys, typingPubkeys],
  );
  const messageProfilesQuery = useUsersBatchQuery(messageProfilePubkeys, {
    enabled: messageProfilePubkeys.length > 0,
  });
  const messageProfiles = React.useMemo(
    () =>
      mergeCurrentProfileIntoLookup(
        messageProfilesQuery.data?.profiles,
        currentProfile,
      ),
    [currentProfile, messageProfilesQuery.data?.profiles],
  );
  const timelineMessages = React.useMemo(
    () =>
      formatTimelineMessages(
        resolvedMessages,
        activeChannel,
        currentPubkey,
        currentProfile?.avatarUrl ?? null,
        messageProfiles,
      ),
    [
      activeChannel,
      currentProfile?.avatarUrl,
      currentPubkey,
      messageProfiles,
      resolvedMessages,
    ],
  );

  /**
   * Main channel: top-level posts plus **direct** replies to the thread root only
   * (`parentId === rootId` in NIP-10 e-tags). Nested replies (`parentId !== rootId`)
   * stay sidebar-only — they must not appear in the main stream even if depth is wrong.
   */
  const mainTimelineMessages = React.useMemo(
    () =>
      timelineMessages.filter((message) => {
        if (message.kind === KIND_SYSTEM_MESSAGE) {
          return true;
        }

        const { parentId, rootId } = message;
        if (!parentId) {
          return true;
        }

        if (rootId && parentId === rootId) {
          return true;
        }

        return false;
      }),
    [timelineMessages],
  );

  /**
   * Reply counts keyed by **main-timeline anchor** (prefer the direct reply to
   * the thread root — e.g. agent message — over the user's root post).
   */
  const threadHintsByAnchorId = React.useMemo(() => {
    const messageById = new Map<string, TimelineMessage>(
      timelineMessages.map((m) => [m.id, m]),
    );
    const mainIds = new Set(mainTimelineMessages.map((m) => m.id));

    const aggregates = new Map<
      string,
      { replyCount: number; pubkeys: string[] }
    >();

    for (const m of timelineMessages) {
      if (m.kind === KIND_SYSTEM_MESSAGE) {
        continue;
      }
      const { parentId, rootId } = m;
      if (!parentId || !rootId || parentId === rootId) {
        continue;
      }

      const anchorId = threadHintAnchorIdForNestedMessage(
        m,
        messageById,
        mainIds,
      );
      const agg = aggregates.get(anchorId) ?? { replyCount: 0, pubkeys: [] };
      agg.replyCount += 1;
      if (m.pubkey) {
        const pk = m.pubkey.toLowerCase();
        if (!agg.pubkeys.some((p) => p.toLowerCase() === pk)) {
          agg.pubkeys.push(m.pubkey);
        }
      }
      aggregates.set(anchorId, agg);
    }

    const out = new Map<string, ThreadConversationHint>();
    for (const [anchorId, agg] of aggregates) {
      const participantPubkeys = agg.pubkeys.slice(0, 5);
      const participantLabels = participantPubkeys.map((pk) =>
        resolveUserLabel({
          pubkey: pk,
          currentPubkey,
          profiles: messageProfiles,
          preferResolvedSelfLabel: true,
        }),
      );
      out.set(anchorId, {
        replyCount: agg.replyCount,
        participantPubkeys,
        participantLabels,
      });
    }
    return out;
  }, [currentPubkey, mainTimelineMessages, messageProfiles, timelineMessages]);
  const replyTargetMessage = React.useMemo(
    () =>
      timelineMessages.find((message) => message.id === replyTargetId) ?? null,
    [replyTargetId, timelineMessages],
  );
  const editTargetMessage = React.useMemo(
    () =>
      timelineMessages.find((message) => message.id === editTargetId) ?? null,
    [editTargetId, timelineMessages],
  );

  const {
    handleCancelEdit,
    handleCancelReply,
    handleDelete,
    handleEdit,
    handleEditSave,
    handleReply,
    handleToggleReaction,
  } = useChannelPaneHandlers({
    deleteMessageMutation,
    editMessageMutation,
    editTargetId,
    replyTargetId,
    sendMessageMutation,
    setEditTargetId,
    setReplyTargetId,
    toggleReactionMutation,
  });

  const handleReplyOpenThread = React.useCallback(
    (message: TimelineMessage) => {
      setThreadRootId(message.rootId ?? message.id);
      setThreadFocusEventId(message.id);
      handleReply(message);
    },
    [handleReply],
  );

  const handleCloseThread = React.useCallback(() => {
    setThreadRootId(null);
    setThreadFocusEventId(null);
    setReplyTargetId(null);
  }, []);

  /** Main channel composer — always posts to the channel timeline (not the open thread). */
  const handleSendChannel = React.useCallback(
    async (
      content: string,
      mentionPubkeys: string[],
      mediaTags?: string[][],
    ) => {
      const parentEventId = threadRootId ? null : replyTargetId;

      await sendMessageMutation.mutateAsync({
        content,
        mentionPubkeys,
        parentEventId,
        mediaTags,
      });

      if (!threadRootId) {
        setReplyTargetId(null);
      }
    },
    [replyTargetId, sendMessageMutation, threadRootId],
  );

  /** Thread sidebar composer — replies inside the active thread. */
  const handleSendThread = React.useCallback(
    async (
      content: string,
      mentionPubkeys: string[],
      mediaTags?: string[][],
    ) => {
      if (!threadRootId) {
        return;
      }

      // Prefer explicit reply chain, then the message that opened the thread (e.g. agent).
      // Never fall back to thread root alone when focus exists — that would create a depth-1
      // reply to the root and duplicate the message in the main channel.
      const parentEventId = replyTargetId ?? threadFocusEventId ?? threadRootId;

      const result = await sendMessageMutation.mutateAsync({
        content,
        mentionPubkeys,
        parentEventId,
        mediaTags,
      });

      setReplyTargetId(result.id);
      if (activeChannelId) {
        void queryClient.invalidateQueries({
          queryKey: channelThreadKey(activeChannelId, threadRootId),
        });
      }
    },
    [
      activeChannelId,
      queryClient,
      replyTargetId,
      sendMessageMutation,
      threadFocusEventId,
      threadRootId,
    ],
  );

  const canReact = activeChannel !== null && activeChannel.archivedAt === null;
  const effectiveToggleReaction = React.useMemo(
    () => (canReact ? handleToggleReaction : undefined),
    [canReact, handleToggleReaction],
  );

  const shouldLoadTimeline =
    activeChannel !== null && activeChannel.channelType !== "forum";
  const isTimelineLoading =
    shouldLoadTimeline &&
    (messagesQuery.isPending ||
      (messagesQuery.isFetching && resolvedMessages.length === 0));
  const requestedAncestorIdsRef = React.useRef<Set<string>>(new Set());
  const resetComposerTargets = React.useCallback(
    (_channelId: string | null) => {
      setReplyTargetId(null);
      setEditTargetId(null);
      setThreadRootId(null);
      setThreadFocusEventId(null);
    },
    [],
  );
  const resetRequestedAncestors = React.useCallback(
    (_channelId: string | null) => {
      requestedAncestorIdsRef.current.clear();
    },
    [],
  );

  React.useEffect(() => {
    resetComposerTargets(activeChannelId);
  }, [activeChannelId, resetComposerTargets]);

  React.useEffect(() => {
    // While the thread panel is open, keep replyTargetId even if the target is not in the
    // current timeline slice momentarily — clearing it would make thread sends fall back to
    // the thread root and surface replies in the main channel.
    if (replyTargetId && !replyTargetMessage && !threadRootId) {
      setReplyTargetId(null);
    }
    if (editTargetId && !editTargetMessage) {
      setEditTargetId(null);
    }
  }, [
    editTargetId,
    editTargetMessage,
    replyTargetId,
    replyTargetMessage,
    threadRootId,
  ]);

  React.useEffect(() => {
    resetRequestedAncestors(activeChannelId);
  }, [activeChannelId, resetRequestedAncestors]);

  React.useEffect(() => {
    if (!activeChannel || activeChannel.channelType === "forum") {
      return;
    }

    const knownEvents = new Map(
      resolvedMessages.map((message) => [message.id, message]),
    );
    const missingAncestorIds = new Set<string>();

    for (const message of resolvedMessages) {
      const thread = getThreadReference(message.tags);

      for (const eventId of [thread.parentId, thread.rootId]) {
        if (
          !eventId ||
          knownEvents.has(eventId) ||
          requestedAncestorIdsRef.current.has(eventId)
        ) {
          continue;
        }

        missingAncestorIds.add(eventId);
      }
    }

    if (missingAncestorIds.size === 0) {
      return;
    }

    for (const eventId of missingAncestorIds) {
      requestedAncestorIdsRef.current.add(eventId);
    }

    const maxRequestedAncestors = 500;
    if (requestedAncestorIdsRef.current.size > maxRequestedAncestors) {
      const excess =
        requestedAncestorIdsRef.current.size - maxRequestedAncestors;
      let removed = 0;
      for (const id of requestedAncestorIdsRef.current) {
        if (removed >= excess) {
          break;
        }
        requestedAncestorIdsRef.current.delete(id);
        removed++;
      }
    }

    let isCancelled = false;

    void Promise.all(
      [...missingAncestorIds].map(async (eventId) => {
        try {
          const event = await getEventById(eventId);

          if (
            isCancelled ||
            getChannelIdFromTags(event.tags) !== activeChannel.id
          ) {
            return;
          }

          queryClient.setQueryData<RelayEvent[]>(
            channelMessagesKey(activeChannel.id),
            (current = []) => mergeMessages(current, event),
          );
        } catch (error) {
          console.error("Failed to load ancestor event", eventId, error);
        }
      }),
    );

    return () => {
      isCancelled = true;
    };
  }, [activeChannel, queryClient, resolvedMessages]);

  return (
    <>
      <div className="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
        <ChatHeader
          actions={
            activeChannel ? (
              <ChannelMembersBar
                channel={activeChannel}
                currentPubkey={currentPubkey}
                onManageChannel={onManageChannel}
                onToggleMembers={() => setIsMembersSidebarOpen((prev) => !prev)}
              />
            ) : null
          }
          channelType={activeChannel?.channelType}
          visibility={activeChannel?.visibility}
          statusBadge={
            activeChannel?.channelType === "dm" && activeDmPresenceStatus ? (
              <PresenceBadge
                data-testid="chat-presence-badge"
                status={activeDmPresenceStatus}
              />
            ) : null
          }
          title={activeChannelTitle}
        />

        <div className="relative z-0 -mt-8 flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
          {activeChannel ? (
            activeChannel.channelType === "forum" ? (
              <React.Suspense
                fallback={<ViewLoadingFallback label="Loading forum..." />}
              >
                <ForumView
                  channel={activeChannel}
                  currentPubkey={currentPubkey}
                />
              </React.Suspense>
            ) : (
              <React.Suspense
                fallback={<ViewLoadingFallback label="Loading channel..." />}
              >
                <ChannelPane
                  activeChannel={activeChannel}
                  currentPubkey={currentPubkey}
                  fetchOlder={fetchOlder}
                  hasOlderMessages={hasOlderMessages}
                  isFetchingOlder={isFetchingOlder}
                  editTarget={
                    editTargetMessage
                      ? {
                          author: editTargetMessage.author,
                          body: editTargetMessage.body,
                          id: editTargetMessage.id,
                        }
                      : null
                  }
                  isSending={sendMessageMutation.isPending}
                  isTimelineLoading={isTimelineLoading}
                  messages={mainTimelineMessages}
                  onCancelEdit={handleCancelEdit}
                  onCancelReply={handleCancelReply}
                  onCloseThread={handleCloseThread}
                  onDelete={handleDelete}
                  onEdit={handleEdit}
                  onEditSave={handleEditSave}
                  onReply={handleReplyOpenThread}
                  onSendChannel={handleSendChannel}
                  onSendThread={handleSendThread}
                  onTargetReached={onTargetReached}
                  onToggleReaction={effectiveToggleReaction}
                  profiles={messageProfiles}
                  replyTargetId={replyTargetId}
                  replyTargetMessage={replyTargetMessage}
                  threadFocusEventId={threadFocusEventId}
                  threadHintsByAnchorId={threadHintsByAnchorId}
                  threadRootId={threadRootId}
                  targetMessageId={
                    activeChannel &&
                    searchAnchor?.channelId === activeChannel.id
                      ? searchAnchor.eventId
                      : null
                  }
                  typingPubkeys={typingPubkeys}
                />
              </React.Suspense>
            )
          ) : (
            <div className="flex min-h-0 flex-1 items-center justify-center px-6 py-8">
              <p className="text-sm text-muted-foreground">
                Select a channel to view messages.
              </p>
            </div>
          )}
        </div>
      </div>

      <MembersSidebar
        channel={activeChannel}
        currentPubkey={currentPubkey}
        open={isMembersSidebarOpen}
        onOpenChange={setIsMembersSidebarOpen}
      />
    </>
  );
}
