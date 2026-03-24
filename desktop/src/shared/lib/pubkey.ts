/**
 * Canonical pubkey normalisation.
 *
 * Hex pubkeys are case-insensitive, but callers compare them with `===`.
 * Trimming guards against stray whitespace from user input or tag parsing.
 */
export function normalizePubkey(pubkey: string): string {
  return pubkey.trim().toLowerCase();
}
