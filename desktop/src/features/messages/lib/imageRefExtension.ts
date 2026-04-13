import { mergeAttributes, Node } from "@tiptap/core";

/**
 * Custom Tiptap inline node for image reference chips.
 *
 * Stores the attachment URL and short hash. Renders as a non-editable
 * inline pill like `![a3f2]`. On send, the composer resolves these to
 * `![image](url)` markdown.
 */
export const ImageRefNode = Node.create({
  name: "imageRef",
  group: "inline",
  inline: true,
  atom: true, // non-editable, treated as a single unit

  addAttributes() {
    return {
      url: { default: null },
      hash: { default: null },
      mediaType: { default: "image" },
      thumb: { default: null },
    };
  },

  parseHTML() {
    return [{ tag: "span[data-image-ref]" }];
  },

  addStorage() {
    return {
      markdown: {
        serialize(
          state: { write: (text: string) => void },
          node: { attrs: { hash?: string } },
        ) {
          state.write(`![${node.attrs.hash ?? "?"}]`);
        },
        parse: {},
      },
    };
  },

  renderHTML({ HTMLAttributes }: { HTMLAttributes: Record<string, string> }) {
    const thumb = HTMLAttributes.thumb || HTMLAttributes.url || "";
    const hash = HTMLAttributes.hash ?? "?";

    return [
      "span",
      mergeAttributes(HTMLAttributes, {
        "data-image-ref": "",
        class:
          "inline-flex items-center rounded-lg overflow-hidden align-baseline cursor-default select-none border border-border/50",
        contenteditable: "false",
        title: `![${hash}]`,
      }),
      [
        "img",
        {
          src: thumb,
          alt: `![${hash}]`,
          class: "inline-block h-16 max-w-48 rounded-lg object-contain",
          draggable: "false",
        },
      ],
    ];
  },
});
