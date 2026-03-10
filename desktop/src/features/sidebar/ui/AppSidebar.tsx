import { CircleDot, FileText, Hash, Plus } from "lucide-react";
import * as React from "react";

import type { Channel } from "@/shared/api/types";
import { ThemeToggle } from "@/shared/theme/ThemeToggle";
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
  SidebarInput,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSkeleton,
  SidebarSeparator,
} from "@/shared/ui/sidebar";

type AppSidebarProps = {
  channels: Channel[];
  isLoading: boolean;
  isCreatingChannel: boolean;
  errorMessage?: string;
  selectedChannelId: string | null;
  onCreateChannel: (input: {
    name: string;
    description?: string;
  }) => Promise<void>;
  onSelectChannel: (channelId: string) => void;
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

function SidebarSection({
  items,
  selectedChannelId,
  title,
  onSelectChannel,
}: {
  items: Channel[];
  selectedChannelId: string | null;
  title: string;
  onSelectChannel: (channelId: string) => void;
}) {
  if (items.length === 0) {
    return null;
  }

  return (
    <SidebarGroup>
      <SidebarGroupLabel>{title}</SidebarGroupLabel>
      <SidebarGroupContent>
        <SidebarMenu>
          {items.map((channel) => (
            <SidebarMenuItem key={channel.id}>
              <SidebarMenuButton
                isActive={selectedChannelId === channel.id}
                onClick={() => onSelectChannel(channel.id)}
                tooltip={channel.name}
                type="button"
              >
                <SidebarChannelIcon channel={channel} />
                <span>{channel.name}</span>
              </SidebarMenuButton>
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
  selectedChannelId,
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
  selectedChannelId: string | null;
}) {
  return (
    <SidebarGroup>
      <SidebarGroupLabel>Streams</SidebarGroupLabel>
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
            onSubmit={onCreateChannel}
          >
            <Input
              autoComplete="off"
              className="h-8 bg-background/80"
              disabled={isCreatingChannel}
              onChange={(event) => onChangeName(event.target.value)}
              placeholder="release-notes"
              ref={createInputRef}
              value={draftName}
            />
            <Input
              autoComplete="off"
              className="h-8 bg-background/80"
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
          <SidebarMenu>
            {items.map((channel) => (
              <SidebarMenuItem key={channel.id}>
                <SidebarMenuButton
                  isActive={selectedChannelId === channel.id}
                  onClick={() => onSelectChannel(channel.id)}
                  tooltip={channel.name}
                  type="button"
                >
                  <SidebarChannelIcon channel={channel} />
                  <span>{channel.name}</span>
                </SidebarMenuButton>
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
  isLoading,
  isCreatingChannel,
  errorMessage,
  selectedChannelId,
  onCreateChannel,
  onSelectChannel,
}: AppSidebarProps) {
  const skeletonRows = ["first", "second", "third", "fourth", "fifth", "sixth"];
  const [query, setQuery] = React.useState("");
  const [isCreateOpen, setIsCreateOpen] = React.useState(false);
  const [draftName, setDraftName] = React.useState("");
  const [draftDescription, setDraftDescription] = React.useState("");
  const [createErrorMessage, setCreateErrorMessage] = React.useState<
    string | undefined
  >();
  const deferredQuery = React.useDeferredValue(query.trim().toLowerCase());
  const createInputRef = React.useRef<HTMLInputElement>(null);

  const filteredChannels = React.useMemo(() => {
    if (!deferredQuery) {
      return channels;
    }

    return channels.filter((channel) =>
      channel.name.toLowerCase().includes(deferredQuery),
    );
  }, [channels, deferredQuery]);

  const streamChannels = filteredChannels.filter(
    (channel) => channel.channelType === "stream",
  );
  const forumChannels = filteredChannels.filter(
    (channel) => channel.channelType === "forum",
  );
  const directMessages = filteredChannels.filter(
    (channel) => channel.channelType === "dm",
  );

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

  return (
    <Sidebar collapsible="offcanvas" variant="sidebar">
      <SidebarHeader className="gap-3">
        <div className="flex items-center gap-3 rounded-xl bg-sidebar-accent/80 px-3 py-3">
          <div className="flex h-6 w-6 items-center justify-center rounded-xl text-lg">
            <span aria-hidden="true">🌱</span>
          </div>
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm font-semibold">Sprout</p>
            <p className="truncate text-xs text-sidebar-foreground/65">
              Humans and agents, together
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <SidebarInput
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Jump to channel"
            value={query}
          />
        </div>
      </SidebarHeader>

      <SidebarSeparator />

      <SidebarContent>
        {isLoading ? (
          <SidebarGroup>
            <SidebarGroupLabel>Channels</SidebarGroupLabel>
            <SidebarGroupContent>
              <SidebarMenu>
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
            />
            <SidebarSection
              items={forumChannels}
              onSelectChannel={onSelectChannel}
              selectedChannelId={selectedChannelId}
              title="Forums"
            />
            <SidebarSection
              items={directMessages}
              onSelectChannel={onSelectChannel}
              selectedChannelId={selectedChannelId}
              title="Direct Messages"
            />
          </>
        ) : null}

        {!isLoading && filteredChannels.length === 0 ? (
          <div className="px-3 py-2 text-sm text-sidebar-foreground/70">
            No channels match that filter.
          </div>
        ) : null}

        {errorMessage ? (
          <div className="px-3 py-2 text-sm text-destructive">
            {errorMessage}
          </div>
        ) : null}
      </SidebarContent>

      <SidebarFooter className="items-end">
        <ThemeToggle className="text-sidebar-foreground/65 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground" />
      </SidebarFooter>
    </Sidebar>
  );
}
