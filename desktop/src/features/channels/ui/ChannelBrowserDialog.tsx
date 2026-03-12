import * as React from "react";
import {
  Compass,
  FileText,
  Hash,
  LogIn,
  Search,
  Users,
  type LucideIcon,
} from "lucide-react";

import type { Channel } from "@/shared/api/types";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import { Input } from "@/shared/ui/input";
import { Button } from "@/shared/ui/button";

function formatRelativeTime(isoString: string | null) {
  if (!isoString) {
    return "No activity";
  }

  const diff = Math.floor((Date.now() - new Date(isoString).getTime()) / 1_000);

  if (diff < 60) {
    return "just now";
  }

  if (diff < 60 * 60) {
    return `${Math.floor(diff / 60)}m ago`;
  }

  if (diff < 60 * 60 * 24) {
    return `${Math.floor(diff / (60 * 60))}h ago`;
  }

  return new Intl.DateTimeFormat("en-US", {
    month: "short",
    day: "numeric",
  }).format(new Date(isoString));
}

function BrowseState({
  icon: Icon,
  title,
  description,
}: {
  icon: LucideIcon;
  title: string;
  description: string;
}) {
  return (
    <div className="flex flex-col items-center justify-center px-6 py-16 text-center">
      <div className="flex h-12 w-12 items-center justify-center rounded-2xl bg-primary/10 text-primary">
        <Icon className="h-5 w-5" />
      </div>
      <p className="mt-4 text-base font-semibold tracking-tight">{title}</p>
      <p className="mt-2 max-w-md text-sm text-muted-foreground">
        {description}
      </p>
    </div>
  );
}

type ChannelBrowserDialogProps = {
  channels: Channel[];
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onJoinChannel: (channelId: string) => Promise<void>;
  onSelectChannel: (channelId: string) => void;
};

export function ChannelBrowserDialog({
  channels,
  open,
  onOpenChange,
  onJoinChannel,
  onSelectChannel,
}: ChannelBrowserDialogProps) {
  const [query, setQuery] = React.useState("");
  const [selectedIndex, setSelectedIndex] = React.useState(0);
  const [joiningChannelId, setJoiningChannelId] = React.useState<string | null>(
    null,
  );
  const inputRef = React.useRef<HTMLInputElement>(null);
  const deferredQuery = React.useDeferredValue(query.trim().toLowerCase());

  const browsableChannels = React.useMemo(() => {
    const filtered = channels.filter(
      (channel) =>
        channel.channelType !== "dm" &&
        channel.visibility === "open" &&
        !channel.archivedAt,
    );

    if (deferredQuery.length === 0) {
      return filtered;
    }

    return filtered.filter(
      (channel) =>
        channel.name.toLowerCase().includes(deferredQuery) ||
        channel.description.toLowerCase().includes(deferredQuery),
    );
  }, [channels, deferredQuery]);

  const notJoined = React.useMemo(
    () => browsableChannels.filter((channel) => !channel.isMember),
    [browsableChannels],
  );

  const joined = React.useMemo(
    () => browsableChannels.filter((channel) => channel.isMember),
    [browsableChannels],
  );

  // Flat list for keyboard navigation: not-joined first, then joined
  const allItems = React.useMemo(
    () => [...notJoined, ...joined],
    [notJoined, joined],
  );

  React.useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (
        event.key.toLowerCase() !== "b" ||
        !(event.metaKey || event.ctrlKey) ||
        event.altKey ||
        !event.shiftKey
      ) {
        return;
      }

      event.preventDefault();
      onOpenChange(true);
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [onOpenChange]);

  React.useEffect(() => {
    if (!open) {
      setQuery("");
      setSelectedIndex(0);
      setJoiningChannelId(null);
      return;
    }

    const timeout = window.setTimeout(() => {
      inputRef.current?.focus();
      inputRef.current?.select();
    }, 0);

    return () => {
      window.clearTimeout(timeout);
    };
  }, [open]);

  React.useEffect(() => {
    setSelectedIndex((current) => {
      if (allItems.length === 0) {
        return 0;
      }

      return Math.min(current, allItems.length - 1);
    });
  }, [allItems]);

  async function handleJoin(channelId: string) {
    setJoiningChannelId(channelId);

    try {
      await onJoinChannel(channelId);
      onOpenChange(false);
      onSelectChannel(channelId);
    } catch {
      setJoiningChannelId(null);
    }
  }

  function handleSelect(channel: Channel) {
    if (channel.isMember) {
      onOpenChange(false);
      onSelectChannel(channel.id);
    } else {
      void handleJoin(channel.id);
    }
  }

  const selectedItem = allItems[selectedIndex];

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent
        className="gap-0 overflow-hidden p-0"
        data-testid="channel-browser-dialog"
      >
        <DialogHeader className="border-b border-border/80 px-6 py-5">
          <DialogTitle className="flex items-center gap-3">
            <span className="flex h-10 w-10 items-center justify-center rounded-2xl bg-primary text-primary-foreground shadow-sm">
              <Compass className="h-4 w-4" />
            </span>
            Browse Channels
          </DialogTitle>
          <DialogDescription>
            Discover and join open channels.
          </DialogDescription>
          <div className="mt-4 flex items-center gap-3 rounded-2xl border border-input bg-card px-3 py-3 shadow-sm">
            <Search className="h-4 w-4 text-muted-foreground" />
            <Input
              className="h-auto border-0 bg-transparent px-0 py-0 text-base shadow-none focus-visible:ring-0"
              data-testid="channel-browser-search"
              onChange={(event) => {
                setQuery(event.target.value);
                setSelectedIndex(0);
              }}
              onKeyDown={(event) => {
                if (event.key === "ArrowDown" && allItems.length > 0) {
                  event.preventDefault();
                  setSelectedIndex((current) =>
                    Math.min(current + 1, allItems.length - 1),
                  );
                  return;
                }

                if (event.key === "ArrowUp" && allItems.length > 0) {
                  event.preventDefault();
                  setSelectedIndex((current) => Math.max(current - 1, 0));
                  return;
                }

                if (
                  event.key === "Enter" &&
                  !event.nativeEvent.isComposing &&
                  selectedItem
                ) {
                  event.preventDefault();
                  handleSelect(selectedItem);
                }
              }}
              placeholder="Search channels by name or description"
              ref={inputRef}
              value={query}
            />
            <div className="hidden items-center gap-1 rounded-lg border border-border bg-background px-2 py-1 text-[10px] font-semibold uppercase tracking-[0.16em] text-muted-foreground sm:flex">
              &#x21E7;&#x2318;B
            </div>
          </div>
        </DialogHeader>

        <div className="max-h-[60vh] overflow-y-auto">
          {browsableChannels.length === 0 ? (
            deferredQuery.length > 0 ? (
              <BrowseState
                description="Try a different name or keyword."
                icon={Search}
                title="No channels match your search"
              />
            ) : (
              <BrowseState
                description="All open channels are available in the sidebar. Create a new channel to get started."
                icon={Compass}
                title="No channels to browse"
              />
            )
          ) : (
            <div className="p-3">
              {notJoined.length > 0 ? (
                <>
                  <div className="mb-3 flex items-center justify-between px-2 text-xs font-semibold uppercase tracking-[0.16em] text-muted-foreground">
                    <span>
                      {notJoined.length} channel
                      {notJoined.length !== 1 ? "s" : ""} to join
                    </span>
                    <span>Enter to join</span>
                  </div>
                  <div className="space-y-2">
                    {notJoined.map((channel) => {
                      const flatIndex = allItems.indexOf(channel);
                      return (
                        <ChannelCard
                          channel={channel}
                          isJoining={joiningChannelId === channel.id}
                          isSelected={flatIndex === selectedIndex}
                          key={channel.id}
                          onJoin={() => {
                            void handleJoin(channel.id);
                          }}
                          onMouseEnter={() => setSelectedIndex(flatIndex)}
                          onSelect={() => handleSelect(channel)}
                        />
                      );
                    })}
                  </div>
                </>
              ) : null}

              {joined.length > 0 ? (
                <>
                  <div className="mb-3 mt-4 flex items-center px-2 text-xs font-semibold uppercase tracking-[0.16em] text-muted-foreground">
                    <span>Joined</span>
                  </div>
                  <div className="space-y-2">
                    {joined.map((channel) => {
                      const flatIndex = allItems.indexOf(channel);
                      return (
                        <ChannelCard
                          channel={channel}
                          isJoining={false}
                          isSelected={flatIndex === selectedIndex}
                          key={channel.id}
                          onMouseEnter={() => setSelectedIndex(flatIndex)}
                          onSelect={() => handleSelect(channel)}
                        />
                      );
                    })}
                  </div>
                </>
              ) : null}
            </div>
          )}
        </div>

        <div className="border-t border-border/80 bg-card/50 px-6 py-3 text-xs text-muted-foreground">
          Showing open channels. Private channels require an invite.
        </div>
      </DialogContent>
    </Dialog>
  );
}

function ChannelCard({
  channel,
  isJoining,
  isSelected,
  onJoin,
  onMouseEnter,
  onSelect,
}: {
  channel: Channel;
  isJoining: boolean;
  isSelected: boolean;
  onJoin?: () => void;
  onMouseEnter: () => void;
  onSelect: () => void;
}) {
  return (
    <button
      className={
        isSelected
          ? "w-full rounded-2xl border border-primary/30 bg-primary/10 px-4 py-4 text-left shadow-sm outline-none transition-colors"
          : "w-full rounded-2xl border border-border/80 bg-card/60 px-4 py-4 text-left shadow-sm outline-none transition-colors hover:border-primary/20 hover:bg-accent"
      }
      data-testid={`browse-channel-${channel.name}`}
      onClick={onSelect}
      onMouseEnter={onMouseEnter}
      type="button"
    >
      <div className="flex items-start gap-3">
        <div className="mt-0.5 flex h-9 w-9 shrink-0 items-center justify-center rounded-2xl bg-secondary text-secondary-foreground">
          {channel.channelType === "forum" ? (
            <FileText className="h-4 w-4" />
          ) : (
            <Hash className="h-4 w-4" />
          )}
        </div>

        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <p className="text-sm font-semibold tracking-tight">
              {channel.name}
            </p>
            <p className="rounded-full bg-muted px-2 py-0.5 text-[10px] font-medium uppercase tracking-[0.16em] text-muted-foreground">
              {channel.channelType}
            </p>
            <div className="ml-auto flex items-center gap-3">
              <span className="flex items-center gap-1 text-xs text-muted-foreground">
                <Users className="h-3 w-3" />
                {channel.memberCount}
              </span>
              <span className="text-xs text-muted-foreground">
                {formatRelativeTime(channel.lastMessageAt)}
              </span>
            </div>
          </div>
          {channel.description ? (
            <p className="mt-1 text-sm leading-6 text-muted-foreground line-clamp-2">
              {channel.description}
            </p>
          ) : null}
        </div>

        {!channel.isMember && onJoin ? (
          <Button
            className="shrink-0"
            disabled={isJoining}
            onClick={(event) => {
              event.stopPropagation();
              onJoin();
            }}
            size="sm"
            type="button"
            variant="default"
          >
            <LogIn className="mr-1.5 h-3.5 w-3.5" />
            {isJoining ? "Joining..." : "Join"}
          </Button>
        ) : null}
      </div>
    </button>
  );
}
