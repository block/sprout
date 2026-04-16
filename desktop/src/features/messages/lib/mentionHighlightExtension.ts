import { Extension } from "@tiptap/core";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import { Decoration, DecorationSet } from "@tiptap/pm/view";

export const mentionHighlightKey = new PluginKey("mentionHighlight");

/**
 * TipTap extension that applies inline `mention-highlight` decorations
 * to `@Name` and `#channel-name` patterns in the document.
 *
 * Accepts `names` (display names) and `channelNames` storage options.
 * On every doc update the plugin scans text nodes and decorates matches.
 */
export const MentionHighlightExtension = Extension.create({
  name: "mentionHighlight",

  addStorage() {
    return {
      names: [] as string[],
      channelNames: [] as string[],
    };
  },

  addProseMirrorPlugins() {
    const extension = this;

    return [
      new Plugin({
        key: mentionHighlightKey,
        state: {
          init(_, state) {
            return buildDecorations(
              state.doc,
              extension.storage.names,
              extension.storage.channelNames,
            );
          },
          apply(tr, oldDecorations) {
            if (tr.docChanged || tr.getMeta(mentionHighlightKey)) {
              return buildDecorations(
                tr.doc,
                extension.storage.names,
                extension.storage.channelNames,
              );
            }
            return oldDecorations;
          },
        },
        props: {
          decorations(state) {
            return this.getState(state) ?? DecorationSet.empty;
          },
        },
      }),
    ];
  },
});

/**
 * Build highlight patterns for @Name and #channel-name matching.
 * Exported for testing — the patterns are the core logic of this extension.
 */
export function buildHighlightPatterns(
  names: string[],
  channelNames: string[],
): RegExp[] {
  const patterns: RegExp[] = [];

  if (names.length > 0) {
    const sortedNames = [...names].sort((a, b) => b.length - a.length);
    const escapedNames = sortedNames.map((n) =>
      n.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"),
    );
    patterns.push(
      new RegExp(`(?:^|(?<=\\s))@(${escapedNames.join("|")})`, "gi"),
    );
  }

  if (channelNames.length > 0) {
    const sortedChannels = [...channelNames].sort(
      (a, b) => b.length - a.length,
    );
    const escapedChannels = sortedChannels.map((n) =>
      n.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"),
    );
    patterns.push(
      new RegExp(`(?:^|(?<=\\s))#(${escapedChannels.join("|")})`, "gi"),
    );
  }

  return patterns;
}

/**
 * Find all highlight matches in a text string given a set of patterns.
 * Returns an array of { from, to } offsets relative to the text start.
 * Exported for testing.
 */
export function findHighlightMatches(
  text: string,
  patterns: RegExp[],
): { from: number; to: number; match: string }[] {
  const results: { from: number; to: number; match: string }[] = [];
  for (const pattern of patterns) {
    pattern.lastIndex = 0;
    let m: RegExpExecArray | null = pattern.exec(text);
    while (m !== null) {
      results.push({ from: m.index, to: m.index + m[0].length, match: m[0] });
      m = pattern.exec(text);
    }
  }
  return results;
}

function buildDecorations(
  doc: Parameters<typeof DecorationSet.create>[0],
  names: string[],
  channelNames: string[],
): DecorationSet {
  if (names.length === 0 && channelNames.length === 0)
    return DecorationSet.empty;

  const decorations: Decoration[] = [];
  const patterns = buildHighlightPatterns(names, channelNames);

  doc.descendants((node, pos) => {
    if (!node.isText || !node.text) return;

    for (const pattern of patterns) {
      pattern.lastIndex = 0;
      let match: RegExpExecArray | null = pattern.exec(node.text);
      while (match !== null) {
        const from = pos + match.index;
        const to = from + match[0].length;
        decorations.push(
          Decoration.inline(from, to, { class: "mention-highlight" }),
        );
        match = pattern.exec(node.text);
      }
    }
  });

  return DecorationSet.create(doc, decorations);
}
