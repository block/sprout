/**
 * Remark plugin that detects #channel-name patterns in text nodes and wraps them
 * in custom HAST `channel-link` elements for styled rendering via react-markdown.
 *
 * When `channelNames` is provided, multi-word channel names (e.g. "my channel")
 * are matched first (longest-first to avoid partial matches), then the plugin
 * falls back to the generic `#\S+` pattern for unknown channels.
 */

import { buildPrefixPattern } from "./mentionPattern";

type RemarkChannelLinksOptions = {
  channelNames?: string[];
};

export default function remarkChannelLinks(
  options?: RemarkChannelLinksOptions,
) {
  const channelPattern = buildPrefixPattern("#", options?.channelNames ?? []);

  return (
    // biome-ignore lint/suspicious/noExplicitAny: remark tree types are not available
    tree: any,
  ) => {
    walkChildren(tree, channelPattern);
  };
}

// biome-ignore lint/suspicious/noExplicitAny: remark tree types are not available
function walkChildren(node: any, channelPattern: RegExp) {
  if (!node?.children || !Array.isArray(node.children)) {
    return;
  }

  for (let i = node.children.length - 1; i >= 0; i--) {
    const child = node.children[i];

    if (child.type === "text") {
      const parts = splitChannelLinks(child.value, channelPattern);
      if (parts.length > 1) {
        node.children.splice(i, 1, ...parts);
      }
    } else {
      walkChildren(child, channelPattern);
    }
  }
}

function splitChannelLinks(text: string, channelPattern: RegExp) {
  // Reset lastIndex — the pattern is reused across text nodes with the `g` flag
  channelPattern.lastIndex = 0;
  // biome-ignore lint/suspicious/noExplicitAny: building mdast-compatible nodes
  const parts: any[] = [];
  let lastIndex = 0;
  let match: RegExpExecArray | null = null;

  while (true) {
    match = channelPattern.exec(text);
    if (!match) {
      break;
    }

    if (match.index > lastIndex) {
      parts.push({ type: "text", value: text.slice(lastIndex, match.index) });
    }

    const channelName = match[0].slice(1);

    parts.push({
      type: "channel-link",
      value: match[0],
      data: {
        hName: "channel-link",
        hChildren: [{ type: "text", value: match[0] }],
        channelName,
      },
    });

    lastIndex = match.index + match[0].length;
  }

  if (parts.length === 0) {
    return [{ type: "text", value: text }];
  }

  if (lastIndex < text.length) {
    parts.push({ type: "text", value: text.slice(lastIndex) });
  }

  return parts;
}
