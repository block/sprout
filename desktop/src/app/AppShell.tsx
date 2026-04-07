import * as React from "react";
import { useQueryClient } from "@tanstack/react-query";

import {
  AppShellOverlays,
  type BrowseDialogType,
} from "@/app/AppShellOverlays";
import { useWebviewZoomShortcuts } from "@/app/useWebviewZoomShortcuts";
import {
  channelsQueryKey,
  useChannelsQuery,
  useCreateChannelMutation,
  useHideDmMutation,
  useOpenDmMutation,
  useSelectedChannel,
} from "@/features/channels/hooks";
import { useUnreadChannels } from "@/features/channels/useUnreadChannels";
import { HomeScreen } from "@/features/home/ui/HomeScreen";
import { useHomeFeedNotifications } from "@/features/notifications/hooks";
import { usePresenceSession } from "@/features/presence/hooks";
import { useProfileQuery } from "@/features/profile/hooks";
import type { SettingsSection } from "@/features/settings/ui/SettingsPanels";
import { SettingsScreen } from "@/features/settings/ui/SettingsScreen";
import { AppSidebar } from "@/features/sidebar/ui/AppSidebar";
import { relayClient } from "@/shared/api/relayClient";
import { useIdentityQuery } from "@/shared/api/hooks";
import { getEventById, joinChannel } from "@/shared/api/tauri";
import type { Channel, RelayEvent, SearchHit } from "@/shared/api/types";
import { ChannelNavigationProvider } from "@/shared/context/ChannelNavigationContext";
import { ViewLoadingFallback } from "@/shared/ui/ViewLoadingFallback";
import {
  SidebarInset,
  SidebarProvider,
  SidebarTrigger,
} from "@/shared/ui/sidebar";

type AppView = "home" | "channel" | "agents" | "workflows";
const DEFAULT_SETTINGS_SECTION: SettingsSection = "profile";

const AgentsScreen = React.lazy(async () => {
  const module = await import("@/features/agents/ui/AgentsScreen");
  return { default: module.AgentsScreen };
});
const ChannelScreen = React.lazy(async () => {
  const module = await import("@/features/channels/ui/ChannelScreen");
  return { default: module.ChannelScreen };
});
const WorkflowsScreen = React.lazy(async () => {
  const module = await import("@/features/workflows/ui/WorkflowsScreen");
  return { default: module.WorkflowsScreen };
});

export function AppShell() {
  useWebviewZoomShortcuts();

  const [selectedView, setSelectedView] = React.useState<AppView>("home");
  const [settingsOpen, setSettingsOpen] = React.useState(false);
  const [settingsSection, setSettingsSection] = React.useState<SettingsSection>(
    DEFAULT_SETTINGS_SECTION,
  );
  const [isChannelManagementOpen, setIsChannelManagementOpen] =
    React.useState(false);
  const [isSearchOpen, setIsSearchOpen] = React.useState(false);
  const [browseDialogType, setBrowseDialogType] =
    React.useState<BrowseDialogType>(null);
  const [searchAnchor, setSearchAnchor] = React.useState<SearchHit | null>(
    null,
  );
  const [searchAnchorChannelId, setSearchAnchorChannelId] = React.useState<
    string | null
  >(null);
  const [searchAnchorEvent, setSearchAnchorEvent] =
    React.useState<RelayEvent | null>(null);
  const queryClient = useQueryClient();

  const selectView = React.useCallback((view: AppView) => {
    React.startTransition(() => {
      setSelectedView(view);
    });
  }, []);

  const identityQuery = useIdentityQuery();
  const profileQuery = useProfileQuery();
  const presenceSession = usePresenceSession(identityQuery.data?.pubkey);
  const { homeBadgeCount, homeFeedQuery, notificationSettings } =
    useHomeFeedNotifications(
      identityQuery.data?.pubkey,
      selectedView === "home",
    );
  const refetchHomeFeedOnLiveMention = React.useEffectEvent(() => {
    void homeFeedQuery.refetch();
  });

  const channelsQuery = useChannelsQuery();
  const { refetch: refetchChannels } = channelsQuery;
  const channels = channelsQuery.data ?? [];
  const memberChannels = React.useMemo(
    () => channels.filter((channel) => channel.isMember),
    [channels],
  );
  const availableChannelIds = React.useMemo(
    () => new Set(channels.map((channel) => channel.id)),
    [channels],
  );
  const { selectedChannel, setSelectedChannelId } = useSelectedChannel(
    channels,
    null,
  );
  const activeChannel = selectedView === "channel" ? selectedChannel : null;

  const { markChannelRead, unreadChannelIds } = useUnreadChannels(
    channels,
    activeChannel,
    // Wait for ChannelScreen to report the latest loaded message before
    // advancing unread state for the active channel.
    null,
    {
      currentPubkey: identityQuery.data?.pubkey,
      onLiveMention: refetchHomeFeedOnLiveMention,
    },
  );

  const createChannelMutation = useCreateChannelMutation();
  const createForumMutation = useCreateChannelMutation();
  const openDmMutation = useOpenDmMutation();
  const hideDmMutation = useHideDmMutation();
  const handleOpenBrowseChannels = React.useCallback(() => {
    setBrowseDialogType("stream");
    void refetchChannels();
  }, [refetchChannels]);
  const handleOpenBrowseForums = React.useCallback(() => {
    setBrowseDialogType("forum");
    void refetchChannels();
  }, [refetchChannels]);
  const handleOpenSearch = React.useCallback(() => {
    setIsSearchOpen(true);
    void refetchChannels();
  }, [refetchChannels]);

  const handleBrowseDialogOpenChange = React.useCallback((open: boolean) => {
    if (!open) {
      setBrowseDialogType(null);
    }
  }, []);

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
    [setSelectedChannelId],
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
        return;
      }

      if (selectedChannel?.id === channelId) {
        selectView("home");
      }
    },
    [hideDmMutation, selectView, selectedChannel?.id],
  );

  const handleOpenSettings = React.useCallback(
    (section: SettingsSection = DEFAULT_SETTINGS_SECTION) => {
      setIsSearchOpen(false);
      setIsChannelManagementOpen(false);
      setSettingsSection(section);
      setSettingsOpen(true);
    },
    [],
  );

  const handleCloseSettings = React.useCallback(() => {
    setSettingsOpen(false);
  }, []);

  const handleTargetReached = React.useCallback((messageId: string) => {
    setSearchAnchor((current) =>
      current?.eventId === messageId ? null : current,
    );
  }, []);

  const handleOpenSearchResult = React.useCallback(
    (hit: SearchHit) => {
      setSearchAnchor(hit);
      setSearchAnchorChannelId(hit.channelId);
      setSearchAnchorEvent({
        id: hit.eventId,
        pubkey: hit.pubkey,
        created_at: hit.createdAt,
        kind: hit.kind,
        tags: hit.channelId ? [["h", hit.channelId]] : [],
        content: hit.content,
        sig: "",
      });
      if (hit.channelId) {
        void handleOpenChannel(hit.channelId);
      }

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

  React.useLayoutEffect(() => {
    if (settingsOpen) {
      return;
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (!(event.metaKey || event.ctrlKey) || event.altKey) {
        return;
      }

      const key = event.key.toLowerCase();
      if (key === "k" && !event.shiftKey) {
        event.preventDefault();
        handleOpenSearch();
        return;
      }

      if (key === "o" && event.shiftKey) {
        event.preventDefault();
        handleOpenBrowseChannels();
      }
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [handleOpenBrowseChannels, handleOpenSearch, settingsOpen]);

  React.useLayoutEffect(() => {
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
      if (settingsOpen) {
        handleCloseSettings();
        return;
      }

      handleOpenSettings();
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [handleCloseSettings, handleOpenSettings, settingsOpen]);

  return (
    <ChannelNavigationProvider
      channels={channels}
      onOpenChannel={handleOpenChannel}
    >
      <SidebarProvider className="h-dvh overflow-hidden overscroll-none">
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
          isPresencePending={presenceSession.isPending}
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
          onHideDm={handleHideDm}
          onOpenBrowseChannels={handleOpenBrowseChannels}
          onOpenBrowseForums={handleOpenBrowseForums}
          onOpenDm={async ({ pubkeys }) => {
            const directMessage = await openDmMutation.mutateAsync({
              pubkeys,
            });
            openChannelView(directMessage.id);
          }}
          onOpenSearch={handleOpenSearch}
          onSelectAgents={() => selectView("agents")}
          onSelectChannel={handleOpenChannel}
          onSelectHome={() => {
            selectView("home");
          }}
          onSelectSettings={handleOpenSettings}
          onSelectWorkflows={() => selectView("workflows")}
          onSetPresenceStatus={(status) => presenceSession.setStatus(status)}
          profile={profileQuery.data}
          selectedChannelId={selectedChannel?.id ?? null}
          selectedView={selectedView}
          unreadChannelIds={unreadChannelIds}
        />

        <SidebarInset className="min-h-0 min-w-0 overflow-hidden">
          {selectedView === "home" ? (
            <HomeScreen
              availableChannelIds={availableChannelIds}
              currentPubkey={identityQuery.data?.pubkey}
              onOpenChannel={handleOpenChannel}
            />
          ) : selectedView === "agents" ? (
            <React.Suspense
              fallback={<ViewLoadingFallback label="Loading agents..." />}
            >
              <AgentsScreen />
            </React.Suspense>
          ) : selectedView === "workflows" ? (
            <React.Suspense
              fallback={<ViewLoadingFallback label="Loading workflows..." />}
            >
              <WorkflowsScreen channels={memberChannels} />
            </React.Suspense>
          ) : (
            <React.Suspense
              fallback={<ViewLoadingFallback label="Loading channel..." />}
            >
              <ChannelScreen
                activeChannel={activeChannel}
                currentIdentity={identityQuery.data}
                currentProfile={profileQuery.data}
                onManageChannel={() => {
                  setIsChannelManagementOpen(true);
                }}
                onMarkChannelRead={markChannelRead}
                onTargetReached={handleTargetReached}
                searchAnchor={searchAnchor}
                searchAnchorChannelId={searchAnchorChannelId}
                searchAnchorEvent={searchAnchorEvent}
              />
            </React.Suspense>
          )}
        </SidebarInset>

        <AppShellOverlays
          activeChannel={activeChannel}
          browseDialogType={browseDialogType}
          channels={channels}
          currentPubkey={identityQuery.data?.pubkey}
          isChannelManagementOpen={isChannelManagementOpen}
          isSearchOpen={isSearchOpen}
          onBrowseChannelJoin={handleBrowseChannelJoin}
          onBrowseDialogOpenChange={handleBrowseDialogOpenChange}
          onChannelManagementOpenChange={setIsChannelManagementOpen}
          onDeleteActiveChannel={() => {
            setIsChannelManagementOpen(false);
            selectView("home");
          }}
          onOpenSearchResult={handleOpenSearchResult}
          onSearchOpenChange={setIsSearchOpen}
          onSelectChannel={handleOpenChannel}
        />

        {settingsOpen ? (
          <SettingsScreen
            currentPubkey={identityQuery.data?.pubkey}
            fallbackDisplayName={identityQuery.data?.displayName}
            isUpdatingDesktopNotifications={
              notificationSettings.isUpdatingDesktopEnabled
            }
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
            section={settingsSection}
          />
        ) : null}
      </SidebarProvider>
    </ChannelNavigationProvider>
  );
}
