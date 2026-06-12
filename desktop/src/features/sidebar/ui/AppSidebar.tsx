// biome-ignore format: keep compact to stay within file size limit
import {
  Activity,
  ArrowDown,
  ArrowUp,
  Bot,
  FolderGit2,
  Home,
  PenSquare,
  Zap,
} from "lucide-react";
import * as React from "react";
import { FeatureGate } from "@/shared/features";
import { SidebarDndContext } from "@/features/sidebar/ui/SidebarDnd";

import { useManagedAgentsQuery } from "@/features/agents/hooks";
import type { Workspace } from "@/features/workspaces/types";
import { AddWorkspaceDialog } from "@/features/workspaces/ui/AddWorkspaceDialog";
import { useDeferredLoad } from "@/shared/hooks/useDeferredStartup";
import {
  useChannelSections,
  type ChannelSection,
} from "@/features/sidebar/lib/useChannelSections";
import { useDmSidebarMetadata } from "@/features/sidebar/useDmSidebarMetadata";
import { useSidebarScrollLock } from "@/features/sidebar/lib/useSidebarScrollLock";
import { useUnreadOverflow } from "@/features/sidebar/lib/useUnreadOverflow";
import {
  CreateSectionDialog,
  DeleteSectionAlertDialog,
  RenameSectionDialog,
} from "@/features/sidebar/ui/ChannelSectionDialogs";
import { MoreUnreadButton } from "@/features/sidebar/ui/MoreUnreadButton";
import { SidebarSection } from "@/features/sidebar/ui/SidebarSection";
import {
  ChannelGroupSection,
  CustomChannelSection,
} from "@/features/sidebar/ui/CustomChannelSection";
import { CreateChannelDialog } from "@/features/sidebar/ui/CreateChannelDialog";
import { NewDirectMessageDialog } from "@/features/sidebar/ui/NewDirectMessageDialog";
import { SidebarProfileCard } from "@/features/sidebar/ui/SidebarProfileCard";
import { SECTION_ACTION_VISIBILITY_CLASS } from "@/features/sidebar/ui/sidebarSectionStyles";
import type {
  Channel,
  ChannelVisibility,
  PresenceStatus,
  Profile,
  UserStatus,
} from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
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
  SidebarRail,
} from "@/shared/ui/sidebar";
import { Skeleton } from "@/shared/ui/skeleton";

type CollapsibleSidebarGroup =
  | "starred"
  | "channels"
  | "forums"
  | "directMessages";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type CreateChannelKind = "stream" | "forum";

type AppSidebarProps = {
  activeWorkspace: Workspace | null;
  channels: Channel[];
  currentPubkey?: string;
  fallbackDisplayName?: string;
  homeBadgeCount: number;
  isAddWorkspaceOpen?: boolean;
  isLoading: boolean;
  isCreatingChannel: boolean;
  isCreatingForum: boolean;
  isOpeningDm: boolean;
  profile?: Profile;
  selfPresenceStatus: PresenceStatus;
  errorMessage?: string;
  selectedChannelId: string | null;
  selectedView:
    | "home"
    | "channel"
    | "agents"
    | "workflows"
    | "pulse"
    | "projects";
  unreadChannelIds: ReadonlySet<string>;
  workspaces: Workspace[];
  onAddWorkspace: (workspace: Workspace) => void;
  onAddWorkspaceOpenChange?: (open: boolean) => void;
  onCreateChannel: (input: {
    name: string;
    description?: string;
    visibility: ChannelVisibility;
    ttlSeconds?: number;
    templateId?: string;
  }) => Promise<void>;
  onCreateForum: (input: {
    name: string;
    description?: string;
    visibility: ChannelVisibility;
    ttlSeconds?: number;
    templateId?: string;
  }) => Promise<void>;
  onOpenAddWorkspace: () => void;
  onOpenBrowseChannels: () => void;
  onOpenBrowseForums: () => void;
  onHideDm: (channelId: string) => void;
  onMarkChannelUnread: (channelId: string) => void;
  onMarkChannelRead: (
    channelId: string,
    lastMessageAt: string | null | undefined,
  ) => void;
  onMarkAllChannelsRead: () => void;
  onOpenDm: (input: { pubkeys: string[] }) => Promise<void>;
  onUpdateWorkspace: (
    id: string,
    updates: Partial<Pick<Workspace, "name" | "relayUrl" | "token">>,
  ) => void;
  onRemoveWorkspace: (id: string) => void;
  onSelectAgents: () => void;
  onSelectProjects: () => void;
  onSelectPulse: () => void;
  onSelectWorkflows: () => void;
  onSelectHome: () => void;
  onSelectChannel: (channelId: string) => void;
  onSelectSettings: (section?: "profile" | "appearance") => void;
  onSetPresenceStatus?: (status: "online" | "away" | "offline") => void;
  onSetUserStatus: (text: string, emoji: string) => void;
  onClearUserStatus: () => void;
  onSwitchWorkspace: (id: string) => void;
  selfUserStatus?: UserStatus;
  isPresencePending?: boolean;
  isNewDmOpen?: boolean;
  onNewDmOpenChange?: (open: boolean) => void;
  isCreateChannelOpen?: boolean;
  onCreateChannelOpenChange?: (open: boolean) => void;
  mutedChannelIds?: ReadonlySet<string>;
  onMuteChannel?: (channelId: string) => void;
  onUnmuteChannel?: (channelId: string) => void;
  starredChannelIds?: ReadonlySet<string>;
  onStarChannel?: (channelId: string) => void;
  onUnstarChannel?: (channelId: string) => void;
};

const SIDEBAR_SKELETON_CACHE_PREFIX = "buzz-sidebar-skeleton-shape.v1";
const sidebarLoadingWidthClasses = [
  "w-14",
  "w-16",
  "w-20",
  "w-24",
  "w-28",
  "w-32",
] as const;

type SidebarLoadingWidthClass = (typeof sidebarLoadingWidthClasses)[number];

type SidebarLoadingRowShape = {
  avatar?: boolean;
  key: string;
  unread?: boolean;
  widthClass: SidebarLoadingWidthClass;
};

type SidebarLoadingShape = {
  channels: SidebarLoadingRowShape[];
  directMessages: SidebarLoadingRowShape[];
};

type SidebarLoadingCachePayload = SidebarLoadingShape & {
  version: 1;
};

const fallbackSidebarLoadingShape: SidebarLoadingShape = {
  channels: [
    { key: "agents", widthClass: "w-20" },
    { key: "engineering", widthClass: "w-28" },
    { key: "general", widthClass: "w-20" },
  ],
  directMessages: [
    { key: "alice", widthClass: "w-24" },
    { key: "bob", widthClass: "w-20" },
  ],
};

function isSidebarLoadingWidthClass(
  value: unknown,
): value is SidebarLoadingWidthClass {
  return sidebarLoadingWidthClasses.includes(value as SidebarLoadingWidthClass);
}

function parseSidebarLoadingRows(
  rows: unknown,
  maxRows: number,
): SidebarLoadingRowShape[] {
  if (!Array.isArray(rows)) return [];

  return rows
    .slice(0, maxRows)
    .filter((row: unknown): row is SidebarLoadingRowShape => {
      if (typeof row !== "object" || row === null) return false;
      const record = row as Record<string, unknown>;
      return (
        typeof record.key === "string" &&
        isSidebarLoadingWidthClass(record.widthClass) &&
        (record.unread === undefined || typeof record.unread === "boolean") &&
        (record.avatar === undefined || typeof record.avatar === "boolean")
      );
    });
}

function parseSidebarLoadingShape(value: unknown): SidebarLoadingShape | null {
  if (typeof value !== "object" || value === null) return null;
  const record = value as Record<string, unknown>;
  if (record.version !== 1) return null;

  const shape = {
    channels: parseSidebarLoadingRows(record.channels, 3),
    directMessages: parseSidebarLoadingRows(record.directMessages, 2),
  };

  return hasSidebarLoadingRows(shape) ? shape : null;
}

function hasSidebarLoadingRows(shape: SidebarLoadingShape) {
  return shape.channels.length > 0 || shape.directMessages.length > 0;
}

function sidebarSkeletonCacheKey(
  workspaceId: string | null | undefined,
  pubkey: string | undefined,
) {
  if (!workspaceId) return null;
  return `${SIDEBAR_SKELETON_CACHE_PREFIX}:${workspaceId}:${pubkey ?? "anonymous"}`;
}

function readSidebarLoadingShape(
  cacheKey: string | null,
): SidebarLoadingShape | null {
  if (!cacheKey || typeof window === "undefined") return null;

  try {
    const raw = window.localStorage.getItem(cacheKey);
    return raw ? parseSidebarLoadingShape(JSON.parse(raw)) : null;
  } catch {
    return null;
  }
}

function writeSidebarLoadingShape(
  cacheKey: string | null,
  shape: SidebarLoadingShape,
) {
  if (
    !cacheKey ||
    !hasSidebarLoadingRows(shape) ||
    typeof window === "undefined"
  ) {
    return;
  }

  const payload: SidebarLoadingCachePayload = {
    channels: shape.channels.slice(0, 3),
    directMessages: shape.directMessages.slice(0, 2),
    version: 1,
  };

  try {
    window.localStorage.setItem(cacheKey, JSON.stringify(payload));
  } catch {
    // localStorage can be unavailable or full in embedded webviews.
  }
}

function sidebarWidthClassForText(text: string): SidebarLoadingWidthClass {
  const length = text.trim().length;
  if (length >= 20) return "w-32";
  if (length >= 14) return "w-28";
  if (length >= 10) return "w-24";
  if (length >= 6) return "w-20";
  return "w-16";
}

function createSidebarLoadingShape({
  directMessages,
  dmChannelLabels,
  streamChannels,
}: {
  directMessages: Channel[];
  dmChannelLabels: Record<string, string>;
  streamChannels: Channel[];
}): SidebarLoadingShape {
  return {
    channels: streamChannels.slice(0, 3).map((channel) => ({
      key: channel.id,
      widthClass: sidebarWidthClassForText(channel.name),
    })),
    directMessages: directMessages.slice(0, 2).map((channel) => ({
      avatar: true,
      key: channel.id,
      widthClass: sidebarWidthClassForText(
        dmChannelLabels[channel.id] ?? channel.name,
      ),
    })),
  };
}

function useSidebarLoadingShape({
  activeWorkspaceId,
  directMessages,
  dmChannelLabels,
  isLoading,
  currentPubkey,
  streamChannels,
}: {
  activeWorkspaceId: string | null | undefined;
  directMessages: Channel[];
  dmChannelLabels: Record<string, string>;
  isLoading: boolean;
  currentPubkey?: string;
  streamChannels: Channel[];
}) {
  const cacheKey = React.useMemo(
    () => sidebarSkeletonCacheKey(activeWorkspaceId, currentPubkey),
    [activeWorkspaceId, currentPubkey],
  );
  const liveShape = React.useMemo(
    () =>
      createSidebarLoadingShape({
        directMessages,
        dmChannelLabels,
        streamChannels,
      }),
    [directMessages, dmChannelLabels, streamChannels],
  );
  const cachedShape = React.useMemo(
    () => readSidebarLoadingShape(cacheKey),
    [cacheKey],
  );

  React.useEffect(() => {
    if (isLoading || !hasSidebarLoadingRows(liveShape)) return;
    writeSidebarLoadingShape(cacheKey, liveShape);
  }, [cacheKey, isLoading, liveShape]);

  if (hasSidebarLoadingRows(liveShape)) return liveShape;
  return cachedShape ?? fallbackSidebarLoadingShape;
}

function SidebarLoadingRow({
  avatar = false,
  widthClass,
}: {
  avatar?: boolean;
  widthClass: string;
}) {
  return (
    <SidebarMenuItem>
      <div className="flex h-8 items-center gap-2 rounded-md px-2">
        <Skeleton
          className={cn(
            "shrink-0",
            avatar ? "h-5 w-5 rounded-full" : "h-4 w-4 rounded-sm",
          )}
        />
        <Skeleton className={cn("h-4 min-w-0", widthClass)} />
      </div>
    </SidebarMenuItem>
  );
}

function SidebarLoadingSection({
  children,
  titleWidthClass,
}: {
  children: React.ReactNode;
  titleWidthClass: string;
}) {
  return (
    <SidebarGroup>
      <div className="group/sidebar-section relative">
        <SidebarGroupLabel asChild>
          <div className="flex h-7 w-fit max-w-[calc(100%-3rem)] items-center gap-1">
            <Skeleton className={cn("h-3.5", titleWidthClass)} />
          </div>
        </SidebarGroupLabel>
      </div>
      <SidebarGroupContent>
        <SidebarMenu>{children}</SidebarMenu>
      </SidebarGroupContent>
    </SidebarGroup>
  );
}

function SidebarLoadingContent({ shape }: { shape: SidebarLoadingShape }) {
  return (
    <div data-testid="sidebar-loading">
      <SidebarLoadingSection titleWidthClass="w-16">
        {shape.channels.map((row) => (
          <SidebarLoadingRow key={row.key} widthClass={row.widthClass} />
        ))}
      </SidebarLoadingSection>
      <SidebarLoadingSection titleWidthClass="w-24">
        {shape.directMessages.map((row) => (
          <SidebarLoadingRow avatar key={row.key} widthClass={row.widthClass} />
        ))}
      </SidebarLoadingSection>
    </div>
  );
}

function SidebarPrimaryNavLoading() {
  return (
    <SidebarMenu data-testid="sidebar-primary-nav-loading">
      {[
        { key: "home", widthClass: "w-16" },
        { key: "agents", widthClass: "w-20" },
      ].map((row) => (
        <SidebarMenuItem key={row.key}>
          <div className="flex h-8 items-center gap-2 rounded-md px-2">
            <Skeleton className="h-4 w-4 shrink-0 rounded-sm" />
            <Skeleton className={cn("h-4", row.widthClass)} />
          </div>
        </SidebarMenuItem>
      ))}
    </SidebarMenu>
  );
}

function SidebarProfileLoadingCard() {
  return (
    <div className="rounded-xl px-2 py-2" data-testid="sidebar-profile-loading">
      <div className="flex min-w-0 items-center gap-3">
        <Skeleton className="h-8 w-8 shrink-0 rounded-full" />
        <div className="min-w-0 flex-1">
          <Skeleton className="h-4 w-28 max-w-full" />
          <Skeleton className="mt-1.5 h-3 w-24 max-w-full" />
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// AppSidebar
// ---------------------------------------------------------------------------

export function AppSidebar({
  activeWorkspace,
  channels,
  currentPubkey,
  fallbackDisplayName,
  homeBadgeCount,
  isAddWorkspaceOpen,
  isLoading,
  isCreatingChannel,
  isCreatingForum,
  isOpeningDm,
  profile,
  selfPresenceStatus,
  errorMessage,
  selectedChannelId,
  selectedView,
  unreadChannelIds,
  workspaces,
  onAddWorkspace,
  onAddWorkspaceOpenChange,
  onCreateChannel,
  onCreateForum,
  onOpenAddWorkspace,
  onOpenBrowseChannels,
  onOpenBrowseForums,
  onHideDm,
  onMarkChannelUnread,
  onMarkChannelRead,
  onMarkAllChannelsRead,
  onOpenDm,
  onUpdateWorkspace,
  onRemoveWorkspace,
  onSelectAgents,
  onSelectProjects,
  onSelectPulse,
  onSelectWorkflows,
  onSelectHome,
  onSelectChannel,
  onSelectSettings,
  onSetPresenceStatus,
  onSetUserStatus,
  onClearUserStatus,
  onSwitchWorkspace,
  selfUserStatus,
  isPresencePending,
  isNewDmOpen: isNewDmOpenProp,
  onNewDmOpenChange,
  isCreateChannelOpen: isCreateChannelOpenProp,
  onCreateChannelOpenChange,
  mutedChannelIds,
  onMuteChannel,
  onUnmuteChannel,
  starredChannelIds,
  onStarChannel,
  onUnstarChannel,
}: AppSidebarProps) {
  const [isNewDmOpenInternal, setIsNewDmOpenInternal] = React.useState(false);
  const isNewDmOpen = isNewDmOpenProp ?? isNewDmOpenInternal;
  const setIsNewDmOpen = onNewDmOpenChange ?? setIsNewDmOpenInternal;
  const scrollRef = React.useRef<HTMLDivElement>(null);
  useSidebarScrollLock(scrollRef);
  const [createDialogKind, setCreateDialogKind] =
    React.useState<CreateChannelKind | null>(null);

  // Allow the create-channel dialog to be opened from outside (e.g. the
  // ⌘⇧N global shortcut in AppShell), mirroring the controlled new-DM lift.
  // When the external flag flips on, open the "stream" create dialog; the
  // close direction is reported back via `onCreateChannelOpenChange` in the
  // dialog's `onOpenChange` below.
  React.useEffect(() => {
    if (isCreateChannelOpenProp) {
      setCreateDialogKind("stream");
    }
  }, [isCreateChannelOpenProp]);
  const [collapsedGroups, setCollapsedGroups] = React.useState<
    Record<CollapsibleSidebarGroup, boolean>
  >({
    starred: false,
    channels: false,
    forums: false,
    directMessages: false,
  });

  const toggleCollapsedGroup = React.useCallback(
    (group: CollapsibleSidebarGroup) => {
      setCollapsedGroups((current) => ({
        ...current,
        [group]: !current[group],
      }));
    },
    [],
  );

  const [collapsedSections, setCollapsedSections] = React.useState<
    Record<string, boolean>
  >({});
  const toggleCollapsedSection = React.useCallback((sectionId: string) => {
    setCollapsedSections((current) => ({
      ...current,
      [sectionId]: !current[sectionId],
    }));
  }, []);

  const {
    sections: channelSections,
    assignments: channelAssignments,
    createSection,
    renameSection,
    deleteSection,
    moveSectionUp,
    moveSectionDown,
    reorderSections,
    assignChannel,
    unassignChannel,
  } = useChannelSections(currentPubkey);

  const [createSectionState, setCreateSectionState] = React.useState<{
    open: boolean;
    pendingChannelId: string | null;
  }>({ open: false, pendingChannelId: null });
  const [renameSectionTarget, setRenameSectionTarget] =
    React.useState<ChannelSection | null>(null);
  const [deleteSectionTarget, setDeleteSectionTarget] =
    React.useState<ChannelSection | null>(null);

  const sectionIds = React.useMemo(
    () => channelSections.map((s) => s.id),
    [channelSections],
  );

  const streamChannels = React.useMemo(
    () => channels.filter((channel) => channel.channelType === "stream"),
    [channels],
  );

  const sectionBuckets = React.useMemo(() => {
    const bySection: Record<string, Channel[]> = {};
    const unassigned: Channel[] = [];
    const sectionIds = new Set(channelSections.map((s) => s.id));

    for (const channel of streamChannels) {
      if (starredChannelIds?.has(channel.id)) continue;
      const sectionId = channelAssignments[channel.id];
      if (sectionId && sectionIds.has(sectionId)) {
        if (!bySection[sectionId]) {
          bySection[sectionId] = [];
        }
        bySection[sectionId].push(channel);
      } else {
        unassigned.push(channel);
      }
    }
    return { bySection, unassigned };
  }, [streamChannels, channelSections, channelAssignments, starredChannelIds]);

  const starredChannels = React.useMemo(() => {
    if (!starredChannelIds || starredChannelIds.size === 0) return [];
    return streamChannels.filter((channel) =>
      starredChannelIds.has(channel.id),
    );
  }, [streamChannels, starredChannelIds]);

  const handleCreateSectionForChannel = React.useCallback(
    (channelId: string) => {
      setCreateSectionState({ open: true, pendingChannelId: channelId });
    },
    [],
  );

  const handleCreateSectionConfirm = React.useCallback(
    (name: string) => {
      const section = createSection(name);
      if (!section) {
        return;
      }
      if (createSectionState.pendingChannelId) {
        assignChannel(createSectionState.pendingChannelId, section.id);
      }
      setCreateSectionState({ open: false, pendingChannelId: null });
    },
    [createSection, assignChannel, createSectionState.pendingChannelId],
  );

  const forumChannels = React.useMemo(
    () => channels.filter((channel) => channel.channelType === "forum"),
    [channels],
  );
  const directMessages = React.useMemo(
    () => channels.filter((channel) => channel.channelType === "dm"),
    [channels],
  );
  const isSelectedDirectMessage =
    selectedView === "channel" &&
    directMessages.some((channel) => channel.id === selectedChannelId);
  const shouldLoadDmMetadata = useDeferredLoad({
    immediate: isSelectedDirectMessage,
    timeoutMs: 400,
  });
  const { dmChannelLabels, dmParticipantsByChannelId, dmPresenceByChannelId } =
    useDmSidebarMetadata({
      currentPubkey,
      directMessages,
      enabled: shouldLoadDmMetadata,
      fallbackDisplayName,
      profileDisplayName: profile?.displayName,
    });
  const sidebarLoadingShape = useSidebarLoadingShape({
    activeWorkspaceId: activeWorkspace?.id,
    currentPubkey,
    directMessages,
    dmChannelLabels,
    isLoading,
    streamChannels,
  });
  const shouldLoadAgentCount = useDeferredLoad({
    immediate: selectedView === "agents",
    timeoutMs: 250,
  });
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
  const {
    scrollToNextAbove,
    scrollToNextBelow,
    unreadAboveCount,
    unreadBelowCount,
  } = useUnreadOverflow({ scrollRef, unreadChannelIds });

  const isCreatingAny =
    createDialogKind === "stream"
      ? isCreatingChannel
      : createDialogKind === "forum"
        ? isCreatingForum
        : false;

  const handleCreateFromDialog = React.useCallback(
    async (input: {
      name: string;
      description?: string;
      visibility: ChannelVisibility;
      ttlSeconds?: number;
      templateId?: string;
    }) => {
      if (createDialogKind === "stream") {
        await onCreateChannel(input);
      } else if (createDialogKind === "forum") {
        await onCreateForum(input);
      }
    },
    [createDialogKind, onCreateChannel, onCreateForum],
  );

  return (
    <Sidebar
      className="!border-r-0"
      collapsible="offcanvas"
      data-testid="app-sidebar"
      variant="sidebar"
    >
      <SidebarHeader
        className="cursor-default select-none pt-11"
        data-tauri-drag-region
      >
        {isLoading ? (
          <SidebarPrimaryNavLoading />
        ) : (
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
                  className="right-2 rounded-full bg-primary/15 px-1.5 text-[11px] text-primary peer-data-[active=true]/menu-button:bg-sidebar-active-foreground/20 peer-data-[active=true]/menu-button:text-sidebar-active-foreground"
                  data-testid="sidebar-home-count"
                >
                  {Math.min(homeBadgeCount, 99)}
                </SidebarMenuBadge>
              ) : null}
            </SidebarMenuItem>
            <FeatureGate feature="pulse">
              <SidebarMenuItem>
                <SidebarMenuButton
                  data-testid="open-pulse-view"
                  isActive={selectedView === "pulse"}
                  onClick={onSelectPulse}
                  tooltip="Pulse"
                  type="button"
                >
                  <Activity className="h-4 w-4" />
                  <span>Pulse</span>
                </SidebarMenuButton>
              </SidebarMenuItem>
            </FeatureGate>
            <FeatureGate feature="projects">
              <SidebarMenuItem>
                <SidebarMenuButton
                  data-testid="open-projects-view"
                  isActive={selectedView === "projects"}
                  onClick={onSelectProjects}
                  tooltip="Projects"
                  type="button"
                >
                  <FolderGit2 className="h-4 w-4" />
                  <span>Projects</span>
                </SidebarMenuButton>
              </SidebarMenuItem>
            </FeatureGate>
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
                  className="right-2 rounded-full bg-sidebar-accent/70 px-1.5 text-[11px] text-sidebar-foreground/75 peer-data-[active=true]/menu-button:bg-sidebar-active-foreground/20 peer-data-[active=true]/menu-button:text-sidebar-active-foreground"
                  data-testid="sidebar-agents-count"
                >
                  {totalAgentCount}
                </SidebarMenuBadge>
              ) : null}
            </SidebarMenuItem>
            <FeatureGate feature="workflows">
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
            </FeatureGate>
          </SidebarMenu>
        )}
      </SidebarHeader>

      <div className="relative flex min-h-0 flex-1 flex-col overflow-hidden">
        {unreadAboveCount > 0 ? (
          <MoreUnreadButton
            count={unreadAboveCount}
            icon={<ArrowUp />}
            onClick={scrollToNextAbove}
            position="top"
            testId="sidebar-more-unread-above"
          />
        ) : null}
        <SidebarContent className="pb-32" ref={scrollRef}>
          {isLoading ? (
            <SidebarLoadingContent shape={sidebarLoadingShape} />
          ) : null}

          {!isLoading ? (
            <>
              {starredChannels.length > 0 ? (
                <ChannelGroupSection
                  browseAriaLabel="Starred channels"
                  createAriaLabel="Starred channels"
                  hasUnread={starredChannels.some((c) =>
                    unreadChannelIds.has(c.id),
                  )}
                  isCollapsed={collapsedGroups.starred}
                  isActiveChannel={selectedView === "channel"}
                  items={starredChannels}
                  listTestId="starred-list"
                  onMarkAllRead={() => {
                    for (const channel of starredChannels) {
                      onMarkChannelRead(channel.id, channel.lastMessageAt);
                    }
                  }}
                  onMarkChannelRead={onMarkChannelRead}
                  onMarkChannelUnread={onMarkChannelUnread}
                  onSelectChannel={onSelectChannel}
                  onToggleCollapsed={() => toggleCollapsedGroup("starred")}
                  selectedChannelId={selectedChannelId}
                  title="Starred"
                  unreadChannelIds={unreadChannelIds}
                  mutedChannelIds={mutedChannelIds}
                  onMuteChannel={onMuteChannel}
                  onUnmuteChannel={onUnmuteChannel}
                  starredChannelIds={starredChannelIds}
                  onStarChannel={onStarChannel}
                  onUnstarChannel={onUnstarChannel}
                />
              ) : null}
              <SidebarDndContext
                channels={channels}
                sections={channelSections}
                sectionIds={sectionIds}
                onAssignChannel={assignChannel}
                onUnassignChannel={unassignChannel}
                onReorderSections={reorderSections}
              >
                {channelSections.map((section, idx) => (
                  <CustomChannelSection
                    key={section.id}
                    section={section}
                    channels={sectionBuckets.bySection[section.id] ?? []}
                    hasUnread={
                      sectionBuckets.bySection[section.id]?.some((c) =>
                        unreadChannelIds.has(c.id),
                      ) ?? false
                    }
                    isCollapsed={collapsedSections[section.id] ?? false}
                    isActiveChannel={selectedView === "channel"}
                    selectedChannelId={selectedChannelId}
                    unreadChannelIds={unreadChannelIds}
                    sections={channelSections}
                    assignments={channelAssignments}
                    isFirst={idx === 0}
                    isLast={idx === channelSections.length - 1}
                    onToggleCollapsed={() => toggleCollapsedSection(section.id)}
                    onSelectChannel={onSelectChannel}
                    onMarkChannelRead={onMarkChannelRead}
                    onMarkChannelUnread={onMarkChannelUnread}
                    onMarkSectionRead={() => {
                      for (const channel of sectionBuckets.bySection[
                        section.id
                      ] ?? []) {
                        onMarkChannelRead(channel.id, channel.lastMessageAt);
                      }
                    }}
                    onAssignChannel={assignChannel}
                    onUnassignChannel={unassignChannel}
                    onCreateSectionForChannel={handleCreateSectionForChannel}
                    onRenameSection={() => setRenameSectionTarget(section)}
                    onDeleteSection={() => setDeleteSectionTarget(section)}
                    onMoveSectionUp={() => moveSectionUp(section.id)}
                    onMoveSectionDown={() => moveSectionDown(section.id)}
                    mutedChannelIds={mutedChannelIds}
                    onMuteChannel={onMuteChannel}
                    onUnmuteChannel={onUnmuteChannel}
                    starredChannelIds={starredChannelIds}
                    onStarChannel={onStarChannel}
                    onUnstarChannel={onUnstarChannel}
                  />
                ))}
                <ChannelGroupSection
                  browseAriaLabel="Browse channels"
                  browseTestId="browse-channels"
                  createAriaLabel="Create a channel"
                  draggable
                  groupClassName={
                    channelSections.length > 0 ? undefined : "pt-1"
                  }
                  hasUnread={unreadChannelIds.size > 0}
                  isCollapsed={collapsedGroups.channels}
                  isActiveChannel={selectedView === "channel"}
                  items={sectionBuckets.unassigned}
                  listTestId="stream-list"
                  onBrowse={onOpenBrowseChannels}
                  onCreateClick={() => setCreateDialogKind("stream")}
                  onMarkAllRead={onMarkAllChannelsRead}
                  onMarkChannelRead={onMarkChannelRead}
                  onMarkChannelUnread={onMarkChannelUnread}
                  onSelectChannel={onSelectChannel}
                  onToggleCollapsed={() => toggleCollapsedGroup("channels")}
                  selectedChannelId={selectedChannelId}
                  title="Channels"
                  unreadChannelIds={unreadChannelIds}
                  sections={channelSections}
                  assignments={channelAssignments}
                  onAssignChannel={assignChannel}
                  onUnassignChannel={unassignChannel}
                  onCreateSectionForChannel={handleCreateSectionForChannel}
                  mutedChannelIds={mutedChannelIds}
                  onMuteChannel={onMuteChannel}
                  onUnmuteChannel={onUnmuteChannel}
                  starredChannelIds={starredChannelIds}
                  onStarChannel={onStarChannel}
                  onUnstarChannel={onUnstarChannel}
                />
              </SidebarDndContext>
              <FeatureGate feature="forum">
                <ChannelGroupSection
                  browseAriaLabel="Browse forums"
                  browseTestId="browse-forums"
                  createAriaLabel="Create a forum"
                  hasUnread={unreadChannelIds.size > 0}
                  isCollapsed={collapsedGroups.forums}
                  isActiveChannel={selectedView === "channel"}
                  items={forumChannels}
                  listTestId="forum-list"
                  onBrowse={onOpenBrowseForums}
                  onCreateClick={() => setCreateDialogKind("forum")}
                  onMarkAllRead={onMarkAllChannelsRead}
                  onMarkChannelRead={onMarkChannelRead}
                  onMarkChannelUnread={onMarkChannelUnread}
                  onSelectChannel={onSelectChannel}
                  onToggleCollapsed={() => toggleCollapsedGroup("forums")}
                  selectedChannelId={selectedChannelId}
                  title="Forums"
                  unreadChannelIds={unreadChannelIds}
                  mutedChannelIds={mutedChannelIds}
                  onMuteChannel={onMuteChannel}
                  onUnmuteChannel={onUnmuteChannel}
                />
              </FeatureGate>
              <SidebarSection
                action={
                  <SidebarGroupAction
                    aria-expanded={isNewDmOpen}
                    aria-label="Start a direct message"
                    className={cn(
                      "top-1/2 -translate-y-1/2 text-sidebar-foreground/50 hover:bg-sidebar-border/35 hover:text-sidebar-foreground",
                      SECTION_ACTION_VISIBILITY_CLASS,
                    )}
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
                isCollapsed={collapsedGroups.directMessages}
                isActiveChannel={selectedView === "channel"}
                items={directMessages}
                channelLabels={dmChannelLabels}
                onHideDm={onHideDm}
                onMarkChannelRead={onMarkChannelRead}
                onMarkChannelUnread={onMarkChannelUnread}
                onSelectChannel={onSelectChannel}
                onToggleCollapsed={() => toggleCollapsedGroup("directMessages")}
                presenceByChannelId={dmPresenceByChannelId}
                selectedChannelId={selectedChannelId}
                testId="dm-list"
                title="Direct Messages"
                unreadChannelIds={unreadChannelIds}
                mutedChannelIds={mutedChannelIds}
                onMuteChannel={onMuteChannel}
                onUnmuteChannel={onUnmuteChannel}
              />
            </>
          ) : null}

          {errorMessage ? (
            <div className="px-3 py-2 text-sm text-destructive">
              {errorMessage}
            </div>
          ) : null}
        </SidebarContent>

        {unreadBelowCount > 0 ? (
          <MoreUnreadButton
            bottomClassName="bottom-28"
            count={unreadBelowCount}
            icon={<ArrowDown />}
            onClick={scrollToNextBelow}
            position="bottom"
            testId="sidebar-more-unread-below"
          />
        ) : null}

        <SidebarFooter className="absolute inset-x-0 bottom-0 z-30 bg-sidebar/55 backdrop-blur-xl supports-[backdrop-filter]:bg-sidebar/45 dark:bg-sidebar/45 dark:supports-[backdrop-filter]:bg-sidebar/35">
          <SidebarMenu>
            <SidebarMenuItem>
              {isLoading ? (
                <SidebarProfileLoadingCard />
              ) : (
                <SidebarProfileCard
                  activeWorkspace={activeWorkspace}
                  isPresencePending={isPresencePending}
                  onOpenAddWorkspace={onOpenAddWorkspace}
                  onOpenSettings={onSelectSettings}
                  onRemoveWorkspace={onRemoveWorkspace}
                  onSetPresenceStatus={onSetPresenceStatus}
                  onSetUserStatus={onSetUserStatus}
                  onClearUserStatus={onClearUserStatus}
                  onSwitchWorkspace={onSwitchWorkspace}
                  onUpdateWorkspace={onUpdateWorkspace}
                  profile={profile}
                  resolvedDisplayName={resolvedDisplayName}
                  selfPresenceStatus={selfPresenceStatus}
                  selfUserStatus={selfUserStatus}
                  workspaces={workspaces}
                />
              )}
            </SidebarMenuItem>
          </SidebarMenu>
        </SidebarFooter>
      </div>

      <CreateChannelDialog
        channelKind={createDialogKind}
        isCreating={isCreatingAny}
        onOpenChange={(open) => {
          if (!open) {
            // If a "stream" dialog driven by the external controller is
            // closing, report it back so AppShell's open state resets.
            if (createDialogKind === "stream") {
              onCreateChannelOpenChange?.(false);
            }
            setCreateDialogKind(null);
          }
        }}
        onCreate={handleCreateFromDialog}
      />

      <NewDirectMessageDialog
        currentPubkey={currentPubkey}
        isPending={isOpeningDm}
        onOpenChange={setIsNewDmOpen}
        onSubmit={onOpenDm}
        open={isNewDmOpen}
      />

      <AddWorkspaceDialog
        onOpenChange={onAddWorkspaceOpenChange ?? (() => {})}
        onSubmit={onAddWorkspace}
        open={isAddWorkspaceOpen ?? false}
      />

      <CreateSectionDialog
        open={createSectionState.open}
        onOpenChange={(open) => {
          if (!open) {
            setCreateSectionState({ open: false, pendingChannelId: null });
          }
        }}
        onConfirm={handleCreateSectionConfirm}
      />

      <RenameSectionDialog
        open={renameSectionTarget !== null}
        onOpenChange={(open) => {
          if (!open) setRenameSectionTarget(null);
        }}
        sectionName={renameSectionTarget?.name ?? ""}
        onConfirm={(newName) => {
          if (renameSectionTarget) {
            renameSection(renameSectionTarget.id, newName);
          }
          setRenameSectionTarget(null);
        }}
      />

      <DeleteSectionAlertDialog
        open={deleteSectionTarget !== null}
        onOpenChange={(open) => {
          if (!open) setDeleteSectionTarget(null);
        }}
        sectionName={deleteSectionTarget?.name ?? ""}
        channelCount={
          deleteSectionTarget
            ? (sectionBuckets.bySection[deleteSectionTarget.id]?.length ?? 0)
            : 0
        }
        onConfirm={() => {
          if (deleteSectionTarget) {
            deleteSection(deleteSectionTarget.id);
            setCollapsedSections((prev) => {
              const next = { ...prev };
              delete next[deleteSectionTarget.id];
              return next;
            });
          }
          setDeleteSectionTarget(null);
        }}
      />
      <SidebarRail />
    </Sidebar>
  );
}
