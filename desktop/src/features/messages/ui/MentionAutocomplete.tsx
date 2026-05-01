import * as React from "react";

import { truncatePubkey } from "@/features/profile/lib/identity";
import { Badge } from "@/shared/ui/badge";
import { cn } from "@/shared/lib/cn";
import { UserAvatar } from "@/shared/ui/UserAvatar";

export type MentionSuggestion = {
  pubkey: string;
  displayName: string;
  avatarUrl?: string | null;
  role?: string | null;
  personaName?: string | null;
};

type MentionAutocompleteProps = {
  suggestions: MentionSuggestion[];
  selectedIndex: number;
  onSelect: (suggestion: MentionSuggestion) => void;
  position?: "above" | "below";
};

export const MentionAutocomplete = React.memo(function MentionAutocomplete({
  suggestions,
  selectedIndex,
  onSelect,
  position = "above",
}: MentionAutocompleteProps) {
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
            key={suggestion.pubkey}
            onMouseDown={(event) => {
              event.preventDefault();
              onSelect(suggestion);
            }}
            tabIndex={-1}
            type="button"
          >
            <UserAvatar
              avatarUrl={suggestion.avatarUrl ?? null}
              displayName={suggestion.displayName}
              size="xs"
            />
            <span className="truncate font-medium">
              {suggestion.displayName}
            </span>
            <span className="shrink-0 font-mono text-[10px] text-muted-foreground/50">
              {truncatePubkey(suggestion.pubkey)}
            </span>
            {suggestion.personaName ? (
              <span className="text-xs text-muted-foreground">
                ({suggestion.personaName})
              </span>
            ) : suggestion.role ? (
              <Badge variant="secondary">{suggestion.role}</Badge>
            ) : null}
          </button>
        ))}
      </div>
    </div>
  );
});
