/**
 * Escape special regex characters in a string.
 */
export function escapeRegExp(str: string): string {
  return str.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * Build a regex that matches a given prefix followed by known multi-word names
 * (longest-first to avoid partial matches), then falling back to prefix + \S+.
 */
export function buildPrefixPattern(
  prefix: string,
  knownNames: string[],
): RegExp {
  const sorted = [...new Set(knownNames)]
    .filter((name) => name.trim().length > 0)
    .sort((a, b) => b.length - a.length);

  const escapedPrefix = escapeRegExp(prefix);

  if (sorted.length === 0) {
    return new RegExp(`${escapedPrefix}\\S+`, "g");
  }

  const nameAlternatives = sorted.map((name) => escapeRegExp(name)).join("|");
  return new RegExp(`${escapedPrefix}(?:${nameAlternatives}|\\S+)`, "g");
}

/**
 * Build a regex that matches @mentions, trying known multi-word names first
 * (longest-first to avoid partial matches), then falling back to @\S+.
 */
export function buildMentionPattern(mentionNames: string[]): RegExp {
  return buildPrefixPattern("@", mentionNames);
}
