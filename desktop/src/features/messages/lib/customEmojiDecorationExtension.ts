import { Extension } from "@tiptap/core";
import { Plugin, PluginKey, type Transaction } from "@tiptap/pm/state";
import { Decoration, DecorationSet } from "@tiptap/pm/view";

import type { CustomEmoji } from "@/shared/lib/remarkCustomEmoji";
import { rewriteRelayUrl } from "@/shared/lib/mediaUrl";
import { escapeRegExp } from "@/shared/lib/mentionPattern";

export const customEmojiDecorationKey = new PluginKey("customEmojiDecoration");

/**
 * TipTap extension that renders known custom-emoji shortcodes (`:shortcode:`)
 * as inline images directly in the composer — mirroring how the message view
 * renders them (remarkCustomEmoji → <img>), so the input previews exactly what
 * will be sent.
 *
 * Crucially, the document text stays `:shortcode:` — the image is purely a
 * ProseMirror decoration overlay. Serialization, the send path, and
 * `buildCustomEmojiTags`/`mergeOutgoingTags` are all untouched. This is the
 * same plain-text-preserving trick `MentionHighlightExtension` uses.
 *
 * Each match gets two decorations:
 *  - an inline decoration that collapses the literal `:shortcode:` text to
 *    zero width (it stays in the doc and is fully selectable/deletable, just
 *    not painted), and
 *  - a widget that paints the emoji <img> at the match start.
 *
 * Only shortcodes present in the provided set are matched; unknown `:foo:`
 * sequences are left as plain text (a user mid-typing `:par` shouldn't flicker).
 */
export const CustomEmojiDecorationExtension = Extension.create({
  name: "customEmojiDecoration",

  addStorage() {
    return {
      customEmoji: [] as CustomEmoji[],
    };
  },

  addProseMirrorPlugins() {
    const extension = this;

    return [
      new Plugin({
        key: customEmojiDecorationKey,
        state: {
          init(_, state) {
            return buildDecorations(state.doc, extension.storage.customEmoji);
          },
          apply(tr, oldDecorations) {
            // Emoji set changed — full rebuild.
            if (tr.getMeta(customEmojiDecorationKey)) {
              return buildDecorations(tr.doc, extension.storage.customEmoji);
            }

            if (!tr.docChanged) {
              return oldDecorations;
            }

            // A `:` in the changed range can create/destroy a shortcode
            // boundary; an edit overlapping an existing emoji decoration can
            // make it stale (e.g. `:party:` → `:part:`). Either way, rebuild.
            if (
              editAffectsColon(tr) ||
              editIntersectsDecoration(tr, oldDecorations)
            ) {
              return buildDecorations(tr.doc, extension.storage.customEmoji);
            }

            return oldDecorations.map(tr.mapping, tr.doc);
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
 * Build a case-insensitive regex matching `:shortcode:` for any known
 * shortcode, longest-first so a longer name isn't shadowed by a shorter
 * prefix. Returns null when there are no known shortcodes. Mirrors
 * `remarkCustomEmoji`'s pattern so the composer and the message view agree on
 * what counts as a custom emoji.
 *
 * Exported for testing.
 */
export function buildEmojiShortcodePattern(
  shortcodes: string[],
): RegExp | null {
  const sorted = [...new Set(shortcodes)]
    .filter((s) => s.trim().length > 0)
    .sort((a, b) => b.length - a.length);
  if (sorted.length === 0) return null;
  const alternatives = sorted.map((s) => escapeRegExp(s)).join("|");
  return new RegExp(`:(?:${alternatives}):`, "gi");
}

/**
 * Returns true if any changed range in the transaction (old or new content)
 * contains a `:` — meaning a shortcode boundary may have been created,
 * modified, or destroyed and a full rebuild is required.
 */
function editAffectsColon(tr: Transaction): boolean {
  for (let i = 0; i < tr.steps.length; i++) {
    const map = tr.mapping.maps[i];

    let found = false;
    map.forEach((oldFrom, oldTo, newFrom, newTo) => {
      if (found) return;

      const clampedNewTo = Math.min(newTo, tr.doc.content.size);
      const clampedNewFrom = Math.min(newFrom, clampedNewTo);
      if (clampedNewFrom < clampedNewTo) {
        const newText = tr.doc.textBetween(
          clampedNewFrom,
          clampedNewTo,
          "\n",
          "\0",
        );
        if (newText.includes(":")) {
          found = true;
          return;
        }
      }

      const clampedOldTo = Math.min(oldTo, tr.before.content.size);
      const clampedOldFrom = Math.min(oldFrom, clampedOldTo);
      if (clampedOldFrom < clampedOldTo) {
        const oldText = tr.before.textBetween(
          clampedOldFrom,
          clampedOldTo,
          "\n",
          "\0",
        );
        if (oldText.includes(":")) {
          found = true;
        }
      }
    });

    if (found) return true;
  }

  return false;
}

/**
 * Returns true if any changed range overlaps an existing emoji decoration —
 * the mapped decoration would be stale and we need a full rebuild.
 */
function editIntersectsDecoration(
  tr: Transaction,
  decorations: DecorationSet,
): boolean {
  let hit = false;
  tr.mapping.maps.forEach((map) => {
    map.forEach((oldFrom, oldTo) => {
      if (hit) return;
      if (decorations.find(oldFrom, oldTo).length > 0) {
        hit = true;
      }
    });
  });
  return hit;
}

function buildEmojiImg(shortcode: string, url: string): HTMLElement {
  const img = document.createElement("img");
  img.src = rewriteRelayUrl(url);
  img.alt = `:${shortcode}:`;
  img.setAttribute("data-custom-emoji", "");
  img.draggable = false;
  // Match the message-view rendering: line-height-sized, baseline-aligned.
  img.className =
    "mx-px inline-block h-[1.25em] w-auto max-w-none align-text-bottom";
  return img;
}

function buildDecorations(
  doc: Parameters<typeof DecorationSet.create>[0],
  customEmoji: CustomEmoji[],
): DecorationSet {
  const pattern = buildEmojiShortcodePattern(
    customEmoji.map((e) => e.shortcode),
  );
  if (!pattern) return DecorationSet.empty;

  const urlByShortcode = new Map(
    customEmoji.map((e) => [e.shortcode.toLowerCase(), e.url]),
  );

  const decorations: Decoration[] = [];

  doc.descendants((node, pos) => {
    if (!node.isText || !node.text) return;

    pattern.lastIndex = 0;
    let match: RegExpExecArray | null = pattern.exec(node.text);
    while (match !== null) {
      const matchText = match[0]; // `:Shortcode:`
      const shortcode = matchText.slice(1, -1).toLowerCase();
      const url = urlByShortcode.get(shortcode);
      if (url) {
        const from = pos + match.index;
        const to = from + matchText.length;
        // Collapse the literal text (stays in the doc, just not painted)…
        decorations.push(
          Decoration.inline(from, to, { class: "custom-emoji-source" }),
        );
        // …and paint the emoji image at the match start. `side: -1` keeps the
        // widget before the (zero-width) text so the caret lands naturally.
        decorations.push(
          Decoration.widget(from, () => buildEmojiImg(shortcode, url), {
            side: -1,
            // Treat the widget as part of the preceding content for mapping so
            // ProseMirror doesn't try to type "into" it.
            ignoreSelection: true,
            key: `emoji:${shortcode}:${from}`,
          }),
        );
      }
      match = pattern.exec(node.text);
    }
  });

  return DecorationSet.create(doc, decorations);
}
