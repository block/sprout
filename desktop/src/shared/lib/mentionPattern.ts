/**
 * Escape special regex characters in a string.
 */
export function escapeRegExp(str: string): string {
  return str.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * Build a regex that matches a given prefix followed by known multi-word names
 * (longest-first to avoid partial matches). When known names are provided,
 * only those names are matched — no generic fallback. When no names are
 * available, returns a never-matching regex so that arbitrary prefix+word
 * patterns are not highlighted as if they were valid matches (e.g. @Will
 * should not render as a mention when only "Will Pfleger" is a real user).
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
    // No known names — don't highlight anything as a mention.
    // Previously fell back to prefix+\S+ which created false positives for
    // messages without p-tags or with unresolved multi-word display names.
    return /(?!)/gi; // never matches
  }

  const nameAlternatives = sorted.map((name) => escapeRegExp(name)).join("|");
  const boundary = "(?=[\\s,;.!?:)\\]}]|$)";
  return new RegExp(`${escapedPrefix}(?:${nameAlternatives})${boundary}`, "gi");
}

/**
 * Build a regex that matches @mentions for known multi-word names
 * (longest-first to avoid partial matches). When no known names are provided,
 * returns a never-matching regex — @word patterns are not highlighted unless
 * they correspond to an actual p-tagged member.
 */
export function buildMentionPattern(mentionNames: string[]): RegExp {
  return buildPrefixPattern("@", mentionNames);
}
