/**
 * Remark plugin that detects @mention patterns in text nodes and wraps them
 * in custom HAST `mention` elements for styled rendering via react-markdown.
 */
export default function remarkMentions() {
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
      const parts = splitMentions(child.value);
      if (parts.length > 1) {
        node.children.splice(i, 1, ...parts);
      }
    } else {
      walkChildren(child);
    }
  }
}

function splitMentions(text: string) {
  const mentionPattern = /@\S+/g;
  // biome-ignore lint/suspicious/noExplicitAny: building mdast-compatible nodes
  const parts: any[] = [];
  let lastIndex = 0;
  let match: RegExpExecArray | null = null;

  while (true) {
    match = mentionPattern.exec(text);
    if (!match) {
      break;
    }

    if (match.index > lastIndex) {
      parts.push({ type: "text", value: text.slice(lastIndex, match.index) });
    }

    parts.push({
      type: "mention",
      value: match[0],
      data: {
        hName: "mention",
        hChildren: [{ type: "text", value: match[0] }],
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
