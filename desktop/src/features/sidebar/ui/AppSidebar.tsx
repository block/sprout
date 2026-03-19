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
  onOpenBrowseForums: () => void;
  onOpenSearch: () => void;
  onOpenDm: (input: { pubkeys: string[] }) => Promise<void>;
  onSelectAgents: () => void;
  onSelectHome: () => void;
  onSelectChannel: (channelId: string) => void;
  onSelectSettings: () => void;
};

// ---------------------------------------------------------------------------
// useCreateForm — shared state + handler for channel/forum creation
// ---------------------------------------------------------------------------

function useCreateForm(
  onCreate: (input: { name: string; description?: string }) => Promise<void>,
  entityLabel: string,
) {
  const [isOpen, setIsOpen] = React.useState(false);
  const [draftName, setDraftName] = React.useState("");
  const [draftDescription, setDraftDescription] = React.useState("");
  const [errorMessage, setErrorMessage] = React.useState<string | undefined>();
  const inputRef = React.useRef<HTMLInputElement>(null);

  React.useEffect(() => {
    if (isOpen) {
      inputRef.current?.focus();
    }
  }, [isOpen]);

  function toggle() {
    setErrorMessage(undefined);
    setIsOpen((current) => !current);
  }

  function cancel() {
    setErrorMessage(undefined);
    setDraftName("");
    setDraftDescription("");
    setIsOpen(false);
  }

  function changeName(value: string) {
    setErrorMessage(undefined);
    setDraftName(value);
  }

  function changeDescription(value: string) {
    setErrorMessage(undefined);
    setDraftDescription(value);
  }

  async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();

    const name = draftName.trim();
    const description = draftDescription.trim();
    if (!name) {
      return;
    }

    setErrorMessage(undefined);

    try {
      await onCreate({
        name,
        description: description || undefined,
      });

      setDraftName("");
      setDraftDescription("");
      setIsOpen(false);
    } catch (error) {
      setErrorMessage(
        error instanceof Error
          ? error.message
          : `Failed to create ${entityLabel}.`,
      );
    }
  }

  return {
    isOpen,
    draftName,
    draftDescription,
    errorMessage,
    inputRef,
    toggle,
    cancel,
    changeName,
    changeDescription,
    handleSubmit,
  };
}

// ---------------------------------------------------------------------------
// SectionHeaderActions — search + create icon buttons for section headers
// ---------------------------------------------------------------------------

function SectionHeaderActions({
  browseAriaLabel,
  browseTestId,
  createAriaLabel,
  closeAriaLabel,
  isCreateOpen,
  onBrowse,
  onToggleCreate,
}: {
  browseAriaLabel: string;
  browseTestId?: string;
  createAriaLabel: string;
  closeAriaLabel: string;
  isCreateOpen: boolean;
  onBrowse: () => void;
  onToggleCreate: () => void;
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
        aria-expanded={isCreateOpen}
        aria-label={isCreateOpen ? closeAriaLabel : createAriaLabel}
        className={SECTION_ICON_BUTTON_CLASS}
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
  );
}

// ---------------------------------------------------------------------------
// ChannelGroupSection — unified Channels / Forums section
// ---------------------------------------------------------------------------

function ChannelGroupSection({
  browseAriaLabel,
  browseTestId,
  closeAriaLabel,
  createAriaLabel,
  createFormTestId,
  createNameTestId,
  createDescriptionTestId,
  groupClassName,
  isActiveChannel,
  isCreating,
  items,
  listTestId,
  namePlaceholder,
  descriptionPlaceholder,
  onBrowse,
  onSelectChannel,
  selectedChannelId,
  title,
  unreadChannelIds,
  form,
}: {
  browseAriaLabel: string;
  browseTestId?: string;
  closeAriaLabel: string;
  createAriaLabel: string;
  createFormTestId: string;
  createNameTestId: string;
  createDescriptionTestId: string;
  groupClassName?: string;
  isActiveChannel: boolean;
  isCreating: boolean;
  items: Channel[];
  listTestId: string;
  namePlaceholder: string;
  descriptionPlaceholder: string;
  onBrowse: () => void;
  onSelectChannel: (channelId: string) => void;
  selectedChannelId: string | null;
  title: string;
  unreadChannelIds: Set<string>;
  form: ReturnType<typeof useCreateForm>;
}) {
  return (
    <SidebarGroup className={groupClassName}>
      <SidebarGroupLabel>{title}</SidebarGroupLabel>
      <SectionHeaderActions
        browseAriaLabel={browseAriaLabel}
        browseTestId={browseTestId}
        closeAriaLabel={closeAriaLabel}
        createAriaLabel={createAriaLabel}
        isCreateOpen={form.isOpen}
        onBrowse={onBrowse}
        onToggleCreate={form.toggle}
      />
      <SidebarGroupContent>
        {form.isOpen ? (
          <form
            className="mb-2 space-y-2 rounded-lg border border-sidebar-border/70 bg-sidebar-accent/60 p-2"
            data-testid={createFormTestId}
            onSubmit={(event) => {
              void form.handleSubmit(event);
            }}
          >
            <Input
              autoComplete="off"
              autoCapitalize="none"
              autoCorrect="off"
              className="h-8 bg-background/80"
              data-testid={createNameTestId}
              disabled={isCreating}
              onChange={(event) => form.changeName(event.target.value)}
              placeholder={namePlaceholder}
              ref={form.inputRef}
              spellCheck={false}
              value={form.draftName}
            />
            <Input
              autoComplete="off"
              className="h-8 bg-background/80"
              data-testid={createDescriptionTestId}
              disabled={isCreating}
              onChange={(event) => form.changeDescription(event.target.value)}
              placeholder={descriptionPlaceholder}
              value={form.draftDescription}
            />
            <div className="flex items-center gap-2">
              <Button
                disabled={isCreating || form.draftName.trim().length === 0}
                size="sm"
                type="submit"
              >
                {isCreating ? "Creating..." : "Create"}
              </Button>
              <Button
                disabled={isCreating}
                onClick={form.cancel}
                size="sm"
                type="button"
                variant="ghost"
              >
                Cancel
              </Button>
            </div>
            {form.errorMessage ? (
              <p className="text-sm text-destructive">{form.errorMessage}</p>
            ) : null}
          </form>
        ) : null}

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
  onOpenBrowseForums,
  onOpenSearch,
  onOpenDm,
  onSelectAgents,
  onSelectHome,
  onSelectChannel,
  onSelectSettings,
}: AppSidebarProps) {
  const skeletonRows = ["first", "second", "third", "fourth", "fifth", "sixth"];
  const [isNewDmOpen, setIsNewDmOpen] = React.useState(false);

  const streamForm = useCreateForm(onCreateChannel, "stream");
  const forumForm = useCreateForm(onCreateForum, "forum");

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
            <ChannelGroupSection
              browseAriaLabel="Browse channels"
              browseTestId="browse-channels"
              closeAriaLabel="Close new stream form"
              createAriaLabel="Create a stream"
              createFormTestId="create-stream-form"
              createNameTestId="create-stream-name"
              createDescriptionTestId="create-stream-description"
              descriptionPlaceholder="What this stream is for"
              form={streamForm}
              groupClassName="pt-1"
              isActiveChannel={selectedView === "channel"}
              isCreating={isCreatingChannel}
              items={streamChannels}
              listTestId="stream-list"
              namePlaceholder="release-notes"
              onBrowse={onOpenBrowseChannels}
              onSelectChannel={onSelectChannel}
              selectedChannelId={selectedChannelId}
              title="Channels"
              unreadChannelIds={unreadChannelIds}
            />
            <ChannelGroupSection
              browseAriaLabel="Browse forums"
              browseTestId="browse-forums"
              closeAriaLabel="Close new forum form"
              createAriaLabel="New forum"
              createFormTestId="create-forum-form"
              createNameTestId="create-forum-name"
              createDescriptionTestId="create-forum-description"
              descriptionPlaceholder="What this forum is for"
              form={forumForm}
              isActiveChannel={selectedView === "channel"}
              isCreating={isCreatingForum}
              items={forumChannels}
              listTestId="forum-list"
              namePlaceholder="design-discussions"
              onBrowse={onOpenBrowseForums}
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
