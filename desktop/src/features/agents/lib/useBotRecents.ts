import * as React from "react";

const STORAGE_KEY = "sprout:bot-recents";
const MAX_RECENTS = 8;

// Default persona display names to seed the list when empty.
// These are resolved to IDs by the consumer.
export const DEFAULT_PERSONA_NAMES = ["Solo", "Ralph", "Scout"] as const;

export function useBotRecents(): {
  recentIds: string[];
  pushRecent: (personaId: string) => void;
} {
  const [recentIds, setRecentIds] = React.useState<string[]>(() => {
    try {
      const raw = localStorage.getItem(STORAGE_KEY);
      return raw ? (JSON.parse(raw) as string[]) : [];
    } catch {
      return [];
    }
  });

  const pushRecent = React.useCallback((personaId: string) => {
    setRecentIds((prev) => {
      const next = [personaId, ...prev.filter((id) => id !== personaId)].slice(
        0,
        MAX_RECENTS,
      );
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
      } catch {
        // localStorage full — ignore
      }
      return next;
    });
  }, []);

  return { recentIds, pushRecent };
}
