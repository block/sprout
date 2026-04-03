import * as React from "react";
import { useQueryClient } from "@tanstack/react-query";

import { ChannelPane } from "@/app/ChannelPane";
import { useActiveChannelHeader } from "@/app/useActiveChannelHeader";
import { useAncestorResolution } from "@/app/useAncestorResolution";
import { useChannelPaneHandlers } from "@/app/useChannelPaneHandlers";
import { useSearchSession } from "@/app/useSearchSession";
import { useViewRouter } from "@/app/useViewRouter";
import { AgentsView } from "@/features/agents/ui/AgentsView";
import { ForumView } from "@/features/forum/ui/ForumView";
import { WorkflowsView } from "@/features/workflows/ui/WorkflowsView";
import { ChatHeader } from "@/features/chat/ui/ChatHeader";
import {
  channelsQueryKey,
  useCreateChannelMutation,
  useHideDmMutation,
  useOpenDmMutation,
  useChannelsQuery,
  useSelectedChannel,
} from "@/features/channels/hooks";
import { useUnreadChannels } from "@/features/channels/useUnreadChannels";
import { ChannelMembersBar } from "@/features/channels/ui/ChannelMembersBar";
import { ChannelManagementSheet } from "@/features/channels/ui/ChannelManagementSheet";
import { HomeView } from "@/features/home/ui/HomeView";
import {
  useChannelMessagesQuery,
  mergeMessages,
  useDeleteMessageMutation,
  useEditMessageMutation,
  useSendMessageMutation,
  useChannelSubscription,
  useToggleReactionMutation,
} from "@/features/messages/hooks";

import { useFetchOlderMessages } from "@/features/messages/useFetchOlderMessages";
import {
  collectMessageAuthorPubkeys,
  formatTimelineMessages,
} from "@/features/messages/lib/formatTimelineMessages";
import { useChannelTyping } from "@/features/messages/useChannelTyping";

import { usePresenceSession } from "@/features/presence/hooks";
import { PresenceBadge } from "@/features/presence/ui/PresenceBadge";
import { useHomeFeedNotifications } from "@/features/notifications/hooks";
import { useProfileQuery, useUsersBatchQuery } from "@/features/profile/hooks";
import { mergeCurrentProfileIntoLookup } from "@/features/profile/lib/identity";
import { ChannelBrowserDialog } from "@/features/channels/ui/ChannelBrowserDialog";
import { SearchDialog } from "@/features/search/ui/SearchDialog";
import { SettingsView } from "@/features/settings/ui/SettingsView";
import { AppSidebar } from "@/features/sidebar/ui/AppSidebar";
import { relayClient } from "@/shared/api/relayClient";
import { joinChannel } from "@/shared/api/tauri";
import { useIdentityQuery } from "@/shared/api/hooks";
import type { Channel, SearchHit } from "@/shared/api/types";
import { ChannelNavigationProvider } from "@/shared/context/ChannelNavigationContext";
import {
  SidebarInset,
  SidebarProvider,
  SidebarTrigger,
} from "@/shared/ui/sidebar";
import { useWebviewZoomShortcuts } from "@/app/useWebviewZoomShortcuts";

export function AppShell() {
  useWebviewZoomShortcuts();
  const [isChannelManagementOpen, setIsChannelManagementOpen] =
    React.useState(false);
  const {
    isSearchOpen,
    setIsSearchOpen,
    searchAnchor,
    searchAnchorChannelId,
    searchAnchorEvent,
    handleOpenSearchResult: _handleOpenSearchResult,
    handleSearchTargetReached: handleTargetReached,
  } = useSearchSession();
  const {
    selectedView,
    setSelectedView,
    settingsSection,
    setSettingsSection,
    handleOpenSettings,
    handleCloseSettings: _handleCloseSettings,
  } = useViewRouter(setIsSearchOpen, setIsChannelManagementOpen);
  const [browseDialogType, setBrowseDialogType] = React.useState<
    "stream" | "forum" | null
  >(null);
  const handleBrowseDialogOpenChange = React.useCallback((open: boolean) => {
    setBrowseDialogType(open ? "stream" : null);
  }, []);
  const [replyTargetId, setReplyTargetId] = React.useState<string | null>(null);
  const [editTargetId, setEditTargetId] = React.useState<string | null>(null);
  const queryClient = useQueryClient();
  const selectView = React.useCallback(
    (view: AppView | MainView) => {
      React.startTransition(() => {
        setSelectedView(view);
      });
    },
    [setSelectedView],
  );
  const identityQuery = useIdentityQuery();
  const profileQuery = useProfileQuery();
  const presenceSession = usePresenceSession(identityQuery.data?.pubkey);
  const { homeBadgeCount, homeFeedQuery, notificationSettings } =
    useHomeFeedNotifications(
      identityQuery.data?.pubkey,
      selectedView === "home",
    );
  const channelsQuery = useChannelsQuery();
  const { refetch: refetchChannels } = channelsQuery;
  const channels = channelsQuery.data ?? [];
  const memberChannels = React.useMemo(
    () => channels.filter((channel) => channel.isMember),
    [channels],
  );
  const { selectedChannel, setSelectedChannelId } = useSelectedChannel(
    channels,
    null,
  );
  const createChannelMutation = useCreateChannelMutation();
  const createForumMutation = useCreateChannelMutation();
  const openDmMutation = useOpenDmMutation();
  const hideDmMutation = useHideDmMutation();
  const activeChannel = selectedView === "channel" ? selectedChannel : null;
  const activeChannelId = activeChannel?.id ?? null;
  const messagesQuery = useChannelMessagesQuery(activeChannel);
  useChannelSubscription(activeChannel);
  const { fetchOlder, isFetchingOlder, hasOlderMessages } =
    useFetchOlderMessages(activeChannel);
  const latestActiveMessage =
    messagesQuery.data?.[messagesQuery.data.length - 1];
  const activeReadAt = latestActiveMessage
    ? new Date(latestActiveMessage.created_at * 1_000).toISOString()
    : (activeChannel?.lastMessageAt ?? null);
  const { unreadChannelIds } = useUnreadChannels(
    channels,
    activeChannel,
    activeReadAt,
  );
  const { activeChannelTitle, activeDmPresenceStatus } = useActiveChannelHeader(
    activeChannel,
    identityQuery.data?.pubkey,
  );
  const sendMessageMutation = useSendMessageMutation(
    activeChannel,
    identityQuery.data,
  );
  const toggleReactionMutation = useToggleReactionMutation();
  const deleteMessageMutation = useDeleteMessageMutation(activeChannel);
  const editMessageMutation = useEditMessageMutation(activeChannel);
  const availableChannelIds = React.useMemo(
    () => new Set(channels.map((channel) => channel.id)),
    [channels],
  );
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
    identityQuery.data?.pubkey,
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
        profileQuery.data,
      ),
    [messageProfilesQuery.data?.profiles, profileQuery.data],
  );
  const timelineMessages = React.useMemo(
    () =>
      formatTimelineMessages(
        resolvedMessages,
        activeChannel,
        identityQuery.data?.pubkey,
        profileQuery.data?.avatarUrl ?? null,
        messageProfiles,
      ),
    [
      activeChannel,
      identityQuery.data?.pubkey,
      messageProfiles,
      profileQuery.data?.avatarUrl,
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

  const previousActiveChannelIdRef = React.useRef<string | null>(
    activeChannelId,
  );

  useAncestorResolution(activeChannel, resolvedMessages);

  const resolveChannel = React.useCallback(
    async (channelId: string): Promise<Channel | null> => {
      const cachedChannels =
        queryClient.getQueryData<Channel[]>(channelsQueryKey);
      const knownChannel =
        channels.find((channel) => channel.id === channelId) ??
        cachedChannels?.find((channel) => channel.id === channelId) ??
        null;

      if (knownChannel) {
        return knownChannel;
      }

      const refreshed = await refetchChannels();
      return (
        refreshed.data?.find((channel) => channel.id === channelId) ?? null
      );
    },
    [channels, queryClient, refetchChannels],
  );
  const openChannelView = React.useCallback(
    (channelId: string) => {
      React.startTransition(() => {
        setSelectedChannelId(channelId);
        setSelectedView("channel");
      });
    },
    [setSelectedChannelId, setSelectedView],
  );

  const handleOpenChannel = React.useCallback(
    async (channelId: string) => {
      try {
        const channel = await resolveChannel(channelId);
        if (!channel) {
          console.error("Failed to resolve channel before opening", channelId);
          return;
        }

        openChannelView(channel.id);
      } catch (error) {
        console.error("Failed to open channel", channelId, error);
      }
    },
    [openChannelView, resolveChannel],
  );

  const handleBrowseChannelJoin = React.useCallback(
    async (channelId: string) => {
      await joinChannel(channelId);
      await queryClient.invalidateQueries({ queryKey: channelsQueryKey });
    },
    [queryClient],
  );

  const handleHideDm = React.useCallback(
    async (channelId: string) => {
      try {
        await hideDmMutation.mutateAsync(channelId);
      } catch {
        // Optimistic rollback handled by onError in the mutation hook.
        return;
      }
      if (selectedChannel?.id === channelId) {
        selectView("home");
      }
    },
    [hideDmMutation, selectView, selectedChannel?.id],
  );

  const handleOpenSearchResult = React.useCallback(
    (hit: SearchHit) => {
      _handleOpenSearchResult(hit, handleOpenChannel);
    },
    [_handleOpenSearchResult, handleOpenChannel],
  );

  const handleCloseSettings = React.useCallback(() => {
    _handleCloseSettings(selectedChannel);
  }, [_handleCloseSettings, selectedChannel]);

  React.useEffect(() => {
    let isCancelled = false;

    void relayClient.preconnect().catch((error) => {
      if (!isCancelled) {
        console.error("Failed to preconnect to relay", error);
      }
    });

    return () => {
      isCancelled = true;
    };
  }, []);
  React.useEffect(() => {
    if (previousActiveChannelIdRef.current === activeChannelId) {
      return;
    }
    previousActiveChannelIdRef.current = activeChannelId;
    setReplyTargetId(null);
  }, [activeChannelId]);
  React.useEffect(() => {
    if (replyTargetId && !replyTargetMessage) {
      setReplyTargetId(null);
    }
    if (editTargetId && !editTargetMessage) {
      setEditTargetId(null);
    }
  }, [editTargetId, editTargetMessage, replyTargetId, replyTargetMessage]);
  React.useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      const isSettingsShortcut =
        (event.key === "," || event.code === "Comma") &&
        (event.metaKey || event.ctrlKey) &&
        !event.altKey &&
        !event.shiftKey;

      if (!isSettingsShortcut) {
        return;
      }

      event.preventDefault();
      if (selectedView === "settings") {
        handleCloseSettings();
        return;
      }

      handleOpenSettings();
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [handleCloseSettings, handleOpenSettings, selectedView]);

  return (
    <ChannelNavigationProvider
      channels={channels}
      onOpenChannel={handleOpenChannel}
    >
      <SidebarProvider className="h-dvh overflow-hidden overscroll-none">
        {selectedView === "settings" ? (
          <div className="flex min-h-0 min-w-0 flex-1 overflow-hidden">
            <SettingsView
              currentPubkey={identityQuery.data?.pubkey}
              fallbackDisplayName={identityQuery.data?.displayName}
              isUpdatingDesktopNotifications={
                notificationSettings.isUpdatingDesktopEnabled
              }
              isPresenceLoading={presenceSession.isLoading}
              isUpdatingPresence={presenceSession.isPending}
              notificationErrorMessage={notificationSettings.errorMessage}
              notificationPermission={notificationSettings.permission}
              notificationSettings={notificationSettings.settings}
              onClose={handleCloseSettings}
              onSectionChange={setSettingsSection}
              onSetDesktopNotificationsEnabled={
                notificationSettings.setDesktopEnabled
              }
              onSetHomeBadgeEnabled={notificationSettings.setHomeBadgeEnabled}
              onSetMentionNotificationsEnabled={
                notificationSettings.setMentionsEnabled
              }
              onSetNeedsActionNotificationsEnabled={
                notificationSettings.setNeedsActionEnabled
              }
              onSetPresence={presenceSession.setStatus}
              presenceError={presenceSession.error}
              presenceStatus={presenceSession.currentStatus}
              section={settingsSection}
            />
          </div>
        ) : (
          <React.Fragment>
            <SidebarTrigger className="fixed left-[80px] top-[8px] z-50 h-6 w-6 text-muted-foreground/70 hover:bg-muted/60 hover:text-foreground" />
            <AppSidebar
              channels={memberChannels}
              currentPubkey={identityQuery.data?.pubkey}
              errorMessage={
                channelsQuery.error instanceof Error
                  ? channelsQuery.error.message
                  : undefined
              }
              fallbackDisplayName={identityQuery.data?.displayName}
              homeBadgeCount={homeBadgeCount}
              isCreatingChannel={createChannelMutation.isPending}
              isCreatingForum={createForumMutation.isPending}
              isLoading={channelsQuery.isLoading}
              isOpeningDm={openDmMutation.isPending}
              selfPresenceStatus={presenceSession.currentStatus}
              onCreateChannel={async ({ description, name, visibility }) => {
                const createdChannel = await createChannelMutation.mutateAsync({
                  name,
                  description,
                  channelType: "stream",
                  visibility,
                });

                openChannelView(createdChannel.id);
              }}
              onCreateForum={async ({ description, name, visibility }) => {
                const createdForum = await createForumMutation.mutateAsync({
                  name,
                  description,
                  channelType: "forum",
                  visibility,
                });

                openChannelView(createdForum.id);
              }}
              onOpenBrowseChannels={() => {
                setBrowseDialogType("stream");
                void refetchChannels();
              }}
              onOpenBrowseForums={() => {
                setBrowseDialogType("forum");
                void refetchChannels();
              }}
              onOpenSearch={() => {
                setIsSearchOpen(true);
                void refetchChannels();
              }}
              onHideDm={handleHideDm}
              onOpenDm={async ({ pubkeys }) => {
                const directMessage = await openDmMutation.mutateAsync({
                  pubkeys,
                });
                openChannelView(directMessage.id);
              }}
              onSelectAgents={() => selectView("agents")}
              onSelectWorkflows={() => selectView("workflows")}
              onSelectHome={() => {
                selectView("home");
                void homeFeedQuery.refetch();
              }}
              onSelectChannel={handleOpenChannel}
              onSelectSettings={handleOpenSettings}
              profile={profileQuery.data}
              selectedChannelId={selectedChannel?.id ?? null}
              selectedView={selectedView}
              unreadChannelIds={unreadChannelIds}
            />

            <SidebarInset className="min-h-0 min-w-0 overflow-hidden">
              {selectedView === "home" ? (
                <ChatHeader
                  description="Personalized feed for mentions, reminders, channel activity, and agent work."
                  mode="home"
                  title="Home"
                />
              ) : selectedView === "agents" ? (
                <ChatHeader
                  description="Create local ACP workers, mint agent tokens, and monitor the relay-visible agent directory."
                  mode="agents"
                  title="Agents"
                />
              ) : selectedView === "workflows" ? (
                <ChatHeader
                  description="Create, manage, and monitor automated workflows across your channels."
                  mode="workflows"
                  title="Workflows"
                />
              ) : (
                <ChatHeader
                  actions={
                    activeChannel ? (
                      <ChannelMembersBar
                        channel={activeChannel}
                        currentPubkey={identityQuery.data?.pubkey}
                        onManageChannel={() => {
                          setIsChannelManagementOpen(true);
                        }}
                      />
                    ) : null
                  }
                  channelType={activeChannel?.channelType}
                  visibility={activeChannel?.visibility}
                  description={channelDescription}
                  statusBadge={
                    activeChannel?.channelType === "dm" &&
                    activeDmPresenceStatus ? (
                      <PresenceBadge
                        data-testid="chat-presence-badge"
                        status={activeDmPresenceStatus}
                      />
                    ) : null
                  }
                  title={activeChannelTitle}
                />
              )}

              <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
                <div
                  className={
                    selectedView === "home"
                      ? "flex min-h-0 flex-1 flex-col"
                      : "hidden"
                  }
                >
                  <HomeView
                    availableChannelIds={availableChannelIds}
                    currentPubkey={identityQuery.data?.pubkey}
                    errorMessage={
                      homeFeedQuery.error instanceof Error
                        ? homeFeedQuery.error.message
                        : undefined
                    }
                    feed={homeFeedQuery.data}
                    isLoading={homeFeedQuery.isLoading}
                    onOpenChannel={handleOpenChannel}
                    onRefresh={() => {
                      void homeFeedQuery.refetch();
                    }}
                  />
                </div>
                <div
                  className={
                    selectedView === "agents"
                      ? "flex min-h-0 flex-1 flex-col"
                      : "hidden"
                  }
                >
                  <AgentsView />
                </div>
                <div
                  className={
                    selectedView === "workflows"
                      ? "flex min-h-0 flex-1 flex-col"
                      : "hidden"
                  }
                >
                  <WorkflowsView channels={memberChannels} />
                </div>
                <div
                  className={
                    selectedView !== "home" &&
                    selectedView !== "agents" &&
                    selectedView !== "workflows"
                      ? "flex min-h-0 flex-1 flex-col overflow-hidden"
                      : "hidden"
                  }
                >
                  {activeChannel?.channelType === "forum" ? (
                    <ForumView
                      channel={activeChannel}
                      currentPubkey={identityQuery.data?.pubkey}
                    />
                  ) : (
                    <ChannelPane
                      activeChannel={activeChannel}
                      currentPubkey={identityQuery.data?.pubkey}
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
                      onTargetReached={handleTargetReached}
                      onToggleReaction={effectiveToggleReaction}
                      profiles={messageProfiles}
                      replyTargetId={replyTargetId}
                      replyTargetMessage={replyTargetMessage}
                      targetMessageId={
                        activeChannel &&
                        searchAnchor?.channelId === activeChannel.id
                          ? searchAnchor.eventId
                          : null
                      }
                      typingPubkeys={typingPubkeys}
                    />
                  )}
                </div>
              </div>
            </SidebarInset>
          </React.Fragment>
        )}

        <ChannelBrowserDialog
          channels={channels}
          channelTypeFilter={browseDialogType ?? "stream"}
          onJoinChannel={handleBrowseChannelJoin}
          onOpenChange={handleBrowseDialogOpenChange}
          onSelectChannel={handleOpenChannel}
          open={browseDialogType !== null}
        />

        <SearchDialog
          channels={channels}
          currentPubkey={identityQuery.data?.pubkey}
          onOpenResult={handleOpenSearchResult}
          onOpenChange={setIsSearchOpen}
          open={isSearchOpen}
        />

        <ChannelManagementSheet
          channel={activeChannel}
          currentPubkey={identityQuery.data?.pubkey}
          onDeleted={() => {
            setIsChannelManagementOpen(false);
            selectView("home");
          }}
          onOpenChange={setIsChannelManagementOpen}
          open={isChannelManagementOpen && activeChannel !== null}
        />
      </SidebarProvider>
    </ChannelNavigationProvider>
  );
}
