import * as React from "react";

import type { ImageRefSuggestion } from "@/features/messages/lib/useImageRefSuggestions";
import { rewriteRelayUrl } from "@/shared/lib/mediaUrl";

type ImageRefAutocompleteProps = {
  suggestions: ImageRefSuggestion[];
  selectedIndex: number;
  onSelect: (suggestion: ImageRefSuggestion) => void;
};

/**
 * Autocomplete dropdown for `![` image reference insertion.
 * Shows attached images by their short hash with a small thumbnail.
 */
export const ImageRefAutocomplete = React.memo(function ImageRefAutocomplete({
  suggestions,
  selectedIndex,
  onSelect,
}: ImageRefAutocompleteProps) {
  if (suggestions.length === 0) return null;

  return (
    <div className="absolute bottom-full left-0 z-10 mb-1 max-h-40 overflow-y-auto rounded-lg border border-border bg-popover p-1 shadow-md">
      {suggestions.map((suggestion, index) => {
        const thumbUrl = suggestion.thumb
          ? rewriteRelayUrl(suggestion.thumb)
          : rewriteRelayUrl(suggestion.url);
        const isVideo = suggestion.type.startsWith("video/");

        return (
          <button
            key={suggestion.url}
            type="button"
            className={`flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-sm ${
              index === selectedIndex
                ? "bg-accent text-accent-foreground"
                : "text-foreground hover:bg-accent/50"
            }`}
            onMouseDown={(e) => {
              e.preventDefault(); // Don't steal focus from editor
              onSelect(suggestion);
            }}
          >
            <div className="h-6 w-6 overflow-hidden rounded border border-border/50">
              {isVideo ? (
                <div className="flex h-full w-full items-center justify-center bg-muted text-[8px] text-muted-foreground">
                  ▶
                </div>
              ) : (
                <img
                  src={thumbUrl}
                  alt={suggestion.hash}
                  className="h-full w-full object-cover"
                />
              )}
            </div>
            <span className="font-mono text-xs">![{suggestion.hash}]</span>
          </button>
        );
      })}
    </div>
  );
});
