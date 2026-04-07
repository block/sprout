import { ChevronLeft, ChevronRight } from "lucide-react";
import * as React from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Outlet, useLocation } from "@tanstack/react-router";

import { AppShellProvider } from "@/app/AppShellContext";
import {
  AppShellOverlays,
  type BrowseDialogType,
} from "@/app/AppShellOverlays";
import { useAppNavigation } from "@/app/navigation/useAppNavigation";
import { useBackForwardControls } from "@/app/navigation/useBackForwardControls";
import { useWebviewZoomShortcuts } from "@/app/useWebviewZoomShortcuts";
import {
  channelsQueryKey,
  useChannelsQuery,
  useCreateChannelMutation,
  useHideDmMutation,
  useOpenDmMutation,
} from "@/features/channels/hooks";
import { useUnreadChannels } from "@/features/channels/useUnreadChannels";
import { useHomeFeedNotifications } from "@/features/notifications/hooks";
import {
  listenForDesktopNotificationActions,
  revealDesktopAppWindow,
  setDesktopAppBadgeCount,
  type DesktopNotificationTarget,
} from "@/features/notifications/lib/desktop";
import { usePresenceSession } from "@/features/presence/hooks";
import { useProfileQuery } from "@/features/profile/hooks";
import type { SettingsSection } from "@/features/settings/ui/SettingsPanels";
import { SettingsScreen } from "@/features/settings/ui/SettingsScreen";
import { AppSidebar } from "@/features/sidebar/ui/AppSidebar";
import { relayClient } from "@/shared/api/relayClient";
import { useIdentityQuery } from "@/shared/api/hooks";
import { joinChannel } from "@/shared/api/tauri";
import type { SearchHit } from "@/shared/api/types";
import { ChannelNavigationProvider } from "@/shared/context/ChannelNavigationContext";
import { Button } from "@/shared/ui/button";
import {
  SidebarInset,
  SidebarProvider,
  SidebarTrigger,
} from "@/shared/ui/sidebar";

type AppView = "home" | "channel" | "agents" | "workflows";
const DEFAULT_SETTINGS_SECTION: SettingsSection = "profile";

function toSearchHit(target: DesktopNotificationTarget): SearchHit | null {
  if (!target.eventId) {
    return null;
  }

  return {
    eventId: target.eventId,
    content: target.content ?? "",
    kind: target.kind ?? 9,
    pubkey: target.pubkey ?? "",
    channelId: target.channelId,
    channelName: target.channelName ?? null,
    createdAt: target.createdAt ?? Math.floor(Date.now() / 1_000),
    score: 0,
  };
}

function deriveShellRoute(pathname: string): {
  selectedChannelId: string | null;
  selectedView: AppView;
} {
  if (pathname.startsWith("/channels/")) {
    const [, , rawChannelId] = pathname.split("/");
    return {
      selectedChannelId: rawChannelId ? decodeURIComponent(rawChannelId) : null,
      selectedView: "channel",
    };
  }

  if (pathname === "/agents") {
    return {
      selectedChannelId: null,
      selectedView: "agents",
    };
  }

  if (pathname === "/workflows" || pathname.startsWith("/workflows/")) {
    return {
      selectedChannelId: null,
      selectedView: "workflows",
    };
  }

  return {
    selectedChannelId: null,
    selectedView: "home",
  };
}

export function AppShell() {
  useWebviewZoomShortcuts();

  const [settingsOpen, setSettingsOpen] = React.useState(false);
  const [settingsSection, setSettingsSection] = React.useState<SettingsSection>(
    DEFAULT_SETTINGS_SECTION,
  );
  const [isChannelManagementOpen, setIsChannelManagementOpen] =
    React.useState(false);
  const [isSearchOpen, setIsSearchOpen] = React.useState(false);
  const [browseDialogType, setBrowseDialogType] =
    React.useState<BrowseDialogType>(null);
  const location = useLocation();
  const queryClient = useQueryClient();
  const { goAgents, goChannel, goHome, goWorkflows, openSearchHit } =
    useAppNavigation();
  const { canGoBack, canGoForward, goBack, goForward } =
    useBackForwardControls();
  const { selectedChannelId, selectedView } = React.useMemo(
    () => deriveShellRoute(location.pathname),
    [location.pathname],
  );

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
  const activeChannel = React.useMemo(
    () =>
      selectedChannelId
        ? (channels.find((channel) => channel.id === selectedChannelId) ?? null)
        : null,
    [channels, selectedChannelId],
  );

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

      if (selectedChannelId === channelId) {
        void goHome();
      }
    },
    [goHome, hideDmMutation, selectedChannelId],
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

  const handleOpenSearchResult = React.useCallback(
    (hit: SearchHit) => {
      void openSearchHit(hit);
    },
    [openSearchHit],
  );

  const handleDesktopNotificationAction = React.useEffectEvent(
    async (target: DesktopNotificationTarget) => {
      await revealDesktopAppWindow();

      if (!target.channelId) {
        void goHome();
        return;
      }

      const anchor = toSearchHit(target);
      if (!anchor) {
        await goChannel(target.channelId);
        return;
      }

      await openSearchHit(anchor);
    },
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
    void setDesktopAppBadgeCount(homeBadgeCount);
  }, [homeBadgeCount]);

  React.useEffect(() => {
    let isCancelled = false;
    let cleanup = () => {};

    void listenForDesktopNotificationActions((target) => {
      if (isCancelled) {
        return;
      }

      void handleDesktopNotificationAction(target);
    }).then((dispose) => {
      if (isCancelled) {
        dispose();
        return;
      }

      cleanup = dispose;
    });

    return () => {
      isCancelled = true;
      cleanup();
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
    <ChannelNavigationProvider channels={channels}>
      <AppShellProvider
        value={{
          markChannelRead,
          openChannelManagement: () => {
            setIsChannelManagementOpen(true);
          },
        }}
      >
        <SidebarProvider className="h-dvh overflow-hidden overscroll-none">
          <div className="fixed left-[80px] top-[8px] z-50 flex items-center gap-1.5">
            <SidebarTrigger className="h-6 w-6 text-muted-foreground/70 hover:bg-muted/60 hover:text-foreground" />
            <Button
              aria-label="Go back"
              className="h-6 w-6 text-muted-foreground/70 hover:bg-muted/60 hover:text-foreground"
              data-testid="global-back"
              disabled={!canGoBack}
              onClick={goBack}
              size="icon"
              variant="ghost"
            >
              <ChevronLeft className="h-3.5 w-3.5" />
            </Button>
            <Button
              aria-label="Go forward"
              className="h-6 w-6 text-muted-foreground/70 hover:bg-muted/60 hover:text-foreground"
              data-testid="global-forward"
              disabled={!canGoForward}
              onClick={goForward}
              size="icon"
              variant="ghost"
            >
              <ChevronRight className="h-3.5 w-3.5" />
            </Button>
          </div>
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
            onCreateChannel={async ({
              description,
              name,
              visibility,
              ttlSeconds,
            }) => {
              const createdChannel = await createChannelMutation.mutateAsync({
                name,
                description,
                channelType: "stream",
                visibility,
                ttlSeconds,
              });

              await goChannel(createdChannel.id);
            }}
            onCreateForum={async ({
              description,
              name,
              visibility,
              ttlSeconds,
            }) => {
              const createdForum = await createForumMutation.mutateAsync({
                name,
                description,
                channelType: "forum",
                visibility,
                ttlSeconds,
              });

              await goChannel(createdForum.id);
            }}
            onHideDm={handleHideDm}
            onOpenBrowseChannels={handleOpenBrowseChannels}
            onOpenBrowseForums={handleOpenBrowseForums}
            onOpenDm={async ({ pubkeys }) => {
              const directMessage = await openDmMutation.mutateAsync({
                pubkeys,
              });
              await goChannel(directMessage.id);
            }}
            onOpenSearch={handleOpenSearch}
            onSelectAgents={() => {
              void goAgents();
            }}
            onSelectChannel={(channelId) => {
              void goChannel(channelId);
            }}
            onSelectHome={() => {
              void goHome();
            }}
            onSelectSettings={handleOpenSettings}
            onSelectWorkflows={() => {
              void goWorkflows();
            }}
            onSetPresenceStatus={(status) => presenceSession.setStatus(status)}
            profile={profileQuery.data}
            selectedChannelId={selectedChannelId}
            selectedView={selectedView}
            unreadChannelIds={unreadChannelIds}
          />

          <SidebarInset className="min-h-0 min-w-0 overflow-hidden">
            <Outlet />
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
              void goHome({ replace: true });
            }}
            onOpenSearchResult={handleOpenSearchResult}
            onSearchOpenChange={setIsSearchOpen}
            onSelectChannel={(channelId) => {
              void goChannel(channelId);
            }}
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
      </AppShellProvider>
    </ChannelNavigationProvider>
  );
}
