import * as React from "react";
import { useQueryClient } from "@tanstack/react-query";

import { useAppShell } from "@/app/AppShellContext";
import { ChatHeader } from "@/features/chat/ui/ChatHeader";
import { useActiveChannelHeader } from "@/features/channels/useActiveChannelHeader";
import { useChannelPaneHandlers } from "@/features/channels/useChannelPaneHandlers";
import { useChannelMembersQuery } from "@/features/channels/hooks";
import { ChannelMembersBar } from "@/features/channels/ui/ChannelMembersBar";
import { EphemeralChannelBadge } from "@/features/channels/ui/EphemeralChannelBadge";
import { MembersSidebar } from "@/features/channels/ui/MembersSidebar";
import {
  useManagedAgentsQuery,
  usePersonasQuery,
} from "@/features/agents/hooks";
import {
  mergeMessages,
  useChannelMessagesQuery,
  useChannelSubscription,
  useDeleteMessageMutation,
  useEditMessageMutation,
  useSendMessageMutation,
  useToggleReactionMutation,
} from "@/features/messages/hooks";
import { channelMessagesKey } from "@/features/messages/lib/messageQueryKeys";
import {
  collectMessageAuthorPubkeys,
  formatTimelineMessages,
} from "@/features/messages/lib/formatTimelineMessages";
import {
  getChannelIdFromTags,
  getThreadReference,
} from "@/features/messages/lib/threading";
import { useFetchOlderMessages } from "@/features/messages/useFetchOlderMessages";
import { useChannelTyping } from "@/features/messages/useChannelTyping";
import { PresenceBadge } from "@/features/presence/ui/PresenceBadge";
import { useUsersBatchQuery } from "@/features/profile/hooks";
import { mergeCurrentProfileIntoLookup } from "@/features/profile/lib/identity";
import { getEventById } from "@/shared/api/tauri";
import type {
  Channel,
  Identity,
  Profile,
  RelayEvent,
} from "@/shared/api/types";
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
  onCloseForumPost: () => void;
  onSelectForumPost: (postId: string) => void;
  selectedForumPostId: string | null;
  targetForumReplyId: string | null;
  targetMessageEvent: RelayEvent | null;
  targetMessageId: string | null;
};

export function ChannelScreen({
  activeChannel,
  currentIdentity,
  currentProfile,
  onCloseForumPost,
  onSelectForumPost,
  selectedForumPostId,
  targetForumReplyId,
  targetMessageEvent,
  targetMessageId,
}: ChannelScreenProps) {
  const queryClient = useQueryClient();
  const { markChannelRead, openChannelManagement } = useAppShell();
  const [isMembersSidebarOpen, setIsMembersSidebarOpen] = React.useState(false);
  const [replyTargetId, setReplyTargetId] = React.useState<string | null>(null);
  const [editTargetId, setEditTargetId] = React.useState<string | null>(null);
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

    markChannelRead(activeChannelId, activeReadAt);
  }, [activeChannelId, activeReadAt, markChannelRead]);

  const {
    activeChannelTitle,
    activeDmPresenceStatus,
    activeChannelEphemeralDisplay,
  } = useActiveChannelHeader(activeChannel, currentPubkey);
  const sendMessageMutation = useSendMessageMutation(
    activeChannel,
    currentIdentity,
  );
  const toggleReactionMutation = useToggleReactionMutation();
  const deleteMessageMutation = useDeleteMessageMutation(activeChannel);
  const editMessageMutation = useEditMessageMutation(activeChannel);

  const resolvedMessages = React.useMemo(() => {
    const currentMessages = messagesQuery.data ?? [];

    if (!activeChannel || !targetMessageEvent) {
      return currentMessages;
    }

    return mergeMessages(currentMessages, targetMessageEvent);
  }, [activeChannel, messagesQuery.data, targetMessageEvent]);
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
  const managedAgentsQuery = useManagedAgentsQuery();
  const messageProfiles = React.useMemo(() => {
    const base =
      mergeCurrentProfileIntoLookup(
        messageProfilesQuery.data?.profiles,
        currentProfile,
      ) ?? {};
    // Merge managed agent names so system messages resolve instantly
    // (without waiting for the relay profile batch query).
    const agents = managedAgentsQuery.data ?? [];
    const merged = { ...base };
    for (const agent of agents) {
      const key = agent.pubkey.toLowerCase();
      if (!merged[key]?.displayName) {
        merged[key] = {
          ...merged[key],
          displayName: agent.name,
          avatarUrl: null,
          nip05Handle: null,
        };
      }
    }
    return merged;
  }, [
    currentProfile,
    managedAgentsQuery.data,
    messageProfilesQuery.data?.profiles,
  ]);
  const channelMembersQuery = useChannelMembersQuery(activeChannel?.id ?? null);
  const channelMembers = channelMembersQuery.data;
  const personasQuery = usePersonasQuery();
  const personaLookup = React.useMemo(() => {
    const agents = managedAgentsQuery.data ?? [];
    const personas = personasQuery.data ?? [];
    const personaById = new Map(personas.map((p) => [p.id, p.displayName]));
    const lookup = new Map<string, string>();
    for (const agent of agents) {
      if (agent.personaId) {
        const personaName = personaById.get(agent.personaId);
        if (personaName) {
          lookup.set(agent.pubkey.toLowerCase(), personaName);
        }
      }
    }
    return lookup;
  }, [managedAgentsQuery.data, personasQuery.data]);
  const timelineMessages = React.useMemo(
    () =>
      formatTimelineMessages(
        resolvedMessages,
        activeChannel,
        currentPubkey,
        currentProfile?.avatarUrl ?? null,
        messageProfiles,
        channelMembers,
        personaLookup,
      ),
    [
      activeChannel,
      channelMembers,
      currentProfile?.avatarUrl,
      currentPubkey,
      messageProfiles,
      personaLookup,
      resolvedMessages,
    ],
  );
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
    handleSend,
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

  const canReact = activeChannel !== null && activeChannel.archivedAt === null;
  const effectiveToggleReaction = React.useMemo(
    () => (canReact ? handleToggleReaction : undefined),
    [canReact, handleToggleReaction],
  );

  const channelDescription = activeChannel
    ? [
        activeChannel.archivedAt ? "Archived." : null,
        !activeChannel.isMember
          ? "Read-only until you join this open channel."
          : null,
        activeChannel.topic,
        activeChannel.description,
        activeChannel.purpose,
        null,
      ]
        .filter((value) => value && value.trim().length > 0)
        .join(" ") || "Channel details and activity."
    : "Connect to the relay to browse channels and read messages.";
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
    if (replyTargetId && !replyTargetMessage) {
      setReplyTargetId(null);
    }
    if (editTargetId && !editTargetMessage) {
      setEditTargetId(null);
    }
  }, [editTargetId, editTargetMessage, replyTargetId, replyTargetMessage]);

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

  const activeChannelEphemeralBadge = activeChannelEphemeralDisplay ? (
    <EphemeralChannelBadge
      display={activeChannelEphemeralDisplay}
      testId="chat-ephemeral-badge"
      variant="header"
    />
  ) : null;

  const headerStatusBadge =
    activeChannel?.channelType === "dm" && activeDmPresenceStatus ? (
      <>
        <PresenceBadge
          data-testid="chat-presence-badge"
          status={activeDmPresenceStatus}
        />
        {activeChannelEphemeralBadge}
      </>
    ) : (
      activeChannelEphemeralBadge
    );

  return (
    <>
      <ChatHeader
        actions={
          activeChannel ? (
            <ChannelMembersBar
              channel={activeChannel}
              currentPubkey={currentPubkey}
              onManageChannel={openChannelManagement}
              onToggleMembers={() => setIsMembersSidebarOpen((prev) => !prev)}
            />
          ) : null
        }
        channelType={activeChannel?.channelType}
        visibility={activeChannel?.visibility}
        description={channelDescription}
        statusBadge={headerStatusBadge}
        title={activeChannelTitle}
      />

      <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
        {activeChannel ? (
          activeChannel.channelType === "forum" ? (
            <React.Suspense fallback={<ViewLoadingFallback kind="forum" />}>
              <ForumView
                channel={activeChannel}
                currentPubkey={currentPubkey}
                onClosePost={onCloseForumPost}
                onSelectPost={onSelectForumPost}
                selectedPostId={selectedForumPostId}
                targetReplyId={targetForumReplyId}
              />
            </React.Suspense>
          ) : (
            <React.Suspense fallback={<ViewLoadingFallback kind="channel" />}>
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
                messages={timelineMessages}
                onCancelEdit={handleCancelEdit}
                onCancelReply={handleCancelReply}
                onDelete={handleDelete}
                onEdit={handleEdit}
                onEditSave={handleEditSave}
                onReply={handleReply}
                onSend={handleSend}
                onToggleReaction={effectiveToggleReaction}
                personaLookup={personaLookup}
                profiles={messageProfiles}
                replyTargetId={replyTargetId}
                replyTargetMessage={replyTargetMessage}
                targetMessageId={targetMessageId}
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

      <MembersSidebar
        channel={activeChannel}
        currentPubkey={currentPubkey}
        open={isMembersSidebarOpen}
        onOpenChange={setIsMembersSidebarOpen}
      />
    </>
  );
}
