import { getCurrentWindow } from "@tauri-apps/api/window";
import { CircleDot, FileText, Hash, Home, Plus, Search } from "lucide-react";
import * as React from "react";

import { getPresenceLabel } from "@/features/presence/lib/presence";
import { usePresenceQuery } from "@/features/presence/hooks";
import { PresenceDot } from "@/features/presence/ui/PresenceBadge";
import { ProfileAvatar } from "@/features/profile/ui/ProfileAvatar";
import type { Channel, PresenceStatus, Profile } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
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
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSkeleton,
  SidebarSeparator,
} from "@/shared/ui/sidebar";

type AppSidebarProps = {
  channels: Channel[];
  currentPubkey?: string;
  fallbackDisplayName?: string;
  isLoading: boolean;
  isCreatingChannel: boolean;
  profile?: Profile;
  selfPresenceStatus: PresenceStatus;
  errorMessage?: string;
  homeUrgentCount?: number;
  selectedChannelId: string | null;
  selectedView: "home" | "channel" | "settings";
  unreadChannelIds: Set<string>;
  onCreateChannel: (input: {
    name: string;
    description?: string;
  }) => Promise<void>;
  onOpenSearch: () => void;
  onSelectHome: () => void;
  onSelectChannel: (channelId: string) => void;
  onSelectSettings: () => void;
};

function SidebarChannelIcon({ channel }: { channel: Channel }) {
  if (channel.channelType === "dm") {
    return <CircleDot className="h-4 w-4" />;
  }

  if (channel.channelType === "forum") {
    return <FileText className="h-4 w-4" />;
  }

  return <Hash className="h-4 w-4" />;
}

function ChannelMenuButton({
  channel,
  isActive,
  hasUnread,
  presenceStatus,
  onSelectChannel,
}: {
  channel: Channel;
  isActive: boolean;
  hasUnread: boolean;
  presenceStatus?: PresenceStatus;
  onSelectChannel: (channelId: string) => void;
}) {
  return (
    <SidebarMenuButton
      className={cn(
        !isActive &&
          hasUnread &&
          "font-semibold text-sidebar-foreground hover:text-sidebar-foreground",
      )}
      data-testid={`channel-${channel.name}`}
      isActive={isActive}
      onClick={() => onSelectChannel(channel.id)}
      tooltip={channel.name}
      type="button"
    >
      <SidebarChannelIcon channel={channel} />
      <span className="min-w-0 flex-1 truncate">{channel.name}</span>
      <div className="ml-auto flex items-center gap-2">
        {presenceStatus ? (
          <PresenceDot
            className="h-2 w-2"
            data-testid={`channel-presence-${channel.name}`}
            status={presenceStatus}
          />
        ) : null}
        {hasUnread && !isActive ? (
          <span
            aria-hidden="true"
            className="h-2.5 w-2.5 shrink-0 rounded-full bg-primary"
            data-testid={`channel-unread-${channel.name}`}
          />
        ) : null}
      </div>
    </SidebarMenuButton>
  );
}

function SidebarSection({
  items,
  isActiveChannel,
  presenceByChannelId,
  selectedChannelId,
  title,
  testId,
  unreadChannelIds,
  onSelectChannel,
}: {
  items: Channel[];
  isActiveChannel: boolean;
  presenceByChannelId?: Record<string, PresenceStatus>;
  selectedChannelId: string | null;
  title: string;
  testId: string;
  unreadChannelIds: Set<string>;
  onSelectChannel: (channelId: string) => void;
}) {
  if (items.length === 0) {
    return null;
  }

  return (
    <SidebarGroup>
      <SidebarGroupLabel>{title}</SidebarGroupLabel>
      <SidebarGroupContent>
        <SidebarMenu data-testid={testId}>
          {items.map((channel) => (
            <SidebarMenuItem key={channel.id}>
              <ChannelMenuButton
                channel={channel}
                hasUnread={unreadChannelIds.has(channel.id)}
                isActive={isActiveChannel && selectedChannelId === channel.id}
                presenceStatus={presenceByChannelId?.[channel.id]}
                onSelectChannel={onSelectChannel}
              />
            </SidebarMenuItem>
          ))}
        </SidebarMenu>
      </SidebarGroupContent>
    </SidebarGroup>
  );
}

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
}) {
  return (
    <SidebarGroup className="pt-1">
      <SidebarGroupLabel>Channels</SidebarGroupLabel>
      <SidebarGroupAction
        aria-expanded={isCreateOpen}
        aria-label={isCreateOpen ? "Close new stream form" : "Create a stream"}
        className="top-3 text-sidebar-foreground/50 hover:bg-sidebar-accent/60 hover:text-sidebar-foreground"
        onClick={onToggleCreate}
        type="button"
      >
        <Plus
          className={
            isCreateOpen
              ? "rotate-45 transition-transform"
              : "transition-transform"
          }
        />
      </SidebarGroupAction>
      <SidebarGroupContent>
        {isCreateOpen ? (
          <form
            className="mb-2 space-y-2 rounded-lg border border-sidebar-border/70 bg-sidebar-accent/60 p-2"
            data-testid="create-stream-form"
            onSubmit={onCreateChannel}
          >
            <Input
              autoComplete="off"
              className="h-8 bg-background/80"
              data-testid="create-stream-name"
              disabled={isCreatingChannel}
              onChange={(event) => onChangeName(event.target.value)}
              placeholder="release-notes"
              ref={createInputRef}
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

export function AppSidebar({
  channels,
  currentPubkey,
  fallbackDisplayName,
  isLoading,
  isCreatingChannel,
  profile,
  selfPresenceStatus,
  errorMessage,
  homeUrgentCount,
  selectedChannelId,
  selectedView,
  unreadChannelIds,
  onCreateChannel,
  onOpenSearch,
  onSelectHome,
  onSelectChannel,
  onSelectSettings,
}: AppSidebarProps) {
  const skeletonRows = ["first", "second", "third", "fourth", "fifth", "sixth"];
  const [isCreateOpen, setIsCreateOpen] = React.useState(false);
  const [draftName, setDraftName] = React.useState("");
  const [draftDescription, setDraftDescription] = React.useState("");
  const [createErrorMessage, setCreateErrorMessage] = React.useState<
    string | undefined
  >();
  const createInputRef = React.useRef<HTMLInputElement>(null);
  const streamChannels = channels.filter(
    (channel) => channel.channelType === "stream",
  );
  const forumChannels = channels.filter(
    (channel) => channel.channelType === "forum",
  );
  const directMessages = channels.filter(
    (channel) => channel.channelType === "dm",
  );
  const dmParticipantPubkeys = React.useMemo(
    () =>
      directMessages
        .flatMap((channel) => channel.participantPubkeys)
        .filter(
          (pubkey) => pubkey.toLowerCase() !== currentPubkey?.toLowerCase(),
        ),
    [currentPubkey, directMessages],
  );
  const dmPresenceQuery = usePresenceQuery(dmParticipantPubkeys, {
    enabled: directMessages.length > 0,
  });
  const dmPresenceByChannelId = React.useMemo(
    () =>
      Object.fromEntries(
        directMessages.map((channel) => {
          const otherParticipantPubkey = channel.participantPubkeys.find(
            (pubkey) => pubkey.toLowerCase() !== currentPubkey?.toLowerCase(),
          );

          return [
            channel.id,
            otherParticipantPubkey
              ? (dmPresenceQuery.data?.[otherParticipantPubkey.toLowerCase()] ??
                "offline")
              : "offline",
          ];
        }),
      ) satisfies Record<string, PresenceStatus>,
    [currentPubkey, directMessages, dmPresenceQuery.data],
  );
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

  function handleDragPointerDown(e: React.PointerEvent) {
    if (e.button !== 0) return;
    const target = e.target as HTMLElement;
    if (target.closest('button, a, input, [role="button"]')) return;
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
        className="gap-3 pt-12"
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
              {homeUrgentCount && homeUrgentCount > 0 ? (
                <span className="ml-auto rounded-full bg-primary px-2 py-0.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-primary-foreground">
                  {homeUrgentCount}
                </span>
              ) : null}
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>

      <SidebarSeparator />

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
            <SidebarSection
              isActiveChannel={selectedView === "channel"}
              items={forumChannels}
              onSelectChannel={onSelectChannel}
              selectedChannelId={selectedChannelId}
              testId="forum-list"
              title="Forums"
              unreadChannelIds={unreadChannelIds}
            />
            <SidebarSection
              isActiveChannel={selectedView === "channel"}
              items={directMessages}
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

      <SidebarSeparator />

      <SidebarFooter>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton
              aria-pressed={selectedView === "settings"}
              className="h-auto gap-3 rounded-xl px-2 py-2"
              data-testid="open-settings"
              isActive={selectedView === "settings"}
              onClick={onSelectSettings}
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
    </Sidebar>
  );
}
