import * as React from "react";

import type { ChannelSuggestion } from "@/features/messages/lib/useChannelLinks";
import { cn } from "@/shared/lib/cn";

type ChannelAutocompleteProps = {
  suggestions: ChannelSuggestion[];
  selectedIndex: number;
  onSelect: (suggestion: ChannelSuggestion) => void;
  position?: "above" | "below";
};

export const ChannelAutocomplete = React.memo(function ChannelAutocomplete({
  suggestions,
  selectedIndex,
  onSelect,
  position = "above",
}: ChannelAutocompleteProps) {
  const listRef = React.useRef<HTMLDivElement>(null);

  React.useEffect(() => {
    const activeItem = listRef.current?.children[selectedIndex] as
      | HTMLElement
      | undefined;
    activeItem?.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

  if (suggestions.length === 0) {
    return null;
  }

  return (
    <div
      className={cn(
        "absolute left-0 right-0 z-50 px-3 sm:px-4",
        position === "below" ? "top-full mt-1" : "bottom-full mb-1",
      )}
    >
      <div
        className="max-h-48 overflow-y-auto rounded-xl border bg-popover p-1 shadow-lg"
        ref={listRef}
      >
        {suggestions.map((suggestion, index) => (
          <button
            className={cn(
              "flex w-full cursor-pointer items-center gap-2 rounded-lg px-3 py-1.5 text-left text-sm",
              index === selectedIndex
                ? "bg-accent text-accent-foreground"
                : "text-popover-foreground hover:bg-accent/50",
            )}
            key={suggestion.id}
            onMouseDown={(event) => {
              event.preventDefault();
              onSelect(suggestion);
            }}
            tabIndex={-1}
            type="button"
          >
            <span className="truncate font-medium">#{suggestion.name}</span>
            <span className="rounded-full bg-muted px-2 py-0.5 text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
              {suggestion.channelType}
            </span>
          </button>
        ))}
      </div>
    </div>
  );
});
