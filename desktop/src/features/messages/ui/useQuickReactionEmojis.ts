import * as React from "react";

const QUICK_REACTION_STORAGE_KEY = "buzz.quick-reaction-emojis.v1";
const DEFAULT_QUICK_REACTIONS = ["👍", "❤️", "😂", "🎉"] as const;
const MAX_STORED_REACTIONS = 24;
const sessionQuickReactionEmojis = new Map<number, string[]>();

type QuickReactionEntry = {
  count: number;
  emoji: string;
  lastUsedAt: number;
};

function canUseLocalStorage() {
  if (typeof window === "undefined") return false;

  try {
    return Boolean(window.localStorage);
  } catch {
    return false;
  }
}

function normalizeEntry(entry: unknown): QuickReactionEntry | null {
  if (!entry || typeof entry !== "object") return null;

  const candidate = entry as Partial<QuickReactionEntry>;
  if (
    typeof candidate.emoji !== "string" ||
    candidate.emoji.trim().length === 0
  ) {
    return null;
  }

  return {
    count: Math.max(1, Math.floor(Number(candidate.count) || 1)),
    emoji: candidate.emoji,
    lastUsedAt: Math.max(0, Number(candidate.lastUsedAt) || 0),
  };
}

function sortEntries(entries: QuickReactionEntry[]) {
  return [...entries].sort((left, right) => {
    const countDelta = right.count - left.count;
    if (countDelta !== 0) return countDelta;
    return right.lastUsedAt - left.lastUsedAt;
  });
}

function readQuickReactionEntries() {
  if (!canUseLocalStorage()) return [];

  try {
    const raw = window.localStorage.getItem(QUICK_REACTION_STORAGE_KEY);
    const parsed = raw ? JSON.parse(raw) : [];
    if (!Array.isArray(parsed)) return [];
    return sortEntries(
      parsed
        .map((entry) => normalizeEntry(entry))
        .filter((entry): entry is QuickReactionEntry => entry !== null),
    );
  } catch {
    return [];
  }
}

function writeQuickReactionEntries(entries: QuickReactionEntry[]) {
  if (!canUseLocalStorage()) return;

  try {
    window.localStorage.setItem(
      QUICK_REACTION_STORAGE_KEY,
      JSON.stringify(sortEntries(entries).slice(0, MAX_STORED_REACTIONS)),
    );
  } catch {
    // Ignore storage failures; the reaction itself should still work.
  }
}

function getQuickReactionEmojis(limit: number) {
  const seen = new Set<string>();
  const next: string[] = [];

  for (const entry of readQuickReactionEntries()) {
    if (seen.has(entry.emoji)) continue;
    seen.add(entry.emoji);
    next.push(entry.emoji);
    if (next.length >= limit) return next;
  }

  for (const emoji of DEFAULT_QUICK_REACTIONS) {
    if (seen.has(emoji)) continue;
    seen.add(emoji);
    next.push(emoji);
    if (next.length >= limit) return next;
  }

  return next;
}

function getSessionQuickReactionEmojis(limit: number) {
  const cached = sessionQuickReactionEmojis.get(limit);
  if (cached) return cached;

  const emojis = getQuickReactionEmojis(limit);
  sessionQuickReactionEmojis.set(limit, emojis);
  return emojis;
}

export function recordQuickReactionEmoji(emoji: string) {
  const trimmed = emoji.trim();
  if (!trimmed) return;

  const entries = readQuickReactionEntries();
  const existing = entries.find((entry) => entry.emoji === trimmed);
  if (existing) {
    existing.count += 1;
    existing.lastUsedAt = Date.now();
  } else {
    entries.push({
      count: 1,
      emoji: trimmed,
      lastUsedAt: Date.now(),
    });
  }

  writeQuickReactionEntries(entries);
}

export function useQuickReactionEmojis(limit = 4) {
  const [emojis, setEmojis] = React.useState(() =>
    getSessionQuickReactionEmojis(limit),
  );

  React.useEffect(() => {
    if (typeof window === "undefined") return;

    const handleStorage = (event: StorageEvent) => {
      if (event.key === QUICK_REACTION_STORAGE_KEY) {
        sessionQuickReactionEmojis.delete(limit);
        setEmojis(getSessionQuickReactionEmojis(limit));
      }
    };

    window.addEventListener("storage", handleStorage);
    setEmojis(getSessionQuickReactionEmojis(limit));

    return () => {
      window.removeEventListener("storage", handleStorage);
    };
  }, [limit]);

  return emojis;
}
