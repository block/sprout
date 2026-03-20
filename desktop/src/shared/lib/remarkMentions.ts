/**
 * Remark plugin that detects @mention patterns in text nodes and wraps them
 * in custom HAST `mention` elements for styled rendering via react-markdown.
 *
 * When `mentionNames` is provided, multi-word display names (e.g. "John Doe")
 * are matched first (longest-first to avoid partial matches), then the plugin
 * falls back to the generic `@\S+` pattern for unknown mentions.
 */

type RemarkMentionsOptions = {
  mentionNames?: string[];
};

export default function remarkMentions(options?: RemarkMentionsOptions) {
  const mentionPattern = buildMentionPattern(options?.mentionNames ?? []);

  return (
    // biome-ignore lint/suspicious/noExplicitAny: remark tree types are not available
    tree: any,
  ) => {
    walkChildren(tree, mentionPattern);
  };
}

// biome-ignore lint/suspicious/noExplicitAny: remark tree types are not available
function walkChildren(node: any, mentionPattern: RegExp) {
  if (!node?.children || !Array.isArray(node.children)) {
    return;
  }

  for (let i = node.children.length - 1; i >= 0; i--) {
    const child = node.children[i];

    if (child.type === "text") {
      const parts = splitMentions(child.value, mentionPattern);
      if (parts.length > 1) {
        node.children.splice(i, 1, ...parts);
      }
    } else {
      walkChildren(child, mentionPattern);
    }
  }
}

function escapeRegExp(str: string): string {
  return str.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function buildMentionPattern(mentionNames: string[]): RegExp {
  // Deduplicate and sort longest-first so "John Doe" is matched before "John"
  const sorted = [...new Set(mentionNames)]
    .filter((name) => name.trim().length > 0)
    .sort((a, b) => b.length - a.length);

  if (sorted.length === 0) {
    return /@\S+/g;
  }

  // Build alternation: try known names first, then fall back to @\S+
  const nameAlternatives = sorted.map((name) => escapeRegExp(name)).join("|");
  return new RegExp(`@(?:${nameAlternatives}|\\S+)`, "g");
}

function splitMentions(text: string, mentionPattern: RegExp) {
  // Reset lastIndex — the pattern is reused across text nodes with the `g` flag
  mentionPattern.lastIndex = 0;
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
