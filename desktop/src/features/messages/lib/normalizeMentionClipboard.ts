/**
 * Detect whether clipboard HTML contains Sprout mention / channel-link
 * elements (marked with `data-mention` or `data-channel-link` attributes).
 */
export function hasMentionClipboardHtml(html: string): boolean {
  return html.includes("data-mention") || html.includes("data-channel-link");
}

/**
 * Normalize clipboard HTML that contains Sprout mention / channel-link
 * elements.  Replaces the styled `<span data-mention>` and
 * `<button data-channel-link>` wrappers with their plain text content so
 * the resulting string is free of formatting that would confuse TipTap's
 * Bold extension (which matches font-weight >= 500 as bold).
 *
 * Returns the flattened plain-text string ready for insertion into the
 * editor.
 */
export function normalizeMentionClipboardHtml(html: string): string {
  const doc = new DOMParser().parseFromString(html, "text/html");

  for (const el of Array.from(
    doc.querySelectorAll("[data-mention], [data-channel-link]"),
  )) {
    const text = doc.createTextNode(el.textContent ?? "");
    el.replaceWith(text);
  }

  return doc.body.textContent ?? "";
}
