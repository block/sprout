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
            return buildDecorations(state.doc, extension.storage.names, extension.storage.channelNames);
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

function buildDecorations(
  doc: Parameters<typeof DecorationSet.create>[0],
  names: string[],
  channelNames: string[],
): DecorationSet {
  if (names.length === 0 && channelNames.length === 0) return DecorationSet.empty;

  const decorations: Decoration[] = [];

  // Build patterns for @Name and #channel-name.
  // Escape special regex chars and sort longest-first for greedy matching.
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
    const sortedChannels = [...channelNames].sort((a, b) => b.length - a.length);
    const escapedChannels = sortedChannels.map((n) =>
      n.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"),
    );
    patterns.push(
      new RegExp(`(?:^|(?<=\\s))#(${escapedChannels.join("|")})`, "gi"),
    );
  }

  doc.descendants((node, pos) => {
    if (!node.isText || !node.text) return;

    for (const pattern of patterns) {
      // We need to match against text that may span from start-of-node.
      // ProseMirror pos points to the start of the text node content.
      pattern.lastIndex = 0;
      let match: RegExpExecArray | null;
      while ((match = pattern.exec(node.text)) !== null) {
        const from = pos + match.index;
        // The match may include a leading whitespace char from the lookbehind —
        // but lookbehind doesn't consume, so match[0] starts at @ or #.
        const to = from + match[0].length;
        decorations.push(
          Decoration.inline(from, to, { class: "mention-highlight" }),
        );
      }
    }
  });

  return DecorationSet.create(doc, decorations);
}
