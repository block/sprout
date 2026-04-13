import * as React from "react";

import type { BlobDescriptor } from "@/shared/api/tauri";
import { shortHash } from "./useMediaUpload";

export type ImageRefSuggestion = {
  url: string;
  hash: string;
  type: string;
  thumb?: string;
};

/**
 * Detects `![` typed in the editor and shows suggestions from attached
 * images. Lightweight alternative to @tiptap/suggestion — works with
 * plain text + cursor position from the existing bridge.
 */
export function useImageRefSuggestions(attachments: BlobDescriptor[]) {
  const [isOpen, setIsOpen] = React.useState(false);
  const [query, setQuery] = React.useState("");
  const [selectedIndex, setSelectedIndex] = React.useState(0);

  const suggestions: ImageRefSuggestion[] = React.useMemo(() => {
    const items = attachments.map((a) => ({
      url: a.url,
      hash: shortHash(a.sha256),
      type: a.type,
      thumb: a.thumb,
    }));
    if (!query) return items;
    return items.filter((s) =>
      s.hash.toLowerCase().includes(query.toLowerCase()),
    );
  }, [attachments, query]);

  /** Call on every editor update with the plain text and cursor position. */
  const updateQuery = React.useCallback(
    (text: string, cursor: number) => {
      if (attachments.length === 0) {
        if (isOpen) setIsOpen(false);
        return;
      }

      // Look for `![` before cursor, not yet closed with `]`
      const before = text.slice(0, cursor);
      const match = /!\[([^\]]*)$/.exec(before);

      if (match) {
        setQuery(match[1]);
        setIsOpen(true);
        setSelectedIndex(0);
      } else if (isOpen) {
        setIsOpen(false);
        setQuery("");
      }
    },
    [attachments.length, isOpen],
  );

  /** Handle keyboard navigation. Returns { handled, suggestion? }. */
  const handleKeyDown = React.useCallback(
    (event: React.KeyboardEvent) => {
      if (!isOpen || suggestions.length === 0) {
        return { handled: false } as const;
      }

      if (event.key === "ArrowDown") {
        event.preventDefault();
        setSelectedIndex((i) => (i + 1) % suggestions.length);
        return { handled: true } as const;
      }

      if (event.key === "ArrowUp") {
        event.preventDefault();
        setSelectedIndex((i) => (i <= 0 ? suggestions.length - 1 : i - 1));
        return { handled: true } as const;
      }

      if (event.key === "Tab" || event.key === "Enter") {
        event.preventDefault();
        const suggestion = suggestions[selectedIndex];
        setIsOpen(false);
        setQuery("");
        return { handled: true, suggestion } as const;
      }

      if (event.key === "Escape") {
        event.preventDefault();
        setIsOpen(false);
        setQuery("");
        return { handled: true } as const;
      }

      return { handled: false } as const;
    },
    [isOpen, suggestions, selectedIndex],
  );

  const clear = React.useCallback(() => {
    setIsOpen(false);
    setQuery("");
    setSelectedIndex(0);
  }, []);

  return {
    isOpen,
    suggestions,
    selectedIndex,
    updateQuery,
    handleKeyDown,
    clear,
  };
}
