import { Extension } from "@tiptap/core";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import { Decoration, DecorationSet } from "@tiptap/pm/view";

export const mentionHighlightKey = new PluginKey("mentionHighlight");

/**
 * TipTap extension that applies inline `mention-highlight` decorations
 * to `@Name` patterns in the document.
 *
 * Accepts a `names` storage option — an array of known display names.
 * On every doc update the plugin scans text nodes and decorates matches.
 */
export const MentionHighlightExtension = Extension.create({
  name: "mentionHighlight",

  addStorage() {
    return {
      names: [] as string[],
    };
  },

  addProseMirrorPlugins() {
    const extension = this;

    return [
      new Plugin({
        key: mentionHighlightKey,
        state: {
          init(_, state) {
            return buildDecorations(state.doc, extension.storage.names);
          },
          apply(tr, oldDecorations) {
            if (tr.docChanged || tr.getMeta(mentionHighlightKey)) {
              return buildDecorations(
                tr.doc,
                extension.storage.names,
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
): DecorationSet {
  if (names.length === 0) return DecorationSet.empty;

  const decorations: Decoration[] = [];

  // Build a regex that matches @Name for any known name.
  // Escape special regex chars in names and sort longest-first for greedy matching.
  const sorted = [...names].sort((a, b) => b.length - a.length);
  const escaped = sorted.map((n) =>
    n.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"),
  );
  const pattern = new RegExp(
    `(?:^|(?<=\\s))@(${escaped.join("|")})`,
    "gi",
  );

  doc.descendants((node, pos) => {
    if (!node.isText || !node.text) return;

    // We need to match against text that may span from start-of-node.
    // ProseMirror pos points to the start of the text node content.
    pattern.lastIndex = 0;
    let match: RegExpExecArray | null;
    while ((match = pattern.exec(node.text)) !== null) {
      const from = pos + match.index;
      // The match may include a leading whitespace char from the lookbehind —
      // but lookbehind doesn't consume, so match[0] starts at @.
      const to = from + match[0].length;
      decorations.push(
        Decoration.inline(from, to, { class: "mention-highlight" }),
      );
    }
  });

  return DecorationSet.create(doc, decorations);
}
