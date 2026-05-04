/**
 * Rehype plugin that groups consecutive image-only paragraphs into a single
 * merged `<p>` containing all the images. The custom `p` component in
 * markdown.tsx detects 2+ images and renders them as a grid gallery.
 *
 * This runs at the HAST (HTML AST) level, before React rendering, so
 * consecutive `![](a)\n![](b)` paragraphs get merged and the `p` component
 * receives all images together.
 *
 * A paragraph is "image-only" when it contains only `<img>` elements
 * (plus optional whitespace text nodes and `<br>` from remark processing).
 */

// Minimal HAST types — avoids adding @types/hast as a dependency.
interface HastText {
  type: "text";
  value: string;
}

interface HastElement {
  type: "element";
  tagName: string;
  properties: Record<string, unknown>;
  children: HastNode[];
}

type HastNode = HastElement | HastText | { type: string };

interface HastRoot {
  type: "root";
  children: HastNode[];
}

function isElement(node: HastNode): node is HastElement {
  return node.type === "element";
}

function isText(node: HastNode): node is HastText {
  return node.type === "text";
}

function isImageOnlyParagraph(node: HastNode): node is HastElement {
  if (!isElement(node) || node.tagName !== "p") {
    return false;
  }

  const meaningful = node.children.filter(
    (child) =>
      !(isText(child) && child.value.trim() === "") &&
      !(isElement(child) && child.tagName === "br"),
  );

  return (
    meaningful.length >= 1 &&
    meaningful.every((child) => isElement(child) && child.tagName === "img")
  );
}

export default function rehypeImageGallery() {
  return (tree: HastRoot) => {
    const newChildren: HastNode[] = [];
    let imageRun: HastElement[] = [];

    function flushRun() {
      if (imageRun.length <= 1) {
        newChildren.push(...imageRun);
      } else {
        // Merge consecutive single-image paragraphs into one paragraph
        // containing all the images. The `p` component in markdown.tsx
        // will detect 2+ images and render the grid gallery.
        const allImages: HastNode[] = [];
        for (const p of imageRun) {
          for (const child of p.children) {
            if (isElement(child) && child.tagName === "img") {
              allImages.push(child);
            }
          }
        }
        newChildren.push({
          type: "element",
          tagName: "p",
          properties: {},
          children: allImages,
        });
      }
      imageRun = [];
    }

    for (const child of tree.children) {
      if (isImageOnlyParagraph(child)) {
        imageRun.push(child);
        continue;
      }
      flushRun();
      newChildren.push(child);
    }
    flushRun();

    tree.children = newChildren;
  };
}
