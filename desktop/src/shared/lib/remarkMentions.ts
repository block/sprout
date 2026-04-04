/**
 * Remark plugin that detects @mention patterns in text nodes and wraps them
 * in custom HAST `mention` elements for styled rendering via react-markdown.
 *
 * When `mentionNames` is provided, multi-word display names (e.g. "John Doe")
 * are matched first (longest-first to avoid partial matches), then the plugin
 * falls back to the generic `@\S+` pattern for unknown mentions.
 */

import { createRemarkPrefixPlugin } from "./createRemarkPrefixPlugin";
import { buildMentionPattern } from "./mentionPattern";

type RemarkMentionsOptions = {
  mentionNames?: string[];
};

export default function remarkMentions(options?: RemarkMentionsOptions) {
  const mentionPattern = buildMentionPattern(options?.mentionNames ?? []);

  return createRemarkPrefixPlugin(mentionPattern, (matchText) => ({
    type: "mention",
    value: matchText,
    data: {
      hName: "mention",
      hChildren: [{ type: "text", value: matchText }],
    },
  }));
}
