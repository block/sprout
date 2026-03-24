/**
 * Escape special regex characters in a string.
 */
export function escapeRegExp(str: string): string {
  return str.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * Build a regex that matches @mentions, trying known multi-word names first
 * (longest-first to avoid partial matches), then falling back to @\S+.
 */
export function buildMentionPattern(mentionNames: string[]): RegExp {
  const sorted = [...new Set(mentionNames)]
    .filter((name) => name.trim().length > 0)
    .sort((a, b) => b.length - a.length);

  if (sorted.length === 0) {
    return /@\S+/g;
  }

  const nameAlternatives = sorted.map((name) => escapeRegExp(name)).join("|");
  return new RegExp(`@(?:${nameAlternatives}|\\S+)`, "g");
}
