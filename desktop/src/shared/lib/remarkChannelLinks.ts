/**
 * Remark plugin that detects #channel-name patterns in text nodes and wraps them
 * in custom HAST `channel-link` elements for styled rendering via react-markdown.
 *
 * When `channelNames` is provided, multi-word channel names (e.g. "my channel")
 * are matched first (longest-first to avoid partial matches), then the plugin
 * falls back to the generic `#\S+` pattern for unknown channels.
 */

import { createRemarkPrefixPlugin } from "./createRemarkPrefixPlugin";
import { buildPrefixPattern } from "./mentionPattern";

type RemarkChannelLinksOptions = {
  channelNames?: string[];
};

export default function remarkChannelLinks(
  options?: RemarkChannelLinksOptions,
) {
  const channelPattern = buildPrefixPattern("#", options?.channelNames ?? []);

  return createRemarkPrefixPlugin(channelPattern, (matchText) => {
    const channelName = matchText.slice(1);
    return {
      type: "channel-link",
      value: matchText,
      data: {
        hName: "channel-link",
        hChildren: [{ type: "text", value: matchText }],
        channelName,
      },
    };
  });
}
