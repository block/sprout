/**
 * Remark plugin that detects #channel-name patterns in text nodes and wraps them
 * in custom HAST `channel-link` elements for styled rendering via react-markdown.
 */
export default function remarkChannelLinks() {
  // biome-ignore lint/suspicious/noExplicitAny: remark tree types are not available
  return (tree: any) => {
    walkChildren(tree);
  };
}

// biome-ignore lint/suspicious/noExplicitAny: remark tree types are not available
function walkChildren(node: any) {
  if (!node?.children || !Array.isArray(node.children)) {
    return;
  }

  for (let i = node.children.length - 1; i >= 0; i--) {
    const child = node.children[i];

    if (child.type === "text") {
      const parts = splitChannelLinks(child.value);
      if (parts.length > 1) {
        node.children.splice(i, 1, ...parts);
      }
    } else {
      walkChildren(child);
    }
  }
}

function splitChannelLinks(text: string) {
  const channelPattern = /#[a-zA-Z0-9][\w-]*/g;
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
