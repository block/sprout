import { mergeAttributes, Node, nodeInputRule } from "@tiptap/core";

import { rewriteRelayUrl } from "@/shared/lib/mediaUrl";
import { escapeRegExp } from "@/shared/lib/mentionPattern";

/**
 * Inline atom node for a custom emoji, modeled on Tiptap's mention/emoji
 * extensions. It renders the emoji image but behaves as a single selectable,
 * copyable, deletable unit — exactly like a built-in unicode emoji glyph —
 * unlike a decoration overlay, which can't be selected.
 *
 * Crucially it serializes to `:shortcode:` everywhere the rest of the app reads
 * the composer:
 *  - Markdown (tiptap-markdown `addStorage().markdown.serialize`) → `:shortcode:`
 *    so the send path (`buildCustomEmojiTags`/`splitOutgoingTags`) is untouched.
 *  - Plain text (`renderText`) → `:shortcode:` so `doc.textContent`,
 *    `doc.textBetween`, and the autocomplete plain-text projection see the
 *    shortcode at its natural width.
 *
 * An input rule converts a completed known `:shortcode:` into the node as the
 * user types or picks from the emoji menu. Unknown `:foo:` sequences stay plain
 * text (a user mid-typing `:par` shouldn't flicker into a node).
 */

export const CUSTOM_EMOJI_NODE_NAME = "customEmoji";

export interface CustomEmojiNodeOptions {
  /** Resolve a (lowercased) shortcode to its image URL. */
  resolveUrl: (shortcode: string) => string | undefined;
  /** All known shortcodes, used to build the input-rule pattern. */
  shortcodes: () => string[];
}

/**
 * Build a case-insensitive regex matching a completed `:shortcode:` for any
 * known shortcode, longest-first so a longer name isn't shadowed by a shorter
 * prefix. Returns null when there are no known shortcodes. Exported for testing
 * and reused by the input rule.
 */
export function buildKnownShortcodeAlternation(
  shortcodes: string[],
): string | null {
  const sorted = [...new Set(shortcodes)]
    .filter((s) => s.trim().length > 0)
    .sort((a, b) => b.length - a.length);
  if (sorted.length === 0) return null;
  return sorted.map((s) => escapeRegExp(s)).join("|");
}

export const CustomEmojiNode = Node.create<CustomEmojiNodeOptions>({
  name: CUSTOM_EMOJI_NODE_NAME,

  // Inline, atomic (selected/deleted as one unit), and a leaf (no content) —
  // the same shape as the built-in emoji "glyph" the user expects to mimic.
  group: "inline",
  inline: true,
  atom: true,
  selectable: true,

  addOptions() {
    return {
      resolveUrl: () => undefined,
      shortcodes: () => [],
    };
  },

  addAttributes() {
    return {
      shortcode: {
        default: "",
        parseHTML: (el) => {
          const e = el as HTMLElement;
          const fromData = e.getAttribute("data-shortcode");
          if (fromData) return fromData;
          // Timeline-rendered emoji (markdown.tsx) carry no data-shortcode;
          // recover it from the `:shortcode:` alt so copy-from-timeline →
          // paste-into-composer still produces a proper node.
          const alt = e.getAttribute("alt") ?? "";
          const m = /^:([^:\s]+):$/.exec(alt);
          return m?.[1] ?? "";
        },
        renderHTML: (attrs) => ({ "data-shortcode": attrs.shortcode }),
      },
      src: {
        default: "",
        parseHTML: (el) => (el as HTMLElement).getAttribute("src") ?? "",
        // `src` is derived from the (relay-owned) set at render time, not
        // persisted in serialized output — see renderText/markdown below.
        renderHTML: () => ({}),
      },
    };
  },

  parseHTML() {
    return [{ tag: "img[data-custom-emoji]" }];
  },

  renderHTML({ node, HTMLAttributes }) {
    const shortcode = String(node.attrs.shortcode ?? "");
    const rawSrc = String(node.attrs.src ?? "");
    const src = rawSrc ? rewriteRelayUrl(rawSrc) : rawSrc;
    return [
      "img",
      mergeAttributes(HTMLAttributes, {
        src,
        alt: `:${shortcode}:`,
        "data-custom-emoji": "",
        "data-shortcode": shortcode,
        draggable: "false",
        // Match the message-view <img data-custom-emoji> sizing exactly.
        class:
          "mx-px inline-block h-[1.25em] w-auto max-w-none align-text-bottom",
      }),
    ];
  },

  // Plain-text projection of the node: the literal shortcode. Powers
  // `doc.textContent` / `doc.textBetween` (and our autocomplete projection),
  // so cursor math treats `:shortcode:` at its real width.
  renderText({ node }) {
    return `:${node.attrs.shortcode}:`;
  },

  addStorage() {
    return {
      markdown: {
        serialize(
          // biome-ignore lint/suspicious/noExplicitAny: prosemirror-markdown state is untyped here
          state: any,
          // biome-ignore lint/suspicious/noExplicitAny: PM node
          node: any,
        ) {
          state.write(`:${node.attrs.shortcode}:`);
        },
        parse: {},
      },
    };
  },

  addInputRules() {
    const options = this.options;
    return [
      nodeInputRule({
        // Lazily resolve the pattern from the *current* known set on each
        // keystroke. An empty set → a regex that can never match.
        find: (text: string) => {
          const alternation = buildKnownShortcodeAlternation(
            options.shortcodes(),
          );
          if (!alternation) return null;
          // Match a completed `:shortcode:` ending at the input position.
          const re = new RegExp(`(:(?:${alternation}):)$`, "i");
          const match = re.exec(text);
          if (!match) return null;
          return {
            index: match.index,
            text: match[0],
            replaceWith: match[1],
            match,
            data: undefined,
          };
        },
        type: this.type,
        getAttributes: (match) => {
          const matched = match[1] ?? "";
          const shortcode = matched.slice(1, -1).toLowerCase();
          return {
            shortcode,
            src: options.resolveUrl(shortcode) ?? "",
          };
        },
      }),
    ];
  },
});
