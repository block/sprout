import * as React from "react";
import { useQueryClient } from "@tanstack/react-query";

import { ChannelPane } from "@/app/ChannelPane";
import { AgentsView } from "@/features/agents/ui/AgentsView";
import { ChatHeader } from "@/features/chat/ui/ChatHeader";
import {
  channelsQueryKey,
  useCreateChannelMutation,
  useChannelsQuery,
  useSelectedChannel,
} from "@/features/channels/hooks";
import { useUnreadChannels } from "@/features/channels/useUnreadChannels";
import { ChannelMembersBar } from "@/features/channels/ui/ChannelMembersBar";
import { ChannelManagementSheet } from "@/features/channels/ui/ChannelManagementSheet";
import { useHomeFeedQuery } from "@/features/home/hooks";
import { HomeView } from "@/features/home/ui/HomeView";
import {
  useChannelMessagesQuery,
  mergeMessages,
  useSendMessageMutation,
  useChannelSubscription,
  useToggleReactionMutation,
} from "@/features/messages/hooks";
import {
  collectMessageAuthorPubkeys,
  formatTimelineMessages,
} from "@/features/messages/lib/formatTimelineMessages";
import { useChannelTyping } from "@/features/messages/useChannelTyping";
import {
  getChannelIdFromTags,
  getThreadReference,
} from "@/features/messages/lib/threading";
import {
  usePresenceQuery,
  usePresenceSession,
} from "@/features/presence/hooks";
import { PresenceBadge } from "@/features/presence/ui/PresenceBadge";
import { useProfileQuery, useUsersBatchQuery } from "@/features/profile/hooks";
import { ChannelBrowserDialog } from "@/features/channels/ui/ChannelBrowserDialog";
import { SearchDialog } from "@/features/search/ui/SearchDialog";
import {
  DEFAULT_SETTINGS_SECTION,
  SettingsView,
  type SettingsSection,
} from "@/features/settings/ui/SettingsView";
import { AppSidebar } from "@/features/sidebar/ui/AppSidebar";
import { relayClient } from "@/shared/api/relayClient";
import { getEventById, joinChannel } from "@/shared/api/tauri";
import { useIdentityQuery } from "@/shared/api/hooks";
import type { Channel, RelayEvent, SearchHit } from "@/shared/api/types";
import {
  SidebarInset,
  SidebarProvider,
  SidebarTrigger,
} from "@/shared/ui/sidebar";

type AppView = "home" | "channel" | "settings" | "agents";
type MainView = Exclude<AppView, "settings">;

export function AppShell() {
  const [selectedView, setSelectedView] = React.useState<AppView>("home");
  const [settingsSection, setSettingsSection] = React.useState<SettingsSection>(
    DEFAULT_SETTINGS_SECTION,
  );
  const [isChannelManagementOpen, setIsChannelManagementOpen] =
    React.useState(false);
  const [isSearchOpen, setIsSearchOpen] = React.useState(false);
  const [isBrowseChannelsOpen, setIsBrowseChannelsOpen] = React.useState(false);
  const [searchAnchor, setSearchAnchor] = React.useState<SearchHit | null>(
    null,
  );
  const [searchAnchorChannelId, setSearchAnchorChannelId] = React.useState<
    string | null
  >(null);
  const [searchAnchorEvent, setSearchAnchorEvent] =
    React.useState<RelayEvent | null>(null);
  const [replyTargetId, setReplyTargetId] = React.useState<string | null>(null);
  const lastNonSettingsViewRef = React.useRef<MainView>("home");
  const queryClient = useQueryClient();
  const identityQuery = useIdentityQuery();
  const profileQuery = useProfileQuery();
  const presenceSession = usePresenceSession(identityQuery.data?.pubkey);
  const homeFeedQuery = useHomeFeedQuery();
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
  const activeChannel = selectedView === "channel" ? selectedChannel : null;
  const activeChannelId = activeChannel?.id ?? null;
  const { unreadChannelIds } = useUnreadChannels(channels, activeChannel);
  const activeDmParticipantPubkeys = React.useMemo(() => {
    if (!activeChannel || activeChannel.channelType !== "dm") {
      return [];
    }

    const currentPubkey = identityQuery.data?.pubkey?.toLowerCase();

    return activeChannel.participantPubkeys.filter(
      (pubkey) => pubkey.toLowerCase() !== currentPubkey,
    );
  }, [activeChannel, identityQuery.data?.pubkey]);
  const activeDmPresenceQuery = usePresenceQuery(activeDmParticipantPubkeys, {
    enabled: activeDmParticipantPubkeys.length > 0,
  });
  const activeDmPresenceStatus =
    activeDmParticipantPubkeys.length > 0
      ? activeDmPresenceQuery.data?.[
          activeDmParticipantPubkeys[0]?.toLowerCase()
        ]
      : null;

  const messagesQuery = useChannelMessagesQuery(activeChannel);
  useChannelSubscription(activeChannel);

  const sendMessageMutation = useSendMessageMutation(
    activeChannel,
    identityQuery.data,
  );
  const toggleReactionMutation = useToggleReactionMutation();
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

  const timelineMessages = React.useMemo(
    () =>
      formatTimelineMessages(
        resolvedMessages,
        activeChannel,
        identityQuery.data?.pubkey,
        profileQuery.data?.avatarUrl ?? null,
        messageProfilesQuery.data?.profiles,
      ),
    [
      activeChannel,
      identityQuery.data?.pubkey,
      profileQuery.data?.avatarUrl,
      messageProfilesQuery.data?.profiles,
      resolvedMessages,
    ],
  );
  const replyTargetMessage = React.useMemo(
    () =>
      timelineMessages.find((message) => message.id === replyTargetId) ?? null,
    [replyTargetId, timelineMessages],
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
        activeChannel.channelType === "forum"
          ? "Forum channels are listed, but this first pass only wires message streams and DMs."
          : null,
      ]
        .filter((value) => value && value.trim().length > 0)
        .join(" ") || "Channel details and activity."
    : "Connect to the relay to browse channels and read messages.";
  const contentPaneKey =
    selectedView === "home"
      ? "home"
      : selectedView === "agents"
        ? "agents"
        : selectedView === "settings"
          ? "settings"
          : `channel:${activeChannel?.id ?? "none"}`;
  const shouldLoadTimeline =
    activeChannel !== null && activeChannel.channelType !== "forum";
  const isTimelineLoading =
    shouldLoadTimeline &&
    (messagesQuery.isPending ||
      (messagesQuery.isFetching && resolvedMessages.length === 0));

  const requestedAncestorIdsRef = React.useRef<Set<string>>(new Set());
  const previousActiveChannelIdRef = React.useRef<string | null>(
    activeChannelId,
  );

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

  const handleOpenChannel = React.useCallback(
    async (channelId: string) => {
      try {
        const channel = await resolveChannel(channelId);
        if (!channel) {
          console.error("Failed to resolve channel before opening", channelId);
          return;
        }

        React.startTransition(() => {
          setSelectedChannelId(channel.id);
          setSelectedView("channel");
        });
      } catch (error) {
        console.error("Failed to open channel", channelId, error);
      }
    },
    [resolveChannel, setSelectedChannelId],
  );

  const handleBrowseChannelJoin = React.useCallback(
    async (channelId: string) => {
      await joinChannel(channelId);
      await queryClient.invalidateQueries({ queryKey: channelsQueryKey });
    },
    [queryClient],
  );

  const handleOpenSettings = React.useCallback(
    (section: SettingsSection = DEFAULT_SETTINGS_SECTION) => {
      setIsSearchOpen(false);
      setIsChannelManagementOpen(false);
      setSettingsSection(section);

      React.startTransition(() => {
        setSelectedView("settings");
      });
    },
    [],
  );

  const handleCloseSettings = React.useCallback(() => {
    const nextView: MainView =
      lastNonSettingsViewRef.current === "channel" && !selectedChannel
        ? "home"
        : lastNonSettingsViewRef.current;

    React.startTransition(() => {
      setSelectedView(nextView);
    });
  }, [selectedChannel]);

  const handleOpenSearchResult = React.useCallback(
    (hit: SearchHit) => {
      setSearchAnchor(hit);
      setSearchAnchorChannelId(hit.channelId);
      setSearchAnchorEvent({
        id: hit.eventId,
        pubkey: hit.pubkey,
        created_at: hit.createdAt,
        kind: hit.kind,
        tags: [["h", hit.channelId]],
        content: hit.content,
        sig: "",
      });
      void handleOpenChannel(hit.channelId);

      void getEventById(hit.eventId)
        .then((event) => {
          setSearchAnchorEvent((current) => {
            if (current?.id !== hit.eventId) {
              return current;
            }

            return event;
          });
        })
        .catch((error) => {
          console.error(
            "Failed to load search result event",
            hit.eventId,
            error,
          );
        });
    },
    [handleOpenChannel],
  );

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
    requestedAncestorIdsRef.current.clear();
  }, [activeChannelId]);

  React.useEffect(() => {
    if (selectedView === "settings") {
      return;
    }

    lastNonSettingsViewRef.current = selectedView;
  }, [selectedView]);

  React.useEffect(() => {
    if (replyTargetId && !replyTargetMessage) {
      setReplyTargetId(null);
    }
  }, [replyTargetId, replyTargetMessage]);

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
            ["channel-messages", activeChannel.id],
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
    <SidebarProvider className="h-dvh overflow-hidden overscroll-none">
      {selectedView === "settings" ? (
        <div className="flex min-h-0 min-w-0 flex-1 overflow-hidden">
          <SettingsView
            currentPubkey={identityQuery.data?.pubkey}
            fallbackDisplayName={identityQuery.data?.displayName}
            isPresenceLoading={presenceSession.isLoading}
            isUpdatingPresence={presenceSession.isPending}
            onClose={handleCloseSettings}
            onSectionChange={setSettingsSection}
            onSetPresence={presenceSession.setStatus}
            presenceError={presenceSession.error}
            presenceStatus={presenceSession.currentStatus}
            section={settingsSection}
          />
        </div>
      ) : (
        <React.Fragment>
          <SidebarTrigger className="fixed left-[80px] top-[9px] z-50 h-6 w-6 text-muted-foreground/70 hover:bg-muted/60 hover:text-foreground" />
          <AppSidebar
            channels={memberChannels}
            currentPubkey={identityQuery.data?.pubkey}
            errorMessage={
              channelsQuery.error instanceof Error
                ? channelsQuery.error.message
                : undefined
            }
            fallbackDisplayName={identityQuery.data?.displayName}
            isCreatingChannel={createChannelMutation.isPending}
            isLoading={channelsQuery.isLoading}
            selfPresenceStatus={presenceSession.currentStatus}
            onCreateChannel={async ({ description, name }) => {
              const createdChannel = await createChannelMutation.mutateAsync({
                name,
                description,
                channelType: "stream",
                visibility: "open",
              });

              React.startTransition(() => {
                setSelectedChannelId(createdChannel.id);
                setSelectedView("channel");
              });
            }}
            onOpenBrowseChannels={() => {
              setIsBrowseChannelsOpen(true);
              void refetchChannels();
            }}
            onOpenSearch={() => {
              setIsSearchOpen(true);
              void refetchChannels();
            }}
            onSelectAgents={() => {
              React.startTransition(() => {
                setSelectedView("agents");
              });
            }}
            onSelectHome={() => {
              React.startTransition(() => {
                setSelectedView("home");
              });

              void homeFeedQuery.refetch();
            }}
            onSelectChannel={handleOpenChannel}
            onSelectSettings={handleOpenSettings}
            profile={profileQuery.data}
            selectedChannelId={selectedChannel?.id ?? null}
            selectedView={selectedView}
            unreadChannelIds={unreadChannelIds}
          />

          <SidebarInset
            className="min-h-0 min-w-0 overflow-hidden"
            key={contentPaneKey}
          >
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
                title={activeChannel?.name ?? "Channels"}
              />
            )}

            <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
              {selectedView === "home" ? (
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
              ) : selectedView === "agents" ? (
                <AgentsView />
              ) : (
                <ChannelPane
                  activeChannel={activeChannel}
                  currentPubkey={identityQuery.data?.pubkey}
                  isSending={sendMessageMutation.isPending}
                  isTimelineLoading={isTimelineLoading}
                  messages={timelineMessages}
                  onCancelReply={() => {
                    setReplyTargetId(null);
                  }}
                  onReply={(message) => {
                    setReplyTargetId((current) =>
                      current === message.id ? null : message.id,
                    );
                  }}
                  onSend={async (content, mentionPubkeys, mediaTags) => {
                    await sendMessageMutation.mutateAsync({
                      content,
                      mentionPubkeys,
                      parentEventId: replyTargetId,
                      mediaTags,
                    });
                    setReplyTargetId(null);
                  }}
                  onTargetReached={(messageId) => {
                    setSearchAnchor((current) =>
                      current?.eventId === messageId ? null : current,
                    );
                  }}
                  onToggleReaction={
                    activeChannel &&
                    activeChannel.archivedAt === null &&
                    activeChannel.channelType !== "forum"
                      ? async (message, emoji, remove) => {
                          await toggleReactionMutation.mutateAsync({
                            emoji,
                            eventId: message.id,
                            remove,
                          });
                        }
                      : undefined
                  }
                  profiles={messageProfilesQuery.data?.profiles}
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
          </SidebarInset>
        </React.Fragment>
      )}

      <ChannelBrowserDialog
        channels={channels}
        onJoinChannel={handleBrowseChannelJoin}
        onOpenChange={setIsBrowseChannelsOpen}
        onSelectChannel={handleOpenChannel}
        open={isBrowseChannelsOpen}
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
          React.startTransition(() => {
            setIsChannelManagementOpen(false);
            setSelectedView("home");
          });
        }}
        onOpenChange={setIsChannelManagementOpen}
        open={isChannelManagementOpen && activeChannel !== null}
      />
    </SidebarProvider>
  );
}
