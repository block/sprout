import * as React from "react";

/**
 * Classifies an array of React children into image vs non-image buckets.
 * Used by both the `p` component and `ImageGalleryGrouper` to detect
 * image-only paragraphs for gallery rendering.
 *
 * "Image children" = any React element whose type is not a plain HTML string
 * (i.e. a React component like DialogPrimitive.Root wrapping an img).
 * "Non-image children" = everything else, excluding whitespace-only strings
 * and `<br>` elements (injected by remarkBreaks between images).
 */
export function classifyChildren(childArray: React.ReactNode[]): {
  imageChildren: React.ReactNode[];
  nonImageChildren: React.ReactNode[];
} {
  const imageChildren = childArray.filter(
    (child) => React.isValidElement(child) && typeof child.type !== "string",
  );
  const nonImageChildren = childArray.filter(
    (child) =>
      !(React.isValidElement(child) && typeof child.type !== "string") &&
      !(typeof child === "string" && child.trim() === "") &&
      !(React.isValidElement(child) && child.type === "br"),
  );
  return { imageChildren, nonImageChildren };
}

/** Returns true when a paragraph contains 2+ images and no other content. */
export function isImageOnlyParagraph(childArray: React.ReactNode[]): boolean {
  const { imageChildren, nonImageChildren } = classifyChildren(childArray);
  return imageChildren.length >= 2 && nonImageChildren.length === 0;
}

/**
 * Returns true when a paragraph contains any image/video child. The custom
 * `img` renderer always emits block-level markup (lightbox/video wrapper),
 * so any such paragraph must render as `<div>` to avoid invalid `<p><div>`
 * nesting — even when mixed with text or links.
 */
export function hasBlockMedia(childArray: React.ReactNode[]): boolean {
  const { imageChildren } = classifyChildren(childArray);
  return imageChildren.length >= 1;
}

export function shallowArrayEqual(a?: string[], b?: string[]): boolean {
  if (a === b) return true;
  if (!a || !b) return false;
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) return false;
  }
  return true;
}
