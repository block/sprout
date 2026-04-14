/**
 * Remark plugin that groups consecutive image-only paragraphs into a single
 * `imageGallery` node. This allows react-markdown to render multiple images
 * as a 2-column grid layout instead of stacking them vertically.
 *
 * A paragraph is considered "image-only" when it contains exactly one child
 * that is an `image` node (ignoring surrounding whitespace text nodes).
 *
 * Single images are left as-is (no gallery wrapper).
 */

// biome-ignore lint/suspicious/noExplicitAny: mdast node types not available as direct deps
type MdastNode = any;

/**
 * Returns the image node if the paragraph contains only a single image
 * (with optional surrounding whitespace text), otherwise null.
 */
function extractSoleImage(node: MdastNode): MdastNode | null {
  if (node.type !== "paragraph") return null;
  let image: MdastNode | null = null;
  for (const child of node.children ?? []) {
    if (child.type === "image") {
      if (image) return null; // more than one image in this paragraph
      image = child;
    } else if (child.type === "text" && child.value.trim() === "") {
      continue; // whitespace text is fine
    } else {
      return null; // non-image, non-whitespace content
    }
  }
  return image;
}

export default function remarkImageGallery() {
  return (tree: MdastNode) => {
    const newChildren: MdastNode[] = [];
    let imageRun: MdastNode[] = [];

    function flushRun() {
      if (imageRun.length <= 1) {
        // Single image or empty — leave as a normal paragraph with the image
        for (const img of imageRun) {
          newChildren.push({
            type: "paragraph",
            children: [img],
          });
        }
      } else {
        // Multiple consecutive images — wrap in a gallery node
        newChildren.push({
          type: "imageGallery",
          children: imageRun.map((img) => ({
            type: "paragraph",
            children: [img],
          })),
          data: {
            hName: "image-gallery",
          },
        });
      }
      imageRun = [];
    }

    for (const child of tree.children ?? []) {
      const img = extractSoleImage(child);
      if (img) {
        imageRun.push(img);
        continue;
      }
      // Not an image-only paragraph — flush any accumulated run
      flushRun();
      newChildren.push(child);
    }

    // Flush any trailing run
    flushRun();

    tree.children = newChildren;
  };
}
