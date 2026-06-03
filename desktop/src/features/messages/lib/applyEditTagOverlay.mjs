/**
 * Pure helper for applying an edit event's imeta tags onto an original
 * message event. Used by both the renderer (formatTimelineMessages.ts)
 * and the post-edit cache update (useEditMessageMutation in hooks.ts) so
 * they stay in sync.
 *
 * Lives in `.mjs` (not `.ts`) so the test runner (`node --test`, no TS
 * loader) can import the same source the production code uses. The
 * TypeScript-facing callers get typed access via the sibling `.d.mts`.
 */

/**
 * Merge the original event's tags with an edit's tags so that:
 *   - `imeta` tags come exclusively from the edit (full new attachment set);
 *   - `emoji` (NIP-30 custom-emoji) tags come exclusively from the edit (the
 *     edited body may add or remove custom emoji, so the shortcode→url set is
 *     rebuilt from the edit);
 *   - all other tag kinds (`h`, `e`, `p` mentions, etc.) come exclusively
 *     from the original — the edit can't rewrite channel membership,
 *     thread refs, or mention targets.
 *
 * When `editTags` is undefined, returns `originalTags` unchanged.
 */
export function applyEditTagOverlay(originalTags, editTags) {
  if (!editTags) return originalTags;
  // Drop the original's imeta + emoji; both are fully replaced by the edit.
  const baseFromOriginal = originalTags.filter(
    (t) => t[0] !== "imeta" && t[0] !== "emoji",
  );
  const overlaidFromEdit = editTags.filter(
    (t) => t[0] === "imeta" || t[0] === "emoji",
  );
  return [...baseFromOriginal, ...overlaidFromEdit];
}
