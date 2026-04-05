import * as React from "react";
import {
  ArrowRight,
  FileText,
  Hash,
  LoaderCircle,
  MessagesSquare,
  Search,
  type LucideIcon,
} from "lucide-react";

import {
  resolveUserLabel,
  resolveUserSecondaryLabel,
} from "@/features/profile/lib/identity";
import { useUsersBatchQuery } from "@/features/profile/hooks";
import { useSearchMessagesQuery } from "@/features/search/hooks";
import type { Channel, SearchHit } from "@/shared/api/types";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import { Input } from "@/shared/ui/input";
import { Skeleton } from "@/shared/ui/skeleton";

const MIN_QUERY_LENGTH = 2;

function describeSearchHit(hit: SearchHit) {
  switch (hit.kind) {
    case 45001:
      return "Forum post";
    case 45003:
      return "Forum reply";
    case 43001:
      return "Agent job";
    case 43003:
      return "Agent update";
    case 46010:
      return "Approval request";
    default:
      return "Message";
  }
}

function truncateContent(content: string) {
  const trimmed = content.trim();
  if (trimmed.length === 0) {
    return "No message body.";
  }

  if (trimmed.length <= 180) {
    return trimmed;
  }

  return `${trimmed.slice(0, 177)}...`;
}

function formatRelativeTime(unixSeconds: number) {
  const diff = Math.floor(Date.now() / 1_000) - unixSeconds;

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
    hour: "numeric",
    minute: "2-digit",
  }).format(new Date(unixSeconds * 1_000));
}

function SearchState({
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

function SearchLoadingState() {
  return (
    <div className="space-y-3 px-3 py-3" data-testid="search-loading">
      {["first", "second", "third"].map((row) => (
        <div
          className="rounded-2xl border border-border/80 bg-card/60 p-4"
          key={row}
        >
          <Skeleton className="h-4 w-32" />
          <Skeleton className="mt-3 h-4 w-full" />
          <Skeleton className="mt-2 h-4 w-3/4" />
        </div>
      ))}
    </div>
  );
}

type SearchDialogProps = {
  channels: Channel[];
  currentPubkey?: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onOpenResult: (hit: SearchHit) => void;
};

export function SearchDialog({
  channels,
  currentPubkey,
  open,
  onOpenChange,
  onOpenResult,
}: SearchDialogProps) {
  const [query, setQuery] = React.useState("");
  const [debouncedQuery, setDebouncedQuery] = React.useState("");
  const [selectedIndex, setSelectedIndex] = React.useState(0);
  const inputRef = React.useRef<HTMLInputElement>(null);
  const channelLookup = React.useMemo(
    () => new Map(channels.map((channel) => [channel.id, channel])),
    [channels],
  );

  const searchQuery = useSearchMessagesQuery(debouncedQuery, {
    enabled: open,
    limit: 12,
  });

  const results = searchQuery.data?.hits ?? [];
  const resultProfilesQuery = useUsersBatchQuery(
    results.map((hit) => hit.pubkey),
    {
      enabled: open && results.length > 0,
    },
  );
  const resultProfiles = resultProfilesQuery.data?.profiles;

  const openResult = React.useCallback(
    (hit: SearchHit) => {
      onOpenChange(false);
      onOpenResult(hit);
    },
    [onOpenChange, onOpenResult],
  );

  React.useEffect(() => {
    const trimmed = query.trim();
    if (trimmed.length < MIN_QUERY_LENGTH) {
      setDebouncedQuery("");
      return;
    }

    const timeout = window.setTimeout(() => {
      setDebouncedQuery(trimmed);
    }, 300);

    return () => {
      window.clearTimeout(timeout);
    };
  }, [query]);

  React.useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (
        event.key.toLowerCase() !== "k" ||
        !(event.metaKey || event.ctrlKey) ||
        event.altKey ||
        event.shiftKey
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
      setDebouncedQuery("");
      setSelectedIndex(0);
    }
  }, [open]);

  React.useEffect(() => {
    setSelectedIndex((current) => {
      if (results.length === 0) {
        return 0;
      }

      return Math.min(current, results.length - 1);
    });
  }, [results]);

  const selectedHit = results[selectedIndex];

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent
        className="gap-0 overflow-hidden p-0"
        data-testid="search-dialog"
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          inputRef.current?.focus();
        }}
      >
        <DialogHeader className="border-b border-border/80 px-6 py-5">
          <DialogTitle className="flex items-center gap-3">
            <span className="flex h-10 w-10 items-center justify-center rounded-2xl bg-primary text-primary-foreground shadow-sm">
              <Search className="h-4 w-4" />
            </span>
            Search
          </DialogTitle>
          <DialogDescription>
            Full-text search across accessible channels.
          </DialogDescription>
          <div className="mt-4 flex items-center gap-3 rounded-2xl border border-input bg-card px-3 py-3 shadow-sm">
            <Search className="h-4 w-4 text-muted-foreground" />
            <Input
              autoFocus
              className="h-auto border-0 bg-transparent px-0 py-0 text-base shadow-none focus-visible:ring-0"
              data-testid="search-input"
              onChange={(event) => {
                setQuery(event.target.value);
                setSelectedIndex(0);
              }}
              onKeyDown={(event) => {
                if (event.key === "ArrowDown" && results.length > 0) {
                  event.preventDefault();
                  setSelectedIndex((current) =>
                    Math.min(current + 1, results.length - 1),
                  );
                  return;
                }

                if (event.key === "ArrowUp" && results.length > 0) {
                  event.preventDefault();
                  setSelectedIndex((current) => Math.max(current - 1, 0));
                  return;
                }

                if (
                  event.key === "Enter" &&
                  !event.nativeEvent.isComposing &&
                  selectedHit
                ) {
                  event.preventDefault();
                  openResult(selectedHit);
                }
              }}
              placeholder="Search messages, approvals, and forum posts"
              ref={inputRef}
              value={query}
            />
            <span className="hidden shrink-0 text-xs text-muted-foreground/50 sm:block">
              &#x2318;K
            </span>
          </div>
        </DialogHeader>

        <div className="max-h-[60vh] overflow-y-auto">
          {debouncedQuery.length < MIN_QUERY_LENGTH ? (
            <SearchState
              description="Type at least two characters to search the relay-backed history for streams, forums, DMs, approvals, and agent updates."
              icon={MessagesSquare}
              title="Search message history"
            />
          ) : searchQuery.isLoading ? (
            <SearchLoadingState />
          ) : searchQuery.error instanceof Error ? (
            <SearchState
              description={searchQuery.error.message}
              icon={LoaderCircle}
              title="Search unavailable"
            />
          ) : results.length === 0 ? (
            <SearchState
              description="Try a different keyword, channel name, or phrase from the message body."
              icon={Search}
              title="No matches found"
            />
          ) : (
            <div className="p-3" data-testid="search-results">
              <div className="mb-3 flex items-center justify-between px-2 text-xs font-semibold uppercase tracking-[0.16em] text-muted-foreground">
                <span>{searchQuery.data?.found ?? results.length} results</span>
                <span>Enter to open</span>
              </div>

              <div className="space-y-2">
                {results.map((hit, index) => {
                  const channel = hit.channelId
                    ? channelLookup.get(hit.channelId)
                    : undefined;
                  const authorLabel = resolveUserLabel({
                    pubkey: hit.pubkey,
                    currentPubkey,
                    profiles: resultProfiles,
                    preferResolvedSelfLabel: true,
                  });
                  const authorSecondaryLabel = resolveUserSecondaryLabel({
                    pubkey: hit.pubkey,
                    profiles: resultProfiles,
                  });

                  return (
                    <button
                      className={
                        index === selectedIndex
                          ? "w-full rounded-2xl border border-primary/30 bg-primary/10 px-4 py-4 text-left shadow-sm outline-none transition-colors"
                          : "w-full rounded-2xl border border-border/80 bg-card/60 px-4 py-4 text-left shadow-sm outline-none transition-colors hover:border-primary/20 hover:bg-accent"
                      }
                      data-testid={`search-result-${hit.eventId}`}
                      key={hit.eventId}
                      onClick={() => openResult(hit)}
                      onMouseEnter={() => setSelectedIndex(index)}
                      type="button"
                    >
                      <div className="flex items-start gap-3">
                        <div className="mt-0.5 flex h-9 w-9 shrink-0 items-center justify-center rounded-2xl bg-secondary text-secondary-foreground">
                          {channel?.channelType === "forum" ? (
                            <FileText className="h-4 w-4" />
                          ) : (
                            <Hash className="h-4 w-4" />
                          )}
                        </div>

                        <div className="min-w-0 flex-1">
                          <div className="flex flex-wrap items-center gap-2">
                            <p className="text-sm font-semibold tracking-tight">
                              {hit.channelName}
                            </p>
                            <p className="rounded-full bg-muted px-2 py-0.5 text-[10px] font-medium uppercase tracking-[0.16em] text-muted-foreground">
                              {describeSearchHit(hit)}
                            </p>
                            <p className="text-xs text-muted-foreground">
                              {authorLabel}
                            </p>
                            <p className="ml-auto whitespace-nowrap text-xs text-muted-foreground">
                              {formatRelativeTime(hit.createdAt)}
                            </p>
                          </div>
                          {authorSecondaryLabel ? (
                            <p className="mt-1 text-xs text-muted-foreground">
                              {authorSecondaryLabel}
                            </p>
                          ) : null}
                          <p className="mt-2 text-sm leading-6 text-foreground">
                            {truncateContent(hit.content)}
                          </p>
                        </div>

                        <ArrowRight className="mt-1 h-4 w-4 shrink-0 text-muted-foreground" />
                      </div>
                    </button>
                  );
                })}
              </div>
            </div>
          )}
        </div>

        <div className="border-t border-border/80 bg-card/50 px-6 py-3 text-xs text-muted-foreground">
          Search is relay-backed and scoped to channels you can access.
        </div>
      </DialogContent>
    </Dialog>
  );
}
