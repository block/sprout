import { Bot, Home, PenSquare, Plus, Search, Zap } from "lucide-react";
import * as React from "react";

import { useManagedAgentsQuery } from "@/features/agents/hooks";
import { getPresenceLabel } from "@/features/presence/lib/presence";
import { PresenceDot } from "@/features/presence/ui/PresenceBadge";
import { ProfileAvatar } from "@/features/profile/ui/ProfileAvatar";
import { ProfilePopover } from "@/features/profile/ui/ProfilePopover";
import { useDmSidebarMetadata } from "@/features/sidebar/useDmSidebarMetadata";
import {
  ChannelMenuButton,
  SidebarSection,
} from "@/features/sidebar/ui/SidebarSection";
import { NewDirectMessageDialog } from "@/features/sidebar/ui/NewDirectMessageDialog";
import type { Channel, PresenceStatus, Profile } from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupAction,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuBadge,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSkeleton,
  SidebarSeparator,
} from "@/shared/ui/sidebar";

// ---------------------------------------------------------------------------
// Shared styles
// ---------------------------------------------------------------------------

const SECTION_ICON_BUTTON_CLASS =
  "flex h-5 w-5 items-center justify-center rounded-md text-sidebar-foreground/50 hover:bg-sidebar-accent/60 hover:text-sidebar-foreground";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type AppSidebarProps = {
  channels: Channel[];
  currentPubkey?: string;
  fallbackDisplayName?: string;
  homeBadgeCount: number;
  isLoading: boolean;
  isOpeningDm: boolean;
  profile?: Profile;
  selfPresenceStatus: PresenceStatus;
  errorMessage?: string;
  selectedChannelId: string | null;
  selectedView: "home" | "channel" | "agents" | "workflows";
  unreadChannelIds: Set<string>;
  onOpenCreateChannel: () => void;
  onOpenCreateForum: () => void;
  onOpenBrowseChannels: () => void;
  onOpenBrowseForums: () => void;
  onOpenSearch: () => void;
  onHideDm: (channelId: string) => void;
  onOpenDm: (input: { pubkeys: string[] }) => Promise<void>;
  onSelectAgents: () => void;
  onSelectWorkflows: () => void;
  onSelectHome: () => void;
  onSelectChannel: (channelId: string) => void;
  onSelectSettings: () => void;
  onSetPresenceStatus?: (status: "online" | "away" | "offline") => void;
  isPresencePending?: boolean;
};

function useDeferredSidebarLoad(
  activateImmediately: boolean,
  timeoutMs: number,
) {
  const [shouldLoad, setShouldLoad] = React.useState(activateImmediately);

  React.useEffect(() => {
    if (shouldLoad || activateImmediately) {
      if (!shouldLoad) {
        setShouldLoad(true);
      }
      return;
    }

    const load = () => {
      setShouldLoad(true);
    };

    if ("requestIdleCallback" in window) {
      const idleId = window.requestIdleCallback(load, { timeout: timeoutMs });
      return () => {
        window.cancelIdleCallback(idleId);
      };
    }

    const timeoutId = globalThis.setTimeout(load, timeoutMs);
    return () => {
      globalThis.clearTimeout(timeoutId);
    };
  }, [activateImmediately, shouldLoad, timeoutMs]);

  return shouldLoad;
}

// ---------------------------------------------------------------------------
// SectionHeaderActions — search + create icon buttons for section headers
// ---------------------------------------------------------------------------

function SectionHeaderActions({
  browseAriaLabel,
  browseTestId,
  createAriaLabel,
  onBrowse,
  onCreate,
}: {
  browseAriaLabel: string;
  browseTestId?: string;
  createAriaLabel: string;
  onBrowse: () => void;
  onCreate: () => void;
}) {
  return (
    <div className="absolute right-1 top-3 flex items-center gap-0.5">
      <button
        aria-label={browseAriaLabel}
        className={SECTION_ICON_BUTTON_CLASS}
        data-testid={browseTestId}
        onClick={onBrowse}
        type="button"
      >
        <Search className="h-3.5 w-3.5" />
      </button>
      <button
        aria-label={createAriaLabel}
        className={SECTION_ICON_BUTTON_CLASS}
        onClick={onCreate}
        type="button"
      >
        <Plus className="h-4 w-4" />
      </button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// ChannelGroupSection — unified Channels / Forums section
// ---------------------------------------------------------------------------

function ChannelGroupSection({
  browseAriaLabel,
  browseTestId,
  createAriaLabel,
  groupClassName,
  isActiveChannel,
  items,
  listTestId,
  onBrowse,
  onCreate,
  onSelectChannel,
  selectedChannelId,
  title,
  unreadChannelIds,
}: {
  browseAriaLabel: string;
  browseTestId?: string;
  createAriaLabel: string;
  groupClassName?: string;
  isActiveChannel: boolean;
  items: Channel[];
  listTestId: string;
  onBrowse: () => void;
  onCreate: () => void;
  onSelectChannel: (channelId: string) => void;
  selectedChannelId: string | null;
  title: string;
  unreadChannelIds: Set<string>;
}) {
  return (
    <SidebarGroup className={groupClassName}>
      <SidebarGroupLabel>{title}</SidebarGroupLabel>
      <SectionHeaderActions
        browseAriaLabel={browseAriaLabel}
        browseTestId={browseTestId}
        createAriaLabel={createAriaLabel}
        onBrowse={onBrowse}
        onCreate={onCreate}
      />
      <SidebarGroupContent>
        {items.length > 0 ? (
          <SidebarMenu data-testid={listTestId}>
            {items.map((channel) => (
              <SidebarMenuItem key={channel.id}>
                <ChannelMenuButton
                  channel={channel}
                  hasUnread={unreadChannelIds.has(channel.id)}
                  isActive={isActiveChannel && selectedChannelId === channel.id}
                  onSelectChannel={onSelectChannel}
                />
              </SidebarMenuItem>
            ))}
          </SidebarMenu>
        ) : null}
      </SidebarGroupContent>
    </SidebarGroup>
  );
}

// ---------------------------------------------------------------------------
// AppSidebar
// ---------------------------------------------------------------------------

export function AppSidebar({
  channels,
  currentPubkey,
  fallbackDisplayName,
  homeBadgeCount,
  isLoading,
  isOpeningDm,
  profile,
  selfPresenceStatus,
  errorMessage,
  selectedChannelId,
  selectedView,
  unreadChannelIds,
  onOpenCreateChannel,
  onOpenCreateForum,
  onOpenBrowseChannels,
  onOpenBrowseForums,
  onOpenSearch,
  onHideDm,
  onOpenDm,
  onSelectAgents,
  onSelectWorkflows,
  onSelectHome,
  onSelectChannel,
  onSelectSettings,
  onSetPresenceStatus,
  isPresencePending,
}: AppSidebarProps) {
  const skeletonRows = ["first", "second", "third", "fourth", "fifth", "sixth"];
  const [isNewDmOpen, setIsNewDmOpen] = React.useState(false);
  const [profilePopoverOpen, setProfilePopoverOpen] = React.useState(false);

  const streamChannels = channels.filter(
    (channel) => channel.channelType === "stream",
  );
  const forumChannels = channels.filter(
    (channel) => channel.channelType === "forum",
  );
  const directMessages = channels.filter(
    (channel) => channel.channelType === "dm",
  );
  const isSelectedDirectMessage =
    selectedView === "channel" &&
    directMessages.some((channel) => channel.id === selectedChannelId);
  const shouldLoadDmMetadata = useDeferredSidebarLoad(
    isSelectedDirectMessage,
    400,
  );
  const { dmChannelLabels, dmParticipantsByChannelId, dmPresenceByChannelId } =
    useDmSidebarMetadata({
      currentPubkey,
      directMessages,
      enabled: shouldLoadDmMetadata,
      fallbackDisplayName,
      profileDisplayName: profile?.displayName,
    });
  const shouldLoadAgentCount = useDeferredSidebarLoad(
    selectedView === "agents",
    250,
  );
  const managedAgentsQuery = useManagedAgentsQuery({
    enabled: shouldLoadAgentCount,
  });
  const totalAgentCount = managedAgentsQuery.data?.length ?? 0;
  const shouldShowAgentCount =
    totalAgentCount > 0 || managedAgentsQuery.isFetched;
  const resolvedDisplayName =
    profile?.displayName?.trim() ||
    fallbackDisplayName?.trim() ||
    "Current identity";

  return (
    <Sidebar
      collapsible="offcanvas"
      data-testid="app-sidebar"
      variant="sidebar"
    >
      <SidebarHeader className="gap-3 pt-10" data-tauri-drag-region>
        <Button
          className="w-full justify-between rounded-xl border border-sidebar-border/80 bg-sidebar-accent/60 px-3 text-sidebar-foreground/80 shadow-sm hover:bg-sidebar-accent hover:text-sidebar-foreground"
          data-testid="open-search"
          onClick={onOpenSearch}
          size="sm"
          type="button"
          variant="ghost"
        >
          <span className="flex items-center gap-2">
            <Search className="h-4 w-4" />
            Search messages
          </span>
          <span className="text-xs text-sidebar-foreground/50">&#x2318;K</span>
        </Button>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton
              isActive={selectedView === "home"}
              onClick={onSelectHome}
              tooltip="Home"
              type="button"
            >
              <Home className="h-4 w-4" />
              <span>Home</span>
            </SidebarMenuButton>
            {homeBadgeCount > 0 ? (
              <SidebarMenuBadge
                className="right-2 rounded-full bg-primary/15 px-1.5 text-[11px] text-primary peer-data-[active=true]/menu-button:bg-sidebar-primary-foreground/20 peer-data-[active=true]/menu-button:text-sidebar-primary-foreground"
                data-testid="sidebar-home-count"
              >
                {Math.min(homeBadgeCount, 99)}
              </SidebarMenuBadge>
            ) : null}
          </SidebarMenuItem>
          <SidebarMenuItem>
            <SidebarMenuButton
              data-testid="open-agents-view"
              isActive={selectedView === "agents"}
              onClick={onSelectAgents}
              tooltip="Agents"
              type="button"
            >
              <Bot className="h-4 w-4" />
              <span>Agents</span>
            </SidebarMenuButton>
            {shouldShowAgentCount ? (
              <SidebarMenuBadge
                className="right-2 rounded-full bg-sidebar-accent/70 px-1.5 text-[11px] text-sidebar-foreground/75 peer-data-[active=true]/menu-button:bg-sidebar-primary-foreground/20 peer-data-[active=true]/menu-button:text-sidebar-primary-foreground"
                data-testid="sidebar-agents-count"
              >
                {totalAgentCount}
              </SidebarMenuBadge>
            ) : null}
          </SidebarMenuItem>
          <SidebarMenuItem>
            <SidebarMenuButton
              data-testid="open-workflows-view"
              isActive={selectedView === "workflows"}
              onClick={onSelectWorkflows}
              tooltip="Workflows"
              type="button"
            >
              <Zap className="h-4 w-4" />
              <span>Workflows</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>

      <SidebarSeparator className="mx-0 w-full" />

      <SidebarContent>
        {isLoading ? (
          <SidebarGroup>
            <SidebarGroupLabel>Channels</SidebarGroupLabel>
            <SidebarGroupContent>
              <SidebarMenu data-testid="sidebar-loading">
                {skeletonRows.map((row) => (
                  <SidebarMenuSkeleton key={row} showIcon />
                ))}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        ) : null}

        {!isLoading ? (
          <>
            <ChannelGroupSection
              browseAriaLabel="Browse channels"
              browseTestId="browse-channels"
              createAriaLabel="Create a stream"
              groupClassName="pt-1"
              isActiveChannel={selectedView === "channel"}
              items={streamChannels}
              listTestId="stream-list"
              onBrowse={onOpenBrowseChannels}
              onCreate={onOpenCreateChannel}
              onSelectChannel={onSelectChannel}
              selectedChannelId={selectedChannelId}
              title="Channels"
              unreadChannelIds={unreadChannelIds}
            />
            <ChannelGroupSection
              browseAriaLabel="Browse forums"
              browseTestId="browse-forums"
              createAriaLabel="New forum"
              isActiveChannel={selectedView === "channel"}
              items={forumChannels}
              listTestId="forum-list"
              onBrowse={onOpenBrowseForums}
              onCreate={onOpenCreateForum}
              onSelectChannel={onSelectChannel}
              selectedChannelId={selectedChannelId}
              title="Forums"
              unreadChannelIds={unreadChannelIds}
            />
            <SidebarSection
              action={
                <SidebarGroupAction
                  aria-expanded={isNewDmOpen}
                  aria-label="Start a direct message"
                  className="top-3 text-sidebar-foreground/50 hover:bg-sidebar-accent/60 hover:text-sidebar-foreground"
                  data-testid="new-dm-trigger"
                  onClick={() => {
                    setIsNewDmOpen(true);
                  }}
                  type="button"
                >
                  <PenSquare className="transition-transform" />
                </SidebarGroupAction>
              }
              dmParticipantsByChannelId={dmParticipantsByChannelId}
              isActiveChannel={selectedView === "channel"}
              items={directMessages}
              channelLabels={dmChannelLabels}
              onHideDm={onHideDm}
              onSelectChannel={onSelectChannel}
              presenceByChannelId={dmPresenceByChannelId}
              selectedChannelId={selectedChannelId}
              testId="dm-list"
              title="Direct Messages"
              unreadChannelIds={unreadChannelIds}
            />
          </>
        ) : null}

        {errorMessage ? (
          <div className="px-3 py-2 text-sm text-destructive">
            {errorMessage}
          </div>
        ) : null}
      </SidebarContent>

      <SidebarSeparator className="mx-0 w-full" />

      <SidebarFooter>
        <SidebarMenu>
          <SidebarMenuItem>
            <ProfilePopover
              open={profilePopoverOpen}
              onOpenChange={setProfilePopoverOpen}
              displayName={resolvedDisplayName}
              nip05={profile?.nip05Handle}
              avatarUrl={profile?.avatarUrl ?? null}
              currentStatus={selfPresenceStatus}
              isStatusPending={isPresencePending}
              onSetStatus={onSetPresenceStatus ?? (() => {})}
              onOpenSettings={onSelectSettings}
            >
              <SidebarMenuButton
                className="h-auto gap-3 rounded-xl px-2 py-2"
                data-testid="open-settings"
                type="button"
              >
                <div
                  className="flex min-w-0 flex-1 items-center gap-3"
                  data-testid="sidebar-profile-card"
                >
                  <div className="relative shrink-0">
                    <ProfileAvatar
                      avatarUrl={profile?.avatarUrl ?? null}
                      className="h-10 w-10 rounded-2xl text-sm"
                      iconClassName="h-5 w-5"
                      label={resolvedDisplayName}
                      testId="sidebar-profile-avatar"
                    />
                    <span
                      aria-label={getPresenceLabel(selfPresenceStatus)}
                      className="absolute -bottom-0.5 -right-0.5 flex h-4 w-4 items-center justify-center rounded-full bg-sidebar"
                      data-testid="self-presence-badge"
                      role="img"
                    >
                      <PresenceDot
                        className="h-2.5 w-2.5"
                        status={selfPresenceStatus}
                      />
                    </span>
                  </div>
                  <div className="min-w-0">
                    <p
                      className="truncate text-sm font-semibold text-current"
                      data-testid="sidebar-profile-name"
                    >
                      {resolvedDisplayName}
                    </p>
                  </div>
                </div>
              </SidebarMenuButton>
            </ProfilePopover>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarFooter>

      <NewDirectMessageDialog
        currentPubkey={currentPubkey}
        isPending={isOpeningDm}
        onOpenChange={setIsNewDmOpen}
        onSubmit={onOpenDm}
        open={isNewDmOpen}
      />
    </Sidebar>
  );
}
