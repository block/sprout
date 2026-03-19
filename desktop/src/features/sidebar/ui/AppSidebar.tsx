import { getCurrentWindow } from "@tauri-apps/api/window";
import { Bot, Home, PenSquare, Plus, Search } from "lucide-react";
import * as React from "react";

import { useManagedAgentsQuery } from "@/features/agents/hooks";
import { getPresenceLabel } from "@/features/presence/lib/presence";
import { PresenceDot } from "@/features/presence/ui/PresenceBadge";
import { ProfileAvatar } from "@/features/profile/ui/ProfileAvatar";
import { useDmSidebarMetadata } from "@/features/sidebar/useDmSidebarMetadata";
import {
  ChannelMenuButton,
  SidebarSection,
} from "@/features/sidebar/ui/SidebarSection";
import { NewDirectMessageDialog } from "@/features/sidebar/ui/NewDirectMessageDialog";
import type { Channel, PresenceStatus, Profile } from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
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

type AppSidebarProps = {
  channels: Channel[];
  currentPubkey?: string;
  fallbackDisplayName?: string;
  homeBadgeCount: number;
  isLoading: boolean;
  isCreatingChannel: boolean;
  isCreatingForum: boolean;
  isOpeningDm: boolean;
  profile?: Profile;
  selfPresenceStatus: PresenceStatus;
  errorMessage?: string;
  selectedChannelId: string | null;
  selectedView: "home" | "channel" | "settings" | "agents";
  unreadChannelIds: Set<string>;
  onCreateChannel: (input: {
    name: string;
    description?: string;
  }) => Promise<void>;
  onCreateForum: (input: {
    name: string;
    description?: string;
  }) => Promise<void>;
  onOpenBrowseChannels: () => void;
  onOpenSearch: () => void;
  onOpenDm: (input: { pubkeys: string[] }) => Promise<void>;
  onSelectAgents: () => void;
  onSelectHome: () => void;
  onSelectChannel: (channelId: string) => void;
  onSelectSettings: () => void;
};

function StreamsSection({
  items,
  isCreateOpen,
  isCreatingChannel,
  draftName,
  draftDescription,
  createInputRef,
  createErrorMessage,
  onToggleCreate,
  onChangeName,
  onChangeDescription,
  onCreateChannel,
  onCancelCreate,
  onSelectChannel,
  isActiveChannel,
  selectedChannelId,
  unreadChannelIds,
  onOpenBrowseChannels,
}: {
  items: Channel[];
  isCreateOpen: boolean;
  isCreatingChannel: boolean;
  draftName: string;
  draftDescription: string;
  createInputRef: React.RefObject<HTMLInputElement | null>;
  createErrorMessage?: string;
  onToggleCreate: () => void;
  onChangeName: (value: string) => void;
  onChangeDescription: (value: string) => void;
  onCreateChannel: (event: React.FormEvent<HTMLFormElement>) => void;
  onCancelCreate: () => void;
  onSelectChannel: (channelId: string) => void;
  isActiveChannel: boolean;
  selectedChannelId: string | null;
  unreadChannelIds: Set<string>;
  onOpenBrowseChannels: () => void;
}) {
  return (
    <SidebarGroup className="pt-1">
      <SidebarGroupLabel>Channels</SidebarGroupLabel>
      <div className="absolute right-1 top-3 flex items-center gap-0.5">
        <button
          aria-label="Browse channels"
          className="flex h-5 w-5 items-center justify-center rounded-md text-sidebar-foreground/50 hover:bg-sidebar-accent/60 hover:text-sidebar-foreground"
          onClick={onOpenBrowseChannels}
          type="button"
        >
          <Search className="h-3.5 w-3.5" />
        </button>
        <button
          aria-expanded={isCreateOpen}
          aria-label={
            isCreateOpen ? "Close new stream form" : "Create a stream"
          }
          className="flex h-5 w-5 items-center justify-center rounded-md text-sidebar-foreground/50 hover:bg-sidebar-accent/60 hover:text-sidebar-foreground"
          onClick={onToggleCreate}
          type="button"
        >
          <Plus
            className={
              isCreateOpen
                ? "h-4 w-4 rotate-45 transition-transform"
                : "h-4 w-4 transition-transform"
            }
          />
        </button>
      </div>
      <SidebarGroupContent>
        {isCreateOpen ? (
          <form
            className="mb-2 space-y-2 rounded-lg border border-sidebar-border/70 bg-sidebar-accent/60 p-2"
            data-testid="create-stream-form"
            onSubmit={onCreateChannel}
          >
            <Input
              autoComplete="off"
              autoCapitalize="none"
              autoCorrect="off"
              className="h-8 bg-background/80"
              data-testid="create-stream-name"
              disabled={isCreatingChannel}
              onChange={(event) => onChangeName(event.target.value)}
              placeholder="release-notes"
              ref={createInputRef}
              spellCheck={false}
              value={draftName}
            />
            <Input
              autoComplete="off"
              className="h-8 bg-background/80"
              data-testid="create-stream-description"
              disabled={isCreatingChannel}
              onChange={(event) => onChangeDescription(event.target.value)}
              placeholder="What this stream is for"
              value={draftDescription}
            />
            <div className="flex items-center gap-2">
              <Button
                disabled={isCreatingChannel || draftName.trim().length === 0}
                size="sm"
                type="submit"
              >
                {isCreatingChannel ? "Creating..." : "Create"}
              </Button>
              <Button
                disabled={isCreatingChannel}
                onClick={onCancelCreate}
                size="sm"
                type="button"
                variant="ghost"
              >
                Cancel
              </Button>
            </div>
            {createErrorMessage ? (
              <p className="text-sm text-destructive">{createErrorMessage}</p>
            ) : null}
          </form>
        ) : null}

        {items.length > 0 ? (
          <SidebarMenu data-testid="stream-list">
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

function ForumsSection({
  items,
  isCreateOpen,
  isCreatingForum,
  draftName,
  draftDescription,
  createInputRef,
  createErrorMessage,
  onToggleCreate,
  onChangeName,
  onChangeDescription,
  onCreateForum,
  onCancelCreate,
  onSelectChannel,
  isActiveChannel,
  selectedChannelId,
  unreadChannelIds,
  onOpenBrowseChannels,
}: {
  items: Channel[];
  isCreateOpen: boolean;
  isCreatingForum: boolean;
  draftName: string;
  draftDescription: string;
  createInputRef: React.RefObject<HTMLInputElement | null>;
  createErrorMessage?: string;
  onToggleCreate: () => void;
  onChangeName: (value: string) => void;
  onChangeDescription: (value: string) => void;
  onCreateForum: (event: React.FormEvent<HTMLFormElement>) => void;
  onCancelCreate: () => void;
  onSelectChannel: (channelId: string) => void;
  isActiveChannel: boolean;
  selectedChannelId: string | null;
  unreadChannelIds: Set<string>;
  onOpenBrowseChannels: () => void;
}) {
  return (
    <SidebarGroup>
      <SidebarGroupLabel>Forums</SidebarGroupLabel>
      <div className="absolute right-1 top-3 flex items-center gap-0.5">
        <button
          aria-label="Browse forums"
          className="flex h-5 w-5 items-center justify-center rounded-md text-sidebar-foreground/50 hover:bg-sidebar-accent/60 hover:text-sidebar-foreground"
          onClick={onOpenBrowseChannels}
          type="button"
        >
          <Search className="h-3.5 w-3.5" />
        </button>
        <button
          aria-expanded={isCreateOpen}
          aria-label={isCreateOpen ? "Close new forum form" : "New forum"}
          className="flex h-5 w-5 items-center justify-center rounded-md text-sidebar-foreground/50 hover:bg-sidebar-accent/60 hover:text-sidebar-foreground"
          onClick={onToggleCreate}
          type="button"
        >
          <Plus
            className={
              isCreateOpen
                ? "h-4 w-4 rotate-45 transition-transform"
                : "h-4 w-4 transition-transform"
            }
          />
        </button>
      </div>
      <SidebarGroupContent>
        {isCreateOpen ? (
          <form
            className="mb-2 space-y-2 rounded-lg border border-sidebar-border/70 bg-sidebar-accent/60 p-2"
            data-testid="create-forum-form"
            onSubmit={onCreateForum}
          >
            <Input
              autoComplete="off"
              autoCapitalize="none"
              autoCorrect="off"
              className="h-8 bg-background/80"
              data-testid="create-forum-name"
              disabled={isCreatingForum}
              onChange={(event) => onChangeName(event.target.value)}
              placeholder="design-discussions"
              ref={createInputRef}
              spellCheck={false}
              value={draftName}
            />
            <Input
              autoComplete="off"
              className="h-8 bg-background/80"
              data-testid="create-forum-description"
              disabled={isCreatingForum}
              onChange={(event) => onChangeDescription(event.target.value)}
              placeholder="What this forum is for"
              value={draftDescription}
            />
            <div className="flex items-center gap-2">
              <Button
                disabled={isCreatingForum || draftName.trim().length === 0}
                size="sm"
                type="submit"
              >
                {isCreatingForum ? "Creating..." : "Create"}
              </Button>
              <Button
                disabled={isCreatingForum}
                onClick={onCancelCreate}
                size="sm"
                type="button"
                variant="ghost"
              >
                Cancel
              </Button>
            </div>
            {createErrorMessage ? (
              <p className="text-sm text-destructive">{createErrorMessage}</p>
            ) : null}
          </form>
        ) : null}

        {items.length > 0 ? (
          <SidebarMenu data-testid="forum-list">
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

export function AppSidebar({
  channels,
  currentPubkey,
  fallbackDisplayName,
  homeBadgeCount,
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
  onCreateChannel,
  onCreateForum,
  onOpenBrowseChannels,
  onOpenSearch,
  onOpenDm,
  onSelectAgents,
  onSelectHome,
  onSelectChannel,
  onSelectSettings,
}: AppSidebarProps) {
  const skeletonRows = ["first", "second", "third", "fourth", "fifth", "sixth"];
  const [isCreateOpen, setIsCreateOpen] = React.useState(false);
  const [isForumCreateOpen, setIsForumCreateOpen] = React.useState(false);
  const [isNewDmOpen, setIsNewDmOpen] = React.useState(false);
  const [draftName, setDraftName] = React.useState("");
  const [draftDescription, setDraftDescription] = React.useState("");
  const [forumDraftName, setForumDraftName] = React.useState("");
  const [forumDraftDescription, setForumDraftDescription] = React.useState("");
  const [createErrorMessage, setCreateErrorMessage] = React.useState<
    string | undefined
  >();
  const [forumCreateErrorMessage, setForumCreateErrorMessage] = React.useState<
    string | undefined
  >();
  const createInputRef = React.useRef<HTMLInputElement>(null);
  const forumCreateInputRef = React.useRef<HTMLInputElement>(null);
  const streamChannels = channels.filter(
    (channel) => channel.channelType === "stream",
  );
  const forumChannels = channels.filter(
    (channel) => channel.channelType === "forum",
  );
  const directMessages = channels.filter(
    (channel) => channel.channelType === "dm",
  );
  const { dmChannelLabels, dmParticipantsByChannelId, dmPresenceByChannelId } =
    useDmSidebarMetadata({
      currentPubkey,
      directMessages,
      fallbackDisplayName,
      profileDisplayName: profile?.displayName,
    });
  const managedAgentsQuery = useManagedAgentsQuery();
  const totalAgentCount = managedAgentsQuery.data?.length ?? 0;
  const shouldShowAgentCount =
    totalAgentCount > 0 || !managedAgentsQuery.isLoading;
  const resolvedDisplayName =
    profile?.displayName?.trim() ||
    fallbackDisplayName?.trim() ||
    "Current identity";

  React.useEffect(() => {
    if (!isCreateOpen) {
      return;
    }

    createInputRef.current?.focus();
  }, [isCreateOpen]);

  async function handleCreateChannel(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();

    const name = draftName.trim();
    const description = draftDescription.trim();
    if (!name) {
      return;
    }

    setCreateErrorMessage(undefined);

    try {
      await onCreateChannel({
        name,
        description: description || undefined,
      });

      setDraftName("");
      setDraftDescription("");
      setIsCreateOpen(false);
    } catch (error) {
      setCreateErrorMessage(
        error instanceof Error ? error.message : "Failed to create stream.",
      );
    }
  }

  async function handleCreateForum(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();

    const name = forumDraftName.trim();
    const description = forumDraftDescription.trim();
    if (!name) {
      return;
    }

    setForumCreateErrorMessage(undefined);

    try {
      await onCreateForum({
        name,
        description: description || undefined,
      });

      setForumDraftName("");
      setForumDraftDescription("");
      setIsForumCreateOpen(false);
    } catch (error) {
      setForumCreateErrorMessage(
        error instanceof Error ? error.message : "Failed to create forum.",
      );
    }
  }

  React.useEffect(() => {
    if (!isForumCreateOpen) {
      return;
    }

    forumCreateInputRef.current?.focus();
  }, [isForumCreateOpen]);

  function handleDragPointerDown(e: React.PointerEvent) {
    if (e.button !== 0) return;
    const target = e.target as HTMLElement;
    if (
      target.closest(
        'button, a, input, textarea, select, [role="button"], [role="textbox"], [contenteditable="true"]',
      )
    ) {
      return;
    }
    e.preventDefault();
    getCurrentWindow().startDragging();
  }

  return (
    <Sidebar
      collapsible="offcanvas"
      data-testid="app-sidebar"
      variant="sidebar"
    >
      <SidebarHeader
        className="gap-3 pt-10"
        onPointerDown={handleDragPointerDown}
      >
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
            <StreamsSection
              createErrorMessage={createErrorMessage}
              createInputRef={createInputRef}
              draftDescription={draftDescription}
              draftName={draftName}
              isCreateOpen={isCreateOpen}
              isCreatingChannel={isCreatingChannel}
              isActiveChannel={selectedView === "channel"}
              items={streamChannels}
              onOpenBrowseChannels={onOpenBrowseChannels}
              onCancelCreate={() => {
                setCreateErrorMessage(undefined);
                setDraftName("");
                setDraftDescription("");
                setIsCreateOpen(false);
              }}
              onChangeDescription={(value) => {
                setCreateErrorMessage(undefined);
                setDraftDescription(value);
              }}
              onChangeName={(value) => {
                setCreateErrorMessage(undefined);
                setDraftName(value);
              }}
              onCreateChannel={(event) => {
                void handleCreateChannel(event);
              }}
              onSelectChannel={onSelectChannel}
              onToggleCreate={() => {
                setCreateErrorMessage(undefined);
                setIsCreateOpen((current) => !current);
              }}
              selectedChannelId={selectedChannelId}
              unreadChannelIds={unreadChannelIds}
            />
            <ForumsSection
              createErrorMessage={forumCreateErrorMessage}
              createInputRef={forumCreateInputRef}
              draftDescription={forumDraftDescription}
              draftName={forumDraftName}
              isActiveChannel={selectedView === "channel"}
              isCreateOpen={isForumCreateOpen}
              isCreatingForum={isCreatingForum}
              items={forumChannels}
              onOpenBrowseChannels={onOpenBrowseChannels}
              onCancelCreate={() => {
                setForumCreateErrorMessage(undefined);
                setForumDraftName("");
                setForumDraftDescription("");
                setIsForumCreateOpen(false);
              }}
              onChangeDescription={(value) => {
                setForumCreateErrorMessage(undefined);
                setForumDraftDescription(value);
              }}
              onChangeName={(value) => {
                setForumCreateErrorMessage(undefined);
                setForumDraftName(value);
              }}
              onCreateForum={(event) => {
                void handleCreateForum(event);
              }}
              onSelectChannel={onSelectChannel}
              onToggleCreate={() => {
                setForumCreateErrorMessage(undefined);
                setIsForumCreateOpen((current) => !current);
              }}
              selectedChannelId={selectedChannelId}
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
              emptyState="No direct messages yet."
              isActiveChannel={selectedView === "channel"}
              items={directMessages}
              channelLabels={dmChannelLabels}
              onSelectChannel={onSelectChannel}
              presenceByChannelId={dmPresenceByChannelId}
              selectedChannelId={selectedChannelId}
              testId="dm-list"
              title="Direct Messages"
              unreadChannelIds={unreadChannelIds}
            />
          </>
        ) : null}

        {!isLoading && channels.length === 0 ? (
          <div className="px-3 py-2 text-sm text-sidebar-foreground/70">
            No channels available yet.
          </div>
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
            <SidebarMenuButton
              aria-pressed={selectedView === "settings"}
              className="h-auto gap-3 rounded-xl px-2 py-2"
              data-testid="open-settings"
              isActive={selectedView === "settings"}
              onClick={() => onSelectSettings()}
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
