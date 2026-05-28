import { LoaderCircle, Search } from "lucide-react";
import * as React from "react";

import {
  MIN_SEARCH_QUERY_LENGTH,
  useSearchResults,
} from "@/features/search/useSearchResults";
import {
  resultIcon,
  resultKey,
  resultTestId,
  type SearchResult,
} from "@/features/search/ui/SearchResultItem";
import type { Channel, SearchHit } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";

type TopbarSearchProps = {
  channels: Channel[];
  className?: string;
  currentPubkey?: string;
  onOpenChannel: (channelId: string) => void;
  onOpenResult: (hit: SearchHit) => void;
};

function resultTitle(result: SearchResult) {
  if (result.kind === "channel") {
    return result.channel.name;
  }

  return result.hit.channelName ?? "Message";
}

function resultSummary(result: SearchResult) {
  if (result.kind === "channel") {
    return result.channel.description || result.channel.channelType;
  }

  return result.hit.content.trim() || "No message body.";
}

export function TopbarSearch({
  channels,
  className,
  currentPubkey,
  onOpenChannel,
  onOpenResult,
}: TopbarSearchProps) {
  const [isOpen, setIsOpen] = React.useState(false);
  const rootRef = React.useRef<HTMLDivElement>(null);
  const {
    channelLookup,
    debouncedQuery,
    query,
    resultProfiles,
    results,
    searchQuery,
    selectedIndex,
    selectedResult,
    setQuery,
    setSelectedIndex,
  } = useSearchResults({ channels, enabled: isOpen, limit: 8 });

  const openResult = React.useCallback(
    (result: SearchResult) => {
      setIsOpen(false);
      setQuery("");

      if (result.kind === "channel") {
        onOpenChannel(result.channel.id);
        return;
      }

      onOpenResult(result.hit);
    },
    [onOpenChannel, onOpenResult, setQuery],
  );

  React.useEffect(() => {
    function handlePointerDown(event: PointerEvent) {
      if (
        event.target instanceof Node &&
        rootRef.current?.contains(event.target)
      ) {
        return;
      }

      setIsOpen(false);
    }

    window.addEventListener("pointerdown", handlePointerDown);
    return () => {
      window.removeEventListener("pointerdown", handlePointerDown);
    };
  }, []);

  const showSuggestions = isOpen && query.trim().length > 0;

  return (
    <div className={cn("relative", className)} ref={rootRef}>
      <div className="flex h-7 items-center gap-2 rounded-lg border border-border/70 bg-muted/45 px-2.5 text-xs text-muted-foreground shadow-xs backdrop-blur transition-colors focus-within:border-border focus-within:bg-muted/70 focus-within:text-foreground hover:bg-muted/70 supports-[backdrop-filter]:bg-muted/35">
        <Search className="h-3.5 w-3.5 shrink-0" />
        <input
          aria-label="Search everything"
          className="min-w-0 flex-1 bg-transparent text-xs text-foreground placeholder:text-muted-foreground outline-none"
          data-testid="open-search"
          onChange={(event) => {
            setIsOpen(true);
            setQuery(event.target.value);
            setSelectedIndex(0);
          }}
          onFocus={() => setIsOpen(true)}
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

            if (event.key === "Escape") {
              event.preventDefault();
              setIsOpen(false);
              return;
            }

            if (
              event.key === "Enter" &&
              !event.nativeEvent.isComposing &&
              selectedResult
            ) {
              event.preventDefault();
              openResult(selectedResult);
            }
          }}
          placeholder="Search everything"
          value={query}
        />
        <kbd className="shrink-0 text-[10px] text-muted-foreground/70">
          &#x2318;K
        </kbd>
      </div>

      {showSuggestions ? (
        <div className="absolute left-1/2 top-full z-50 mt-1 w-[560px] max-w-[min(80vw,560px)] -translate-x-1/2 overflow-hidden rounded-xl border border-border/80 bg-popover text-popover-foreground shadow-xl">
          {debouncedQuery.length < MIN_SEARCH_QUERY_LENGTH ? (
            <p className="px-3 py-3 text-xs text-muted-foreground">
              Type at least two characters to search.
            </p>
          ) : searchQuery.isLoading && results.length === 0 ? (
            <div className="flex items-center gap-2 px-3 py-3 text-xs text-muted-foreground">
              <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
              Searching...
            </div>
          ) : searchQuery.error instanceof Error && results.length === 0 ? (
            <p className="px-3 py-3 text-xs text-destructive">
              {searchQuery.error.message}
            </p>
          ) : results.length === 0 ? (
            <p className="px-3 py-3 text-xs text-muted-foreground">
              No matches found.
            </p>
          ) : (
            <div className="max-h-[360px] overflow-y-auto p-1.5">
              {results.map((result, index) => (
                <button
                  className={cn(
                    "flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-left transition-colors",
                    index === selectedIndex
                      ? "bg-accent text-accent-foreground"
                      : "hover:bg-accent/70",
                  )}
                  key={resultKey(result)}
                  onClick={() => openResult(result)}
                  onMouseEnter={() => setSelectedIndex(index)}
                  type="button"
                  data-testid={resultTestId(result)}
                >
                  {React.createElement(resultIcon(result, channelLookup), {
                    className: "h-3.5 w-3.5 shrink-0 text-muted-foreground",
                  })}
                  <span className="min-w-0 flex-1 truncate text-xs">
                    <span className="font-medium">{resultTitle(result)}</span>
                    <span className="mx-1.5 text-muted-foreground">-</span>
                    <span className="text-muted-foreground">
                      {resultSummary(result)}
                    </span>
                  </span>
                  <span className="shrink-0 text-[10px] uppercase tracking-wide text-muted-foreground/70">
                    {result.kind === "channel" ? "Channel" : "Message"}
                  </span>
                </button>
              ))}
            </div>
          )}
        </div>
      ) : null}
    </div>
  );
}
