/**
 * Canonical pubkey normalisation.
 *
 * Hex pubkeys are case-insensitive, but callers compare them with `===`.
 * Trimming guards against stray whitespace from user input or tag parsing.
 */
export function normalizePubkey(pubkey: string): string {
  return pubkey.trim().toLowerCase();
}

/**
 * Shorten a pubkey for display: first 8 chars + ellipsis + last N chars.
 * tailLength defaults to 4 (e.g. "abcd1234…5678").
 */
export function truncatePubkey(pubkey: string, tailLength = 4): string {
  if (pubkey.length <= 16) return pubkey;
  return `${pubkey.slice(0, 8)}…${pubkey.slice(-tailLength)}`;
}
